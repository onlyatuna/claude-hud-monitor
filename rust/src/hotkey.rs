// src/hotkey.rs — Global hotkey manager (mirrors Python system/hotkey.py)
//
// Windows: Win32 RegisterHotKey in a dedicated background thread.
//   Alt+C  (id 9527) → toggle visibility
//   Alt+Shift+C (id 9528) → toggle click-through
//
// macOS: uses pynput equivalent (rdev crate) — TODO: implement via rdev.
// Linux: rdev or x11 shortcut libraries.
//
// Results are communicated back via atomic flag polling to avoid
// cross-thread GUI calls.

use log::{info, warn};
use parking_lot::Mutex;
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
type HotkeyNotifyCallback = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;

/// Outcome of one global-hotkey registration (`GlobalHotkeyManager.toggle_registered` /
/// `clickthrough_registered` in Python).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyStatus {
    Registered,
    /// The system refused the registration. The payload is `GetLastError()` taken right after
    /// `RegisterHotKey` (1409 = `ERROR_HOTKEY_ALREADY_REGISTERED`).
    Failed(u32),
    /// This platform has no global-hotkey backend.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    Unsupported,
}

/// What `HotkeyManager::start` achieved: one status per hotkey, with the label used in messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyRegistration {
    pub toggle: HotkeyStatus,
    pub toggle_label: String,
    pub clickthrough: HotkeyStatus,
    pub clickthrough_label: String,
}

impl HotkeyRegistration {
    /// Python `clickthrough_registered`.
    pub fn clickthrough_registered(&self) -> bool {
        self.clickthrough == HotkeyStatus::Registered
    }

    /// The tray messages Python shows for failed registrations, in its order: for each failed
    /// hotkey `hotkey_failed` (with the Win32 error) and then `unavailable`
    /// (`python/system/hotkey.py:70-86`). The caller appends the "use the tray" hint
    /// (`python/main.py:77-84`).
    pub fn failure_notices(&self) -> Vec<String> {
        let mut notices = Vec::new();
        if let HotkeyStatus::Failed(err) = self.toggle {
            notices.push(format!(
                "{} 全域快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用",
                self.toggle_label
            ));
            notices.push(format!("{} 已被占用，請使用系統匣操作", self.toggle_label));
        }
        if let HotkeyStatus::Failed(err) = self.clickthrough {
            notices.push(format!(
                "{} 穿透模式快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用",
                self.clickthrough_label
            ));
            notices.push(format!("{} 已被占用，請使用系統匣操作", self.clickthrough_label));
        }
        notices
    }
}

/// Whether the startup click-through mode may stay on (`_safe_init_click_through`,
/// `python/ui/hud_window.py:426-437`). Click-through makes the window ignore the mouse, and the
/// click-through hotkey is the way back out, so it is only allowed when that hotkey is registered.
/// No manager (hotkeys disabled or failed to start) means not registered.
pub fn click_through_startup_allowed(registration: Option<&HotkeyRegistration>) -> bool {
    registration.is_some_and(HotkeyRegistration::clickthrough_registered)
}

pub struct HotkeyManager {
    registration: HotkeyRegistration,
    toggle_flag: Arc<AtomicBool>,
    clickthrough_flag: Arc<AtomicBool>,
    #[allow(dead_code)]
    notify_cb: HotkeyNotifyCallback,
    #[cfg(target_os = "windows")]
    thread_id: Arc<AtomicU32>,
    _thread: Option<thread::JoinHandle<()>>,
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        {
            let tid = self.thread_id.load(Ordering::SeqCst);
            if tid != 0 {
                #[link(name = "user32")]
                extern "system" {
                    fn PostThreadMessageW(
                        idThread: u32,
                        Msg: u32,
                        wParam: usize,
                        lParam: isize,
                    ) -> i32;
                }
                const WM_QUIT: u32 = 0x0012;
                unsafe {
                    PostThreadMessageW(tid, WM_QUIT, 0, 0);
                }
            }
        }
        if let Some(h) = self._thread.take() {
            let _ = h.join();
        }
        info!("[Hotkey] HotkeyManager dropped and worker thread joined");
    }
}

