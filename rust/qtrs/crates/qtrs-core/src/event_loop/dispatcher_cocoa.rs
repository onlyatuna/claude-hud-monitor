use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::event::{NativeEventFilter, NativeEventFilterChain, NativeMessage};
use crate::event_loop::dispatcher::{DispatchResult, EventDispatcher, EventDispatcherHandle};
use crate::timer::{TimerEntry, TimerId, TimerRegistry};

pub const K_CF_RUN_LOOP_RUN_FINISHED: i32 = 1;
pub const K_CF_RUN_LOOP_RUN_STOPPED: i32 = 2;
pub const K_CF_RUN_LOOP_RUN_TIMED_OUT: i32 = 3;
pub const K_CF_RUN_LOOP_RUN_HANDLED_SOURCE: i32 = 4;

pub struct CFRunLoopSource {
    signaled: AtomicBool,
}

impl Default for CFRunLoopSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CFRunLoopSource {
    pub fn new() -> Self {
        Self {
            signaled: AtomicBool::new(false),
        }
    }

    pub fn signal(&self) {
        self.signaled.store(true, Ordering::SeqCst);
    }

    pub fn take_signal(&self) -> bool {
        self.signaled.swap(false, Ordering::SeqCst)
    }
}

pub struct CFRunLoopTimer {
    pub id: TimerId,
    pub interval: Duration,
    pub single_shot: bool,
    pub next_fire: Mutex<Option<Instant>>,
}

impl CFRunLoopTimer {
    pub fn new(id: TimerId, interval: Duration, single_shot: bool) -> Self {
        Self {
            id,
            interval,
            single_shot,
            next_fire: Mutex::new(Some(Instant::now() + interval)),
        }
    }

    pub fn check_and_fire(&self, now: Instant) -> bool {
        let mut next = self.next_fire.lock().unwrap();
        if let Some(target) = *next {
            if now >= target {
                if self.single_shot {
                    *next = None;
                } else {
                    *next = Some(now + self.interval);
                }
                return true;
            }
        }
        false
    }
}

pub struct CFRunLoopEngine {
    source: Arc<CFRunLoopSource>,
    timers: Mutex<HashMap<TimerId, Arc<CFRunLoopTimer>>>,
    cond: Condvar,
    lock: Mutex<bool>,
}

impl Default for CFRunLoopEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CFRunLoopEngine {
    pub fn new() -> Self {
        Self {
            source: Arc::new(CFRunLoopSource::new()),
            timers: Mutex::new(HashMap::new()),
            cond: Condvar::new(),
            lock: Mutex::new(false),
        }
    }

    pub fn wake_up(&self) {
        self.source.signal();
        let _guard = self.lock.lock().unwrap();
        self.cond.notify_all();
    }

    pub fn add_timer(&self, id: TimerId, interval: Duration, single_shot: bool) {
        let timer = Arc::new(CFRunLoopTimer::new(id, interval, single_shot));
        self.timers.lock().unwrap().insert(id, timer);
        let _guard = self.lock.lock().unwrap();
        self.cond.notify_all();
    }

    pub fn remove_timer(&self, id: TimerId) {
        self.timers.lock().unwrap().remove(&id);
    }

    pub fn run_in_mode(&self, timeout: Option<Duration>) -> i32 {
        let start = Instant::now();
        let guard = self.lock.lock().unwrap();

        if self.source.take_signal() {
            return K_CF_RUN_LOOP_RUN_HANDLED_SOURCE;
        }

        let expired = self.collect_expired(start);
        if !expired.is_empty() {
            return K_CF_RUN_LOOP_RUN_FINISHED;
        }

        // 3. Calculate minimum wait time
        let wait_time = match timeout {
            Some(t) => {
                let earliest = self.next_timer_delay(start);
                match earliest {
                    Some(et) => t.min(et),
                    None => t,
                }
            }
            None => self
                .next_timer_delay(start)
                .unwrap_or(Duration::from_secs(3600)),
        };

        if wait_time.is_zero() {
            return K_CF_RUN_LOOP_RUN_TIMED_OUT;
        }

        // Wait on condition variable
        let (_new_guard, _res) = self.cond.wait_timeout(guard, wait_time).unwrap();

        if self.source.take_signal() {
            K_CF_RUN_LOOP_RUN_HANDLED_SOURCE
        } else {
            let now = Instant::now();
            let exp = self.collect_expired(now);
            if !exp.is_empty() {
                K_CF_RUN_LOOP_RUN_FINISHED
            } else {
                K_CF_RUN_LOOP_RUN_TIMED_OUT
            }
        }
    }

