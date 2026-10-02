#![allow(unused_imports, dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};
use std::time::Duration;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WAIT_TIMEOUT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::INFINITE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, KillTimer,
    MsgWaitForMultipleObjectsEx, PeekMessageW, PostMessageW, RegisterClassExW, SetTimer,
    TranslateMessage, HWND_MESSAGE, MSG, MWMO_ALERTABLE, PM_REMOVE, QS_ALLINPUT, WM_QUIT, WM_TIMER,
    WNDCLASSEXW,
};
pub(crate) fn remove_posted_timer_event(hwnd: HWND, timer_id: u32) {
    if !hwnd.is_null() {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, hwnd, WM_TIMER, WM_TIMER, PM_REMOVE) != 0 {
                if msg.wParam as u32 == timer_id {
                    continue;
                } else {
                    PENDING_WM_TIMERS.with(|q| q.borrow_mut().push(msg.wParam as u32));
                }
            }
        }
    }

    PENDING_WM_TIMERS.with(|q| {
        q.borrow_mut().retain(|&id| id != timer_id);
    });
}

use crate::event::{NativeEventFilter, NativeEventFilterChain, NativeMessage};
pub use crate::event_loop::dispatcher::DispatchResult;
use crate::event_loop::dispatcher::{EventDispatcher, EventDispatcherHandle};
use crate::object::{ObjectId, QObject};
use crate::timer::{calculate_next_timeout, current_time_ms, TimerEntry, TimerId, TimerRegistry};

