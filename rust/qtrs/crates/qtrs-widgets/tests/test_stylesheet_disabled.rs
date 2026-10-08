//! RC-34 (G8.5.e, `:disabled` only): a disabled widget matches QSS `:disabled` rules.
//!
//! `pseudoClass(QStyle::State)` maps a missing `State_Enabled` to `PseudoClass_Disabled`
//! (qstylesheetstyle.cpp:1756-1765; `"disabled"` is parsed at qcssparser.cpp:301).
//! Reference colours: PySide6 6.11.2 — `QLabel:disabled`, `QFrame#F:disabled`,
//! `QPushButton:disabled` and `QProgressBar:disabled` all apply once the widget is disabled.

use qtrs_gui::tiny_skia::Color;
use qtrs_widgets::{Button, Frame, Label, ProgressBar, Widget};

fn rgb(r: u8, g: u8, b: u8) -> Option<Color> {
    Some(Color::from_rgba8(r, g, b, 255))
}

#[test]
fn a_disabled_label_uses_the_disabled_rule() {
    let l = Label::new("a");
    l.set_style_sheet("QLabel { color: #00ff00; } QLabel:disabled { color: #ff0000; }");
    assert_eq!(l.resolved_style().color, rgb(0, 255, 0));
    l.set_enabled(false);
    assert_eq!(l.resolved_style().color, rgb(255, 0, 0));
    l.set_enabled(true);
    assert_eq!(l.resolved_style().color, rgb(0, 255, 0));
}

#[test]
fn frame_button_and_progress_bar_match_disabled() {
    let mut f = Frame::new();
    f.base.object_data.set_object_name("F");
    f.set_style_sheet("QFrame#F:disabled { color: #ff0000; }");
    let b = Button::new("b");
    b.set_style_sheet("QPushButton:disabled { color: #ff0000; }");
    let g = ProgressBar::new();
    g.set_style_sheet("QProgressBar:disabled { color: #ff0000; }");
    assert_eq!(f.resolved_style().color, None);
    assert_eq!(b.resolved_style().color, None);
    assert_eq!(g.resolved_groove_style().color, None);
    f.set_enabled(false);
    b.set_enabled(false);
    g.set_enabled(false);
    assert_eq!(f.resolved_style().color, rgb(255, 0, 0));
    assert_eq!(b.resolved_style().color, rgb(255, 0, 0));
    assert_eq!(g.resolved_groove_style().color, rgb(255, 0, 0));
}

/// `pseudoClass` sets `PseudoClass_Hover` only inside the `State_Enabled` branch, so a button that
/// was under the mouse when it got disabled takes the `:disabled` rule, not `:hover`.
#[test]
fn a_hovered_button_that_is_disabled_drops_hover() {
    use qtrs_core::event::{Event, EventKind};
    use qtrs_core::object::qobject::QObject;
    let mut b = Button::new("b");
    b.set_style_sheet(
        "QPushButton:hover { color: #00ff00; } QPushButton:disabled { color: #ff0000; }",
    );
    b.event(&mut Event::new_spontaneous(EventKind::Enter { x: 1, y: 1 }));
    assert_eq!(b.resolved_style().color, rgb(0, 255, 0));
    b.set_enabled(false);
    b.set_style_sheet(
        "QPushButton:disabled { color: #ff0000; } QPushButton:hover { color: #00ff00; }",
    );
    assert_eq!(b.resolved_style().color, rgb(255, 0, 0));
}
