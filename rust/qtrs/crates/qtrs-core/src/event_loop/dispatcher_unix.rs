use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::event::{NativeEventFilter, NativeEventFilterChain, NativeMessage};
use crate::event_loop::dispatcher::{DispatchResult, EventDispatcher, EventDispatcherHandle};
use crate::event_loop::socket_notifier::{SocketDescriptor, SocketEvent, SocketNotifier};
use crate::timer::{TimerEntry, TimerId, TimerRegistry};

pub const EPOLLIN: u32 = 0x001;
pub const EPOLLPRI: u32 = 0x002;
pub const EPOLLOUT: u32 = 0x004;
pub const EPOLLERR: u32 = 0x008;
pub const EPOLLHUP: u32 = 0x010;

pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;

#[derive(Debug)]
pub struct EventFd {
    counter: AtomicU64,
}

impl EventFd {
    pub fn new(init: u64) -> Self {
        Self {
            counter: AtomicU64::new(init),
        }
    }

    pub fn write(&self, val: u64) {
        self.counter.fetch_add(val, Ordering::SeqCst);
    }

    pub fn read(&self) -> u64 {
        self.counter.swap(0, Ordering::SeqCst)
    }

    pub fn is_readable(&self) -> bool {
        self.counter.load(Ordering::SeqCst) > 0
    }
}

/// Simulated Linux `timerfd` structure
#[derive(Debug)]
pub struct TimerFd {
    pub id: TimerId,
    pub interval: Duration,
    pub single_shot: bool,
    pub next_fire: Mutex<Option<Instant>>,
    pub expirations: AtomicU64,
}

impl TimerFd {
    pub fn new(id: TimerId, interval: Duration, single_shot: bool) -> Self {
        let next_fire = Instant::now() + interval;
        Self {
            id,
            interval,
            single_shot,
            next_fire: Mutex::new(Some(next_fire)),
            expirations: AtomicU64::new(0),
        }
    }

