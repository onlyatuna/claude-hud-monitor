//! Restart after repeated panics.
//!
//! The HUD builds with `panic = "unwind"` and contains panics at its boundaries (slots, timers,
//! WndProc, provider workers) so one bad event does not kill the window. Over days a recurring
//! panic can leave the UI half-updated for good, so a burst of panics ends in a clean restart.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Panics tolerated inside [`PANIC_WINDOW`] before the process restarts itself.
pub const PANIC_LIMIT: usize = 5;
/// Sliding window the limit applies to.
pub const PANIC_WINDOW: Duration = Duration::from_secs(60 * 60);
/// Command-line flag the restarted process carries; it retries the single-instance lock while
/// the old process finishes exiting.
pub const RESTART_FLAG: &str = "--restarted-after-panic";

/// Sliding-window panic counter.
pub struct PanicWindow {
    times: VecDeque<Instant>,
    limit: usize,
    window: Duration,
}

impl PanicWindow {
    pub const fn new(limit: usize, window: Duration) -> Self {
        Self {
            times: VecDeque::new(),
            limit,
            window,
        }
    }

    /// Records a panic at `now`; `true` when the limit is reached inside the window.
    pub fn record(&mut self, now: Instant) -> bool {
        while let Some(&oldest) = self.times.front() {
            if now.saturating_duration_since(oldest) > self.window {
                self.times.pop_front();
            } else {
                break;
            }
        }
        self.times.push_back(now);
        self.times.len() >= self.limit
    }
}

static WINDOW: std::sync::Mutex<PanicWindow> =
    std::sync::Mutex::new(PanicWindow::new(PANIC_LIMIT, PANIC_WINDOW));

/// Counts one panic; `true` when the process should restart. Never panics (called from the
/// panic hook): a poisoned or contended lock counts as "do not restart".
pub fn note_panic() -> bool {
    match WINDOW.try_lock() {
        Ok(mut w) => w.record(Instant::now()),
        Err(_) => false,
    }
}

/// Starts a fresh copy of this executable with the same arguments and exits. Returns only if
/// the new process could not be started, in which case the current one keeps running.
pub fn restart_process() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(std::env::args().skip(1).filter(|a| a != RESTART_FLAG))
        .arg(RESTART_FLAG);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | DETACHED_PROCESS-free: the HUD is a GUI app, so only hide a console.
        cmd.creation_flags(0x0800_0000);
    }
    if cmd.spawn().is_ok() {
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_trips_only_inside_the_window() {
        let mut w = PanicWindow::new(3, Duration::from_secs(100));
        let t0 = Instant::now();
        assert!(!w.record(t0));
        assert!(!w.record(t0 + Duration::from_secs(10)));
        assert!(w.record(t0 + Duration::from_secs(20)));
    }

    #[test]
    fn old_panics_age_out_of_the_window() {
        let mut w = PanicWindow::new(3, Duration::from_secs(100));
        let t0 = Instant::now();
        assert!(!w.record(t0));
        assert!(!w.record(t0 + Duration::from_secs(50)));
        // The first one is 120 s old by now, so only two remain counted plus this one.
        assert!(!w.record(t0 + Duration::from_secs(120)));
        assert!(w.record(t0 + Duration::from_secs(130)));
    }
}
