#![cfg(windows)]

use parking_lot::Mutex;
use qtrs_gui::geometry::primitives::Point;
use qtrs_widgets::{Action, Menu};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_ACTIVATEAPP, WM_CANCELMODE,
    WM_CAPTURECHANGED, WM_KILLFOCUS,
};

static POPUP_TEST_LOCK: Mutex<()> = Mutex::new(());

unsafe extern "system" fn find_process_window(hwnd: HWND, lparam: isize) -> i32 {
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == std::process::id() {
        let out = lparam as *mut HWND;
        *out = hwnd;
        return 0; // stop enumeration
    }
    1 // continue
}

fn get_popup_hwnd() -> HWND {
    let mut found = 0 as HWND;
    for _ in 0..50 {
        thread::sleep(Duration::from_millis(10));
        unsafe {
            EnumWindows(Some(find_process_window), &mut found as *mut _ as isize);
        }
        if found != 0 as HWND {
            return found;
        }
    }
    found
}

#[test]
fn test_popup_exits_on_kill_focus() {
    let _guard = POPUP_TEST_LOCK.lock();
    let mut menu = Menu::new("TestMenu");
    let act = Action::new_ref("Action 1");
    menu.add_action(act);

    let finished = Arc::new(AtomicBool::new(false));
    let finished_clone = Arc::clone(&finished);

    let bg = thread::spawn(move || {
        let hwnd = get_popup_hwnd();
        assert_ne!(hwnd, 0 as HWND, "Popup window should be found");
        unsafe {
            PostMessageW(hwnd, WM_KILLFOCUS, 0, 0);
        }
    });

    let chosen = menu.exec_popup(Point::new(100, 100));
    finished_clone.store(true, Ordering::SeqCst);
    assert!(chosen.is_none());

    bg.join().expect("Background thread failed");
    assert!(finished.load(Ordering::SeqCst));
}

#[test]
fn test_popup_exits_on_activateapp_deactivated() {
    let _guard = POPUP_TEST_LOCK.lock();
    let mut menu = Menu::new("TestMenu");
    let act = Action::new_ref("Action 1");
    menu.add_action(act);

    let bg = thread::spawn(move || {
        let hwnd = get_popup_hwnd();
        assert_ne!(hwnd, 0 as HWND, "Popup window should be found");
        unsafe {
            // wParam = 0 (FALSE / Deactivated)
            PostMessageW(hwnd, WM_ACTIVATEAPP, 0, 0);
        }
    });

    let chosen = menu.exec_popup(Point::new(100, 100));
    assert!(chosen.is_none());

    bg.join().expect("Background thread failed");
}

#[test]
fn test_popup_exits_on_capture_changed() {
    let _guard = POPUP_TEST_LOCK.lock();
    let mut menu = Menu::new("TestMenu");
    let act = Action::new_ref("Action 1");
    menu.add_action(act);

    let bg = thread::spawn(move || {
        let hwnd = get_popup_hwnd();
        assert_ne!(hwnd, 0 as HWND, "Popup window should be found");
        unsafe {
            // lParam = 0 (another window took capture)
            PostMessageW(hwnd, WM_CAPTURECHANGED, 0, 0);
        }
    });

    let chosen = menu.exec_popup(Point::new(100, 100));
    assert!(chosen.is_none());

    bg.join().expect("Background thread failed");
}

#[test]
fn test_popup_exits_on_cancel_mode() {
    let _guard = POPUP_TEST_LOCK.lock();
    let mut menu = Menu::new("TestMenu");
    let act = Action::new_ref("Action 1");
    menu.add_action(act);

    let bg = thread::spawn(move || {
        let hwnd = get_popup_hwnd();
        assert_ne!(hwnd, 0 as HWND, "Popup window should be found");
        unsafe {
            PostMessageW(hwnd, WM_CANCELMODE, 0, 0);
        }
    });

    let chosen = menu.exec_popup(Point::new(100, 100));
    assert!(chosen.is_none());

    bg.join().expect("Background thread failed");
}
