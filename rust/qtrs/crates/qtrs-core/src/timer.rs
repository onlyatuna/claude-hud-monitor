//! Timer module modeled after Qt timer mechanism (QTimer, QTimerInfo).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

#[cfg(windows)]
use windows_sys::Win32::Foundation::HWND;
#[cfg(not(windows))]
pub type HWND = *mut std::ffi::c_void;

use crate::object::{ObjectData, ObjectId, QObject};
use crate::signal::Signal;

static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Returns monotonic timestamp in milliseconds.
#[inline]
pub fn current_time_ms() -> u64 {
    START_TIME.elapsed().as_millis() as u64
}

/// Unique timer identifier: `TimerId`. Modeled after `Qt::TimerId`.
/// Unique timer identifier: `TimerId`. Modeled after `Qt::TimerId`.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TimerId(pub u32);

impl TimerId {
    pub const INVALID: Self = TimerId(0);

    #[inline]
    pub fn is_valid(self) -> bool {
        self.0 != 0
    }
}

impl Default for TimerId {
    fn default() -> Self {
        Self::INVALID
    }
}

impl std::fmt::Display for TimerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TimerId({})", self.0)
    }
}

/// Timer precision type: `TimerType`. Modeled after `Qt::TimerType`.
/// Timer precision type: `TimerType`. Modeled after `Qt::TimerType`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum TimerType {
    /// Precise timer (1ms resolution).
    Precise,
    /// Coarse timer (5% tolerance default).
    #[default]
    Coarse,
    /// Very coarse timer (1-second resolution).
    VeryCoarse,
}

/// Registered timer entry matching Qt `WinTimerInfo`.

#[derive(Clone, Debug)]
pub struct TimerEntry {
    pub id: TimerId,
    pub interval_ms: u64,
    pub next_fire_ms: u64,
    pub timer_type: TimerType,
    pub receiver: ObjectId,
    pub win32_timer_id: u32,
    pub in_timer_event: bool,
    pub single_shot: bool,
}

impl TimerEntry {
    pub fn new(
        id: TimerId,
        interval_ms: u64,
        next_fire_ms: u64,
        timer_type: TimerType,
        receiver: ObjectId,
    ) -> Self {
        Self {
            id,
            interval_ms,
            next_fire_ms,
            timer_type,
            receiver,
            win32_timer_id: 0,
            in_timer_event: false,
            single_shot: false,
        }
    }
}

/// Calculates next deadline matching calculateNextTimeout.
pub fn calculate_next_timeout(
    timer_type: &mut TimerType,
    interval_ms: u64,
    current_time_ms: u64,
) -> (u64, u64) {
    let mut interval = interval_ms;
    let mut current_time = current_time_ms;
    if interval == 0 {
        return (0, current_time);
    }
    match *timer_type {
        TimerType::Precise => {}
        TimerType::Coarse => {
            if interval >= 20000 {
                *timer_type = TimerType::VeryCoarse;
                if interval < 1000 {
                    interval = 1000;
                } else {
                    interval = (interval + 500) / 1000 * 1000;
                }
                current_time = current_time / 1000 * 1000;
            } else if interval <= 20 {
                *timer_type = TimerType::Precise;
            }
        }
        TimerType::VeryCoarse => {
            if interval < 1000 {
                interval = 1000;
            } else {
                interval = (interval + 500) / 1000 * 1000;
            }
            current_time = current_time / 1000 * 1000;
        }
    }
    let timeout = current_time + interval;
    (interval, timeout)
}

/// Timer registry matching Qt `WinTimerDict`.

#[derive(Debug, Default)]
pub struct TimerRegistry {
    entries: HashMap<TimerId, TimerEntry>,
    next_id: u32,
}

impl TimerRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            next_id: 1,
        }
    }

    /// Returns the number of registered timers.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn register(
        &mut self,
        receiver: ObjectId,
        interval_ms: u64,
        timer_type: TimerType,
        single_shot: bool,
    ) -> TimerId {
        let id = TimerId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }

        let now = current_time_ms();
        let mut actual_type = timer_type;
        let (adjusted_interval, next_fire_ms) =
            calculate_next_timeout(&mut actual_type, interval_ms, now);

        let entry = TimerEntry {
            id,
            interval_ms: adjusted_interval,
            next_fire_ms,
            timer_type: actual_type,
            receiver,
            win32_timer_id: 0,
            in_timer_event: false,
            single_shot,
        };

        self.entries.insert(id, entry);
        id
    }

    pub fn unregister(&mut self, id: TimerId) -> Option<TimerEntry> {
        self.entries.remove(&id)
    }

    pub fn get(&self, id: TimerId) -> Option<&TimerEntry> {
        self.entries.get(&id)
    }

    pub fn get_mut(&mut self, id: TimerId) -> Option<&mut TimerEntry> {
        self.entries.get_mut(&id)
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.entries.values().map(|e| e.next_fire_ms).min()
    }
}