    fn next_timer_delay(&self, now: Instant) -> Option<Duration> {
        let timers = self.timers.lock().unwrap();
        let mut min_delay: Option<Duration> = None;
        for t in timers.values() {
            if let Some(target) = *t.next_fire.lock().unwrap() {
                let delay = if target > now {
                    target - now
                } else {
                    Duration::from_millis(0)
                };
                min_delay = Some(min_delay.map_or(delay, |m| m.min(delay)));
            }
        }
        min_delay
    }

    fn collect_expired(&self, now: Instant) -> Vec<TimerId> {
        let timers = self.timers.lock().unwrap();
        let mut expired = Vec::new();
        for (id, t) in timers.iter() {
            if t.check_and_fire(now) {
                expired.push(*id);
            }
        }
        expired
    }
}

#[derive(Clone)]
pub struct CocoaEventDispatcherHandle {
    engine: Arc<CFRunLoopEngine>,
    wakeup_pending: Arc<AtomicBool>,
}

impl EventDispatcherHandle for CocoaEventDispatcherHandle {
    fn wake_up(&self) {
        if !self.wakeup_pending.swap(true, Ordering::Release) {
            self.engine.wake_up();
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CocoaNativeEvent {
    MouseDown {
        x: f64,
        y: f64,
        button: u16,
        modifiers: u32,
    },
    MouseUp {
        x: f64,
        y: f64,
        button: u16,
        modifiers: u32,
    },
    MouseMoved {
        x: f64,
        y: f64,
        modifiers: u32,
    },
    ScrollWheel {
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
    },
    KeyDown {
        key_code: u16,
        modifiers: u32,
        is_repeat: bool,
    },
    KeyUp {
        key_code: u16,
        modifiers: u32,
    },
    WindowResized {
        width: f64,
        height: f64,
    },
    WindowCloseRequested,
}

pub struct CocoaEventDispatcher {
    engine: Arc<CFRunLoopEngine>,
    wakeup_pending: Arc<AtomicBool>,
    pending_timers: Mutex<Vec<TimerId>>,
    appkit_event_queue: Arc<Mutex<Vec<CocoaNativeEvent>>>,
    native_filters: NativeEventFilterChain,
}

impl Default for CocoaEventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl CocoaEventDispatcher {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(CFRunLoopEngine::new()),
            wakeup_pending: Arc::new(AtomicBool::new(false)),
            pending_timers: Mutex::new(Vec::new()),
            appkit_event_queue: Arc::new(Mutex::new(Vec::new())),
            native_filters: NativeEventFilterChain::new(),
        }
    }

    pub fn clone_handle(&self) -> CocoaEventDispatcherHandle {
        CocoaEventDispatcherHandle {
            engine: Arc::clone(&self.engine),
            wakeup_pending: Arc::clone(&self.wakeup_pending),
        }
    }

    pub fn post_appkit_event(&self, event: CocoaNativeEvent) {
        self.appkit_event_queue.lock().unwrap().push(event);
        self.engine.wake_up();
    }

    pub fn pump_appkit_events(&mut self) -> Vec<CocoaNativeEvent> {
        let events = std::mem::take(&mut *self.appkit_event_queue.lock().unwrap());
        let mut accepted = Vec::new();
        for event in events {
            let mut result = 0isize;
            #[cfg(target_os = "macos")]
            let msg = NativeMessage::Mac(std::ptr::null_mut());
            #[cfg(not(target_os = "macos"))]
            let msg = NativeMessage::Custom("NSEvent", std::ptr::null_mut());
            if !self.filter_native_event("NSEvent", &msg, &mut result) {
                accepted.push(event);
            }
        }
        accepted
    }
}

impl EventDispatcher for CocoaEventDispatcher {
    fn wake_up(&self) {
        if !self.wakeup_pending.swap(true, Ordering::Release) {
            self.engine.wake_up();
        }
    }

    fn clone_handle(&self) -> Arc<dyn EventDispatcherHandle> {
        Arc::new(self.clone_handle())
    }

    fn install_native_event_filter(&mut self, filter: Box<dyn NativeEventFilter>) {
        self.native_filters.install(filter);
    }

    fn filter_native_event(
        &mut self,
        event_type: &str,
        msg: &NativeMessage,
        result: &mut isize,
    ) -> bool {
        self.native_filters.filter_native(event_type, msg, result)
    }