/// Parses a hotkey string like "Alt+C", "Ctrl+Shift+H", "F10" into (fsModifiers, vkCode).
#[cfg(target_os = "windows")]
pub fn parse_hotkey(s: &str) -> (u32, u32) {
    let mut mods = 0u32;
    let mut vk = 0u32;

    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_WIN: u32 = 0x0008;

    for part in s.split('+').map(|p| p.trim()) {
        match part.to_lowercase().as_str() {
            "alt" => mods |= MOD_ALT,
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "shift" => mods |= MOD_SHIFT,
            "win" | "windows" | "super" => mods |= MOD_WIN,
            other => {
                if other.len() == 1 {
                    let ch = other.chars().next().unwrap().to_ascii_uppercase();
                    if ch.is_ascii_alphanumeric() {
                        vk = ch as u32;
                    }
                } else if other.starts_with('f') && other.len() >= 2 {
                    if let Ok(num) = other[1..].parse::<u32>() {
                        if (1..=24).contains(&num) {
                            vk = 0x70 + (num - 1); // VK_F1 = 0x70
                        }
                    }
                } else {
                    match other {
                        "space" => vk = 0x20,
                        "tab" => vk = 0x09,
                        "esc" | "escape" => vk = 0x1B,
                        _ => {}
                    }
                }
            }
        }
    }

    if vk == 0 {
        // Fallback default: Alt+C
        (MOD_ALT, b'C' as u32)
    } else {
        (mods, vk)
    }
}

/// "Alt+Shift+C"-style name of a `RegisterHotKey` modifier/key pair, for messages.
#[cfg(target_os = "windows")]
pub fn format_hotkey(mods: u32, vk: u32) -> String {
    let mut s = String::new();
    for (bit, name) in [(0x0002, "Ctrl"), (0x0001, "Alt"), (0x0004, "Shift"), (0x0008, "Win")] {
        if mods & bit != 0 {
            s.push_str(name);
            s.push('+');
        }
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => s.push(vk as u8 as char),
        0x70..=0x87 => s.push_str(&format!("F{}", vk - 0x70 + 1)),
        0x20 => s.push_str("Space"),
        0x09 => s.push_str("Tab"),
        0x1B => s.push_str("Esc"),
        other => s.push_str(&format!("VK {other:#04X}")),
    }
    s
}

impl HotkeyManager {
    /// Registration status of the global hotkeys. `start` returns once the registrations have
    /// been attempted, so this is final; a refused registration does not make `start` fail, since
    /// the other hotkey may still work (Python keeps it too).
    pub fn registration(&self) -> &HotkeyRegistration {
        &self.registration
    }

    pub fn start(hotkey_str: &str) -> Result<Self, String> {
        let toggle_flag = Arc::new(AtomicBool::new(false));
        let ct_flag = Arc::new(AtomicBool::new(false));
        let notify_cb = Arc::new(Mutex::new(None));
        #[cfg(target_os = "windows")]
        let (mods, vk) = parse_hotkey(hotkey_str);
        #[cfg(not(target_os = "windows"))]
        let _ = hotkey_str;

        #[cfg(target_os = "windows")]
        {
            let t_flag = Arc::clone(&toggle_flag);
            let c_flag = Arc::clone(&ct_flag);
            let notify_clone = Arc::clone(&notify_cb);
            let thread_id = Arc::new(AtomicU32::new(0));
            let tid_clone = Arc::clone(&thread_id);
            let (registered_tx, registered_rx) = std::sync::mpsc::channel();
            let handle = thread::Builder::new()
                .name("hotkey-win32".to_owned())
                .spawn(move || {
                    windows_hotkey_loop(
                        t_flag,
                        c_flag,
                        notify_clone,
                        tid_clone,
                        mods,
                        vk,
                        registered_tx,
                    )
                })
                .map_err(|e| format!("Failed to start hotkey thread: {e}"))?;
            // The worker reports once both `RegisterHotKey` calls are done (Python waits on
            // `_ready_event`). The channel closes if the thread ends before reporting.
            let registration = registered_rx
                .recv()
                .map_err(|_| "hotkey thread ended before registering".to_owned())?;
            Ok(Self {
                registration,
                toggle_flag,
                clickthrough_flag: ct_flag,
                notify_cb,
                thread_id,
                _thread: Some(handle),
            })
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("[Hotkey] Global hotkeys not implemented on this platform");
            Ok(Self {
                registration: HotkeyRegistration {
                    toggle: HotkeyStatus::Unsupported,
                    toggle_label: String::new(),
                    clickthrough: HotkeyStatus::Unsupported,
                    clickthrough_label: String::new(),
                },
                toggle_flag,
                clickthrough_flag: ct_flag,
                notify_cb,
                _thread: None,
            })
        }
    }

