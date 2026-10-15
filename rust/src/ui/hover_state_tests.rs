//! RC-91 (G8.4.d), HUD integration path: the leaf hover transitions that can be observed on the
//! HUD's real widget tree.
//!
//! Scope, stated so it is not over-read: this only verifies the observable hover state of the one
//! HUD widget that reacts to Enter/Leave, `LayoutToggleBtn` (`Button::state()`), when the cursor
//! moves between it and its sibling `HeaderTitle` and when it leaves the window. It does NOT verify
//! the order of Enter/Leave along the ancestor chain, nor that the common ancestor receives neither
//! event: no other HUD widget records Enter/Leave, so those are not observable here. They stay
//! covered by the generic-tree test `enter_leave_ancestor_chain_minus_common_ancestor`
//! (`ferrokit-widgets/tests/test_event_propagation.rs`).
//!
//! The moves go through `EventTreeDispatcher::dispatch_mouse_move` on the real root with the
//! parent links `adopt_tree` makes (the HUD's are made at its first paint). Programmatic dispatch:
//! no real mouse, no native input.
use std::sync::Arc;

use ferrokit_core::event::MouseButtons;
use ferrokit_core::event_loop::EventLoop;
use ferrokit_gui::geometry::primitives::Point;
use ferrokit_widgets::{hit_test, Button, ButtonState, EventTreeDispatcher, WidgetRef};
use parking_lot::Mutex;

use super::hud_window::HUDWindow;
use crate::config::Config;
use crate::refresh_controller::RefreshController;

fn hud() -> HUDWindow {
    let cfg = Config {
        ui_mode: "cards".into(),
        layout_mode: "vertical".into(),
        appearance: "dark".into(),
        window_x: Some(0),
        window_y: Some(0),
        vertical_width: 280,
        vertical_height: 463,
        ..Config::default()
    };
    let cfg = Arc::new(Mutex::new(cfg));
    let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
    let hud =
        HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();
    hud.window.root_widget().borrow().update_layout();
    // Parent links are made when the window paints; a hover only ever follows a paint.
    ferrokit_widgets::widget::adopt_tree(&hud.window.root_widget());
    hud
}

/// The centre of `w` in root (window) coordinates.
fn centre(w: &WidgetRef) -> Point {
    let g = w.borrow().geometry();
    let (mut x, mut y) = (g.x + g.width / 2, g.y + g.height / 2);
    let mut cur = w.borrow().parent_widget().and_then(|p| p.upgrade());
    while let Some(parent) = cur {
        if parent.borrow().parent_widget().is_none() {
            break; // the root: window coordinates start here
        }
        let pg = parent.borrow().geometry();
        x += pg.x;
        y += pg.y;
        cur = parent.borrow().parent_widget().and_then(|p| p.upgrade());
    }
    Point::new(x, y)
}

fn move_to(d: &mut EventTreeDispatcher, hud: &HUDWindow, target: &WidgetRef) {
    let root = hud.window.root_widget();
    let p = centre(target);
    // The point must really resolve to `target`; otherwise the step would not be the transition it names.
    let hit = hit_test(&root, p).expect("point is inside the window").0;
    assert_eq!(
        hit.borrow().id(),
        target.borrow().id(),
        "hit test at the centre of the widget returns another widget"
    );
    d.dispatch_mouse_move(&root, p, p, MouseButtons::NO_BUTTON);
}

fn state(btn: &WidgetRef) -> ButtonState {
    btn.borrow()
        .as_any()
        .downcast_ref::<Button>()
        .expect("layout_toggle_btn is a Button")
        .state()
}

/// Leaf hover transitions only (see the module docs): Normal -> Hovered on the toggle button, back to
/// Normal when the cursor moves to the sibling title, Hovered again on return, Normal when the cursor
/// leaves the window. Not an Enter/Leave ancestor-chain order or common-ancestor test.
#[test]
fn toggle_button_hover_state_follows_the_cursor_on_the_real_hud_tree() {
    let _el = EventLoop::new();
    let hud = hud();
    let btn = hud.layout_toggle_btn.clone();
    let title = hud.title_label.clone();
    let mut d = EventTreeDispatcher::new();

    assert_eq!(state(&btn), ButtonState::Normal, "initial");

    move_to(&mut d, &hud, &btn);
    assert_eq!(state(&btn), ButtonState::Hovered, "cursor on the button");

    move_to(&mut d, &hud, &title);
    assert_eq!(
        state(&btn),
        ButtonState::Normal,
        "cursor moved to the sibling title"
    );

    move_to(&mut d, &hud, &btn);
    assert_eq!(
        state(&btn),
        ButtonState::Hovered,
        "cursor back on the button"
    );

    d.handle_mouse_leave();
    assert_eq!(state(&btn), ButtonState::Normal, "cursor left the window");
}
