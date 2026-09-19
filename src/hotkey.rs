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
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct HotkeyManager {
    toggle_flag: Arc<AtomicBool>,
    clickthrough_flag: Arc<AtomicBool>,
    egui_ctx: Arc<Mutex<Option<eframe::egui::Context>>>,
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

impl HotkeyManager {
    pub fn start(hotkey_str: &str) -> Result<Self, String> {
        let toggle_flag = Arc::new(AtomicBool::new(false));
        let ct_flag = Arc::new(AtomicBool::new(false));
        let egui_ctx = Arc::new(Mutex::new(None));

        #[cfg(target_os = "windows")]
        let (mods, vk) = parse_hotkey(hotkey_str);

        #[cfg(target_os = "windows")]
        {
            let t_flag = Arc::clone(&toggle_flag);
            let c_flag = Arc::clone(&ct_flag);
            let ctx_clone = Arc::clone(&egui_ctx);
            let thread_id = Arc::new(AtomicU32::new(0));
            let tid_clone = Arc::clone(&thread_id);
            let handle = thread::Builder::new()
                .name("hotkey-win32".to_owned())
                .spawn(move || windows_hotkey_loop(t_flag, c_flag, ctx_clone, tid_clone, mods, vk))
                .map_err(|e| format!("Failed to start hotkey thread: {e}"))?;
            Ok(Self {
                toggle_flag,
                clickthrough_flag: ct_flag,
                egui_ctx,
                thread_id,
                _thread: Some(handle),
            })
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("[Hotkey] Global hotkeys not implemented on this platform");
            Ok(Self {
                toggle_flag,
                clickthrough_flag: ct_flag,
                egui_ctx,
                _thread: None,
            })
        }
    }

    /// Sets the egui Context so the hotkey thread can trigger 0ms immediate repaint on event
    pub fn set_context(&self, ctx: eframe::egui::Context) {
        if let Ok(mut guard) = self.egui_ctx.lock() {
            *guard = Some(ctx);
        }
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
    egui_ctx: Arc<Mutex<Option<eframe::egui::Context>>>,
    thread_id: Arc<AtomicU32>,
    mods: u32,
    vk: u32,
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
        if ok1 == 0 {
            warn!(
                "[Hotkey] Main hotkey registration failed (mods={:#x}, vk={:#x})",
                mods, vk
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
        if ok2 == 0 {
            warn!(
                "[Hotkey] Click-through hotkey registration failed (ct_mods={:#x}, vk={:#x})",
                ct_mods, vk
            );
        } else {
            info!(
                "[Hotkey] Click-through hotkey registered (ct_mods={:#x}, vk={:#x})",
                ct_mods, vk
            );
        }

        loop {
            let ret = GetMessageW(&mut msg, 0, 0, 0);
            if ret <= 0 {
                break;
            }
            if msg.message == WM_HOTKEY {
                if msg.wParam == HOTKEY_ID_TOGGLE as usize {
                    toggle_flag.store(true, Ordering::SeqCst);
                    if let Ok(guard) = egui_ctx.lock() {
                        if let Some(ctx) = guard.as_ref() {
                            ctx.request_repaint();
                        }
                    }
                } else if msg.wParam == HOTKEY_ID_CLICKTHROUGH as usize {
                    ct_flag.store(true, Ordering::SeqCst);
                    if let Ok(guard) = egui_ctx.lock() {
                        if let Some(ctx) = guard.as_ref() {
                            ctx.request_repaint();
                        }
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

#[cfg(test)]
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
}
