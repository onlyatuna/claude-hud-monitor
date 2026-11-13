//! The tray icon with a quota number: a rounded dark tile and the number in bold, one pixmap per
//! size Windows may ask for (it never scales an icon up, so a 16 px picture would be blown up on a
//! 150 % display).
//!
//! The text is drawn grey-scale: ClearType needs an opaque background and the tile corners are
//! transparent. Remaining mode adds a solid bar under the digits: it stays visible at 16 px where
//! a dot or a corner mark would not.

use ferrokit_gui::color::Color;
use ferrokit_gui::geometry::primitives::{PointF, RectF};
use ferrokit_gui::image::{Icon, IconMode, IconState};
use ferrokit_gui::paint::brush::Brush;
use ferrokit_gui::paint::painter::{Painter, RenderHint};
use ferrokit_gui::paint::pixmap::Pixmap;
use ferrokit_gui::text::font::{Font, FontWeight};
use ferrokit_gui::text::smoothing::TextSmoothing;

use super::gauge::skia;
use crate::tray_quota::IconSpec;

/// Icon sizes at 100 / 125 / 150 / 200 % display scaling.
pub const SIZES: [u32; 4] = [16, 20, 24, 32];

const TILE: &str = "#12161e";
const FAMILY: &str = "Segoe UI";

/// One icon holding the number at every size in [`SIZES`].
pub fn build_icon(spec: &IconSpec) -> Icon {
    let mut icon = Icon::new();
    for size in SIZES {
        icon.add_pixmap(&render(spec, size), IconMode::Normal, IconState::Off);
    }
    icon
}

fn font(px: f32) -> Font {
    Font::new(FAMILY, px).with_weight(FontWeight::Bold)
}

/// Where `text` draws ink when its baseline starts at `(origin, origin * 3)` of a scratch pixmap:
/// `(left, top, width, height)` of the pixels that are clearly covered.
fn ink_box(text: &str, font: &Font, scratch_size: u32) -> Option<(i32, i32, i32, i32)> {
    let origin = scratch_size as f32 / 4.0;
    let mut pm = Pixmap::new(scratch_size, scratch_size)?;
    {
        let mut p = Painter::begin(&mut pm);
        p.set_text_smoothing(TextSmoothing::OFF);
        p.draw_text_colored(
            PointF::new(origin, origin * 3.0),
            text,
            font,
            skia(Color::from("#ffffff")),
        );
    }
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..scratch_size {
        for x in 0..scratch_size {
            if pm.pixel(x, y).is_some_and(|c| c.alpha() >= 16) {
                x0 = x0.min(x as i32);
                y0 = y0.min(y as i32);
                x1 = x1.max(x as i32);
                y1 = y1.max(y as i32);
            }
        }
    }
    (x1 >= x0).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// The icon at `size` px square.
pub fn render(spec: &IconSpec, size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size, size).expect("icon size is positive");
    let s = size as i32;
    let pad = ((size as f32 / 16.0).round() as i32).max(1);
    let bar = ((size as f32 / 8.0).round() as i32).max(2);
    let avail_w = s - 2 * pad;
    let avail_h = if spec.remaining {
        s - 3 * pad - bar
    } else {
        s - 2 * pad
    };

    // The largest whole pixel size whose ink fits the box.
    let scratch = size * 4;
    let mut px = size as f32;
    let (font, ink) = loop {
        let f = font(px);
        match ink_box(&spec.text, &f, scratch) {
            Some(ink) if (ink.2 <= avail_w && ink.3 <= avail_h) || px <= 5.0 => {
                break (f, Some(ink));
            }
            None => break (f, None),
            Some(_) => px -= 1.0,
        }
    };

    let mut p = Painter::begin(&mut pm);
    p.set_text_smoothing(TextSmoothing::OFF);
    p.set_render_hint(RenderHint::Antialiasing, true);
    p.set_pen(None);
    p.set_brush(Brush::Color(skia(Color::from(TILE))));
    let radius = size as f32 * 0.22;
    p.draw_rounded_rect(
        RectF::new(0.0, 0.0, size as f32, size as f32),
        radius,
        radius,
    );

    let color = skia(Color::from_rgb(
        i32::from(spec.rgb[0]),
        i32::from(spec.rgb[1]),
        i32::from(spec.rgb[2]),
        255,
    ));
    let Some((left, top, w, h)) = ink else {
        return pm;
    };
    // Centre the ink in the digit box: `origin` is where the scratch drew the baseline start.
    let origin = (scratch / 4) as i32;
    let want_left = pad + (avail_w - w) / 2;
    let want_top = pad + (avail_h - h) / 2;
    let (ox, oy) = (want_left - (left - origin), want_top - (top - origin * 3));
    p.draw_text_colored(PointF::new(ox as f32, oy as f32), &spec.text, &font, color);
    if spec.remaining {
        let bar_w = w.max(s / 2).min(avail_w);
        p.fill_rect(
            RectF::new(
                (pad + (avail_w - bar_w) / 2) as f32,
                (s - pad - bar) as f32,
                bar_w as f32,
                bar as f32,
            ),
            color,
        );
    }
    drop(p);
    pm
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str, remaining: bool) -> IconSpec {
        IconSpec {
            text: text.to_owned(),
            rgb: [0xf5, 0x9e, 0x0b],
            remaining,
        }
    }

    #[test]
    fn icon_has_every_size() {
        let icon = build_icon(&spec("42", false));
        for size in SIZES {
            let want = ferrokit_gui::geometry::primitives::Size::new(size as i32, size as i32);
            assert_eq!(
                icon.actual_size(want, IconMode::Normal, IconState::Off),
                want
            );
        }
    }

    /// Needs a font (the Linux CI runner has none, so nothing is drawn there).
    #[cfg(windows)]
    #[test]
    fn icon_changes_with_text_and_mode() {
        let a = render(&spec("42", false), 16);
        assert_eq!(a.data(), render(&spec("42", false), 16).data());
        assert_ne!(a.data(), render(&spec("43", false), 16).data());
        assert_ne!(a.data(), render(&spec("42", true), 16).data());
        // Something was drawn on the tile besides the tile itself.
        assert_ne!(a.data(), render(&spec("", false), 16).data());
    }
}