    fn process_events(
        &mut self,
        can_wait: bool,
        next_timer_timeout: Option<Duration>,
    ) -> DispatchResult {
        crate::object::ThreadContext::assert_main_thread("CocoaEventDispatcher::process_events");

        let pumped = self.pump_appkit_events();
        let had_pumped = !pumped.is_empty();
        self.wakeup_pending.store(false, Ordering::Release);

        if had_pumped {
            return DispatchResult::Awoken;
        }
        let timeout = if can_wait {
            next_timer_timeout
        } else {
            Some(Duration::from_millis(0))
        };

        let run_result = self.engine.run_in_mode(timeout);

        match run_result {
            K_CF_RUN_LOOP_RUN_HANDLED_SOURCE => DispatchResult::Awoken,
            K_CF_RUN_LOOP_RUN_FINISHED => {
                let now = Instant::now();
                let exp = self.engine.collect_expired(now);
                if !exp.is_empty() {
                    self.pending_timers.lock().unwrap().extend(exp);
                }
                DispatchResult::Normal
            }
            K_CF_RUN_LOOP_RUN_TIMED_OUT => {
                if can_wait && next_timer_timeout.is_some() {
                    DispatchResult::Timeout
                } else {
                    DispatchResult::Normal
                }
            }
            _ => DispatchResult::Normal,
        }
    }
    fn register_timer(&mut self, entry: &TimerEntry) {
        self.engine.add_timer(
            entry.id,
            Duration::from_millis(entry.interval_ms),
            entry.single_shot,
        );
    }

    fn unregister_timer(&mut self, entry: &TimerEntry) {
        self.engine.remove_timer(entry.id);
    }

    fn send_timer_events(&mut self, registry: &mut TimerRegistry) {
        let pending = std::mem::take(&mut *self.pending_timers.lock().unwrap());
        for id in pending {
            let Some(entry) = registry.get(id) else {
                continue;
            };
            if entry.in_timer_event {
                continue;
            }
            let receiver = entry.receiver;
            let single_shot = entry.single_shot;

            if let Some(entry) = registry.get_mut(id) {
                entry.in_timer_event = true;
            }

            let handled = crate::object::with_object_mut(receiver, |obj| {
                let mut event = crate::event::Event::new(crate::event::EventKind::Timer {
                    timer_id: id.0 as u64,
                });
                obj.event(&mut event);
            })
            .is_some();
            if !handled {
                crate::timer::dispatch_single_shot_callback(receiver);
            }

            if single_shot {
                registry.unregister(id);
                self.engine.remove_timer(id);
            } else if let Some(entry) = registry.get_mut(id) {
                entry.in_timer_event = false;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cocoa_run_loop_source_signal() {
        let source = CFRunLoopSource::new();
        assert!(!source.take_signal());

        source.signal();
        assert!(source.take_signal());
        assert!(!source.take_signal());
    }

    #[test]
    fn test_cocoa_dispatcher_cross_thread_wakeup() {
        crate::object::ThreadContext::init_current(true, None);
        let mut dispatcher = CocoaEventDispatcher::new();
        let handle = dispatcher.clone_handle();

        let thread_handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            handle.wake_up();
        });

        let result = dispatcher.process_events(true, Some(Duration::from_millis(500)));
        assert_eq!(result, DispatchResult::Awoken);

        thread_handle.join().unwrap();
    }

    #[test]
    fn test_cocoa_dispatcher_timeout() {
        crate::object::ThreadContext::init_current(true, None);
        let mut dispatcher = CocoaEventDispatcher::new();
        let start = Instant::now();
        let result = dispatcher.process_events(true, Some(Duration::from_millis(30)));
        assert_eq!(result, DispatchResult::Timeout);
        assert!(start.elapsed() >= Duration::from_millis(25));
    }

    #[test]
    fn test_cocoa_dispatcher_timer_integration() {
        crate::object::ThreadContext::init_current(true, None);
        let mut dispatcher = CocoaEventDispatcher::new();
        let mut entry = TimerEntry::new(
            TimerId(202),
            20,
            0,
            crate::timer::TimerType::Precise,
            crate::object::ObjectId::next(),
        );
        entry.single_shot = true;
        dispatcher.register_timer(&entry);
        std::thread::sleep(Duration::from_millis(30));

        let result = dispatcher.process_events(false, None);
        assert_eq!(result, DispatchResult::Normal);

        dispatcher.unregister_timer(&entry);
    }

    #[test]
    fn test_cocoa_dispatcher_appkit_event_pumping() {
        crate::object::ThreadContext::init_current(true, None);
        let mut dispatcher = CocoaEventDispatcher::new();
        dispatcher.post_appkit_event(CocoaNativeEvent::MouseDown {
            x: 100.0,
            y: 200.0,
            button: 0,
            modifiers: 0,
        });

        let result = dispatcher.process_events(false, None);
        assert_eq!(result, DispatchResult::Awoken);
    }

    #[test]
    fn test_cocoa_dispatcher_main_thread_enforcement() {
        let handle = std::thread::spawn(|| {
            crate::object::ThreadContext::init_current(false, None);
            let mut dispatcher = CocoaEventDispatcher::new();
            let _ = dispatcher.process_events(false, None);
        });

        let join_res = handle.join();
        assert!(
            join_res.is_err(),
            "Calling process_events from non-main thread must panic"
        );
    }
}
