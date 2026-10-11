//! `QTimer::start()` on an already-running timer restarts it (Qt: `QTimer::start` stops the
//! running timer, then starts a new one). A restarted timer must still deliver `timeout`.
//!
//! Regression for RC-11a: restarting a `Timer` used to re-register the underlying object, which
//! marked the object's own liveness token dead, so the dispatcher refused to deliver the
//! `timer_event` and the restarted timer never fired.
#![cfg(windows)]

use qtrs_core::event_loop::EventLoop;
use qtrs_core::object::{register_qobject, unregister_qobject, ObjectData, ObjectId, QObject};
use qtrs_core::timer::{Timer, TimerType};
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

/// QObject whose `timer_event` counts deliveries (object-timer path, `QObject::startTimer`).
struct TickObject {
    data: ObjectData,
    ticks: Arc<AtomicU32>,
}

impl TickObject {
    fn new(ticks: Arc<AtomicU32>) -> Self {
        Self {
            data: ObjectData::new(ObjectId::next()),
            ticks,
        }
    }
}

impl QObject for TickObject {
    fn object_data(&self) -> &ObjectData {
        &self.data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.data
    }
    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn timer_event(&mut self, _timer_id: u64) {
        self.ticks.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn start_timer_fires_timer_event() {
    let mut el = EventLoop::new();
    let ticks = Arc::new(AtomicU32::new(0));
    let mut obj = Box::new(TickObject::new(Arc::clone(&ticks)));
    // SAFETY: `obj` is boxed and not moved until the timer is killed and the object unregistered.
    unsafe { register_qobject(&mut *obj) };

    let id = obj.start_timer(20, TimerType::Precise);
    assert!(id.is_valid());
    assert!(
        pump_until(&mut el, Duration::from_secs(2), || ticks
            .load(Ordering::SeqCst)
            >= 3),
        "QObject::start_timer must arm delivery; got {} ticks",
        ticks.load(Ordering::SeqCst)
    );
    obj.kill_timer(id);
    unsafe { unregister_qobject(obj.object_data().id) };
}

#[test]
fn kill_timer_stops_firing() {
    let mut el = EventLoop::new();
    let ticks = Arc::new(AtomicU32::new(0));
    let mut obj = Box::new(TickObject::new(Arc::clone(&ticks)));
    // SAFETY: `obj` is boxed and not moved until the object is unregistered below.
    unsafe { register_qobject(&mut *obj) };

    let id = obj.start_timer(20, TimerType::Precise);
    assert!(pump_until(&mut el, Duration::from_secs(2), || ticks
        .load(Ordering::SeqCst)
        >= 1));
    assert!(obj.kill_timer(id));
    let snapshot = ticks.load(Ordering::SeqCst);
    let settle = Instant::now() + Duration::from_millis(200);
    while Instant::now() < settle {
        el.process_events(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        ticks.load(Ordering::SeqCst),
        snapshot,
        "killed object timer must stop"
    );
    unsafe { unregister_qobject(obj.object_data().id) };
}

#[test]
fn zero_interval_object_timer_fires_repeatedly() {
    let mut el = EventLoop::new();
    let ticks = Arc::new(AtomicU32::new(0));
    let mut obj = Box::new(TickObject::new(Arc::clone(&ticks)));
    // SAFETY: `obj` is boxed and not moved until the timer is killed and the object unregistered.
    unsafe { register_qobject(&mut *obj) };

    let id = obj.start_timer(0, TimerType::Coarse);
    assert!(
        pump_until(&mut el, Duration::from_secs(2), || ticks
            .load(Ordering::SeqCst)
            >= 5),
        "zero-interval object timer must repost ZeroTimer; got {} ticks",
        ticks.load(Ordering::SeqCst)
    );
    obj.kill_timer(id);
    unsafe { unregister_qobject(obj.object_data().id) };
}
