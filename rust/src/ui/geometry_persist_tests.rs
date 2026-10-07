//! G12.5.d / RC-17: Python persists the HUD geometry from the window's own events
//! (`hud_window.py:595-609,624-649`): `moveEvent`/`resizeEvent` restart a 250 ms single-shot,
//! `mouseReleaseEvent` and `closeEvent` persist at once. The events here are real Win32 messages
//! (`SetWindowPos`, `WM_LBUTTONUP`, `WM_CLOSE`) on the HUD's real window; the providers are stubs
//! (TI-01) and the debouncer is the real one. The 250 ms deadline is not an assertion target: the

use std::ptr::null_mut;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::hud_window::HUDWindow;
use crate::config::{Config, MIN_TABLE_HEIGHT, MIN_TABLE_WIDTH};
use crate::refresh_controller::RefreshController;

type Hwnd = *mut core::ffi::c_void;

#[link(name = "user32")]
extern "system" {
    fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn SendMessageW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
}

const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const WM_CLOSE: u32 = 0x0010;
const WM_LBUTTONUP: u32 = 0x0202;

struct Fx {
    hud: HUDWindow,
    cfg: Arc<Mutex<Config>>,
    hwnd: Hwnd,
}

fn fx() -> Fx {
    let mut c = Config::default();
    c.ui_mode = "table".into();
    c.window_x = Some(10);
    c.window_y = Some(10);
    c.table_width = 400;
    c.table_height = 500;
    let cfg = Arc::new(Mutex::new(c));
    let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
    let hud = HUDWindow::with_providers(
        Arc::clone(&cfg),
        ctrl,
        crate::providers::stub::stub_providers(),
    )
    .unwrap();
    let hwnd = hud.window.native_handle() as Hwnd;
    Fx { hud, cfg, hwnd }
}

fn move_to(f: &Fx, x: i32, y: i32) {
    let ok = unsafe {
        SetWindowPos(
            f.hwnd,
            null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
    assert_ne!(ok, 0);
}

fn resize_to(f: &Fx, w: i32, h: i32) {
    let ok = unsafe {
        SetWindowPos(
            f.hwnd,
            null_mut(),
            0,
            0,
            w,
            h,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
    assert_ne!(ok, 0);
}

/// The window's geometry as the HUD sees it (logical pixels: the OS sizes are physical).
fn geom(f: &Fx) -> (i32, i32, i32, i32) {
    let g = f.hud.window.geometry();
    (g.x, g.y, g.width, g.height)
}

/// What Python's `_persist_geometry` stores for the window as it is now: its position and its size
/// raised to the mode's minimum.
fn expected(f: &Fx) -> (i32, i32, i32, i32) {
    let g = geom(f);
    (
        g.0,
        g.1,
        g.2.max(MIN_TABLE_WIDTH as i32),
        g.3.max(MIN_TABLE_HEIGHT as i32),
    )
}

fn stored(f: &Fx) -> (i32, i32, i32, i32) {
    let c = f.cfg.lock();
    (
        c.window_x.unwrap(),
        c.window_y.unwrap(),
        c.table_width as i32,
        c.table_height as i32,
    )
}

fn wait_until(mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !done() {
        assert!(Instant::now() < end, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_move_updates_the_config_position_and_schedules_one_save() {
    let f = fx();
    move_to(&f, 123, 77);
    let g = geom(&f);
    assert_ne!((g.0, g.1), (10, 10), "the window moved");
    let s = stored(&f);
    assert_eq!((s.0, s.1), (g.0, g.1));
    assert!(
        f.hud.debouncer.is_pending(),
        "a move must schedule the debounced save"
    );
    assert_eq!(
        f.hud.debouncer.save_count(),
        0,
        "and not save synchronously"
    );
}

#[test]
fn mouse_release_persists_at_once_and_cancels_the_pending_save() {
    let f = fx();
    move_to(&f, 200, 150);
    assert!(f.hud.debouncer.is_pending());
    unsafe { SendMessageW(f.hwnd, WM_LBUTTONUP, 0, (30 << 16) | 30) };
    assert!(
        !f.hud.debouncer.is_pending(),
        "Python _persist_geometry stops the timer"
    );
    assert_eq!(f.hud.debouncer.save_count(), 1);
    assert_eq!(stored(&f), expected(&f));
    assert_ne!((geom(&f).0, geom(&f).1), (10, 10), "the window moved");
}

#[test]
fn close_persists_the_current_geometry() {
    let f = fx();
    move_to(&f, 55, 66);
    unsafe { SendMessageW(f.hwnd, WM_CLOSE, 0, 0) };
    assert_eq!(f.hud.debouncer.save_count(), 1);
    assert!(!f.hud.debouncer.is_pending());
    let g = geom(&f);
    let s = stored(&f);
    assert_eq!((s.0, s.1), (g.0, g.1));
    assert_ne!((g.0, g.1), (10, 10));
}

#[test]
fn a_size_below_the_minimum_is_stored_clamped() {
    let mut f = fx();
    // The HUD's own minimum keeps the OS from sending such a size; lift it to deliver one.
    f.hud.window.set_minimum_size(1, 1);
    resize_to(&f, 50, 40);
    let c = f.cfg.lock();
    assert_eq!(
        (c.table_width, c.table_height),
        (MIN_TABLE_WIDTH, MIN_TABLE_HEIGHT)
    );
}

#[test]
fn a_resize_is_saved_once_with_the_size_the_window_has() {
    // Opening the burst case at HUD level is not possible: one debug-build resize renders for
    // 180-300 ms, longer than the 250 ms deadline, so a burst here would legitimately save per
    // resize. The restart/coalescing semantics are `config.rs::test_resize_debounce_*`.
    let f = fx();
    resize_to(&f, 430, 530);
    wait_until(|| f.hud.debouncer.save_count() > 0);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(f.hud.debouncer.save_count(), 1);
    assert_eq!(stored(&f), expected(&f));
    assert_ne!(geom(&f).2, 400, "the window was resized");
}

#[test]
fn the_debounced_save_holds_the_latest_position_and_size() {
    let f = fx();
    move_to(&f, 300, 40);
    resize_to(&f, 450, 540);
    move_to(&f, 310, 45);
    wait_until(|| f.hud.debouncer.save_count() > 0);
    assert_eq!(stored(&f), expected(&f));
    assert_ne!((geom(&f).0, geom(&f).1), (10, 10));
}
