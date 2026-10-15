//! The cards-mode `LayoutToggleBtn` paint, normal and `:hover`, against PySide6 (G9.7.b sub-scope 5).
//!
//! Expected pixels: PySide6 6.11.2 `QPushButton("⇄")` styled by the real `get_cards_stylesheet(False)`, a
//! 28x18 child of a 60x40 widget, on the `offscreen` platform with `QTest.mouseMove` for the real
//! `:hover` state (`tools/pyside_probes/button_hover.py`; straight ARGB `grab()`), at DPR 1.0 and 1.25 in
//! `test_data/button_hover_qt_*.txt`.
//!
//! Compared: the background and the border ring. Not compared: the glyph (the offscreen platform's font
//! is not the Windows one), so everything inside the padding box (5 px from the sides, 3 px from top and
//! bottom) is masked; the table-mode sheet (the button is hidden there), the disabled state (the button
//! has no disabled path), geometry (it does not change on hover: `sizeHint` 28x18 before and after) and a
//! real cursor are out of scope.
//!
//! The straight edges and the background must match within 2 levels. The rounded corners are looser:
//! Qt strokes a pen of at most one device pixel with `QCosmeticStroker` (`qpaintengine_raster.cpp:656`)
//! and a wider one with its own area rasteriser, neither of which tiny-skia reproduces; the measured
//! corner maxima (111 levels at DPR 1.0 in the hover state, an opaque 1px arc; 16 at DPR 1.25) are the
//! tolerances below.
use ferrokit_core::event::{Event, EventKind};
use ferrokit_core::object::qobject::QObject;
use ferrokit_gui::geometry::primitives::Rect;
use ferrokit_gui::paint::Painter;
use ferrokit_gui::Pixmap;
use ferrokit_widgets::{Button, Widget};

use super::test_support::CardsOracleSetup;

type Rows = Vec<Vec<u32>>;

fn qt_rows(data: &str, state: &str) -> Rows {
    let mut lines = data
        .lines()
        .skip_while(|l| !l.starts_with(&format!("{state} dpr=")));
    lines.next().expect("state in oracle data");
    lines
        .take_while(|l| !l.contains("dpr="))
        .map(|l| {
            l.split(' ')
                .map(|p| u32::from_str_radix(p, 16).unwrap())
                .collect()
        })
        .collect()
}

fn paint(dpr: f32, hover: bool, rows: usize, cols: usize) -> Rows {
    let _setup = CardsOracleSetup::at_dpr(dpr);
    let mut button = Button::new("\u{21c4}");
    button.set_object_name("LayoutToggleBtn");
    button.set_geometry(Rect::new(11, 11, 28, 18));
    if hover {
        button.event(&mut Event::new_spontaneous(EventKind::Enter { x: 5, y: 5 }));
    }
    let mut pixmap = Pixmap::with_dpr(cols as u32, rows as u32, dpr).unwrap();
    pixmap.fill(ferrokit_gui::tiny_skia::Color::TRANSPARENT);
    let mut painter = Painter::begin(&mut pixmap);
    button.paint_event(&mut painter);
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
    for (state, hover) in [("normal", false), ("hover", true)] {
        let qt = qt_rows(data, state);
        let got = paint(dpr, hover, qt.len(), qt[0].len());
        let (rows, cols) = (qt.len(), qt[0].len());
        let glyph_x = (5.0 * dpr).round() as usize;
        let glyph_y = (3.0 * dpr).round() as usize;
        // Within the radius (4 px) plus two pixels of the arc's width and anti-aliasing, of two sides.
        let corner = (4.0 * dpr).ceil() as usize + 2;
        let mut failures = Vec::new();
        for (y, (qrow, grow)) in qt.iter().zip(&got).enumerate() {
            for (x, (q, g)) in qrow.iter().zip(grow).enumerate() {
                if (glyph_x..cols - glyph_x).contains(&x) && (glyph_y..rows - glyph_y).contains(&y)
                {
                    continue;
                }
                let in_corner =
                    (x < corner || x + corner >= cols) && (y < corner || y + corner >= rows);
                let limit = if in_corner { corner_tolerance } else { 2 };
                if distance(*q, *g) > limit {
                    failures.push(format!("({x},{y}) qt {q:08x} ferrokit {g:08x} > {limit}"));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "DPR {dpr} {state}: {} pixels differ from PySide6:\n{}",
            failures.len(),
            failures
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

#[test]
fn layout_toggle_btn_normal_and_hover_match_pyside6_at_dpr_1_0() {
    check(1.0, include_str!("test_data/button_hover_qt_1.0.txt"), 112);
}

#[test]
fn layout_toggle_btn_normal_and_hover_match_pyside6_at_dpr_1_25() {
    check(1.25, include_str!("test_data/button_hover_qt_1.25.txt"), 17);
}
