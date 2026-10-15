//! The background and border a style sheet paints on a `Label` (RC-210).
//!
//! Qt fills each border edge as a rectangle inside the border box and strokes the rounded corners as
//! 45-degree arcs (`qDrawBorder`, `qcssutil.cpp:313-369`); the rounded background runs along the middle
//! of the border (`QRenderRule::borderClip`). ferrokit used to fill the box and stroke a pen centred on the
//! rectangle edge, so a 1px border was two half-covered pixel rows.
//!
//! Expected pixels are PySide6 6.11.2 measurements (`rust/tools/pyside_probes/label_border.py`, offscreen
//! platform, `QWidget.render` of an empty-text `QLabel` onto a transparent image, straight ARGB) at DPR
//! 1.0 and 1.25 in `tests/data/label_border_qt_*.txt`. The labels have no text, so no glyph is involved.
//!
//! The straight edges and the background are compared within 2 levels. The corner arcs are compared more
//! loosely: Qt strokes a pen of at most one device pixel with `QCosmeticStroker` (a thin-line
//! rasteriser, `qpaintengine_raster.cpp:656-659`) and a wider one with its own area rasteriser, neither
//! of which tiny-skia reproduces. The measured corner maxima are 126 levels (DPR 1.0, an opaque 1px
//! arc) and 21 levels (DPR 1.25).
#![cfg(windows)]
use ferrokit_gui::geometry::primitives::Rect;
use ferrokit_gui::paint::Painter;
use ferrokit_gui::Pixmap;
use ferrokit_widgets::{Label, Widget};

/// The application device pixel ratio is process-global: tests take turns.
static DPR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// (case, style sheet, width, height, corner radius in px)
const CASES: [(&str, &str, i32, i32, f32); 5] = [
    (
        "badge",
        "QLabel { background-color: rgba(255, 255, 255, 0.06); border: 1px solid rgba(255, 255, 255, 0.08); border-radius: 3px; padding: 1px 4px; }",
        40,
        15,
        3.0,
    ),
    (
        "r4_clear",
        "QLabel { background-color: transparent; border: 1px solid rgba(255, 255, 255, 0.12); border-radius: 4px; }",
        28,
        18,
        4.0,
    ),
    (
        "r0",
        "QLabel { background-color: #204060; border: 1px solid #ff8040; }",
        20,
        12,
        0.0,
    ),
    (
        "border_only",
        "QLabel { border: 1px solid #ff8040; border-radius: 3px; }",
        20,
        12,
        3.0,
    ),
    (
        "b2",
        "QLabel { background-color: #204060; border: 2px solid #ff8040; border-radius: 4px; }",
        24,
        16,
        4.0,
    ),
];

type Rows = Vec<Vec<u32>>;

fn qt_rows(data: &str, case: &str) -> Rows {
    let mut lines = data
        .lines()
        .skip_while(|l| !l.starts_with(&format!("{case} dpr=")));
    lines.next().expect("case in oracle data");
    lines
        .take_while(|l| !l.contains("dpr="))
        .map(|l| {
            l.split(' ')
                .map(|p| u32::from_str_radix(p, 16).unwrap())
                .collect()
        })
        .collect()
}

fn paint(sheet: &str, w: i32, h: i32, dpr: f32, rows: usize, cols: usize) -> Rows {
    let mut label = Label::new("");
    label.set_style_sheet(sheet);
    label.set_geometry(Rect::new(0, 0, w, h));
    let mut pixmap = Pixmap::with_dpr(cols as u32, rows as u32, dpr).unwrap();
    pixmap.fill(ferrokit_gui::tiny_skia::Color::TRANSPARENT);
    let mut painter = Painter::begin(&mut pixmap);
    label.paint_event(&mut painter);
    drop(painter);
    (0..rows as u32)
        .map(|y| {
            (0..cols as u32)
                .map(|x| {
                    let c = pixmap.pixel(x, y).unwrap().demultiply();
                    (c.alpha() as u32) << 24
                        | (c.red() as u32) << 16
                        | (c.green() as u32) << 8
                        | c.blue() as u32
                })
                .collect()
        })
        .collect()
}

/// Largest difference of any premultiplied channel (and of alpha) between two straight ARGB pixels.
fn distance(a: u32, b: u32) -> i32 {
    let pm = |v: u32, s: u32| (((v >> (8 * s)) & 255) * (v >> 24) / 255) as i32;
    let alpha = ((a >> 24) as i32 - (b >> 24) as i32).abs();
    (0..3)
        .map(|s| (pm(a, s) - pm(b, s)).abs())
        .fold(alpha, i32::max)
}

fn check(dpr: f32, data: &str, corner_tolerance: i32) {
    let _guard = DPR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    ferrokit_gui::text::font_database::set_application_device_pixel_ratio(dpr);
    let mut failures = Vec::new();
    for (case, sheet, w, h, radius) in CASES {
        let qt = qt_rows(data, case);
        let got = paint(sheet, w, h, dpr, qt.len(), qt[0].len());
        // Pixels within a radius (plus the arc's own width) of two sides are the corner arcs.
        let corner = (radius * dpr).ceil() as usize + 1;
        for (y, (qrow, grow)) in qt.iter().zip(&got).enumerate() {
            for (x, (q, g)) in qrow.iter().zip(grow).enumerate() {
                let in_corner = radius > 0.0
                    && (x < corner || x + corner >= qrow.len())
                    && (y < corner || y + corner >= qt.len());
                let limit = if in_corner { corner_tolerance } else { 2 };
                if distance(*q, *g) > limit {
                    failures.push(format!(
                        "{case} ({x},{y}) qt {q:08x} ferrokit {g:08x} > {limit}{}",
                        if in_corner { " (corner)" } else { "" }
                    ));
                }
            }
        }
    }
    ferrokit_gui::text::font_database::set_application_device_pixel_ratio(1.0);
    assert!(
        failures.is_empty(),
        "DPR {dpr}: {} pixels differ from PySide6:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn label_background_and_border_match_pyside6_at_dpr_1_0() {
    check(1.0, include_str!("data/label_border_qt_1.0.txt"), 127);
}

#[test]
fn label_background_and_border_match_pyside6_at_dpr_1_25() {
    check(1.25, include_str!("data/label_border_qt_1.25.txt"), 24);
}
