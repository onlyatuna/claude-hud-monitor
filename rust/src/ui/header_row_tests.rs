//! G9.7.b, sub-scope 2: the HUD header row when its optional items are shown and hidden.
//!
//! The expected rectangles are PySide6 6.11.2 measurements (Windows, DirectWrite) from
//! `rust/tools/pyside_probes/header_row_visibility.py`: the header of `hud_window.py:278-288`
//! with the real cards style sheet, 256 wide and 86 high (the vertical HUD's content width and its
//! header height at 463), at DPR 1.25 and 1.0. The ghost label is fixed at 18x18 on both sides: its
//! own size hint differs (Qt 17x16 / 21x20, ferrokit 18x18 / 18x17: the label has no `font-size`, so the
//! default font size decides, and ferrokit fixes it at 13px), a separate root cause that this file does
//! not test. Each step shows or hides one item with the call the
//! HUD itself makes (`set_visible`, `hud_window.rs:741,809,830`) and lays the window out again.
//! Rectangles are relative to the header row; a hidden item is `None`. The header's height is not what
//! is measured here: at DPR 1.25 it is the 86 that `hud_window.rs` already pins against PySide6, so
//! x, y, width and height are compared; at DPR 1.0 the Rust HUD's header is 89 high for the same
//! 463-high window and the probe's 86 does not apply, so only x and width are compared there.

use super::hud_window::HUDWindow;
use crate::config::Config;
use crate::refresh_controller::RefreshController;
use ferrokit_gui::geometry::primitives::Size;
use ferrokit_widgets::WidgetRef;
use parking_lot::Mutex;
use std::sync::Arc;

type Geom = Option<(i32, i32, i32, i32)>;

/// One row of the probe: dot, title, ghost, toggle, time.
type Row = [Geom; 5];

const fn g(x: i32, y: i32, w: i32, h: i32) -> Geom {
    Some((x, y, w, h))
}

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
    let mut hud =
        HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();
    // The tree's parent links are set at the first render (`adopt_tree`), as they are in a HUD that is
    // showing; before that a visibility change has no parent layout to request.
    hud.window.render_and_present();
    {
        let ghost = hud.ghost_label.borrow();
        ghost.set_minimum_size(Size::new(18, 18));
        ghost.set_maximum_size(Size::new(18, 18));
    }
    hud
}

fn rect(w: &WidgetRef, with_y: bool) -> Geom {
    let w = w.borrow();
    if !w.is_visible() {
        return None;
    }
    let r = w.geometry();
    Some(if with_y {
        (r.x, r.y, r.width, r.height)
    } else {
        (r.x, 0, r.width, 0)
    })
}

fn row(hud: &HUDWindow, with_y: bool) -> Row {
    // The `LayoutRequest` that `setVisible` posts is delivered by the event loop before a paint; the
    // window is not shown here, so deliver it the way the paint does (`do_render_and_present`).
    ferrokit_widgets::WidgetCommandQueue::flush_layouts();
    hud.window.root_widget().borrow().update_layout();
    [
        rect(&hud.status_dot, with_y),
        rect(&hud.title_label, with_y),
        rect(&hud.ghost_label, with_y),
        rect(&hud.layout_toggle_btn, with_y),
        rect(&hud.time_label, with_y),
    ]
}

/// Drops y and height from an expected row, as `rect` does for the actual one.
fn x_only(row: Row) -> Row {
    row.map(|g| g.map(|(x, _, w, _)| (x, 0, w, 0)))
}

fn run(dpr: f32, expected: [(&str, Row); 6]) {
    let _setup = crate::ui::test_support::CardsOracleSetup::at_dpr(dpr);
    let hud = hud();
    let with_y = dpr == 1.25;
    let row = |h: &HUDWindow| row(h, with_y);
    let set = |w: &WidgetRef, v: bool| w.borrow().set_visible(v);
    // The HUD starts with the ghost hidden and the toggle shown.
    let mut seen = vec![("start", row(&hud))];
    set(&hud.ghost_label, true);
    seen.push(("ghost_on", row(&hud)));
    set(&hud.layout_toggle_btn, false);
    seen.push(("toggle_off", row(&hud)));
    set(&hud.ghost_label, false);
    seen.push(("ghost_off", row(&hud)));
    set(&hud.layout_toggle_btn, true);
    seen.push(("toggle_on", row(&hud)));
    set(&hud.ghost_label, false);
    set(&hud.layout_toggle_btn, false);
    seen.push(("both_off", row(&hud)));
    for ((name, got), (want_name, want)) in seen.iter().zip(expected.iter()) {
        assert_eq!(name, want_name);
        let want = if with_y { *want } else { x_only(*want) };
        assert_eq!(*got, want, "step {name}: [dot, title, ghost, toggle, time]");
    }
}

#[test]
fn header_row_show_hide_matches_pyside6_at_dpr_1_25() {
    run(
        1.25,
        [
            (
                "start",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 144, 86),
                    None,
                    g(163, 32, 28, 22),
                    g(212, 0, 44, 86),
                ],
            ),
            (
                "ghost_on",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 135, 86),
                    g(154, 34, 18, 18),
                    g(178, 32, 28, 22),
                    g(212, 0, 44, 86),
                ],
            ),
            (
                "toggle_off",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 144, 86),
                    g(163, 34, 18, 18),
                    None,
                    g(212, 0, 44, 86),
                ],
            ),
            (
                "ghost_off",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 144, 86),
                    None,
                    None,
                    g(212, 0, 44, 86),
                ],
            ),
            (
                "toggle_on",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 144, 86),
                    None,
                    g(163, 32, 28, 22),
                    g(212, 0, 44, 86),
                ],
            ),
            (
                "both_off",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 144, 86),
                    None,
                    None,
                    g(212, 0, 44, 86),
                ],
            ),
        ],
    );
}

#[test]
fn header_row_show_hide_matches_pyside6_at_dpr_1_0() {
    run(
        1.0,
        [
            (
                "start",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 142, 86),
                    None,
                    g(161, 32, 28, 22),
                    g(216, 0, 40, 86),
                ],
            ),
            (
                "ghost_on",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 139, 86),
                    g(158, 34, 18, 18),
                    g(182, 32, 28, 22),
                    g(216, 0, 40, 86),
                ],
            ),
            (
                "toggle_off",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 142, 86),
                    g(161, 34, 18, 18),
                    None,
                    g(216, 0, 40, 86),
                ],
            ),
            (
                "ghost_off",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 142, 86),
                    None,
                    None,
                    g(216, 0, 40, 86),
                ],
            ),
            (
                "toggle_on",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 142, 86),
                    None,
                    g(161, 32, 28, 22),
                    g(216, 0, 40, 86),
                ],
            ),
            (
                "both_off",
                [
                    g(0, 0, 7, 86),
                    g(13, 0, 142, 86),
                    None,
                    None,
                    g(216, 0, 40, 86),
                ],
            ),
        ],
    );
}