thread_local! {
    static PENDING_WM_TIMERS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

pub const WM_QTRS_WAKEUP: u32 = 0x0400 + 101;

const CLASS_NAME: &[u16] = &[
    'Q' as u16, 't' as u16, 'r' as u16, 's' as u16, 'M' as u16, 'e' as u16, 's' as u16, 's' as u16,
    'a' as u16, 'g' as u16, 'e' as u16, 'W' as u16, 'i' as u16, 'n' as u16, 'd' as u16, 'o' as u16,
    'w' as u16, 'C' as u16, 'l' as u16, 'a' as u16, 's' as u16, 's' as u16, 0,
];

unsafe extern "system" fn internal_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_TIMER {
        let timer_id = wparam as u32;
        if crate::timer::has_thread_timer_context() {
            dispatch_thread_timer(timer_id, hwnd);
        } else {
            PENDING_WM_TIMERS.with(|q| q.borrow_mut().push(timer_id));
        }
        return 0;
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn dispatch_thread_timer(raw_id: u32, internal_hwnd: HWND) {
    let id = TimerId(raw_id);
    let mut receiver_opt = None;
    let mut single_shot = false;

    crate::timer::with_thread_timer_context(|ctx| {
        let mut reg = ctx.registry.lock().unwrap();
        if let Some(entry) = reg.get_mut(id) {
            if !entry.in_timer_event {
                entry.in_timer_event = true;
                let (adjusted, next_fire) = calculate_next_timeout(
                    &mut entry.timer_type,
                    entry.interval_ms,
                    current_time_ms(),
                );
                entry.interval_ms = adjusted;
                entry.next_fire_ms = next_fire;
                single_shot = entry.single_shot;
                receiver_opt = Some(entry.receiver);
                if single_shot {
                    reg.unregister(id);
                }
            }
        }
    });

    let Some(receiver) = receiver_opt else {
        return;
    };

    if single_shot {
        unsafe {
            KillTimer(internal_hwnd, raw_id as usize);
        }
        remove_posted_timer_event(internal_hwnd, raw_id);
    }

    let handled = crate::object::with_object_mut(receiver, |obj| {
        obj.timer_event(raw_id as u64);
    })
    .is_some();
    if !handled {
        crate::timer::dispatch_single_shot_callback(receiver);
    }

    if !single_shot {
        crate::timer::with_thread_timer_context(|ctx| {
            let mut reg = ctx.registry.lock().unwrap();
            if let Some(entry) = reg.get_mut(id) {
                entry.in_timer_event = false;
            } else {
                unsafe {
                    KillTimer(internal_hwnd, raw_id as usize);
                }
            }
        });
    }
}

static REGISTER_CLASS_ONCE: Once = Once::new();

fn ensure_window_class_registered() {
    REGISTER_CLASS_ONCE.call_once(|| unsafe {
        windows_sys::Win32::Media::timeBeginPeriod(1);

        let h_instance = GetModuleHandleW(ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(internal_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: h_instance,
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: CLASS_NAME.as_ptr(),
            hIconSm: ptr::null_mut(),
        };
        RegisterClassExW(&wc);
    });
}

pub struct Win32EventDispatcher {
    pub internal_hwnd: HWND,
    pub wakeup_pending: Arc<AtomicBool>,
    pub native_filters: NativeEventFilterChain,
}

impl Default for Win32EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Win32EventDispatcher {
    pub fn new() -> Self {
        ensure_window_class_registered();

        let internal_hwnd = unsafe {
            let h_instance = GetModuleHandleW(ptr::null());
            CreateWindowExW(
                0,
                CLASS_NAME.as_ptr(),
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                h_instance,
                ptr::null(),
            )
        };

        if internal_hwnd.is_null() {
            panic!("Failed to create Win32 message-only window for Win32EventDispatcher");
        }

        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, internal_hwnd, 0, 0, PM_REMOVE) != 0 {
                DefWindowProcW(msg.hwnd, msg.message, msg.wParam, msg.lParam);
            }
        }

        Self {
            internal_hwnd,
            wakeup_pending: Arc::new(AtomicBool::new(false)),
            native_filters: NativeEventFilterChain::new(),
        }
    }

    pub fn wake_up(&self) {
        if !self.wakeup_pending.swap(true, Ordering::Release) {
            unsafe {
                PostMessageW(self.internal_hwnd, WM_QTRS_WAKEUP, 0, 0);
            }
        }
    }

    pub fn clone_handle(&self) -> Win32EventDispatcherHandle {
        Win32EventDispatcherHandle {
            internal_hwnd: self.internal_hwnd,
            wakeup_pending: Arc::clone(&self.wakeup_pending),
        }
    }

    pub fn install_native_event_filter(&mut self, filter: Box<dyn NativeEventFilter>) {
        self.native_filters.install(filter);
    }

    pub fn filter_native_event(
        &mut self,
        event_type: &str,
        msg: &NativeMessage,
        result: &mut isize,
    ) -> bool {
        self.native_filters.filter_native(event_type, msg, result)
    }

    pub fn process_events(
        &mut self,
        can_wait: bool,
        next_timer_timeout: Option<Duration>,
    ) -> DispatchResult {
        let deadline = next_timer_timeout.map(|d| std::time::Instant::now() + d);

        loop {
            let mut processed_ui_message = false;
            let mut awoken = false;

            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_QUIT {
                        return DispatchResult::Quit(msg.wParam as i32);
                    }
                    let mut native_result = 0isize;
                    if self.native_filters.filter_native(
                        "windows_generic_MSG",
                        &NativeMessage::Windows(&msg),
                        &mut native_result,
                    ) {
                        processed_ui_message = true;
                        continue;
                    }

                    if msg.hwnd == self.internal_hwnd {
                        if msg.message == WM_QTRS_WAKEUP {
                            self.wakeup_pending.store(false, Ordering::Release);
                            awoken = true;
                        } else if msg.message == WM_TIMER {
                            DispatchMessageW(&msg);
                        } else {
                            DefWindowProcW(msg.hwnd, msg.message, msg.wParam, msg.lParam);
                        }
                        continue;
                    }

                    processed_ui_message = true;
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            if awoken {
                return DispatchResult::Awoken;
            }

            if processed_ui_message {
                return DispatchResult::Normal;
            }

            if !can_wait {
                return DispatchResult::Timeout;
            }

            let timeout_ms = match deadline {
                Some(dl) => {
                    let now = std::time::Instant::now();
                    if now >= dl {
                        return DispatchResult::Timeout;
                    }
                    let remaining = dl - now;
                    remaining.as_millis().min(u32::MAX as u128) as u32
                }
                None => INFINITE,
            };

            let wait_ret = unsafe {
                MsgWaitForMultipleObjectsEx(0, ptr::null(), timeout_ms, QS_ALLINPUT, MWMO_ALERTABLE)
            };

            if wait_ret == WAIT_TIMEOUT {
                return DispatchResult::Timeout;
            }
        }
    }

    pub fn process_events_with_timers(
        &mut self,
        can_wait: bool,
        next_timer_timeout: Option<Duration>,
        registry: &mut TimerRegistry,
    ) -> DispatchResult {
        let result = self.process_events(can_wait, next_timer_timeout);
        self.send_timer_events(registry);
        result
    }

    pub fn register_timer(&mut self, entry: &TimerEntry) {
        if entry.interval_ms == 0 {
            return;
        }
        let id = entry.id.0 as usize;
        let interval = entry.interval_ms.min(u32::MAX as u64) as u32;
        unsafe {
            SetTimer(self.internal_hwnd, id, interval, None);
        }
    }

    pub fn unregister_timer(&mut self, entry: &TimerEntry) {
        if entry.interval_ms == 0 {
            return;
        }
        let id = if entry.win32_timer_id != 0 {
            entry.win32_timer_id as usize
        } else {
            entry.id.0 as usize
        };
        unsafe {
            KillTimer(self.internal_hwnd, id);
        }
    }

    pub fn send_timer_events(&mut self, registry: &mut TimerRegistry) {
        let pending = PENDING_WM_TIMERS.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for raw_id in pending {
            let id = TimerId(raw_id);

            let Some(entry) = registry.get(id) else {
                continue;
            };
            if entry.in_timer_event {
                continue;
            }
            let receiver = entry.receiver;
            let interval_ms = entry.interval_ms;
            let mut timer_type = entry.timer_type;
            let single_shot = entry.single_shot;

            if let Some(entry) = registry.get_mut(id) {
                entry.in_timer_event = true;
                let (adjusted, next_fire) =
                    calculate_next_timeout(&mut timer_type, interval_ms, current_time_ms());
                entry.timer_type = timer_type;
                entry.interval_ms = adjusted;
                entry.next_fire_ms = next_fire;
            }

            if single_shot {
                registry.unregister(id);
                unsafe {
                    KillTimer(self.internal_hwnd, raw_id as usize);
                }
                remove_posted_timer_event(self.internal_hwnd, raw_id);
            }

            let handled = crate::object::with_object_mut(receiver, |obj| {
                obj.timer_event(raw_id as u64);
            })
            .is_some();
            if !handled {
                crate::timer::dispatch_single_shot_callback(receiver);
            }

            if !single_shot {
                if let Some(entry) = registry.get_mut(id) {
                    entry.in_timer_event = false;
                } else {
                    unsafe {
                        KillTimer(self.internal_hwnd, raw_id as usize);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Win32EventDispatcherHandle {
    internal_hwnd: HWND,
    wakeup_pending: Arc<AtomicBool>,
}

impl Win32EventDispatcherHandle {
    pub fn wake_up(&self) {
        if !self.wakeup_pending.swap(true, Ordering::Release) {
            unsafe {
                PostMessageW(self.internal_hwnd, WM_QTRS_WAKEUP, 0, 0);
            }
        }
    }
}

impl EventDispatcherHandle for Win32EventDispatcherHandle {
    fn wake_up(&self) {
        self.wake_up();
    }
}

unsafe impl Send for Win32EventDispatcherHandle {}
unsafe impl Sync for Win32EventDispatcherHandle {}

impl EventDispatcher for Win32EventDispatcher {
    fn wake_up(&self) {
        self.wake_up();
    }

    fn clone_handle(&self) -> Arc<dyn EventDispatcherHandle> {
        Arc::new(self.clone_handle())
    }

    fn install_native_event_filter(&mut self, filter: Box<dyn NativeEventFilter>) {
        self.install_native_event_filter(filter);
    }

    fn filter_native_event(
        &mut self,
        event_type: &str,
        msg: &NativeMessage,
        result: &mut isize,
    ) -> bool {
        self.filter_native_event(event_type, msg, result)
    }

    fn process_events(
        &mut self,
        can_wait: bool,
        next_timer_timeout: Option<Duration>,
    ) -> DispatchResult {
        self.process_events(can_wait, next_timer_timeout)
    }

    fn register_timer(&mut self, entry: &TimerEntry) {
        self.register_timer(entry);
    }

    fn unregister_timer(&mut self, entry: &TimerEntry) {
        self.unregister_timer(entry);
    }

    fn send_timer_events(&mut self, registry: &mut TimerRegistry) {
        self.send_timer_events(registry);
    }
}

unsafe impl Send for Win32EventDispatcher {}
unsafe impl Sync for Win32EventDispatcher {}
impl Drop for Win32EventDispatcher {
    fn drop(&mut self) {
        if !self.internal_hwnd.is_null() {
            unsafe {
                DestroyWindow(self.internal_hwnd);
            }
            self.internal_hwnd = ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_thread_wakeup() {
        let mut dispatcher = Win32EventDispatcher::new();
        let dispatcher_clone = dispatcher.clone_handle();

        let start = std::time::Instant::now();

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            dispatcher_clone.wake_up();
        });

        let result = dispatcher.process_events(true, None);

        let elapsed = start.elapsed();
        assert_eq!(result, DispatchResult::Awoken);
        assert!(elapsed >= std::time::Duration::from_millis(45));
        assert!(elapsed < std::time::Duration::from_millis(150));
    }

    #[test]
    fn test_timeout_sleep() {
        let mut dispatcher = Win32EventDispatcher::new();
        let start = std::time::Instant::now();

        let result = dispatcher.process_events(true, Some(std::time::Duration::from_millis(50)));

        let elapsed = start.elapsed();
        assert_eq!(result, DispatchResult::Timeout);
        assert!(elapsed >= std::time::Duration::from_millis(45));
        assert!(elapsed < std::time::Duration::from_millis(100));
    }

    #[test]
    fn test_quit_message() {
        std::thread::spawn(|| {
            let mut dispatcher = Win32EventDispatcher::new();

            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage(42);
            }

            let result = dispatcher.process_events(false, None);
            assert_eq!(result, DispatchResult::Quit(42));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn test_high_frequency_wakeup_dedup() {
        let mut dispatcher = Win32EventDispatcher::new();
        let mut handles = Vec::new();

        for _ in 0..10 {
            let handle = dispatcher.clone_handle();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    handle.wake_up();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let result = dispatcher.process_events(false, None);
        assert_eq!(result, DispatchResult::Awoken);
        assert!(!dispatcher.wakeup_pending.load(Ordering::Acquire));
    }

    #[test]
    fn test_register_timer_wm_timer_dispatch() {
        use crate::object::ObjectData;
        use crate::timer::TimerType;
        use std::sync::{Arc, Mutex};

        struct MockReceiver {
            data: ObjectData,
            hits: Arc<Mutex<Vec<u64>>>,
        }

        impl QObject for MockReceiver {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }

            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn timer_event(&mut self, timer_id: u64) {
                self.hits.lock().unwrap().push(timer_id);
            }
        }

        let hits = Arc::new(Mutex::new(Vec::new()));
        let mut receiver = MockReceiver {
            data: ObjectData::new(ObjectId(7)),
            hits: Arc::clone(&hits),
        };
        let mut registry = TimerRegistry::new();
        let id = registry.register(ObjectId(7), 20, TimerType::Precise, true);
        let entry = registry.get(id).unwrap().clone();

        let mut dispatcher = Win32EventDispatcher::new();
        dispatcher.register_timer(&entry);
        unsafe { crate::object::register_qobject(&mut receiver) };

        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        while hits.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            let _ = dispatcher.process_events_with_timers(
                true,
                Some(Duration::from_millis(50)),
                &mut registry,
            );
        }

        let fired = hits.lock().unwrap().clone();
        assert!(!fired.is_empty(), "expected at least one WM_TIMER dispatch");
        assert!(fired.iter().all(|&h| h == id.0 as u64));

        if let Some(entry) = registry.get(id).cloned() {
            dispatcher.unregister_timer(&entry);
            registry.unregister(id);
        }
    }

    #[test]
    fn test_native_event_filter_intercept_in_dispatcher() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_USER};

        struct HotkeyFilter {
            intercepted: Arc<AtomicBool>,
        }
        impl NativeEventFilter for HotkeyFilter {
            fn native_event_filter(
                &mut self,
                event_type: &str,
                msg: &NativeMessage,
                _result: &mut isize,
            ) -> bool {
                if event_type == "windows_generic_MSG" {
                    if let NativeMessage::Windows(m) = msg {
                        if m.message == WM_USER + 77 {
                            self.intercepted.store(true, Ordering::Release);
                            return true;
                        }
                    }
                }
                false
            }
        }

        let mut dispatcher = Win32EventDispatcher::new();
        let intercepted = Arc::new(AtomicBool::new(false));

        dispatcher.install_native_event_filter(Box::new(HotkeyFilter {
            intercepted: Arc::clone(&intercepted),
        }));

        unsafe {
            PostMessageW(dispatcher.internal_hwnd, WM_USER + 77, 0, 0);
        }

        let res = dispatcher.process_events(false, None);
        assert!(matches!(
            res,
            DispatchResult::Normal | DispatchResult::Timeout
        ));

        assert!(intercepted.load(Ordering::Acquire));
    }
}