    /// Sets notification callback triggered immediately on hotkey press
    #[allow(dead_code)]
    pub fn set_notify_callback<F: Fn() + Send + Sync + 'static>(&self, cb: F) {
        *self.notify_cb.lock() = Some(Arc::new(cb));
    }
    /// Returns true and clears the flag if a toggle event is pending.
    pub fn poll_toggle(&self) -> bool {
        self.toggle_flag
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Returns true and clears the flag if a click-through event is pending.
    pub fn poll_clickthrough(&self) -> bool {
        self.clickthrough_flag
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

/// Windows-specific Win32 RegisterHotKey message loop.
/// Computes modifiers for the secondary click-through hotkey such that it never conflicts
/// with the primary hotkey even when Shift, Ctrl, or Alt are already present.
#[cfg(target_os = "windows")]
pub fn compute_ct_mods(mods: u32) -> u32 {
    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;

    if (mods & MOD_SHIFT) == 0 {
        mods | MOD_SHIFT
    } else if (mods & MOD_CONTROL) == 0 {
        mods | MOD_CONTROL
    } else if (mods & MOD_ALT) == 0 {
        mods | MOD_ALT
    } else {
        mods ^ MOD_SHIFT // If all are selected, invert Shift
    }
}

#[cfg(target_os = "windows")]
fn windows_hotkey_loop(
    toggle_flag: Arc<AtomicBool>,
    ct_flag: Arc<AtomicBool>,
    notify_cb: HotkeyNotifyCallback,
    thread_id: Arc<AtomicU32>,
    mods: u32,
    vk: u32,
    registered: std::sync::mpsc::Sender<HotkeyRegistration>,
) {
    use std::mem::MaybeUninit;

    #[link(name = "user32")]
    extern "system" {
        fn RegisterHotKey(hWnd: isize, id: i32, fsModifiers: u32, vk: u32) -> i32;
        fn UnregisterHotKey(hWnd: isize, id: i32) -> i32;
        fn GetMessageW(lpMsg: *mut MSG, hWnd: isize, wMsgFilterMin: u32, wMsgFilterMax: u32)
            -> i32;
        fn PeekMessageW(
            lpMsg: *mut MSG,
            hWnd: isize,
            wMsgFilterMin: u32,
            wMsgFilterMax: u32,
            wRemoveMsg: u32,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
        fn GetLastError() -> u32;
    }

    const WM_HOTKEY: u32 = 0x0312;
    const WM_QUIT: u32 = 0x0012;
    const MOD_NOREPEAT: u32 = 0x4000;
    const HOTKEY_ID_TOGGLE: i32 = 9527;
    const HOTKEY_ID_CLICKTHROUGH: i32 = 9528;

    // Force message queue creation before publishing Thread ID to eliminate race condition on fast exit
    unsafe {
        let mut msg: MSG = MaybeUninit::zeroed().assume_init();
        PeekMessageW(&mut msg, 0, 0, 0, 0); // PM_NOREMOVE=0 creates thread message queue
        thread_id.store(GetCurrentThreadId(), Ordering::SeqCst);

        let ok1 = RegisterHotKey(0, HOTKEY_ID_TOGGLE, mods | MOD_NOREPEAT, vk);
        // Read the error before anything else (the logger) can overwrite it.
        let toggle = if ok1 == 0 {
            HotkeyStatus::Failed(GetLastError())
        } else {
            HotkeyStatus::Registered
        };
        if ok1 == 0 {
            warn!(
                "[Hotkey] Main hotkey registration failed (mods={:#x}, vk={:#x}, {:?})",
                mods, vk, toggle
            );
        } else {
            info!(
                "[Hotkey] Main hotkey registered (mods={:#x}, vk={:#x})",
                mods, vk
            );
        }

        // Secondary hotkey: toggle click-through (distinct non-conflicting modifier)
        let ct_mods = compute_ct_mods(mods);
        let ok2 = RegisterHotKey(0, HOTKEY_ID_CLICKTHROUGH, ct_mods | MOD_NOREPEAT, vk);
        let clickthrough = if ok2 == 0 {
            HotkeyStatus::Failed(GetLastError())
        } else {
            HotkeyStatus::Registered
        };
        if ok2 == 0 {
            warn!(
                "[Hotkey] Click-through hotkey registration failed (ct_mods={:#x}, vk={:#x}, {:?})",
                ct_mods, vk, clickthrough
            );
        } else {
            info!(
                "[Hotkey] Click-through hotkey registered (ct_mods={:#x}, vk={:#x})",
                ct_mods, vk
            );
        }

        // `start` blocks on this; a dropped receiver only means nobody is waiting any more.
        let _ = registered.send(HotkeyRegistration {
            toggle,
            toggle_label: format_hotkey(mods, vk),
            clickthrough,
            clickthrough_label: format_hotkey(ct_mods, vk),
        });

        loop {
            let ret = GetMessageW(&mut msg, 0, 0, 0);
            if ret <= 0 {
                break;
            }
            if msg.message == WM_HOTKEY {
                if msg.wParam == HOTKEY_ID_TOGGLE as usize {
                    toggle_flag.store(true, Ordering::SeqCst);
                    if let Some(cb) = notify_cb.lock().as_ref() {
                        cb();
                    }
                } else if msg.wParam == HOTKEY_ID_CLICKTHROUGH as usize {
                    ct_flag.store(true, Ordering::SeqCst);
                    if let Some(cb) = notify_cb.lock().as_ref() {
                        cb();
                    }
                }
            } else if msg.message == WM_QUIT {
                break;
            }
        }

        let _ = UnregisterHotKey(0, HOTKEY_ID_TOGGLE);
        let _ = UnregisterHotKey(0, HOTKEY_ID_CLICKTHROUGH);
        info!("[Hotkey] Hotkeys unregistered and worker loop terminated");
    }
}

// ── Win32 FFI declarations ────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[allow(non_snake_case)]
struct MSG {
    hwnd: usize,
    message: u32,
    wParam: usize,
    lParam: isize,
    time: u32,
    pt: POINT,
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotkey() {
        const MOD_ALT: u32 = 0x0001;
        const MOD_CONTROL: u32 = 0x0002;
        const MOD_SHIFT: u32 = 0x0004;

        // Default "Alt+C"
        let (m, k) = parse_hotkey("Alt+C");
        assert_eq!(m, MOD_ALT);
        assert_eq!(k, b'C' as u32);

        // "Ctrl+Shift+H"
        let (m2, k2) = parse_hotkey("Ctrl+Shift+H");
        assert_eq!(m2, MOD_CONTROL | MOD_SHIFT);
        assert_eq!(k2, b'H' as u32);

        // Function key "F12"
        let (m3, k3) = parse_hotkey("F12");
        assert_eq!(m3, 0);
        assert_eq!(k3, 0x70 + 11);

        // Invalid fallback to Alt+C
        let (m4, k4) = parse_hotkey("InvalidKeyString");
        assert_eq!(m4, MOD_ALT);
        assert_eq!(k4, b'C' as u32);
    }

    #[test]
    fn test_compute_ct_mods() {
        const MOD_ALT: u32 = 0x0001;
        const MOD_CONTROL: u32 = 0x0002;
        const MOD_SHIFT: u32 = 0x0004;

        // 1. When Shift is absent, Shift is added
        assert_eq!(compute_ct_mods(MOD_ALT), MOD_ALT | MOD_SHIFT);
        assert_eq!(compute_ct_mods(MOD_CONTROL), MOD_CONTROL | MOD_SHIFT);

        // 2. When Shift is present, Control is added
        assert_eq!(
            compute_ct_mods(MOD_SHIFT | MOD_ALT),
            MOD_SHIFT | MOD_ALT | MOD_CONTROL
        );

        // 3. When Shift and Control are present, Alt is added (crucial fix for Ctrl+Shift+C)
        assert_eq!(
            compute_ct_mods(MOD_CONTROL | MOD_SHIFT),
            MOD_CONTROL | MOD_SHIFT | MOD_ALT
        );
        assert_ne!(
            compute_ct_mods(MOD_CONTROL | MOD_SHIFT),
            MOD_CONTROL | MOD_SHIFT
        );

        // 4. When all three are present, Shift is toggled off
        let all = MOD_ALT | MOD_CONTROL | MOD_SHIFT;
        assert_eq!(compute_ct_mods(all), MOD_ALT | MOD_CONTROL);
        assert_ne!(compute_ct_mods(all), all);
    }

    // ---- registration result (RC-12) -----------------------------------------------------

    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const VK_F21: u32 = 0x70 + 20;
    const VK_F22: u32 = 0x70 + 21;
    const ERROR_HOTKEY_ALREADY_REGISTERED: u32 = 1409;

    /// Another registration of `mods + vk` in this session, held for the guard's lifetime. This is
    /// what "the hotkey is taken by another program" looks like to `RegisterHotKey`.
    struct Squatter(i32);

    impl Squatter {
        fn new(id: i32, mods: u32, vk: u32) -> Self {
            #[link(name = "user32")]
            extern "system" {
                fn RegisterHotKey(hWnd: isize, id: i32, fsModifiers: u32, vk: u32) -> i32;
            }
            // Same flags as the manager uses: `MOD_NOREPEAT` is part of what conflicts.
            assert_ne!(unsafe { RegisterHotKey(0, id, mods | 0x4000, vk) }, 0, "test setup: combo is free");
            Squatter(id)
        }
    }

    impl Drop for Squatter {
        fn drop(&mut self) {
            #[link(name = "user32")]
            extern "system" {
                fn UnregisterHotKey(hWnd: isize, id: i32) -> i32;
            }
            unsafe { UnregisterHotKey(0, self.0) };
        }
    }

    #[test]
    fn start_reports_that_both_hotkeys_registered() {
        let hk = HotkeyManager::start("Ctrl+Alt+Shift+F23").expect("start");
        let r = hk.registration();
        assert_eq!(r.toggle, HotkeyStatus::Registered);
        assert_eq!(r.clickthrough, HotkeyStatus::Registered);
        assert!(r.failure_notices().is_empty());
        assert!(click_through_startup_allowed(Some(r)));
    }

    #[test]
    fn start_reports_a_taken_toggle_hotkey_and_keeps_the_other() {
        // Toggle = Ctrl+Alt+Shift+F22 is taken; click-through = Ctrl+Alt+F22 is free.
        let _squat = Squatter::new(1, MOD_CONTROL | MOD_ALT | MOD_SHIFT, VK_F22);
        let hk = HotkeyManager::start("Ctrl+Alt+Shift+F22").expect("start still returns the manager");
        let r = hk.registration();
        assert_eq!(r.toggle, HotkeyStatus::Failed(ERROR_HOTKEY_ALREADY_REGISTERED));
        assert_eq!(r.clickthrough, HotkeyStatus::Registered, "an independent hotkey is not given up");
        assert!(click_through_startup_allowed(Some(r)), "the click-through way out still exists");
        let notices = r.failure_notices();
        assert_eq!(notices.len(), 2);
        assert_eq!(
            notices[0],
            "Ctrl+Alt+Shift+F22 全域快捷鍵註冊失敗 (Win32 Error: 1409)，可能已被其他程式佔用"
        );
    }

    #[test]
    fn a_taken_click_through_hotkey_disables_startup_click_through() {
        // Toggle = Ctrl+Alt+F21 is free; click-through = Ctrl+Alt+Shift+F21 is taken.
        let _squat = Squatter::new(2, MOD_CONTROL | MOD_ALT | MOD_SHIFT, VK_F21);
        let hk = HotkeyManager::start("Ctrl+Alt+F21").expect("start");
        let r = hk.registration();
        assert_eq!(r.toggle, HotkeyStatus::Registered);
        assert_eq!(r.clickthrough, HotkeyStatus::Failed(ERROR_HOTKEY_ALREADY_REGISTERED));
        assert!(!r.clickthrough_registered());
        assert!(
            !click_through_startup_allowed(Some(r)),
            "the window must not start click-through without a way back out"
        );
        assert_eq!(
            r.failure_notices()[0],
            "Ctrl+Alt+Shift+F21 穿透模式快捷鍵註冊失敗 (Win32 Error: 1409)，可能已被其他程式佔用"
        );
    }

    #[test]
    fn failure_notices_follow_python_order_and_text() {
        // python/system/hotkey.py:74,77,83,86: per failed hotkey, `hotkey_failed` then `unavailable`.
        let r = HotkeyRegistration {
            toggle: HotkeyStatus::Failed(1409),
            toggle_label: "Alt+C".into(),
            clickthrough: HotkeyStatus::Failed(5),
            clickthrough_label: "Alt+Shift+C".into(),
        };
        assert_eq!(
            r.failure_notices(),
            vec![
                "Alt+C 全域快捷鍵註冊失敗 (Win32 Error: 1409)，可能已被其他程式佔用",
                "Alt+C 已被占用，請使用系統匣操作",
                "Alt+Shift+C 穿透模式快捷鍵註冊失敗 (Win32 Error: 5)，可能已被其他程式佔用",
                "Alt+Shift+C 已被占用，請使用系統匣操作",
            ]
        );
    }

    #[test]
    fn click_through_needs_a_registered_click_through_hotkey() {
        let reg = |ct| HotkeyRegistration {
            toggle: HotkeyStatus::Registered,
            toggle_label: String::new(),
            clickthrough: ct,
            clickthrough_label: String::new(),
        };
        assert!(click_through_startup_allowed(Some(&reg(HotkeyStatus::Registered))));
        assert!(!click_through_startup_allowed(Some(&reg(HotkeyStatus::Failed(1409)))));
        assert!(!click_through_startup_allowed(Some(&reg(HotkeyStatus::Unsupported))));
        assert!(!click_through_startup_allowed(None), "hotkeys disabled or not started");
    }

    #[test]
    fn hotkey_labels_name_modifiers_then_key() {
        assert_eq!(format_hotkey(MOD_ALT, b'C' as u32), "Alt+C");
        assert_eq!(format_hotkey(MOD_ALT | MOD_SHIFT, b'C' as u32), "Alt+Shift+C");
        assert_eq!(format_hotkey(MOD_CONTROL | MOD_SHIFT | MOD_ALT, 0x70 + 11), "Ctrl+Alt+Shift+F12");
        assert_eq!(format_hotkey(0, 0x20), "Space");
    }
}