#[derive(Clone)]
pub struct ThreadTimerContext {
    pub registry: Arc<Mutex<TimerRegistry>>,
    #[cfg(windows)]
    pub internal_hwnd: HWND,
}

thread_local! {
    static THREAD_TIMER_CONTEXT: RefCell<Option<ThreadTimerContext>> = const { RefCell::new(None) };
    static SINGLE_SHOT_CALLBACKS: RefCell<HashMap<ObjectId, Box<dyn FnOnce() + Send>>> =
        RefCell::new(HashMap::new());
}

pub fn register_thread_timer_context(
    registry: Arc<Mutex<TimerRegistry>>,
    #[cfg(windows)] internal_hwnd: HWND,
) {
    THREAD_TIMER_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(ThreadTimerContext {
            registry,
            #[cfg(windows)]
            internal_hwnd,
        });
    });
}

pub fn unregister_thread_timer_context() {
    let _ = THREAD_TIMER_CONTEXT.try_with(|ctx| {
        if let Ok(mut borrow) = ctx.try_borrow_mut() {
            *borrow = None;
        }
    });
}

pub fn has_thread_timer_context() -> bool {
    THREAD_TIMER_CONTEXT
        .try_with(|ctx| {
            if let Ok(borrow) = ctx.try_borrow() {
                borrow.is_some()
            } else {
                false
            }
        })
        .unwrap_or(false)
}

pub fn with_thread_timer_context<R>(f: impl FnOnce(&ThreadTimerContext) -> R) -> Option<R> {
    THREAD_TIMER_CONTEXT
        .try_with(|ctx| ctx.borrow().as_ref().map(f))
        .ok()
        .flatten()
}

/// Stops and unregisters all active timers registered to the specified receiver object.
/// Modeled after Qt's QObjectPrivate::moveToThread_helper timer cleanup.
pub fn stop_timers_for_object(receiver: ObjectId) -> usize {
    with_thread_timer_context(|ctx| {
        let mut reg = ctx.registry.lock().unwrap();
        let to_remove: Vec<TimerId> = reg
            .entries
            .iter()
            .filter(|(_, entry)| entry.receiver == receiver)
            .map(|(&id, _)| id)
            .collect();
        let count = to_remove.len();
        for id in to_remove {
            reg.unregister(id);
        }
        count
    })
    .unwrap_or(0)
}

/// Starts an object timer registered to the current thread's timer registry.
pub fn start_object_timer(receiver: ObjectId, interval_ms: u64, timer_type: TimerType) -> TimerId {
    with_thread_timer_context(|ctx| {
        let mut reg = ctx.registry.lock().unwrap();
        reg.register(receiver, interval_ms, timer_type, false)
    })
    .unwrap_or(TimerId::INVALID)
}

pub(crate) fn register_single_shot_callback(
    receiver: ObjectId,
    callback: impl FnOnce() + Send + 'static,
) {
    SINGLE_SHOT_CALLBACKS.with(|callbacks| {
        callbacks.borrow_mut().insert(receiver, Box::new(callback));
    });
}

pub(crate) fn dispatch_single_shot_callback(receiver: ObjectId) -> bool {
    let callback = SINGLE_SHOT_CALLBACKS.with(|callbacks| callbacks.borrow_mut().remove(&receiver));
    if let Some(callback) = callback {
        callback();
        true
    } else {
        false
    }
}

/// Kills an object timer by TimerId in the current thread's timer registry.
pub fn kill_object_timer(id: TimerId) -> bool {
    with_thread_timer_context(|ctx| {
        let mut reg = ctx.registry.lock().unwrap();
        reg.unregister(id).is_some()
    })
    .unwrap_or(false)
}

/// Timer event structure matching `QTimerEvent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerEvent {
    pub timer_id: TimerId,
}

impl TimerEvent {
    pub fn new(timer_id: TimerId) -> Self {
        Self { timer_id }
    }
}

