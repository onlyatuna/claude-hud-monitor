//! G8.8.a / G12.5.i: the HUD's Python `setToolTip` calls, through the real qtrs tool tip protocol.
//!
//! Python sources: `provider_card.py:125`, `usage_table.py:318,328,339,373-375`,
//! `hud_window.py:155,160`. Hover goes through `EventTreeDispatcher` on the HUD's real widget tree
//! with real timers (700 ms wake-up, RC-11c); the providers are stubs (TI-01). The tip's
//! appearance is not asserted.
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Local, TimeZone, Utc};
use parking_lot::Mutex;
use qtrs_core::event_loop::EventLoop;
use qtrs_core::event::MouseButtons;
use qtrs_gui::geometry::primitives::Point;
use qtrs_widgets::tooltip::ToolTip;
use qtrs_widgets::{EventTreeDispatcher, WidgetRef};

use super::hud_window::HUDWindow;
use crate::config::Config;
use crate::providers::base::UsageMetrics;
use crate::refresh_controller::RefreshController;

const WEEK_NOTE: &str = "照目前速度，重設前不會用完";

fn hud(ui_mode: &str) -> HUDWindow {
    let mut cfg = Config::default();
    cfg.ui_mode = ui_mode.into();
    cfg.layout_mode = "vertical".into();
    cfg.appearance = "dark".into();
    cfg.window_x = Some(0);
    cfg.window_y = Some(0);
    cfg.vertical_width = 280;
    cfg.vertical_height = 463;
    cfg.table_width = 400;
    cfg.table_height = 500;
    let cfg = Arc::new(Mutex::new(cfg));
    let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
    let hud = HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();
    // A test window is never the active window; Qt shows tips for an inactive one only with this.
    hud.window.root_widget().borrow().set_always_show_tool_tips(true);
    hud.window.root_widget().borrow().update_layout();
    // Parent links are made when the window paints; a hover only ever follows a paint.
    qtrs_widgets::widget::adopt_tree(&hud.window.root_widget());
    hud
}

/// 30 % used at half the window: on pace, so both run-out notes are "will not run out".
fn healthy(id: &str) -> UsageMetrics {
    UsageMetrics {
        provider_id: id.into(),
        metric1_val: Some(30.0),
        metric1_text: "30%".into(),
        metric1_reset: Some(Utc::now() + chrono::Duration::minutes(150)),
        metric2_val: Some(30.0),
        metric2_text: "30%".into(),
        metric2_reset: Some(Utc::now() + chrono::Duration::hours(84)),
        ..Default::default()
    }
}

fn offline(id: &str, error: &str) -> UsageMetrics {
    UsageMetrics { provider_id: id.into(), error: Some(error.into()), ..Default::default() }
}

fn stale(id: &str, error: &str) -> UsageMetrics {
    UsageMetrics {
        error: Some(error.into()),
        stale: true,
        last_success: Some(Utc.with_ymd_and_hms(2024, 3, 5, 14, 7, 9).unwrap()),
        ..healthy(id)
    }
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

/// Where the OS cursor is: the global position of a real hover. A tip is placed beside the cursor
/// (`place_tip`), so it never ends up under it. A made-up position can put the tip under the real
/// cursor of the machine running the test (a CI desktop parks it mid-screen); the tip window then
/// gets real mouse enter/move/leave messages, which go through the same tool tip controller and
/// cancel the wake-up timer of the next hover.
fn real_cursor() -> Point {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut pt = POINT { x: 0, y: 0 };
        // SAFETY: a valid out pointer.
        if unsafe { GetCursorPos(&mut pt) } != 0 {
            return Point::new(pt.x, pt.y);
        }
    }
    Point::new(300, 300)
}

fn hover(d: &mut EventTreeDispatcher, hud: &HUDWindow, target: &WidgetRef) {
    let p = centre(target);
    d.dispatch_mouse_move(&hud.window.root_widget(), p, real_cursor(), MouseButtons::NO_BUTTON);
}

