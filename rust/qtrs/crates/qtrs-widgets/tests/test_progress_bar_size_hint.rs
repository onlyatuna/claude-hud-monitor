//! `QProgressBar::sizeHint`/`minimumSizeHint` (`qprogressbar.cpp:396-418`), passed through
//! `QStyleSheetStyle::sizeFromContents(CT_ProgressBar)` (`qstylesheetstyle.cpp:5311-5320,5485-5490`).
//! Expected numbers are PySide6 measurements (`rust/tools/qss_box_audit/`, cases P4-P15) taken
//! with explicit `font-family`/`font-size` so they do not depend on the application font.
#![cfg(windows)]
use qtrs_widgets::progress_bar::ProgressBar;
use qtrs_widgets::scroll::Orientation;
use qtrs_widgets::Widget;

fn hints(qss: &str, vertical: bool) -> ([i32; 2], [i32; 2]) {
    // the PySide6 numbers are from a DPR 1.25 screen (DirectWrite text engine)
    qtrs_gui::text::font_database::set_application_device_pixel_ratio(1.25);
    let mut p = ProgressBar::new();
    p.set_style_sheet(qss);
    if vertical {
        p.set_orientation(Orientation::Vertical);
    }
    let (h, m) = (p.size_hint(), p.minimum_size_hint());
    ([h.width, h.height], [m.width, m.height])
}

#[test]
fn test_progress_bar_hint_follows_font_and_box() {
    let cases: [(&str, &str, ([i32; 2], [i32; 2])); 5] = [
        ("P4", "QProgressBar { font-family: 'Segoe UI'; border: none; font-size: 12px; }", ([87, 24], [87, 18])),
        ("P5", "QProgressBar { font-family: 'Segoe UI'; border: none; font-size: 20px; }", ([107, 35], [107, 29])),
        ("P6", "QProgressBar { font-family: 'Segoe UI'; font-size: 12px; border: 1px solid #888; padding: 2px 3px; }", ([95, 30], [95, 18])),
        ("P7", "QProgressBar { font-family: 'Segoe UI'; min-width: 50px; max-width: 200px; font-size: 12px; border: none; }", ([87, 24], [87, 18])),
        ("P10", "QProgressBar { font-family: 'Segoe UI'; font-size: 14px; border: none; }", ([95, 27], [95, 21])),
    ];
    for (name, qss, expected) in cases {
        assert_eq!(hints(qss, false), expected, "PySide6 {name}");
    }
}

#[test]
fn test_progress_bar_hint_uses_chunk_width() {
    // `PM_ProgressBarChunkWidth` comes from `::chunk { width }`; the default and anything below 9 is 9.
    assert_eq!(
        hints("QProgressBar { font-family: 'Segoe UI'; border: none; font-size: 12px; } QProgressBar::chunk { width: 20px; }", false).0,
        [164, 24],
        "PySide6 P12"
    );
    assert_eq!(
        hints("QProgressBar { font-family: 'Segoe UI'; border: none; font-size: 12px; } QProgressBar::chunk { width: 5px; }", false).0,
        [87, 24],
        "PySide6 P13"
    );
}

#[test]
fn test_progress_bar_hint_is_clamped_to_content_box_min_max() {
    // P14/P15 shape: `max-height: 5px` clamps the content height, then the box is added.
    assert_eq!(hints("QProgressBar { font-family: 'Segoe UI'; border: none; font-size: 12px; min-height: 5px; max-height: 5px; }", false).0, [87, 5]);
    assert_eq!(
        hints("QProgressBar { font-family: 'Segoe UI'; border: 1px solid #888; padding: 2px; font-size: 12px; min-height: 5px; max-height: 5px; }", false).0,
        [93, 11]
    );
}

#[test]
fn test_vertical_progress_bar_hint_is_transposed_before_the_style() {
    // PySide6 P11: vertical, `min-width: 5px; max-width: 5px` -> hint 5x91, minimumSizeHint 17x91.
    assert_eq!(
        hints("QProgressBar { font-family: 'Segoe UI'; border: none; font-size: 12px; min-width: 5px; max-width: 5px; }", true),
        ([5, 87], [18, 87])
    );
}

#[test]
fn test_vertical_progress_bar_min_max_are_not_transposed() {
    // PySide6 P11: QSS `min-width`/`max-width` are widget coordinates whatever the orientation:
    // minimumSize 5x0, maximumSize 5xQWIDGETSIZE_MAX (`QStyleSheetStyle::setGeometry`).
    let mut p = ProgressBar::new();
    p.set_style_sheet("QProgressBar { border: none; min-width: 5px; max-width: 5px; }");
    p.set_orientation(Orientation::Vertical);
    let (min, max) = (p.minimum_size(), p.maximum_size());
    assert_eq!(([min.width, min.height], [max.width, max.height]), ([5, 0], [5, 16777215]));
}
