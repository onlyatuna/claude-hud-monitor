use crate::event::{NativeEventFilter, NativeEventFilterChain, NativeMessage};
use crate::timer::{TimerEntry, TimerRegistry};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchResult {
    Quit(i32),
    Awoken,
    Timeout,
    Normal,
}

impl DispatchResult {
    pub fn is_quit(&self) -> bool {
        matches!(self, DispatchResult::Quit(_))
    }

    pub fn is_awoken(&self) -> bool {
        matches!(self, DispatchResult::Awoken)
    }

    pub fn is_timeout(&self) -> bool {
        matches!(self, DispatchResult::Timeout)
    }
}

pub trait EventDispatcherHandle: Send + Sync {
    fn wake_up(&self);
}

pub trait EventDispatcher: Send + Sync {
    fn wake_up(&self);
    fn clone_handle(&self) -> Arc<dyn EventDispatcherHandle>;
    fn install_native_event_filter(&mut self, filter: Box<dyn NativeEventFilter>);
    fn filter_native_event(
        &mut self,
        event_type: &str,
        msg: &NativeMessage,
        result: &mut isize,
    ) -> bool;
    fn process_events(
        &mut self,
        can_wait: bool,
        next_timer_timeout: Option<Duration>,
    ) -> DispatchResult;
    fn register_timer(&mut self, entry: &TimerEntry);
    fn unregister_timer(&mut self, entry: &TimerEntry);
    fn send_timer_events(&mut self, registry: &mut TimerRegistry);
    fn register_socket_notifier(&mut self, _notifier: &Arc<crate::event_loop::SocketNotifier>) {}
    fn unregister_socket_notifier(&mut self, _notifier: &Arc<crate::event_loop::SocketNotifier>) {}
}

#[derive(Debug, Default)]
pub struct GenericEventDispatcher {
    wake_up_flag: Arc<AtomicBool>,
    native_filters: NativeEventFilterChain,
}

impl GenericEventDispatcher {
    pub fn new() -> Self {
        Self {
            wake_up_flag: Arc::new(AtomicBool::new(false)),
            native_filters: NativeEventFilterChain::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenericEventDispatcherHandle {
    wake_up_flag: Arc<AtomicBool>,
}

impl EventDispatcherHandle for GenericEventDispatcherHandle {
    fn wake_up(&self) {
        self.wake_up_flag.store(true, Ordering::Release);
    }
}

impl EventDispatcher for GenericEventDispatcher {
    fn wake_up(&self) {
        self.wake_up_flag.store(true, Ordering::Release);
    }

    fn clone_handle(&self) -> Arc<dyn EventDispatcherHandle> {
        Arc::new(GenericEventDispatcherHandle {
            wake_up_flag: Arc::clone(&self.wake_up_flag),
        })
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
        if self.wake_up_flag.swap(false, Ordering::AcqRel) {
            return DispatchResult::Awoken;
        }

        if can_wait {
            if let Some(timeout) = next_timer_timeout {
                if timeout.is_zero() {
                    return DispatchResult::Timeout;
                }
                std::thread::park_timeout(timeout);
            } else {
                std::thread::park();
            }

            if self.wake_up_flag.swap(false, Ordering::AcqRel) {
                DispatchResult::Awoken
            } else {
                DispatchResult::Timeout
            }
        } else {
            DispatchResult::Normal
        }
    }

    fn register_timer(&mut self, _entry: &TimerEntry) {}

    fn unregister_timer(&mut self, _entry: &TimerEntry) {}

    fn send_timer_events(&mut self, _registry: &mut TimerRegistry) {}
}

#[cfg(windows)]
pub type DefaultEventDispatcher = super::dispatcher_win::Win32EventDispatcher;

#[cfg(target_os = "linux")]
pub type DefaultEventDispatcher = super::dispatcher_unix::UnixEventDispatcher;

#[cfg(target_os = "macos")]
pub type DefaultEventDispatcher = super::dispatcher_cocoa::CocoaEventDispatcher;

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub type DefaultEventDispatcher = GenericEventDispatcher;

pub fn create_default_dispatcher() -> DefaultEventDispatcher {
    DefaultEventDispatcher::new()
}
