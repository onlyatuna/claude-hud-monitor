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
use std::sync::mpsc::{self, Sender, Receiver};
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
}

impl RefreshController {
    pub fn new(interval_secs: u64) -> Self {
        let (tx, rx) = mpsc::channel::<WorkerResult>();
        Self {
            interval: Duration::from_secs(interval_secs.max(20)),
            states: HashMap::new(),
            result_tx: tx,
            result_rx: rx,
        }
    }

    /// Returns true if any provider worker is currently running.
    pub fn is_busy(&self) -> bool {
        self.states.values().any(|s| s.running)
    }
    /// Register a provider and launch its first fetch.
    #[allow(dead_code)]
    pub fn register_and_launch(
        &mut self,
        provider: Box<dyn Provider + Send>,
        arc_provider: Arc<dyn Provider + Send + Sync>,
    ) {
        let id = provider.provider_id().to_owned();
        self.states.insert(id.clone(), ProviderState::default());
        self.launch(&id, arc_provider);
    }

    /// Call regularly (e.g. every 1 second) from the UI thread to trigger
    /// scheduled refreshes and check for due providers.
    pub fn poll(&mut self, providers: &HashMap<String, Arc<dyn Provider + Send + Sync>>) {
        let now = Instant::now();
        let ids: Vec<String> = self.states.keys().cloned().collect();
        for id in ids {
            let state = self.states.get(&id).unwrap();
            let should_launch = !state.running
                && state.due.map_or(true, |due| now >= due);
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
        thread::Builder::new()
            .name(format!("quota-{}", id))
            .spawn(move || {
                let metrics = provider.fetch_usage();
                let _ = tx.send(WorkerResult {
                    provider_id: id_owned,
                    generation,
                    metrics,
                });
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
        loop {
            match self.result_rx.try_recv() {
                Ok(item) => {
                    if let Some(result) = self.complete(item, providers) {
                        updates.push(result);
                    }
                }
                Err(_) => break,
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
            let delay_secs = u64::min(900, (self.interval.as_secs() as f64
                * 2f64.powi((state.failures as i32 - 1).min(4))) as u64);
            let delay = Duration::from_secs_f64(
                if let Some(ra) = result.retry_after {
                    (delay_secs as f64).max(ra)
                } else {
                    delay_secs as f64
                }
            );
            warn!(
                "provider={} error={} retry={:.1}s",
                item.provider_id, result.error_code, delay.as_secs_f64()
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
