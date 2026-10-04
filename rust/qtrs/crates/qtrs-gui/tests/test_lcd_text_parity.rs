//! LCD (ClearType) text against PySide6 6.11.2 on Windows (`tools/second_layer_harness/qt_lcd_dump.py`):
//! the same text drawn by Qt onto several destinations, compared pixel by pixel.
//!
//! The fixtures are 48x24 RGBA dumps of `QPainter.drawText(2, 17, text)` with a 12 px font,
//! text colour (226, 232, 240) over (22, 25, 32), at ClearType gamma 1.2 and RGB sub-pixel order.
#![cfg(windows)]

use qtrs_gui::geometry::primitives::PointF;
use qtrs_gui::paint::{Painter, Pen, Pixmap};
use qtrs_gui::text::font::Font;
use qtrs_gui::text::smoothing::TextSmoothing;
use qtrs_gui::tiny_skia::Color;

const W: u32 = 48;
const H: u32 = 24;
const BASE_X: f32 = 2.0;
const BASE_Y: f32 = 17.0;

fn fixture(dir: &str, case: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{dir}/{case}.bin", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn premultiply(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|p| {
            let a = p[3] as u32;
            let m = |c: u8| ((c as u32 * a + 127) / 255) as u8;
            [m(p[0]), m(p[1]), m(p[2]), p[3]]
        })
        .collect()
}

/// Draws `text` the way the fixture was drawn and returns the premultiplied pixels.
fn render(
    family: &str,
    text: &str,
    background: Color,
    foreground: Color,
    smoothing: TextSmoothing,
) -> Vec<u8> {
    let font = Font::new(family, 12.0);
    let mut pm = Pixmap::with_dpr(W, H, 1.0).unwrap();
    pm.fill(background);
    {
        let mut painter = Painter::begin(&mut pm);
        painter.set_text_smoothing(smoothing);
        painter.set_pen(Some(Pen::new(foreground, 1.0)));
        // `draw_text` takes the baseline origin, like `QPainter::drawText(x, y, text)`.
        painter.draw_text(PointF::new(BASE_X, BASE_Y), text, &font);
    }
    pm.data().to_vec()
}

/// (mean, max) absolute channel difference over the colour channels.
fn diff(got: &[u8], want: &[u8]) -> (f64, i32) {
    let (mut sum, mut max) = (0i64, 0i32);
    for (g, w) in got.chunks_exact(4).zip(want.chunks_exact(4)) {
        for c in 0..3 {
            let d = (g[c] as i32 - w[c] as i32).abs();
            sum += d as i64;
            max = max.max(d);
        }
    }
    (sum as f64 / (got.len() / 4 * 3) as f64, max)
}

const LCD: TextSmoothing = TextSmoothing {
    cleartype: true,
    gamma: 1.2,
};
const BG: fn() -> Color = || Color::from_rgba8(22, 25, 32, 255);
const FG: fn() -> Color = || Color::from_rgba8(226, 232, 240, 255);

fn check(dir: &str, family: &str, text: &str, case: &str, bg: Color, fg: Color, smoothing: TextSmoothing, mean: f64, max: i32) {
    let want = premultiply(&fixture(dir, case));
    let got = render(family, text, bg, fg, smoothing);
    let (m, x) = diff(&got, &want);
    println!("{dir}/{case}: mean {m:.3} max {x}");
    assert!(m <= mean && x <= max, "{dir}/{case}: mean {m:.3} (<= {mean}) max {x} (<= {max})");
}

#[test]
fn lcd_text_on_opaque_destination_matches_qt() {
    for (dir, family, text) in [
        ("lcd_segoe12", "Segoe UI", "Hag"),
        ("lcd_jhenghei12", "Microsoft JhengHei UI", "Hag中"),
    ] {
        check(dir, family, text, "rgb32_opaque", BG(), FG(), LCD, 0.1, 2);
        check(dir, family, text, "argb_pre_opaque", BG(), FG(), LCD, 0.1, 2);
    }
}

#[test]
fn lcd_text_on_translucent_destination_is_grey_like_qt() {
    for (dir, family, text) in [("lcd_segoe12", "Segoe UI", "Hag"), ("lcd_jhenghei12", "Microsoft JhengHei UI", "Hag中")] {
        check(dir, family, text, "argb_pre_clear", Color::TRANSPARENT, FG(), LCD, 0.3, 3);
        check(dir, family, text, "argb_pre_half", Color::from_rgba8(22, 25, 32, 128), FG(), LCD, 0.3, 3);
    }
}

#[test]
fn lcd_text_with_translucent_colour_matches_qt() {
    check("lcd_segoe12", "Segoe UI", "Hag", "rgb32_textalpha", BG(), Color::from_rgba8(226, 232, 240, 128), LCD, 1.5, 8);
}

#[test]
fn grey_text_without_cleartype_matches_qt_a8_path() {
    // `QImage::Format_ARGB32` is not a LCD destination in Qt, so the glyph cache is `A8`.
    check("lcd_segoe12", "Segoe UI", "Hag", "argb_straight", BG(), FG(), TextSmoothing::OFF, 0.3, 3);
}
