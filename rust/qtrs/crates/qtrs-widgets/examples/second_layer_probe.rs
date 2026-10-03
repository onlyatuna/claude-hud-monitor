//! Harness probe (not production): a frameless layered qtrs `Window` titled "SLTEST" with an
//! opaque magenta rounded-rect fill (same root/paint-handler pattern as the HUD), driven by
//! `tools/second_layer_harness/driver.py`. `SLTRACE=1` prints the debug resize trace.
#![cfg(windows)]
use qtrs_core::event_loop::EventLoop;
use qtrs_gui::geometry::primitives::{Rect, RectF};
use qtrs_gui::paint::Brush;
use qtrs_gui::tiny_skia::Color;
use qtrs_platform::WindowFlags;
use qtrs_widgets::window::Window;
use qtrs_widgets::EmptyWidget;

#[link(name = "user32")]
extern "system" {
    fn SetProcessDpiAwarenessContext(value: isize) -> i32;
}

fn drain() {
    if std::env::var("SLTRACE").is_ok() {
        for e in qtrs_platform::resize_trace::take() {
            eprintln!("{:?} xy=({},{}) phys={}x{}", e.kind, e.x, e.y, e.physical_width, e.physical_height);
        }
    }
}

fn main() {
    // Same awareness as the HUD binary (per-monitor v2); without it DWM bitmap-scales the window.
    unsafe { SetProcessDpiAwarenessContext(-4) };
    if std::env::var("SLTRACE").is_ok() {
        qtrs_platform::resize_trace::set_enabled(true);
    }
    let mut el = EventLoop::new();
    let flags = WindowFlags::FRAMELESS | WindowFlags::LAYERED | WindowFlags::CUSTOM_FRAMELESS;
    let mut win = Box::new(Window::new("SLTEST", Rect::new(200, 150, 500, 400), flags).expect("window"));
    // SAFETY: boxed, single thread, `Drop` unregisters.
    unsafe { win.register() };
    let root = win.root_widget();
    if let Some(empty) = root.borrow_mut().as_any_mut().downcast_mut::<EmptyWidget>() {
        empty.set_paint_handler(move |p| {
            let (w, h) = (p.device().width(), p.device().height());
            drain();
            p.set_brush(Brush::Color(Color::from_rgba8(255, 0, 255, 255)));
            p.set_pen(None);
            p.draw_rounded_rect(RectF::new(0.0, 0.0, w, h), 8.0, 8.0);
        });
    }
    let hwnd = win.native_handle();
    win.set_mouse_press_handler(move |pos, button| {
        if button == qtrs_platform::MouseButton::Left {
            let edges = qtrs_platform::window::calc_frameless_edge(hwnd, pos, false);
            if !edges.is_empty() {
                qtrs_platform::window::post_system_resize(hwnd, edges);
                return true;
            }
        }
        false
    });
    win.set_mouse_move_handler(|_| drain());
    win.show();
    el.exec();
}
