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
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
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

pub struct RefreshController {
    pub interval: Duration,
    pub states: HashMap<String, ProviderState>,
    result_tx: Sender<WorkerResult>,
    pub result_rx: Receiver<WorkerResult>,
    egui_ctx: Option<egui::Context>,
}

impl RefreshController {
    pub fn new(interval_secs: u64) -> Self {
        let (tx, rx) = mpsc::channel::<WorkerResult>();
        Self {
            interval: Duration::from_secs(interval_secs.max(20)),
            states: HashMap::new(),
            result_tx: tx,
            result_rx: rx,
            egui_ctx: None,
        }
    }

    /// Store egui Context to immediately request repaint when workers complete.
    pub fn set_egui_ctx(&mut self, ctx: egui::Context) {
        self.egui_ctx = Some(ctx);
    }

    /// Returns true if any provider worker is currently running.
    pub fn is_busy(&self) -> bool {
        self.states.values().any(|s| s.running)
    }

    /// Call regularly (e.g. every 1 second) from the UI thread to trigger
    /// scheduled refreshes and check for due providers.
    pub fn poll(&mut self, providers: &HashMap<String, Arc<dyn Provider + Send + Sync>>) {
        let now = Instant::now();
        let ids: Vec<String> = self.states.keys().cloned().collect();
        for id in ids {
            let state = self.states.get(&id).unwrap();
            let should_launch = !state.running && state.due.is_none_or(|due| now >= due);
            if should_launch {
                if let Some(provider) = providers.get(&id) {
                    self.launch(&id, Arc::clone(provider));
                }
            }
        }
    }

    /// Manual refresh: invalidate in-flight or schedule immediately.
    pub fn refresh(&mut self, providers: &HashMap<String, Arc<dyn Provider + Send + Sync>>) {
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

    /// Set a new interval and reset next-due timestamps.
    pub fn set_interval(&mut self, secs: u64) {
        self.interval = Duration::from_secs(secs.max(20));
        let now = Instant::now();
        for state in self.states.values_mut() {
            if state.failures == 0 {
                state.due = Some(now + self.interval);
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
        let ctx_opt = self.egui_ctx.clone();
        thread::Builder::new()
            .name(format!("quota-{}", id))
            .spawn(move || {
                let started = Instant::now();
                let metrics_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    provider.fetch_usage()
                }));
                let metrics = match metrics_res {
                    Ok(m) => m,
                    Err(panic_info) => {
                        let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_info.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "執行緒內部異常 (Worker thread panicked)".to_string()
                        };
                        log::error!(
                            "[RefreshController] Provider {} panicked: {}",
                            id_owned,
                            msg
                        );
                        UsageMetrics {
                            provider_id: id_owned.clone(),
                            error: Some(format!("執行異常: {}", msg)),
                            error_code: "PANIC".to_string(),
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
                if let Some(ctx) = ctx_opt {
                    ctx.request_repaint();
                }
            })
            .expect("failed to spawn quota worker");
    }

    /// Drain all pending results from workers.
    /// Returns Vec of updated UsageMetrics to emit to the UI.
    pub fn drain_results(
        &mut self,
        providers: &HashMap<String, Arc<dyn Provider + Send + Sync>>,
    ) -> Vec<UsageMetrics> {
        let mut updates = Vec::new();
        while let Ok(item) = self.result_rx.try_recv() {
            if let Some(result) = self.complete(item, providers) {
                updates.push(result);
            }
        }
        updates
    }

    fn complete(
        &mut self,
        item: WorkerResult,
        providers: &HashMap<String, Arc<dyn Provider + Send + Sync>>,
    ) -> Option<UsageMetrics> {
        let state = self.states.get_mut(&item.provider_id)?;
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
            let delay_secs = u64::min(
                900,
                (self.interval.as_secs() as f64 * 2f64.powi((state.failures as i32 - 1).min(4)))
                    as u64,
            );
            let delay_f64 = if let Some(ra) = result.retry_after {
                if ra.is_finite() && ra >= 0.0 {
                    (delay_secs as f64).max(ra).clamp(1.0, 86400.0)
                } else {
                    delay_secs as f64
                }
            } else {
                delay_secs as f64
            };
            let delay = Duration::from_secs_f64(delay_f64);
            warn!(
                "provider={} error={} retry={:.1}s",
                item.provider_id,
                result.error_code,
                delay.as_secs_f64()
            );
            state.due = Some(now + delay);

            // Preserve last successful data as stale
            if let Some(cached) = &state.cached {
                let mut stale = cached.clone();
                stale.error = Some(err.clone());
                stale.error_code = result.error_code.clone();
                stale.retry_after = result.retry_after;
                stale.stale = true;
                result = stale;
            }
        } else {
            state.failures = 0;
            state.due = Some(now + self.interval);
            result.last_success = Some(Utc::now());
            result.stale = false;
            state.cached = Some(result.clone());
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        ctrl.launch("mock_panic", prov);

        // Wait for worker thread to finish
        std::thread::sleep(Duration::from_millis(200));

        let results = ctrl.drain_results(&providers);
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.provider_id, "mock_panic");
        assert_eq!(r.error_code, "PANIC");
        assert!(
            r.error.as_ref().unwrap().contains("Worker thread panicked")
                || r.error.as_ref().unwrap().contains("Intentional mock")
        );

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
}
