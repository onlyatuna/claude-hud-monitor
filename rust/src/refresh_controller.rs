// src/refresh_controller.rs — Per-provider scheduling with backoff
//
// Mirrors Python core/refresh_controller.py:
//   - Each provider has one concurrent worker at a time.
//   - Failed fetches use exponential back-off (capped at 900s).
//   - Manual refresh invalidates in-flight results; a new query is queued
//     after the old one finishes.
//   - Results flow via std::sync::mpsc channels (equivalent to Python queue).
//   - Stale metrics preserve last successful data while showing error.

use crate::providers::{Provider, UsageMetrics};
use chrono::Utc;
use log::warn;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// State for one provider
#[derive(Default)]
pub struct ProviderState {
    /// Monotonically increasing; incremented on manual refresh or new launch.
    pub generation: u64,
    pub running: bool,
    pub pending: bool,
    pub failures: u32,
    /// Monotonic timestamp when next auto-refresh is due.
    pub due: Option<Instant>,
    pub cached: Option<UsageMetrics>,
}

/// Messages flowing from worker threads back to the controller.
pub struct WorkerResult {
    pub provider_id: String,
    pub generation: u64,
    pub metrics: UsageMetrics,
}

/// What one drain of the worker results produced.
#[derive(Default)]
pub struct Drained {
    /// The results to show, in completion order (Python `updated`).
    pub updates: Vec<UsageMetrics>,
    /// Results after which no provider was running: Python `_complete` emits
    /// `busy_changed(False)` for each. Counted at completion, so a provider that `poll` launches
    /// right after the drain does not hide them.
    pub idle_completions: usize,
}

type NotifyCallback = Arc<parking_lot::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;

pub struct RefreshController {
    pub interval: Duration,
    pub states: HashMap<String, ProviderState>,
    /// Generations of a state added later start above every earlier one, so a worker that was
    /// still running for a removed instance can never match a re-added instance of the same id.
    next_epoch: u64,
    /// Relative jitter of every due time (0 in unit tests, 0.25 in the app).
    jitter: f64,
    result_tx: Sender<WorkerResult>,
    pub result_rx: Receiver<WorkerResult>,
    pub updated: ferrokit_core::signal::Signal<UsageMetrics>,
    notify_callback: NotifyCallback,
    closed: bool,
}

impl RefreshController {
    pub fn new(interval_secs: u64) -> Self {
        let (tx, rx) = mpsc::channel::<WorkerResult>();
        Self {
            interval: Duration::from_secs(interval_secs.max(20)),
            states: HashMap::new(),
            next_epoch: 0,
            jitter: 0.0,
            result_tx: tx,
            result_rx: rx,
            updated: ferrokit_core::signal::Signal::new(),
            notify_callback: Arc::new(parking_lot::Mutex::new(None)),
            closed: false,
        }
    }

