//! `QTimer::start()` on an already-running timer restarts it (Qt: `QTimer::start` stops the
//! running timer, then starts a new one). A restarted timer must still deliver `timeout`.
//!
//! Regression for RC-11a: restarting a `Timer` used to re-register the underlying object, which
//! marked the object's own liveness token dead, so the dispatcher refused to deliver the
//! `timer_event` and the restarted timer never fired.
#![cfg(windows)]

use qtrs_core::event_loop::EventLoop;
use qtrs_core::object::QObject;
use qtrs_core::timer::Timer;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn counting_timer(interval_ms: u64, single_shot: bool) -> (Timer, Arc<AtomicU32>) {
    let count = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&count);
    let mut timer = Timer::new();
    timer.set_interval(interval_ms);
    timer.set_single_shot(single_shot);
    timer.timeout.connect(move |_| {
        c.fetch_add(1, Ordering::SeqCst);
    });
    (timer, count)
}

/// Pumps the loop until `cond` or `limit` elapses; returns whether `cond` became true.
fn pump_until(el: &mut EventLoop, limit: Duration, cond: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        el.process_events(false);
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    cond()
}

#[test]
fn restarted_single_shot_timer_still_fires() {
    let mut el = EventLoop::new();
    let (mut timer, count) = counting_timer(40, true);

    unsafe { timer.start() };
    unsafe { timer.start() };

    assert!(
        pump_until(&mut el, Duration::from_millis(500), || count
            .load(Ordering::SeqCst)
            >= 1),
        "a timer that was start()ed twice never fired"
    );
    assert_eq!(count.load(Ordering::SeqCst), 1, "single-shot fires once");
    assert!(!timer.is_active());
}

#[test]
fn stop_then_start_fires() {
    let mut el = EventLoop::new();
    let (mut timer, count) = counting_timer(40, true);

    unsafe { timer.start() };
    timer.stop();
    unsafe { timer.start() };

    assert!(
        pump_until(&mut el, Duration::from_millis(500), || count
            .load(Ordering::SeqCst)
            >= 1),
        "stop() followed by start() never fired"
    );
}

#[test]
fn restart_postpones_the_deadline() {
    // This is how the tooltip wake-up timer is used: every mouse move restarts it, and the
    // timeout must be measured from the *last* start.
    let mut el = EventLoop::new();
    let (mut timer, count) = counting_timer(120, true);

    unsafe { timer.start() };
    let first = Instant::now();
    assert!(
        !pump_until(&mut el, Duration::from_millis(70), || count
            .load(Ordering::SeqCst)
            >= 1)
    );
    unsafe { timer.start() };
    let restarted = Instant::now();

    // Past the first deadline (120 ms after `first`) but before the restarted one.
    let before_restarted_deadline = pump_until(
        &mut el,
        (first + Duration::from_millis(150)).saturating_duration_since(Instant::now()),
        || count.load(Ordering::SeqCst) >= 1,
    );
    assert!(
        !before_restarted_deadline,
        "restart must discard the original deadline"
    );

    assert!(pump_until(&mut el, Duration::from_millis(500), || count
        .load(Ordering::SeqCst)
        >= 1));
    assert!(
        restarted.elapsed() >= Duration::from_millis(100),
        "fired {:?} after restart, expected about 120 ms",
        restarted.elapsed()
    );
}

#[test]
fn restarted_repeating_timer_keeps_firing() {
    let mut el = EventLoop::new();
    let (mut timer, count) = counting_timer(25, false);

    unsafe { timer.start() };
    unsafe { timer.start() };

    assert!(
        pump_until(&mut el, Duration::from_millis(600), || count
            .load(Ordering::SeqCst)
            >= 3),
        "restarted repeating timer fired {} times",
        count.load(Ordering::SeqCst)
    );
    timer.stop();
}

#[test]
fn restarting_a_timer_keeps_the_object_alive() {
    let _el = EventLoop::new();
    let (mut timer, _count) = counting_timer(1000, true);

    unsafe { timer.start() };
    assert!(timer.object_data().is_alive());
    unsafe { timer.start() };
    assert!(
        timer.object_data().is_alive(),
        "re-registering an object must not kill its own liveness token"
    );
    timer.stop();
}
