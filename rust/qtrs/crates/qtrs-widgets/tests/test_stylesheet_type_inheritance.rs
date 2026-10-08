//! RC-32 (G8.5.a): a QSS type selector matches the class and every class it inherits from.
//!
//! `QStyleSheetStyleSelector::nodeNameEquals` walks `metaObject()->superClass()`
//! (qstylesheetstyle.cpp:1579-1601), so `QFrame { }` styles a `QLabel` and `QAbstractButton { }`
//! a `QPushButton`, but not the other way round. A type selector weighs the same whichever class
//! it names (qcssparser.cpp:1973-1974), so between `QLabel` and `QFrame` the later rule wins.
//! Reference colours: PySide6 6.11.2, the widget's own style sheet, `palette().color(...)`.

use qtrs_gui::tiny_skia::Color;
use qtrs_widgets::style::stylesheet::{QStyleSheetStyle, WidgetStyleContext};

fn red() -> Option<Color> {
    Some(Color::from_rgba8(255, 0, 0, 255))
}

fn green() -> Option<Color> {
    Some(Color::from_rgba8(0, 255, 0, 255))
}

fn color(qss: &str, type_name: &str, object_name: &str) -> Option<Color> {
    let ctx = WidgetStyleContext {
        type_name,
        object_name,
        ..Default::default()
    };
    QStyleSheetStyle::parse(qss).resolve(&ctx).color
}

#[test]
fn a_base_class_selector_styles_a_subclass() {
    assert_eq!(color("QFrame { color: #ff0000; }", "QLabel", ""), red());
    assert_eq!(
        color("QAbstractButton { color: #ff0000; }", "QPushButton", ""),
        red()
    );
    assert_eq!(color("QFrame#T { color: #ff0000; }", "QLabel", "T"), red());
}

#[test]
fn a_selector_does_not_style_a_base_class_or_an_unrelated_class() {
    assert_eq!(color("QLabel { color: #ff0000; }", "QFrame", ""), None);
    assert_eq!(
        color("QFrame { color: #ff0000; }", "QProgressBar", ""),
        None
    );
}

#[test]
fn a_base_class_selector_weighs_the_same_as_the_class_itself() {
    assert_eq!(
        color(
            "QLabel { color: #00ff00; } QFrame { color: #ff0000; }",
            "QLabel",
            ""
        ),
        red()
    );
    assert_eq!(
        color(
            "QFrame { color: #ff0000; } QLabel { color: #00ff00; }",
            "QLabel",
            ""
        ),
        green()
    );
}