    /// Jitters every schedule by up to `jitter` (a fraction) either way.
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = jitter;
        self
    }

    /// Store callback to immediately request repaint / notify event loop when workers complete.
    #[allow(dead_code)]
    pub fn set_notify_callback<F: Fn() + Send + Sync + 'static>(&mut self, cb: F) {
        *self.notify_callback.lock() = Some(Arc::new(cb));
    }

    /// Python `stop()`: after shutdown nothing launches and late worker results are discarded.
    pub fn stop(&mut self) {
        self.closed = true;
        for state in self.states.values_mut() {
            state.generation += 1;
        }
    }
    /// Returns true if any provider worker is currently running.
    #[allow(dead_code)]
    pub fn is_busy(&self) -> bool {
        self.states.values().any(|s| s.running)
    }

    /// Call regularly (e.g. every 1 second) from the UI thread to trigger
    /// scheduled refreshes and check for due providers.
    pub fn poll(
        &mut self,
        providers: &HashMap<String, Arc<dyn Provider + Send + Sync>>,
    ) -> Drained {
        if self.closed {
            return Drained::default();
        }
        let drained = self.drain_results(providers);
        let now = Instant::now();
        let ids: Vec<String> = self.states.keys().cloned().collect();
        for id in ids {
            let state = self.states.get(&id).unwrap();
            let should_launch = !state.running && state.due.is_none_or(|due| now >= due);
            if should_launch && let Some(provider) = providers.get(&id) {
                self.launch(&id, Arc::clone(provider));
            }
        }
        drained
    }

    /// Manual refresh: invalidate in-flight or schedule immediately.
    pub fn refresh(&mut self, providers: &HashMap<String, Arc<dyn Provider + Send + Sync>>) {
        if self.closed {
            return;
        }
        let ids: Vec<String> = self.states.keys().cloned().collect();
        for id in ids {
            let state = self.states.get_mut(&id).unwrap();
            if state.running {
                state.generation += 1;
                state.pending = true;
            } else if let Some(provider) = providers.get(&id) {
                self.launch(&id, Arc::clone(provider));
            }
        }
    }

    /// Adds an instance and fetches it at once. The state exists before the worker starts.
    pub fn add_instance(&mut self, id: &str, provider: Arc<dyn Provider + Send + Sync>) {
        if self.closed || self.states.contains_key(id) {
            return;
        }
        self.next_epoch += 1 << 32;
        self.states.insert(
            id.to_owned(),
            ProviderState {
                generation: self.next_epoch,
                ..Default::default()
            },
        );
        self.launch(id, provider);
    }

    /// Drops an instance: its state goes, so a result still in flight is dropped on arrival.
    pub fn remove_instance(&mut self, id: &str) {
        self.states.remove(id);
    }

    /// Set a new interval and reset next-due timestamps.
    #[allow(dead_code)]
    pub fn set_interval(&mut self, secs: u64) {
        self.interval = Duration::from_secs(secs.max(20));
        let now = Instant::now();
        for (id, state) in self.states.iter_mut() {
            if state.failures == 0 {
                state.due = Some(now + jitter_delay(self.interval, self.jitter, id));
            }
        }
    }

    /// Launch a background worker for `provider_id`.
    pub fn launch(&mut self, id: &str, provider: Arc<dyn Provider + Send + Sync>) {
        let state = self.states.get_mut(id).unwrap();
        state.running = true;
        state.pending = false;
        state.generation += 1;
        let generation = state.generation;

        let tx = self.result_tx.clone();
        let id_owned = id.to_owned();
        let notify_cb_shared = Arc::clone(&self.notify_callback);
        thread::Builder::new()
            .name(format!("quota-{}", id))
            .spawn(move || {
                let started = Instant::now();
                // The provider's panic is logged once below (Python `except Exception` path), so the
                // panic hook stays silent for this call only.
                let metrics_res = crate::logger::with_panic_log_suppressed(|| {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        provider.fetch_usage()
                    }))
                });
                let metrics = match metrics_res {
                    Ok(m) => m,
                    Err(panic_payload) => {
                        let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "非字串 panic payload".to_string()
                        };
                        warn!("provider={} failure=panic: {}", id_owned, msg);
                        UsageMetrics {
                            provider_id: id_owned.clone(),
                            error: Some("查詢失敗，請查看診斷紀錄".to_string()),
                            error_code: "unexpected".to_string(),
                            ..Default::default()
                        }
                    }
                };
                let elapsed = started.elapsed();
                let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
                if elapsed > Duration::from_millis(2000) {
                    warn!(
                        "provider={} slow_fetch elapsed_ms={:.1} status={}",
                        id_owned,
                        elapsed_ms,
                        if metrics.error.is_some() {
                            "error"
                        } else {
                            "ok"
                        }
                    );
                } else {
                    log::info!(
                        "provider={} fetch_ms={:.1} status={}",
                        id_owned,
                        elapsed_ms,
                        if metrics.error.is_some() {
                            "error"
                        } else {
                            "ok"
                        }
                    );
                }

                let _ = tx.send(WorkerResult {
                    provider_id: id_owned,
                    generation,
                    metrics,
                });
                if let Some(cb) = notify_cb_shared.lock().as_ref() {
                    cb();
                }
            })
            .expect("failed to spawn quota worker");
    }

    /// Drain all pending results from workers.
    pub fn drain_results(
        &mut self,
        providers: &HashMap<String, Arc<dyn Provider + Send + Sync>>,
    ) -> Drained {
        let mut drained = Drained::default();
        while !self.closed {
            let Ok(item) = self.result_rx.try_recv() else {
                break;
            };
            if let Some(result) = self.complete(item, providers) {
                self.updated.emit(&result);
                drained.updates.push(result);
                if !self.is_busy() {
                    drained.idle_completions += 1;
                }
            }
        }
        drained
    }

    fn complete(
        &mut self,
        item: WorkerResult,
        providers: &HashMap<String, Arc<dyn Provider + Send + Sync>>,
    ) -> Option<UsageMetrics> {
        if self.closed {
            return None;
        }
        let state = self.states.get_mut(&item.provider_id)?;
        // If the result is from a previous incarnation (older epoch), silently drop it.
        // It does not belong to the current instance lifecycle at all.
        if (item.generation >> 32) < (state.generation >> 32) {
            return None;
        }
        state.running = false;
        // Stale result from an older generation — re-launch if pending
        if item.generation != state.generation || state.pending {
            if let Some(provider) = providers.get(&item.provider_id) {
                self.launch(&item.provider_id, Arc::clone(provider));
            }
            return None;
        }

        let now = Instant::now();
        let mut result = item.metrics;

        if let Some(ref err) = result.error.clone() {
            state.failures += 1;
            let backoff_secs = u64::min(
                900,
                (self.interval.as_secs() as f64 * 2f64.powi((state.failures as i32 - 1).min(4)))
                    as u64,
            );
            let mut delay = jitter_delay(
                Duration::from_secs(backoff_secs),
                self.jitter,
                &item.provider_id,
            );
            if let Some(ra) = result.retry_after.filter(|ra| ra.is_finite() && *ra >= 0.0) {
                delay = delay.max(Duration::from_secs_f64(ra.clamp(1.0, 86400.0)));
            }
            delay = delay.min(Duration::from_secs(86400));
            warn!(
                "provider={} error={} retry={:.1}s",
                item.provider_id,
                result.error_code,
                delay.as_secs_f64()
            );
            state.due = Some(now + delay);

            if result.error_code == "auth" {
                // An authentication failure is terminal: the account shows OFF, not the last
                // good value as stale. Only transient failures keep the previous data (below).
                state.cached = None;
            } else if let Some(cached) = &state.cached {
                // Preserve last successful data as stale
                let mut stale = cached.clone();
                stale.error = Some(err.clone());
                stale.error_code = result.error_code.clone();
                stale.retry_after = result.retry_after;
                stale.stale = true;
                result = stale;
            }
        } else {
            state.failures = 0;
            state.due = Some(now + jitter_delay(self.interval, self.jitter, &item.provider_id));
            result.last_success = Some(Utc::now());
            result.stale = false;
            state.cached = Some(result.clone());
        }

        Some(result)
    }
}