    pub fn check_and_fire(&self, now: Instant) -> bool {
        let mut next = self.next_fire.lock().unwrap();
        if let Some(target) = *next {
            if now >= target {
                self.expirations.fetch_add(1, Ordering::SeqCst);
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

    pub fn read_expirations(&self) -> u64 {
        self.expirations.swap(0, Ordering::SeqCst)
    }
}

#[cfg(target_os = "linux")]
mod linux_epoll {
    use std::os::raw::c_int;

    pub const EPOLLIN: u32 = 0x001;
    pub const EPOLLOUT: u32 = 0x004;
    pub const EPOLLERR: u32 = 0x008;
    pub const EPOLLHUP: u32 = 0x010;
    pub const EPOLL_CLOEXEC: c_int = 0x80000;

    pub const EPOLL_CTL_ADD: c_int = 1;
    pub const EPOLL_CTL_DEL: c_int = 2;
    pub const EPOLL_CTL_MOD: c_int = 3;

    #[repr(C)]
    #[cfg_attr(target_arch = "x86_64", repr(packed))]
    #[derive(Debug, Copy, Clone)]
    pub struct EpollEvent {
        pub events: u32,
        pub data: u64,
    }

    extern "C" {
        pub fn epoll_create1(flags: c_int) -> c_int;
        pub fn epoll_ctl(epfd: c_int, op: c_int, fd: c_int, event: *mut EpollEvent) -> c_int;
        pub fn epoll_wait(
            epfd: c_int,
            events: *mut EpollEvent,
            maxevents: c_int,
            timeout: c_int,
        ) -> c_int;
        pub fn close(fd: c_int) -> c_int;
    }
}

pub struct EpollReactor {
    pub event_fd: Arc<EventFd>,
    pub timer_fds: Mutex<HashMap<TimerId, Arc<TimerFd>>>,
    pub socket_notifiers: Mutex<HashMap<(SocketDescriptor, SocketEvent), Arc<SocketNotifier>>>,
    pub pending_socket_events: Mutex<Vec<(SocketDescriptor, SocketEvent)>>,
    cond: Condvar,
    lock: Mutex<bool>,
    #[cfg(target_os = "linux")]
    epoll_fd: std::os::raw::c_int,
}

impl Default for EpollReactor {
    fn default() -> Self {
        Self::new()
    }
}

impl EpollReactor {
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        let epoll_fd = unsafe { linux_epoll::epoll_create1(linux_epoll::EPOLL_CLOEXEC) };

        Self {
            event_fd: Arc::new(EventFd::new(0)),
            timer_fds: Mutex::new(HashMap::new()),
            socket_notifiers: Mutex::new(HashMap::new()),
            pending_socket_events: Mutex::new(Vec::new()),
            cond: Condvar::new(),
            lock: Mutex::new(false),
            #[cfg(target_os = "linux")]
            epoll_fd,
        }
    }

    pub fn wake_up(&self) {
        self.event_fd.write(1);
        let _guard = self.lock.lock().unwrap();
        self.cond.notify_all();
    }

    pub fn register_timer(&self, id: TimerId, interval: Duration, single_shot: bool) {
        let tfd = Arc::new(TimerFd::new(id, interval, single_shot));
        self.timer_fds.lock().unwrap().insert(id, tfd);
        let _guard = self.lock.lock().unwrap();
        self.cond.notify_all();
    }

    pub fn unregister_timer(&self, id: TimerId) {
        self.timer_fds.lock().unwrap().remove(&id);
    }

    pub fn register_socket_notifier(&self, notifier: &Arc<SocketNotifier>) {
        let key = (notifier.descriptor(), notifier.event_type());
        self.socket_notifiers
            .lock()
            .unwrap()
            .insert(key, Arc::clone(notifier));
        #[cfg(target_os = "linux")]
        if self.epoll_fd >= 0 {
            let mut ev = linux_epoll::EpollEvent {
                events: match notifier.event_type() {
                    SocketEvent::Read => {
                        linux_epoll::EPOLLIN | linux_epoll::EPOLLERR | linux_epoll::EPOLLHUP
                    }
                    SocketEvent::Write => {
                        linux_epoll::EPOLLOUT | linux_epoll::EPOLLERR | linux_epoll::EPOLLHUP
                    }
                },
                data: notifier.descriptor() as u64,
            };
            unsafe {
                linux_epoll::epoll_ctl(
                    self.epoll_fd,
                    linux_epoll::EPOLL_CTL_ADD,
                    notifier.descriptor() as std::os::raw::c_int,
                    &mut ev,
                );
            }
        }

        let _guard = self.lock.lock().unwrap();
        self.cond.notify_all();
    }

    pub fn unregister_socket_notifier(&self, notifier: &Arc<SocketNotifier>) {
        let key = (notifier.descriptor(), notifier.event_type());
        self.socket_notifiers.lock().unwrap().remove(&key);

        #[cfg(target_os = "linux")]
        if self.epoll_fd >= 0 {
            unsafe {
                linux_epoll::epoll_ctl(
                    self.epoll_fd,
                    linux_epoll::EPOLL_CTL_DEL,
                    notifier.descriptor() as std::os::raw::c_int,
                    std::ptr::null_mut(),
                );
            }
        }
    }

    pub fn trigger_socket_event(&self, fd: SocketDescriptor, event: SocketEvent) {
        self.pending_socket_events.lock().unwrap().push((fd, event));
        let _guard = self.lock.lock().unwrap();
        self.cond.notify_all();
    }

    pub fn epoll_wait(
        &self,
        timeout: Option<Duration>,
    ) -> (bool, Vec<TimerId>, Vec<(SocketDescriptor, SocketEvent)>) {
        let start = Instant::now();
        let guard = self.lock.lock().unwrap();
        let pending_sockets = std::mem::take(&mut *self.pending_socket_events.lock().unwrap());

        // Check if eventfd is readable
        if self.event_fd.is_readable() {
            self.event_fd.read();
            return (true, self.collect_expired_timers(start), pending_sockets);
        }

        // Return immediately if there are pending socket events
        if !pending_sockets.is_empty() {
            return (false, self.collect_expired_timers(start), pending_sockets);
        }

        // Check if any timers expired
        let expired = self.collect_expired_timers(start);
        if !expired.is_empty() {
            return (false, expired, Vec::new());
        }
        // Calculate sleep timeout
        let sleep_duration = match timeout {
            Some(t) => {
                let earliest_timer = self.next_timer_delay(start);
                match earliest_timer {
                    Some(td) => t.min(td),
                    None => t,
                }
            }
            None => self
                .next_timer_delay(start)
                .unwrap_or(Duration::from_secs(3600)),
        };

        if sleep_duration.is_zero() {
            return (false, Vec::new(), Vec::new());
        }
        #[cfg(target_os = "linux")]
        if self.epoll_fd >= 0 {
            let timeout_ms = match sleep_duration {
                d if d.is_zero() => 0,
                d => (d.as_millis() as std::os::raw::c_int).min(i32::MAX as std::os::raw::c_int),
            };

            let mut events = [linux_epoll::EpollEvent { events: 0, data: 0 }; 64];
            let nfds = unsafe {
                linux_epoll::epoll_wait(self.epoll_fd, events.as_mut_ptr(), 64, timeout_ms)
            };

            if nfds > 0 {
                let mut kernel_sockets = Vec::new();
                for i in 0..nfds as usize {
                    let ev = events[i];
                    let fd = ev.data as SocketDescriptor;
                    let sk_event = if (ev.events & linux_epoll::EPOLLOUT) != 0 {
                        SocketEvent::Write
                    } else {
                        SocketEvent::Read
                    };
                    kernel_sockets.push((fd, sk_event));
                }
                let now = Instant::now();
                let expired = self.collect_expired_timers(now);
                let mut combined = pending_sockets;
                combined.extend(kernel_sockets);
                return (false, expired, combined);
            }
        }

        // Wait on condition variable or timeout
        let (_new_guard, _timeout_res) = self.cond.wait_timeout(guard, sleep_duration).unwrap();
        let now = Instant::now();
        let was_awoken = if self.event_fd.is_readable() {
            self.event_fd.read();
            true
        } else {
            false
        };

        let expired = self.collect_expired_timers(now);
        let pending_sockets = std::mem::take(&mut *self.pending_socket_events.lock().unwrap());
        (was_awoken, expired, pending_sockets)
    }

    fn next_timer_delay(&self, now: Instant) -> Option<Duration> {
        let tfds = self.timer_fds.lock().unwrap();
        let mut min_delay: Option<Duration> = None;
        for tfd in tfds.values() {
            if let Some(target) = *tfd.next_fire.lock().unwrap() {
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

    fn collect_expired_timers(&self, now: Instant) -> Vec<TimerId> {
        let tfds = self.timer_fds.lock().unwrap();
        let mut expired = Vec::new();
        for (id, tfd) in tfds.iter() {
            if tfd.check_and_fire(now) {
                expired.push(*id);
            }
        }
        expired
    }
}
impl Drop for EpollReactor {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if self.epoll_fd >= 0 {
            unsafe {
                linux_epoll::close(self.epoll_fd);
            }
        }
    }
}

/// Linux/Unix cross-thread wakeup handle
#[derive(Clone)]
pub struct UnixEventDispatcherHandle {
    reactor: Arc<EpollReactor>,
    wakeup_pending: Arc<AtomicBool>,
}

impl EventDispatcherHandle for UnixEventDispatcherHandle {
    fn wake_up(&self) {
        if !self.wakeup_pending.swap(true, Ordering::Release) {
            self.reactor.wake_up();
        }
    }
}

pub struct UnixEventDispatcher {
    reactor: Arc<EpollReactor>,
    wakeup_pending: Arc<AtomicBool>,
    pending_timers: Mutex<Vec<TimerId>>,
    native_filters: NativeEventFilterChain,
}

impl Default for UnixEventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl UnixEventDispatcher {
    pub fn new() -> Self {
        Self {
            reactor: Arc::new(EpollReactor::new()),
            wakeup_pending: Arc::new(AtomicBool::new(false)),
            pending_timers: Mutex::new(Vec::new()),
            native_filters: NativeEventFilterChain::new(),
        }
    }

    pub fn clone_handle(&self) -> UnixEventDispatcherHandle {
        UnixEventDispatcherHandle {
            reactor: Arc::clone(&self.reactor),
            wakeup_pending: Arc::clone(&self.wakeup_pending),
        }
    }
}

impl EventDispatcher for UnixEventDispatcher {
    fn wake_up(&self) {
        if !self.wakeup_pending.swap(true, Ordering::Release) {
            self.reactor.wake_up();
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
        self.wakeup_pending.store(false, Ordering::Release);

        let timeout = if can_wait {
            next_timer_timeout
        } else {
            Some(Duration::from_millis(0))
        };

        let (awoken, expired_timers, triggered_sockets) = self.reactor.epoll_wait(timeout);

        if !triggered_sockets.is_empty() {
            let notifiers = self.reactor.socket_notifiers.lock().unwrap();
            for (fd, ev) in triggered_sockets {
                if let Some(notifier) = notifiers.get(&(fd, ev)) {
                    notifier.notify();
                }
            }
        }

        if awoken {
            return DispatchResult::Awoken;
        }
        if !expired_timers.is_empty() {
            self.pending_timers.lock().unwrap().extend(expired_timers);
            return DispatchResult::Normal;
        }
        if can_wait && next_timer_timeout.is_some() {
            DispatchResult::Timeout
        } else {
            DispatchResult::Normal
        }
    }

    fn register_timer(&mut self, entry: &TimerEntry) {
        self.reactor.register_timer(
            entry.id,
            Duration::from_millis(entry.interval_ms),
            entry.single_shot,
        );
    }

    fn unregister_timer(&mut self, entry: &TimerEntry) {
        self.reactor.unregister_timer(entry.id);
    }

    fn register_socket_notifier(&mut self, notifier: &Arc<SocketNotifier>) {
        self.reactor.register_socket_notifier(notifier);
    }

    fn unregister_socket_notifier(&mut self, notifier: &Arc<SocketNotifier>) {
        self.reactor.unregister_socket_notifier(notifier);
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
                self.reactor.unregister_timer(id);
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
    fn test_unix_eventfd_write_read() {
        let efd = EventFd::new(0);
        assert!(!efd.is_readable());

        efd.write(1);
        assert!(efd.is_readable());

        efd.write(2);
        assert_eq!(efd.read(), 3);
        assert!(!efd.is_readable());
    }

    #[test]
    fn test_unix_dispatcher_cross_thread_wakeup() {
        let mut dispatcher = UnixEventDispatcher::new();
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
    fn test_unix_dispatcher_timeout() {
        let mut dispatcher = UnixEventDispatcher::new();
        let start = Instant::now();
        let result = dispatcher.process_events(true, Some(Duration::from_millis(30)));
        assert_eq!(result, DispatchResult::Timeout);
        assert!(start.elapsed() >= Duration::from_millis(25));
    }

    #[test]
    fn test_unix_dispatcher_timer_integration() {
        let mut dispatcher = UnixEventDispatcher::new();
        let mut entry = TimerEntry::new(
            TimerId(101),
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
    fn test_unix_dispatcher_socket_notifier_activation() {
        let mut dispatcher = UnixEventDispatcher::new();
        let fd = 42; // Simulated socket FD (e.g. X11 or D-Bus)
        let notifier = Arc::new(SocketNotifier::new(fd, SocketEvent::Read));

        let triggered_fd = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&triggered_fd);
        notifier.activated().connect(move |&sock| {
            *sink.lock().unwrap() = Some(sock);
        });

        dispatcher.register_socket_notifier(&notifier);

        // Simulate incoming socket data (EPOLLIN)
        dispatcher
            .reactor
            .trigger_socket_event(fd, SocketEvent::Read);

        // Dispatch events
        let res = dispatcher.process_events(false, None);
        assert_eq!(res, DispatchResult::Normal);
        assert_eq!(*triggered_fd.lock().unwrap(), Some(fd));

        // Disabling notifier should prevent signals
        *triggered_fd.lock().unwrap() = None;
        notifier.set_enabled(false);
        dispatcher
            .reactor
            .trigger_socket_event(fd, SocketEvent::Read);
        dispatcher.process_events(false, None);
        assert_eq!(*triggered_fd.lock().unwrap(), None);

        dispatcher.unregister_socket_notifier(&notifier);
    }
}