/// High-level timer object matching `QTimer`.
pub struct Timer {
    data: ObjectData,
    id: TimerId,
    interval_ms: u64,
    single_shot: bool,
    timer_type: TimerType,
    pub timeout: Signal<()>,
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl Timer {
    pub fn new() -> Self {
        let id = ObjectId::next();
        Self {
            data: ObjectData::new(id),
            id: TimerId::INVALID,
            interval_ms: 0,
            single_shot: false,
            timer_type: TimerType::Coarse,
            timeout: Signal::with_emitter(id),
        }
    }

    /// Starts the timer with its configured interval (`QTimer::start`).
    ///
    /// # Safety
    /// Keep this timer alive at its current address until it is stopped and unregistered.
    /// Do not access it through an alias while event callbacks may run; callbacks are dispatched
    /// on the physical registration thread.
    pub unsafe fn start(&mut self) -> TimerId {
        if self.is_active() {
            self.stop();
        }

        // SAFETY: Timer::start requires the caller to keep this timer pinned in place until
        // stop/unregister; see the method's Safety contract.
        unsafe { crate::object::register_qobject(self) };

        let receiver = self.data.id;
        let interval_ms = self.interval_ms;
        let timer_type = self.timer_type;
        let single_shot = self.single_shot;

        let id = THREAD_TIMER_CONTEXT.with(|ctx| {
            let borrow = ctx.borrow();
            let Some(ctx) = borrow.as_ref() else {
                return TimerId::INVALID;
            };

            let mut reg = ctx.registry.lock().unwrap();
            let timer_id = reg.register(receiver, interval_ms, timer_type, single_shot);

            if interval_ms == 0 {
                crate::event_loop::post_event_to_thread(
                    crate::object::ThreadId::current(),
                    receiver,
                    crate::event::Event::new(crate::event::EventKind::ZeroTimer {
                        timer_id: timer_id.0 as u64,
                    }),
                );
            } else {
                #[cfg(windows)]
                {
                    if !ctx.internal_hwnd.is_null() {
                        let interval = interval_ms.min(u32::MAX as u64) as u32;
                        unsafe {
                            windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(
                                ctx.internal_hwnd,
                                timer_id.0 as usize,
                                interval,
                                None,
                            );
                        }
                    }
                }
            }

            timer_id
        });

        self.id = id;
        id
    }

    /// Starts the timer with the specified interval (`QTimer::start(msec)`).
    ///
    /// # Safety
    /// Follows the safety requirements of [`Timer::start`].
    pub unsafe fn start_with_interval(&mut self, ms: u64) -> TimerId {
        self.interval_ms = ms;
        unsafe { self.start() }
    }

    /// Stops the timer (`QTimer::stop`).
    pub fn stop(&mut self) {
        if self.is_active() {
            let id = self.id;
            let interval_ms = self.interval_ms;
            let _ = THREAD_TIMER_CONTEXT.try_with(|ctx| {
                if let Ok(borrow) = ctx.try_borrow() {
                    if let Some(ctx) = borrow.as_ref() {
                        let mut reg = ctx.registry.lock().unwrap();
                        reg.unregister(id);
                        if interval_ms > 0 {
                            #[cfg(windows)]
                            {
                                if !ctx.internal_hwnd.is_null() {
                                    unsafe {
                                        windows_sys::Win32::UI::WindowsAndMessaging::KillTimer(
                                            ctx.internal_hwnd,
                                            id.0 as usize,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            });
            self.id = TimerId::INVALID;
        }
    }

    pub fn is_active(&self) -> bool {
        self.id.is_valid()
    }

    pub fn set_interval(&mut self, ms: u64) {
        self.interval_ms = ms;
    }

    pub fn interval(&self) -> u64 {
        self.interval_ms
    }

    pub fn set_single_shot(&mut self, v: bool) {
        self.single_shot = v;
    }

    pub fn is_single_shot(&self) -> bool {
        self.single_shot
    }

    pub fn set_timer_type(&mut self, t: TimerType) {
        self.timer_type = t;
    }

    pub fn timer_type(&self) -> TimerType {
        self.timer_type
    }

    pub fn timer_id(&self) -> TimerId {
        self.id
    }

    /// Returns remaining time in milliseconds (`QTimer::remainingTime`).
    pub fn remaining_time(&self) -> i64 {
        if !self.is_active() {
            return -1;
        }
        let now = current_time_ms();
        let deadline_opt = with_thread_timer_context(|ctx| {
            ctx.registry
                .lock()
                .unwrap()
                .get(self.id)
                .map(|e| e.next_fire_ms)
        })
        .flatten();

        let Some(deadline) = deadline_opt else {
            return -1;
        };

        if deadline > now {
            (deadline - now) as i64
        } else {
            0
        }
    }

    /// Starts a timer in a caller-supplied registry.
    ///
    /// # Safety
    /// Keep this timer alive at its current address until it is stopped and unregistered.
    /// Do not access it through an alias while timer event callbacks may run.
    pub unsafe fn start_with_registry(&mut self, registry: &mut TimerRegistry) -> TimerId {
        if self.is_active() {
            self.stop_with_registry(registry);
        }
        unsafe { crate::object::register_qobject(self) };
        self.id = registry.register(
            self.data.id,
            self.interval_ms,
            self.timer_type,
            self.single_shot,
        );
        self.id
    }

    /// Starts a timer in a caller-supplied registry and dispatcher.
    ///
    /// # Safety
    /// Keep this timer alive at its current address until it is stopped and unregistered.
    /// Do not access it through an alias while timer event callbacks may run.
    pub unsafe fn start_with<D: crate::event_loop::EventDispatcher + ?Sized>(
        &mut self,
        registry: &mut TimerRegistry,
        dispatcher: &mut D,
    ) -> TimerId {
        if self.is_active() {
            self.stop_with(registry, dispatcher);
        }
        unsafe { crate::object::register_qobject(self) };
        self.id = registry.register(
            self.data.id,
            self.interval_ms,
            self.timer_type,
            self.single_shot,
        );
        if let Some(entry) = registry.get(self.id) {
            dispatcher.register_timer(entry);
        }
        self.id
    }

    /// Timer registry matching Qt `WinTimerDict`.
    pub fn stop_with<D: crate::event_loop::EventDispatcher + ?Sized>(
        &mut self,
        registry: &mut TimerRegistry,
        dispatcher: &mut D,
    ) {
        if self.is_active() {
            if let Some(entry) = registry.unregister(self.id) {
                dispatcher.unregister_timer(&entry);
            }
            self.id = TimerId::INVALID;
        }
    }

    /// Timer registry matching Qt `WinTimerDict`.
    pub fn stop_with_registry(&mut self, registry: &mut TimerRegistry) {
        if self.is_active() {
            registry.unregister(self.id);
            self.id = TimerId::INVALID;
        }
    }

    pub fn timer_event_with_registry(&mut self, event: &TimerEvent, registry: &mut TimerRegistry) {
        if event.timer_id == self.id {
            if self.single_shot {
                self.stop_with_registry(registry);
            }
            self.timeout.emit(&());
        }
    }

    /// Fires a single-shot timer callback (`QTimer::singleShot`).
    pub fn single_shot(interval_ms: u64, callback: impl FnOnce() + Send + 'static) {
        let thread_id = crate::object::ThreadId::current();
        if interval_ms == 0 {
            crate::event_loop::post_event_to_thread(
                thread_id,
                crate::object::ObjectId(0),
                crate::event::Event::new(crate::event::EventKind::MetaCall(Box::new(move |_| {
                    callback();
                }))),
            );
            return;
        }

        let receiver = ObjectId::next();
        register_single_shot_callback(receiver, callback);
        with_thread_timer_context(|ctx| {
            let timer_id = ctx.registry.lock().unwrap().register(
                receiver,
                interval_ms,
                TimerType::Coarse,
                true,
            );
            #[cfg(windows)]
            if !ctx.internal_hwnd.is_null() {
                unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(
                        ctx.internal_hwnd,
                        timer_id.0 as usize,
                        interval_ms.min(u32::MAX as u64) as u32,
                        None,
                    );
                }
            }
        });
    }
}

impl QObject for Timer {
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
    /// Handles timer events for this object (`QObject::timerEvent`).
    fn timer_event(&mut self, timer_id: u64) {
        if timer_id == self.id.0 as u64 {
            if self.single_shot {
                self.id = TimerId::INVALID;
            }
            self.timeout.emit(&());
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.stop();
        // SAFETY: Drop occurs after the timer's event callbacks have completed.
        unsafe { crate::object::unregister_qobject(self.data.id) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_id() {
        assert_eq!(TimerId::INVALID, TimerId(0));
        assert!(!TimerId::INVALID.is_valid());
        assert_eq!(TimerId::default(), TimerId::INVALID);

        let valid_id = TimerId(1);
        assert!(valid_id.is_valid());
        assert_ne!(valid_id, TimerId::INVALID);
        assert_eq!(valid_id.0, 1);
    }

    #[test]
    fn test_timer_type_defaults() {
        assert_eq!(TimerType::default(), TimerType::Coarse);
        assert_ne!(TimerType::Precise, TimerType::Coarse);
        assert_ne!(TimerType::VeryCoarse, TimerType::Coarse);
    }

    #[test]
    fn test_timer_entry_creation() {
        let entry = TimerEntry::new(TimerId(10), 100, 1100, TimerType::Precise, ObjectId(99));
        assert_eq!(entry.id, TimerId(10));
        assert_eq!(entry.interval_ms, 100);
        assert_eq!(entry.next_fire_ms, 1100);
        assert_eq!(entry.timer_type, TimerType::Precise);
        assert_eq!(entry.receiver, ObjectId(99));
        assert_eq!(entry.win32_timer_id, 0);
        assert!(!entry.in_timer_event);
        assert!(!entry.single_shot);
    }

    #[test]
    fn test_calculate_next_timeout() {
        // 1. Precise
        let mut t1 = TimerType::Precise;
        let (inv1, to1) = calculate_next_timeout(&mut t1, 15, 100);
        assert_eq!(t1, TimerType::Precise);
        assert_eq!(inv1, 15);
        assert_eq!(to1, 115);

        let mut t2 = TimerType::Coarse;
        let (inv2, to2) = calculate_next_timeout(&mut t2, 10, 100);
        assert_eq!(t2, TimerType::Precise);
        assert_eq!(inv2, 10);
        assert_eq!(to2, 110);

        let mut t3 = TimerType::Coarse;
        let (inv3, to3) = calculate_next_timeout(&mut t3, 25000, 5500);
        assert_eq!(t3, TimerType::VeryCoarse);
        assert_eq!(inv3, 25000);
        assert_eq!(to3, 5000 + 25000);

        let mut t4 = TimerType::VeryCoarse;
        let (inv4, to4) = calculate_next_timeout(&mut t4, 1400, 1200);
        assert_eq!(t4, TimerType::VeryCoarse);
        assert_eq!(inv4, 1000);
        assert_eq!(to4, 1000 + 1000);
    }

    #[test]
    fn test_timer_registry_crud_and_deadline() {
        let mut registry = TimerRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.next_deadline(), None);

        let receiver = ObjectId(100);
        let id1 = registry.register(receiver, 500, TimerType::Precise, false);
        assert!(id1.is_valid());
        assert_eq!(registry.len(), 1);

        let entry1 = registry.get(id1).expect("entry1 should exist");
        assert_eq!(entry1.interval_ms, 500);
        assert!(!entry1.single_shot);

        let id2 = registry.register(receiver, 100, TimerType::Precise, true);
        assert!(id2.is_valid());
        assert_ne!(id1, id2);
        assert_eq!(registry.len(), 2);

        let deadline = registry.next_deadline().unwrap();
        let entry2 = registry.get(id2).unwrap();
        assert_eq!(deadline, entry2.next_fire_ms);

        if let Some(entry_mut) = registry.get_mut(id1) {
            entry_mut.in_timer_event = true;
        }
        assert!(registry.get(id1).unwrap().in_timer_event);

        let removed2 = registry.unregister(id2).expect("id2 removed");
        assert_eq!(removed2.id, id2);
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.next_deadline(),
            Some(registry.get(id1).unwrap().next_fire_ms)
        );

        registry.unregister(id1);
        assert!(registry.is_empty());
        assert_eq!(registry.next_deadline(), None);
    }

    #[test]
    fn test_timer_start_stop_and_timeout_order() {
        use std::sync::{Arc, Mutex};

        let mut registry = TimerRegistry::new();
        let mut timer = Timer::new();
        timer.set_interval(100);
        timer.set_single_shot(true);

        let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let log2 = Arc::clone(&log);
        timer.timeout.connect(move |_| {
            log2.lock().unwrap().push("timeout");
        });

        unsafe { timer.start_with_registry(&mut registry) };
        assert!(timer.is_active());
        let id = timer.id;
        assert!(registry.get(id).is_some());

        timer.timer_event_with_registry(&TimerEvent::new(id), &mut registry);
        assert!(!timer.is_active());
        assert!(registry.get(id).is_none());
        assert_eq!(*log.lock().unwrap(), ["timeout"]);
    }

    #[test]
    fn test_timer_ergonomic_api_with_event_loop() {
        use crate::event_loop::EventLoop;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        let mut event_loop = EventLoop::new();
        let mut timer = Timer::new();
        timer.set_interval(10);
        timer.set_single_shot(true);

        let fired = Arc::new(AtomicBool::new(false));
        let f_clone = Arc::clone(&fired);
        timer.timeout.connect(move |_| {
            f_clone.store(true, Ordering::SeqCst);
        });

        unsafe { timer.start() };
        assert!(timer.is_active());

        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        while !fired.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            event_loop.process_events(false);
            std::thread::sleep(Duration::from_millis(5));
        }

        assert!(fired.load(Ordering::SeqCst));
        assert!(!timer.is_active());
    }

    fn spin_until(
        dispatcher: &mut crate::event_loop::Win32EventDispatcher,
        registry: &mut TimerRegistry,
        timeout: std::time::Duration,
        mut condition: impl FnMut() -> bool,
    ) -> bool {
        let start = std::time::Instant::now();
        while !condition() {
            if start.elapsed() > timeout {
                return false;
            }
            dispatcher.process_events_with_timers(false, None, registry);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        true
    }

    #[test]
    fn test_single_shot_timer() {
        use crate::event_loop::Win32EventDispatcher;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::time::Duration;

        let mut dispatcher = Win32EventDispatcher::new();
        let mut registry = TimerRegistry::new();

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let mut timer = Timer::new();
        timer.set_single_shot(true);
        timer.set_interval(30);
        timer.timeout.connect(move |_| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        unsafe { timer.start_with(&mut registry, &mut dispatcher) };
        assert!(timer.is_active());

        let ok = spin_until(
            &mut dispatcher,
            &mut registry,
            Duration::from_millis(500),
            || counter.load(Ordering::SeqCst) >= 1,
        );

        assert!(ok, "Timer failed to fire within timeout");
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        std::thread::sleep(Duration::from_millis(50));
        dispatcher.process_events_with_timers(false, None, &mut registry);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(
            !timer.is_active(),
            "Single shot timer should be inactive after firing"
        );
    }

    #[test]
    fn test_timer_cancellation() {
        use crate::event_loop::Win32EventDispatcher;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::time::Duration;

        let mut dispatcher = Win32EventDispatcher::new();
        let mut registry = TimerRegistry::new();

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let mut timer = Timer::new();
        timer.set_interval(50);
        timer.timeout.connect(move |_| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        unsafe { timer.start_with(&mut registry, &mut dispatcher) };

        timer.stop_with(&mut registry, &mut dispatcher);
        assert!(!timer.is_active());

        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(100) {
            dispatcher.process_events_with_timers(false, None, &mut registry);
            std::thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn test_zero_timer_immediate_dispatch() {
        use crate::event_loop::EventLoop;
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut event_loop = EventLoop::new();

        let mut timer = Timer::new();
        timer.set_interval(0);
        timer.set_single_shot(true);

        let fired = Arc::new(AtomicBool::new(false));
        let f_clone = Arc::clone(&fired);
        timer.timeout.connect(move |_| {
            f_clone.store(true, Ordering::SeqCst);
        });

        unsafe { timer.start() };
        assert!(timer.is_active());

        assert!(!fired.load(Ordering::SeqCst));

        let processed = event_loop.process_events(false);
        assert!(processed);
        assert!(
            fired.load(Ordering::SeqCst),
            "Zero timer should fire immediately in the first process_events call"
        );
        assert!(
            !timer.is_active(),
            "Single shot zero timer should be inactive after firing"
        );
    }

    #[test]
    fn test_timer_remaining_time() {
        use crate::event_loop::EventLoop;

        let mut timer = Timer::new();
        timer.set_interval(1000);

        assert_eq!(timer.remaining_time(), -1);

        let _event_loop = EventLoop::new();
        unsafe { timer.start() };
        assert!(timer.is_active());

        let rem = timer.remaining_time();
        assert!(
            rem > 0 && rem <= 1000,
            "remaining_time should be positive within interval, got {}",
            rem
        );

        timer.stop();
        assert_eq!(timer.remaining_time(), -1);
    }

    #[test]
    fn test_qtimer_single_shot_static_api_zero_delay() {
        use crate::event_loop::EventLoop;
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut event_loop = EventLoop::new();
        let executed = Arc::new(AtomicBool::new(false));
        let ex_clone = Arc::clone(&executed);

        Timer::single_shot(0, move || {
            ex_clone.store(true, Ordering::SeqCst);
        });

        assert!(!executed.load(Ordering::SeqCst));
        let processed = event_loop.process_events(false);
        assert!(processed);
        assert!(executed.load(Ordering::SeqCst));
    }
}