fn pump_until(el: &mut EventLoop, ms: u64, cond: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(ms) {
        el.process_events(false);
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    cond()
}

/// `QToolTip::hideText` hides after Qt's 300 ms hide delay; waits for it.
fn settle(el: &mut EventLoop) {
    ToolTip::hide_text();
    assert!(pump_until(el, 1500, || !ToolTip::is_visible()), "tip never hid");
}

/// Hovers `target` and returns the tip that appears (None if none does within 2.5 s).
fn tip_after_hover(el: &mut EventLoop, hud: &HUDWindow, target: &WidgetRef) -> Option<String> {
    settle(el);
    let mut d = EventTreeDispatcher::new();
    hover(&mut d, hud, target);
    let shown = pump_until(el, 2500, ToolTip::is_visible).then(ToolTip::text);
    d.handle_mouse_leave();
    settle(el);
    shown
}

fn tip(w: &WidgetRef) -> String {
    w.borrow().tool_tip()
}

#[test]
fn table_tool_tips_follow_usage_table_py() {
    let mut h = hud("table");
    h.table.update_metrics(&offline("claude", "boom"));
    h.table.update_metrics(&healthy("codex"));
    h.table.update_metrics(&stale("agy", "late"));

    let c = &h.table.columns["claude"];
    // Offline: `for w in value_cells + [header]: setToolTip(error)` and `dial.setToolTip(error)`.
    for (name, w) in [("header", &c.header), ("m1_reset", &c.m1_reset), ("m1_countdown", &c.m1_countdown),
                      ("m2_val", &c.m2_val), ("m2_reset", &c.m2_reset), ("m2_countdown", &c.m2_countdown),
                      ("dial", &c.dial)] {
        assert_eq!(tip(w), "boom", "offline {name}");
    }
    assert_eq!(tip(&c.name), "", "name has no tip of its own; the header's applies");

    // Healthy: no error, no stale: the cells have no tip; the dial and the pill carry the run-out notes.
    let c = &h.table.columns["codex"];
    for w in [&c.header, &c.m1_reset, &c.m1_countdown, &c.m2_reset, &c.m2_countdown] {
        assert_eq!(tip(w), "");
    }
    assert_eq!(tip(&c.m2_val), WEEK_NOTE);
    assert_eq!(tip(&c.dial), format!("5 小時：{WEEK_NOTE}\n1 週：{WEEK_NOTE}"));

    // Stale: header "舊資料 <stamp>\n<error>", pill and dial end with the error.
    let c = &h.table.columns["agy"];
    let stamp = Utc.with_ymd_and_hms(2024, 3, 5, 14, 7, 9).unwrap().with_timezone(&Local).format("%m/%d %H:%M:%S");
    assert_eq!(tip(&c.header), format!("舊資料 {stamp}\nlate"));
    assert_eq!(tip(&c.m1_reset), "late");
    assert_eq!(tip(&c.m2_val), format!("{WEEK_NOTE}\nlate"));
    assert_eq!(tip(&c.dial), format!("5 小時：{WEEK_NOTE}\n1 週：{WEEK_NOTE}\nlate"));
}

#[test]
fn a_recovered_column_loses_its_error_tool_tips() {
    let mut h = hud("table");
    h.table.update_metrics(&offline("claude", "boom"));
    h.table.update_metrics(&healthy("claude"));
    let c = &h.table.columns["claude"];
    assert_eq!(tip(&c.header), "");
    assert_eq!(tip(&c.m1_reset), "");
    assert_eq!(tip(&c.dial), format!("5 小時：{WEEK_NOTE}\n1 週：{WEEK_NOTE}"));
}

#[test]
fn card_and_header_button_tool_tips_follow_python() {
    let mut h = hud("cards");
    assert_eq!(tip(&h.ghost_label), "滑鼠穿透中 (Alt+Shift+C 解除)");
    assert_eq!(tip(&h.layout_toggle_btn), "切換 橫向並排 / 直式堆疊 佈局");
    h.cards.get_mut("claude").unwrap().update_metrics(&offline("claude", "card-boom"));
    assert_eq!(tip(&h.cards["claude"].widget()), "card-boom");
    h.cards.get_mut("claude").unwrap().update_metrics(&healthy("claude"));
    assert_eq!(tip(&h.cards["claude"].widget()), "", "`setToolTip(data.error or \"\")` clears it");
}

#[test]
fn hovering_a_table_widget_shows_its_tool_tip_after_the_wake_up_delay() {
    let mut el = EventLoop::new();
    let mut h = hud("table");
    h.table.update_metrics(&offline("claude", "boom"));
    h.table.update_metrics(&healthy("codex"));
    let claude = &h.table.columns["claude"];
    let codex = &h.table.columns["codex"];

    let mut d = EventTreeDispatcher::new();
    settle(&mut el);
    hover(&mut d, &h, &claude.dial);
    assert!(!pump_until(&mut el, 300, ToolTip::is_visible), "shown before the 700 ms wake-up");
    assert!(pump_until(&mut el, 2500, ToolTip::is_visible));
    assert_eq!(ToolTip::text(), "boom");
    d.handle_mouse_leave();
    settle(&mut el);

    assert_eq!(tip_after_hover(&mut el, &h, &claude.m2_val).as_deref(), Some("boom"));
    // The run-out note of a healthy column's dial.
    assert_eq!(
        tip_after_hover(&mut el, &h, &codex.dial).as_deref(),
        Some(format!("5 小時：{WEEK_NOTE}\n1 週：{WEEK_NOTE}").as_str())
    );
    // A cell with no tip of its own and no parent tip shows nothing.
    assert_eq!(tip_after_hover(&mut el, &h, &codex.m1_reset), None);
}

#[test]
fn a_child_without_a_tip_falls_back_to_its_parents() {
    let mut el = EventLoop::new();
    let mut h = hud("table");
    h.table.update_metrics(&offline("claude", "boom"));
    // `name` sits in a child widget of `header` and has no tip: the tip bubbles up to the header.
    assert_eq!(tip_after_hover(&mut el, &h, &h.table.columns["claude"].name).as_deref(), Some("boom"));

    let mut h = hud("cards");
    h.cards.get_mut("claude").unwrap().update_metrics(&offline("claude", "card-boom"));
    let card = &h.cards["claude"];
    // The Python card is one widget with the tip; its title and value labels have none.
    assert_eq!(tip_after_hover(&mut el, &h, &card.title).as_deref(), Some("card-boom"));
    assert_eq!(tip_after_hover(&mut el, &h, &card.m1_val).as_deref(), Some("card-boom"));
}

#[test]
fn header_button_and_ghost_tool_tips_show_on_hover() {
    let mut el = EventLoop::new();
    let mut h = hud("cards");
    assert_eq!(
        tip_after_hover(&mut el, &h, &h.layout_toggle_btn).as_deref(),
        Some("切換 橫向並排 / 直式堆疊 佈局")
    );
    h.set_click_through(true);
    h.window.root_widget().borrow().update_layout();
    assert_eq!(
        tip_after_hover(&mut el, &h, &h.ghost_label).as_deref(),
        Some("滑鼠穿透中 (Alt+Shift+C 解除)")
    );
}

#[test]
fn leaving_before_the_wake_up_shows_no_tip_and_a_click_hides_it() {
    let mut el = EventLoop::new();
    let mut h = hud("table");
    h.table.update_metrics(&offline("claude", "boom"));
    let dial = h.table.columns["claude"].dial.clone();

    settle(&mut el);
    let mut d = EventTreeDispatcher::new();
    hover(&mut d, &h, &dial);
    pump_until(&mut el, 300, || false);
    d.handle_mouse_leave();
    assert!(!pump_until(&mut el, 1200, ToolTip::is_visible), "a tip appeared after the cursor left");

    hover(&mut d, &h, &dial);
    assert!(pump_until(&mut el, 2500, ToolTip::is_visible));
    let p = centre(&dial);
    d.dispatch_event(
        &h.window.root_widget(),
        &mut qtrs_core::event::Event::new_spontaneous(qtrs_core::event::EventKind::MouseButtonPress { x: p.x, y: p.y, button: 1 }),
    );
    assert!(!ToolTip::is_visible(), "a click hides the tip at once");
}
