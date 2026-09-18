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
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;

pub struct HotkeyManager {
    toggle_flag: Arc<AtomicBool>,
    clickthrough_flag: Arc<AtomicBool>,
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
                    fn PostThreadMessageW(idThread: u32, Msg: u32, wParam: usize, lParam: isize) -> i32;
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

impl HotkeyManager {
    pub fn start() -> Result<Self, String> {
        let toggle_flag = Arc::new(AtomicBool::new(false));
        let ct_flag = Arc::new(AtomicBool::new(false));

        #[cfg(target_os = "windows")]
        {
            let t_flag = Arc::clone(&toggle_flag);
            let c_flag = Arc::clone(&ct_flag);
            let thread_id = Arc::new(AtomicU32::new(0));
            let tid_clone = Arc::clone(&thread_id);
            let handle = thread::Builder::new()
                .name("hotkey-win32".to_owned())
                .spawn(move || windows_hotkey_loop(t_flag, c_flag, tid_clone))
                .map_err(|e| format!("Failed to start hotkey thread: {e}"))?;
            return Ok(Self {
                toggle_flag,
                clickthrough_flag: ct_flag,
                thread_id,
                _thread: Some(handle),
            });
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("[Hotkey] Global hotkeys not implemented on this platform");
            Ok(Self {
                toggle_flag,
                clickthrough_flag: ct_flag,
                _thread: None,
            })
        }
    }

    /// Returns true and clears the flag if a toggle event is pending.
    pub fn poll_toggle(&self) -> bool {
        self.toggle_flag.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok()
    }

    /// Returns true and clears the flag if a click-through event is pending.
    pub fn poll_clickthrough(&self) -> bool {
        self.clickthrough_flag.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok()
    }
}

/// Windows-specific Win32 RegisterHotKey message loop.
#[cfg(target_os = "windows")]
fn windows_hotkey_loop(toggle_flag: Arc<AtomicBool>, ct_flag: Arc<AtomicBool>, thread_id: Arc<AtomicU32>) {
    use std::mem::MaybeUninit;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    const WM_HOTKEY: u32 = 0x0312;
    const WM_QUIT: u32 = 0x0012;
    const MOD_ALT: u32 = 0x0001;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_NOREPEAT: u32 = 0x4000;
    const HOTKEY_ID_TOGGLE: i32 = 9527;
    const HOTKEY_ID_CLICKTHROUGH: i32 = 9528;
    const VK_C: u32 = b'C' as u32;

    // Force message queue creation and store Thread ID
    unsafe {
        thread_id.store(GetCurrentThreadId(), Ordering::SeqCst);
        let mut msg: MSG = MaybeUninit::zeroed().assume_init();
        PeekMessageW(&mut msg, 0, 0, 0, 0); // PM_NOREMOVE=0

        let ok1 = RegisterHotKey(0, HOTKEY_ID_TOGGLE, MOD_ALT | MOD_NOREPEAT, VK_C);
        if ok1 == 0 {
            warn!("[Hotkey] Alt+C registration failed");
        } else {
            info!("[Hotkey] Alt+C registered");
        }

        let ok2 = RegisterHotKey(0, HOTKEY_ID_CLICKTHROUGH, MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, VK_C);
        if ok2 == 0 {
            warn!("[Hotkey] Alt+Shift+C registration failed");
        } else {
            info!("[Hotkey] Alt+Shift+C registered");
        }

        loop {
            let ret = GetMessageW(&mut msg, 0, 0, 0);
            if ret <= 0 { break; }
            if msg.message == WM_HOTKEY {
                if msg.wParam == HOTKEY_ID_TOGGLE as usize {
                    toggle_flag.store(true, Ordering::SeqCst);
                } else if msg.wParam == HOTKEY_ID_CLICKTHROUGH as usize {
                    ct_flag.store(true, Ordering::SeqCst);
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
struct POINT { x: i32, y: i32 }

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

#[cfg(target_os = "windows")]
extern "system" {
    fn RegisterHotKey(hWnd: usize, id: i32, fsModifiers: u32, vk: u32) -> i32;
    fn UnregisterHotKey(hWnd: usize, id: i32) -> i32;
    fn GetMessageW(lpMsg: *mut MSG, hWnd: usize, wMsgFilterMin: u32, wMsgFilterMax: u32) -> i32;
    fn PeekMessageW(lpMsg: *mut MSG, hWnd: usize, wMsgFilterMin: u32, wMsgFilterMax: u32, wRemoveMsg: u32) -> i32;
}