/// Scales `delay` by a factor in `[1 - jitter, 1 + jitter]` (instances drift apart instead of
/// hitting the usage endpoint in the same second every cycle); never below the 20 s minimum.
/// `jitter == 0` leaves `delay` as it is.
fn jitter_delay(delay: Duration, jitter: f64, id: &str) -> Duration {
    if jitter <= 0.0 {
        return delay;
    }
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
        .hash(&mut hasher);
    let unit = (hasher.finish() % 10_000) as f64 / 10_000.0;
    delay
        .mul_f64(1.0 - jitter + 2.0 * jitter * unit)
        .max(Duration::from_secs(20))
}

#[cfg(test)]
mod stop_tests {
    use super::*;

    #[test]
    fn test_stop_ignores_late_results() {
        let mut ctrl = RefreshController::new(60);
        ctrl.states
            .insert("test".to_owned(), ProviderState::default());
        let generation = 1;
        ctrl.states.get_mut("test").unwrap().generation = generation;
        ctrl.states.get_mut("test").unwrap().running = true;
        ctrl.stop();
        ctrl.result_tx
            .send(WorkerResult {
                provider_id: "test".to_owned(),
                generation,
                metrics: UsageMetrics::default(),
            })
            .unwrap();
        let providers = HashMap::new();
        let drained = ctrl.drain_results(&providers);
        assert!(drained.updates.is_empty());
        assert!(ctrl.poll(&providers).updates.is_empty());
        assert!(ctrl.states["test"].generation > generation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the controller publishes for a failure with `code` after one good result.
    fn failure_after_success(code: &str) -> UsageMetrics {
        let mut ctrl = RefreshController::new(60);
        let providers: HashMap<String, Arc<dyn Provider + Send + Sync>> = HashMap::new();
        ctrl.states.insert("x".to_owned(), ProviderState::default());
        let send = |ctrl: &RefreshController, metrics: UsageMetrics| {
            ctrl.result_tx
                .send(WorkerResult {
                    provider_id: "x".to_owned(),
                    generation: 0,
                    metrics,
                })
                .unwrap();
        };
        send(
            &ctrl,
            UsageMetrics {
                provider_id: "x".to_owned(),
                metric1_val: Some(40.0),
                ..Default::default()
            },
        );
        assert_eq!(ctrl.drain_results(&providers).updates.len(), 1);
        send(
            &ctrl,
            UsageMetrics {
                provider_id: "x".to_owned(),
                error: Some("failed".to_owned()),
                error_code: code.to_owned(),
                ..Default::default()
            },
        );
        ctrl.drain_results(&providers).updates.remove(0)
    }

    #[test]
    fn an_auth_failure_is_offline_and_a_transient_one_keeps_the_value() {
        let auth = failure_after_success("auth");
        assert!(!auth.stale);
        assert_eq!(auth.metric1_val, None);
        for code in ["network", "rate_limit", "http"] {
            let kept = failure_after_success(code);
            assert!(kept.stale, "{code}");
            assert_eq!(kept.metric1_val, Some(40.0), "{code}");
        }
    }

    struct Quick;
    impl Provider for Quick {
        fn provider_id(&self) -> &str {
            "x"
        }
        fn display_name(&self) -> &str {
            "x"
        }
        fn fetch_usage(&self) -> UsageMetrics {
            UsageMetrics {
                provider_id: "x".to_owned(),
                ..Default::default()
            }
        }
    }

    #[test]
    fn a_late_result_of_a_removed_instance_is_dropped() {
        let mut ctrl = RefreshController::new(60);
        let providers: HashMap<String, Arc<dyn Provider + Send + Sync>> = HashMap::new();
        ctrl.add_instance("x", Arc::new(Quick));
        ctrl.remove_instance("x");
        std::thread::sleep(Duration::from_millis(300));
        assert!(ctrl.drain_results(&providers).updates.is_empty());
        // added again under the same id: the old worker's result is not taken for the new one
        ctrl.add_instance("x", Arc::new(Quick));
        ctrl.remove_instance("x");
        ctrl.add_instance("x", Arc::new(Quick));
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(ctrl.drain_results(&providers).updates.len(), 1);
    }

    struct MockPanicProvider;
    impl Provider for MockPanicProvider {
        fn provider_id(&self) -> &str {
            "mock_panic"
        }
        fn display_name(&self) -> &str {
            "Mock Panic"
        }
        fn fetch_usage(&self) -> UsageMetrics {
            panic!("Intentional mock provider panic!");
        }
    }

    #[test]
    fn test_worker_panic_catch() {
        let mut ctrl = RefreshController::new(60);
        let mut providers: HashMap<String, Arc<dyn Provider + Send + Sync>> = HashMap::new();
        let prov = Arc::new(MockPanicProvider);
        providers.insert("mock_panic".to_string(), prov.clone());

        ctrl.states
            .insert("mock_panic".to_string(), ProviderState::default());
        // The worker calls the notify callback after it has sent its result.
        let (finished, finished_rx) = std::sync::mpsc::channel();
        ctrl.set_notify_callback(move || {
            let _ = finished.send(());
        });
        ctrl.launch("mock_panic", prov);

        finished_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the worker did not finish");

        let results = ctrl.drain_results(&providers).updates;
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.provider_id, "mock_panic");
        assert_eq!(r.error_code, "unexpected");
        assert_eq!(r.error.as_deref(), Some("查詢失敗，請查看診斷紀錄"));

        let state = ctrl.states.get("mock_panic").unwrap();
        assert!(
            !state.running,
            "state.running must be false after panic recovery"
        );
        assert_eq!(state.failures, 1);
    }

    #[test]
    fn test_retry_after_clamp_and_safety() {
        let mut ctrl = RefreshController::new(60);
        let providers: HashMap<String, Arc<dyn Provider + Send + Sync>> = HashMap::new();
        ctrl.states
            .insert("test".to_string(), ProviderState::default());

        // 1. Extreme large retry_after clamped to 86400.0 (1 day)
        let item_huge = WorkerResult {
            provider_id: "test".to_string(),
            generation: 0,
            metrics: UsageMetrics {
                provider_id: "test".to_string(),
                error: Some("Rate limit".to_string()),
                error_code: "429".to_string(),
                retry_after: Some(1e20),
                ..Default::default()
            },
        };
        ctrl.complete(item_huge, &providers);
        let state = ctrl.states.get("test").unwrap();
        let due_diff = state.due.unwrap().duration_since(Instant::now());
        assert!(due_diff.as_secs() <= 86400);
        assert!(due_diff.as_secs() >= 86395);

        // 2. NaN retry_after falls back to default delay without panicking
        let item_nan = WorkerResult {
            provider_id: "test".to_string(),
            generation: 0,
            metrics: UsageMetrics {
                provider_id: "test".to_string(),
                error: Some("Rate limit".to_string()),
                error_code: "429".to_string(),
                retry_after: Some(f64::NAN),
                ..Default::default()
            },
        };
        ctrl.complete(item_nan, &providers);

        // 3. Negative retry_after falls back to default delay without panicking
        let item_neg = WorkerResult {
            provider_id: "test".to_string(),
            generation: 0,
            metrics: UsageMetrics {
                provider_id: "test".to_string(),
                error: Some("Rate limit".to_string()),
                error_code: "429".to_string(),
                retry_after: Some(-100.0),
                ..Default::default()
            },
        };
        ctrl.complete(item_neg, &providers);
    }

    #[test]
    fn test_set_interval_clamps_min() {
        let mut ctrl = RefreshController::new(60);
        ctrl.set_interval(5); // Minimum is 20
        assert_eq!(ctrl.interval, Duration::from_secs(20));
    }
    /// Returns once the test sends on `release`, so the test decides when the worker finishes.
    struct GatedProvider {
        release: parking_lot::Mutex<std::sync::mpsc::Receiver<()>>,
    }
    impl Provider for GatedProvider {
        fn provider_id(&self) -> &str {
            "mock_instant"
        }
        fn display_name(&self) -> &str {
            "Mock Instant"
        }
        fn fetch_usage(&self) -> UsageMetrics {
            let _ = self.release.lock().recv_timeout(Duration::from_secs(5));
            UsageMetrics {
                provider_id: "mock_instant".to_string(),
                metric1_text: "42%".to_string(),
                ..Default::default()
            }
        }
    }

    #[test]
    fn test_refresh_controller_pipeline_and_late_notify() {
        let mut ctrl = RefreshController::new(60);
        let mut providers: HashMap<String, Arc<dyn Provider + Send + Sync>> = HashMap::new();
        let (release, release_rx) = std::sync::mpsc::channel();
        let prov = Arc::new(GatedProvider {
            release: parking_lot::Mutex::new(release_rx),
        });
        providers.insert("mock_instant".to_string(), prov.clone());

        ctrl.states
            .insert("mock_instant".to_string(), ProviderState::default());

        // 1. Launch before setting the callback; the worker waits in `fetch_usage`.
        ctrl.launch("mock_instant", prov);

        // 2. Set the callback while the worker is still running, then let it finish.
        let (notified, notified_rx) = std::sync::mpsc::channel();
        ctrl.set_notify_callback(move || {
            let _ = notified.send(());
        });
        release.send(()).unwrap();

        // 3. The worker reads the callback when it finishes, so the late callback is called.
        notified_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the callback set while the worker ran was not called");

        // 4. Poll should drain and return the updates
        let updates = ctrl.poll(&providers).updates;
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].provider_id, "mock_instant");
        assert_eq!(updates[0].metric1_text, "42%");
        assert!(!ctrl.states.get("mock_instant").unwrap().running);
    }
}
