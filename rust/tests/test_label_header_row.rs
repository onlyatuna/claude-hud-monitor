//! G9.7.b, first sub-scope: real `Label`s (not the oracle's synthetic probes) in a layout.
//!
//! The configuration is the HUD header row reduced to its two text-driven labels
//! (`hud_window.py:280-288`): a horizontal layout (margins 0, spacing 6) of `QLabel#HeaderTitle`,
//! a stretch and `QLabel#HeaderStatus`, under the HUD's QLabel style sheet. The expected numbers are
//! PySide6 6.11.2 measurements on Windows (DirectWrite) from `rust/tools/pyside_probes/label_header_row.py`
//! at DPR 1.25 (`QT_SCREEN_SCALE_FACTORS=1.25`) and DPR 1.0 (`QT_ENABLE_HIGHDPI_SCALING=0`).
//!
//! Text widths of these labels are also pinned by `test_label_text_metric_rounding.rs`; this file adds
//! what that one does not: the height, the minimum size hint, and the rectangles the layout gives the
//! labels when it is exactly as wide as its hint, wider, and narrower than its minimum.
#![cfg(windows)]
use ferrokit_core::object::QObject;
use ferrokit_gui::geometry::primitives::{Margins, Rect, Size};
use ferrokit_widgets::{BoxLayout, Label, Layout, Widget, WidgetRef};
use std::cell::RefCell;
use std::rc::Rc;

const SHEET: &str = "\
QLabel { color: #e2e8f0; font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif; }
QLabel#HeaderTitle { font-size: 10.5px; font-weight: 800; letter-spacing: 1.0px; color: #94a3b8; }
QLabel#HeaderStatus { font-size: 9.5px; color: #64748b; font-family: 'Consolas', monospace; }";

/// The application device pixel ratio is process-global: tests take turns.
static DPR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Row {
    title: WidgetRef,
    status: WidgetRef,
    layout: BoxLayout,
}

fn label(object_name: &str, text: &str) -> WidgetRef {
    let mut l = Label::new(text);
    l.set_object_name(object_name);
    l.set_style_sheet(SHEET);
    Rc::new(RefCell::new(Box::new(l)))
}

fn row() -> Row {
    let title = label("HeaderTitle", "AI AGENT HUD (3-IN-1)");
    let status = label("HeaderStatus", "Updated 12:34:56");
    let mut layout = BoxLayout::horizontal();
    layout.set_margins(Margins::new(0, 0, 0, 0));
    layout.set_spacing(6);
    layout.add_widget(Rc::clone(&title));
    layout.add_stretch(1);
    layout.add_widget(Rc::clone(&status));
    Row {
        title,
        status,
        layout,
    }
}

fn dims(s: Size) -> (i32, i32) {
    (s.width, s.height)
}

fn rect_of(w: &WidgetRef) -> (i32, i32, i32, i32) {
    let g = w.borrow().geometry();
    (g.x, g.y, g.width, g.height)
}

type Geom = (i32, i32, i32, i32);

/// What the probe prints for one device pixel ratio.
struct Expected {
    title_hint: (i32, i32),
    status_hint: (i32, i32),
    layout_hint: (i32, i32),
    layout_min: (i32, i32),
    /// (layout width, title rect, status rect) at height 30.
    cases: [(i32, Geom, Geom); 3],
}

fn check(dpr: f32, want: &Expected) {
    let _guard = DPR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    ferrokit_gui::text::font_database::set_application_device_pixel_ratio(dpr);
    let mut r = row();

    {
        let title = r.title.borrow();
        let status = r.status.borrow();
        assert_eq!(dims(title.size_hint()), want.title_hint, "title size hint");
        assert_eq!(
            dims(title.minimum_size_hint()),
            want.title_hint,
            "title minimum size hint"
        );
        assert_eq!(
            dims(status.size_hint()),
            want.status_hint,
            "status size hint"
        );
        assert_eq!(
            dims(status.minimum_size_hint()),
            want.status_hint,
            "status minimum size hint"
        );
    }

    assert_eq!(
        dims(r.layout.size_hint()),
        want.layout_hint,
        "layout size hint"
    );
    assert_eq!(
        dims(r.layout.minimum_size()),
        want.layout_min,
        "layout minimum"
    );
    for (width, title_rect, status_rect) in want.cases {
        r.layout.set_geometry(Rect::new(0, 0, width, 30));
        assert_eq!(
            rect_of(&r.title),
            title_rect,
            "title rect, layout width {width}"
        );
        assert_eq!(
            rect_of(&r.status),
            status_rect,
            "status rect, layout width {width}"
        );
    }
}

#[test]
fn the_header_row_matches_pyside6_at_dpr_1_25() {
    check(
        1.25,
        &Expected {
            title_hint: (144, 15),
            status_hint: (88, 12),
            layout_hint: (238, 15),
            layout_min: (238, 15),
            cases: [
                (238, (0, 0, 144, 30), (150, 0, 88, 30)),
                (338, (0, 0, 144, 30), (250, 0, 88, 30)),
                (228, (0, 0, 134, 30), (140, 0, 88, 30)),
            ],
        },
    );
}

#[test]
fn the_header_row_matches_pyside6_at_dpr_1_0() {
    check(
        1.0,
        &Expected {
            title_hint: (142, 15),
            status_hint: (80, 12),
            layout_hint: (228, 15),
            layout_min: (228, 15),
            cases: [
                (228, (0, 0, 142, 30), (148, 0, 80, 30)),
                (328, (0, 0, 142, 30), (248, 0, 80, 30)),
                (218, (0, 0, 132, 30), (138, 0, 80, 30)),
            ],
        },
    );
}
