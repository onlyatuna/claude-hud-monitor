//! G11.1.d: a window that gains or loses the system focus becomes (or stops being)
//! `QApplication::activeWindow()`, receives `WindowActivate` / `WindowDeactivate`, and every
//! widget in it answers `isActiveWindow()` accordingly.
//!
//! Qt: `QApplicationPrivate::setActiveWindow` (qapplication.cpp:1816-1880) sets the active window
//! and sends the events; `QWidget::event` (qwidget.cpp:9317-9327) passes them to visible
//! non-window children; `QWidget::isActiveWindow` (qwidget.cpp:6967).
#![cfg(windows)]

use std::cell::RefCell;
use std::rc::Rc;

use qtrs_core::event::{Event, EventKind};
use qtrs_core::event_loop::EventLoop;
use qtrs_gui::geometry::primitives::Rect;
use qtrs_platform::window_system_interface::Delivery;
use qtrs_platform::{dispatch_window_system_event, WindowFlags, WindowSystemEvent};
use qtrs_widgets::application::Application;
use qtrs_widgets::widget::{EmptyWidget, WidgetRef};
use qtrs_widgets::window::Window;
use qtrs_widgets::{Label, Widget};

struct Fixture {
    _el: EventLoop,
    win: Box<Window>,
    hwnd: windows_sys::Win32::Foundation::HWND,
    child: WidgetRef,
    seen: Rc<RefCell<Vec<&'static str>>>,
}

fn fixture() -> Fixture {
    let el = EventLoop::new();
    let mut win = Box::new(
        Window::new(
            "Activation",
            Rect::new(0, 0, 200, 100),
            WindowFlags::empty(),
        )
        .expect("create window"),
    );
    // SAFETY: boxed (stable address), single thread, `Drop` unregisters.
    unsafe { win.register() };
    let hwnd = win.native_handle() as windows_sys::Win32::Foundation::HWND;
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&seen);
    win.set_window_event_handler(move |ev: &mut Event| match ev.kind {
        EventKind::WindowActivate => log.borrow_mut().push("activate"),
        EventKind::WindowDeactivate => log.borrow_mut().push("deactivate"),
        _ => {}
    });
    let child: WidgetRef = Rc::new(RefCell::new(Box::new(Label::new("child"))));
    let root = win.root_widget();
    child.borrow().set_parent_widget(Some(Rc::downgrade(&root)));
    root.borrow_mut().add_child(Rc::clone(&child));
    Fixture {
        _el: el,
        win,
        hwnd,
        child,
        seen,
    }
}

fn focus(fx: &Fixture, gained: bool) {
    let ev = if gained {
        WindowSystemEvent::FocusIn
    } else {
        WindowSystemEvent::FocusOut
    };
    dispatch_window_system_event(Delivery::Default, fx.hwnd, ev);
}

#[test]
fn gaining_and_losing_focus_sets_the_active_window_and_sends_the_events() {
    let fx = fixture();
    assert_eq!(Application::active_window(), None);
    assert!(!fx.child.borrow().is_active_window());

    focus(&fx, true);
    assert_eq!(Application::active_window(), Some(fx.win.id()));
    assert_eq!(*fx.seen.borrow(), ["activate"]);
    assert!(
        fx.child.borrow().is_active_window(),
        "a child follows its window"
    );
    assert!(fx.win.root_widget().borrow().is_active_window());

    // Already active: Qt's `setActiveWindow` returns early, no second event.
    focus(&fx, true);
    assert_eq!(*fx.seen.borrow(), ["activate"]);

    focus(&fx, false);
    assert_eq!(Application::active_window(), None);
    assert_eq!(*fx.seen.borrow(), ["activate", "deactivate"]);
    assert!(!fx.child.borrow().is_active_window());
}

#[test]
fn a_widget_outside_any_window_is_not_the_active_window() {
    let fx = fixture();
    focus(&fx, true);
    let orphan = EmptyWidget::new();
    assert!(!orphan.is_active_window());
    let _ = &fx.win;
}

#[test]
fn a_window_that_was_not_active_cannot_deactivate_the_active_one() {
    let a = fixture();
    let b = fixture();
    focus(&a, true);
    focus(&b, false);
    assert_eq!(Application::active_window(), Some(a.win.id()));
    assert!(b.seen.borrow().is_empty());
}
