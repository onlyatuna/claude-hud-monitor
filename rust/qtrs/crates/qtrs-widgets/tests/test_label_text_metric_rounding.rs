//! RC-21: `QLabel::sizeHint` width under the HUD's real style sheet, whose `QLabel` rule is
//! `font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif`.
//! `setFontFamilyFromValues` (`qcssparser.cpp:1252`) turns the list into `QFont::setFamilies`, and
//! the first installed family (Segoe UI) is used. The expected numbers are PySide6 measurements
//! (Qt 6, Windows, DirectWrite) of the same sheet at DPR 1.25 (`QT_SCREEN_SCALE_FACTORS=1.25`) and
//! DPR 1.0 (`QT_ENABLE_HIGHDPI_SCALING=0`): the text widths are 59/144/88 and 58/142/80.
#![cfg(windows)]
use qtrs_core::object::QObject;
use qtrs_widgets::{Label, Widget};

const SHEET: &str = "\
QLabel { color: #e2e8f0; font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif; }
QLabel#HeaderTitle { font-size: 10.5px; font-weight: 800; letter-spacing: 1.0px; color: #94a3b8; }
QLabel#MetricTitle { font-size: 10px; font-weight: 700; color: #94a3b8; letter-spacing: 0.6px; }
QLabel#HeaderStatus { font-size: 9.5px; color: #64748b; font-family: 'Consolas', monospace; }";

fn hint_width(object_name: &str, text: &str, dpr: f32) -> i32 {
    qtrs_gui::text::font_database::set_application_device_pixel_ratio(dpr);
    let mut l = Label::new(text);
    l.set_object_name(object_name);
    l.set_style_sheet(SHEET);
    l.size_hint().width
}

fn widths(dpr: f32) -> [i32; 3] {
    [
        hint_width("MetricTitle", "WEEKLY 7D", dpr),
        hint_width("HeaderTitle", "AI AGENT HUD (3-IN-1)", dpr),
        hint_width("HeaderStatus", "Updated 12:34:56", dpr),
    ]
}

#[test]
fn label_widths_with_a_font_family_list_match_pyside6_at_dpr_1_25() {
    assert_eq!(widths(1.25), [59, 144, 88]);
}

#[test]
fn label_widths_with_a_font_family_list_match_pyside6_at_dpr_1_0() {
    assert_eq!(widths(1.0), [58, 142, 80]);
}
