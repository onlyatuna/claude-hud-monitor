#![allow(dead_code)]
//! Green -> amber -> red gauge colours (port of the colour half of `python/ui/gauge.py`).
//!
//! A fixed gradient (stops at the status bands 0 / 50 / 75 / 90 %) is revealed up to the current value.
//! Colours are nudged (lightness only) until they read on the surfaces they sit on.

use ferrokit_gui::geometry::primitives::{PointF, RectF};
use ferrokit_gui::paint::brush::{Brush, LinearGradient};
use ferrokit_gui::paint::painter::{Painter, RenderHint};
use ferrokit_gui::tiny_skia::Color;

/// Status band starts (python `STOPS`).
pub const STOPS: [(u32, &str); 4] = [(0, "green"), (50, "yellow"), (75, "orange"), (90, "red")];
/// Default minimum contrast ratio of `readable`.
pub const MIN_CONTRAST: f64 = 3.0;

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

fn rgb8(c: Color) -> (u8, u8, u8) {
    let q = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    (q(c.red()), q(c.green()), q(c.blue()))
}

/// Python `SURFACES[dark]`: the surfaces the gauge must read on.
pub fn surfaces(dark: bool) -> [Color; 3] {
    if dark {
        [
            rgb(0x0e, 0x11, 0x17),
            rgb(0x1e, 0x1e, 0x24),
            rgb(0x30, 0x30, 0x3a),
        ]
    } else {
        [
            rgb(0xf8, 0xfa, 0xfc),
            rgb(0xf6, 0xf4, 0xfa),
            rgb(0xe6, 0xe4, 0xed),
        ]
    }
}

/// `'#rrggbb'` -> opaque colour; invalid input -> black.
pub fn parse_hex(s: &str) -> Color {
    let h = s.trim().strip_prefix('#').unwrap_or("");
    if h.len() != 6 || !h.is_ascii() {
        return Color::BLACK;
    }
    match u32::from_str_radix(h, 16) {
        Ok(v) => rgb((v >> 16) as u8, (v >> 8) as u8, v as u8),
        Err(_) => Color::BLACK,
    }
}

/// `QColor.name()` style `'#rrggbb'` (lowercase).
pub fn to_hex(c: Color) -> String {
    let (r, g, b) = rgb8(c);
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Python `_luminance`: WCAG relative luminance.
pub fn luminance(c: Color) -> f64 {
    let lin = |v: f32| {
        let v = v as f64;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.red()) + 0.7152 * lin(c.green()) + 0.0722 * lin(c.blue())
}

/// Python `contrast`.
pub fn contrast(fg: Color, bg: Color) -> f64 {
    let (a, b) = (luminance(fg), luminance(bg));
    let (low, high) = if a <= b { (a, b) } else { (b, a) };
    (high + 0.05) / (low + 0.05)
}

/// `QColor::getHslF` semantics: hue 0..1 (achromatic -> -1 in Qt; here 0), saturation, lightness.
pub fn to_hsl(c: Color) -> (f64, f64, f64) {
    let (r, g, b) = rgb8(c);
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = if l <= 0.5 {
        d / (max + min)
    } else {
        d / (2.0 - max - min)
    };
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

/// `QColor::fromHslF` (h, s, l all 0..1) rounded to 8-bit channels.
pub fn from_hsl(h: f64, s: f64, l: f64) -> Color {
    let (h, s, l) = (h.clamp(0.0, 1.0), s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let q8 = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    if s == 0.0 {
        let v = q8(l);
        return rgb(v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let chan = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    rgb(
        q8(chan(h + 1.0 / 3.0)),
        q8(chan(h)),
        q8(chan(h - 1.0 / 3.0)),
    )
}

/// Python `readable`: `color` unchanged when it reads on every surface, otherwise lighter (dark) /
/// darker (light) in the same hue.
pub fn readable(
    color: Color,
    dark: bool,
    surfaces_override: Option<&[Color]>,
    minimum: f64,
) -> Color {
    let default = surfaces(dark);
    let surf: &[Color] = match surfaces_override {
        Some(s) if !s.is_empty() => s,
        _ => &default,
    };
    if surf.iter().all(|s| contrast(color, *s) >= minimum) {
        return color;
    }
    let (hue, sat, mut light) = to_hsl(color);
    let step = if dark { 0.025 } else { -0.025 };
    for _ in 0..40 {
        light = (light + step).clamp(0.0, 1.0);
        let candidate = from_hsl(hue.max(0.0), sat, light);
        if surf.iter().all(|s| contrast(candidate, *s) >= minimum) {
            return candidate;
        }
    }
    if dark {
        rgb(0xf2, 0xf2, 0xf7)
    } else {
        rgb(0x1f, 0x1f, 0x24)
    }
}

/// `THEMES[dark|light]['scale'][name]` from `python/ui/styles.py`.
pub fn scale_color(theme_dark: bool, name: &str) -> Color {
    let hex = match (theme_dark, name) {
        (true, "green") => "#8CC79C",
        (true, "yellow") => "#E3C66A",
        (true, "orange") => "#E8A574",
        (true, "red") => "#E07B7B",
        (false, "green") => "#78B08A",
        (false, "yellow") => "#D8B85A",
        (false, "orange") => "#DC9461",
        (false, "red") => "#D46868",
        _ => "#000000",
    };
    parse_hex(hex)
}

/// Python `ramp_stops`.
pub fn ramp_stops(dark: bool, surfaces_override: Option<&[Color]>) -> Vec<(u32, Color)> {
    STOPS
        .iter()
        .map(|&(pos, name)| {
            (
                pos,
                readable(
                    scale_color(dark, name),
                    dark,
                    surfaces_override,
                    MIN_CONTRAST,
                ),
            )
        })
        .collect()
}

/// Python `ramp_color`: gauge colour at `value` percent (smooth blend between the readable stops).
pub fn ramp_color(value: f64, dark: bool, surfaces_override: Option<&[Color]>) -> Color {
    let stops = ramp_stops(dark, surfaces_override);
    let value = value.clamp(0.0, 100.0);
    for w in stops.windows(2) {
        let (p0, c0) = w[0];
        let (p1, c1) = w[1];
        if value <= p1 as f64 {
            let f = if p1 > p0 {
                (value - p0 as f64) / (p1 - p0) as f64
            } else {
                0.0
            };
            let (a, b) = (rgb8(c0), rgb8(c1));
            let mix = |a: u8, b: u8| {
                let a = a as f64;
                (a + (b as f64 - a) * f).round_ties_even().clamp(0.0, 255.0) as u8
            };
            return rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2));
        }
    }
    stops[stops.len() - 1].1
}

/// Python `GaugeBar.paintEvent`: fills `track` with `track_color`, then the gradient revealed up to
/// `fraction` (0..=1). The gradient spans the whole track. With `min_fill_is_height` the fill is at
/// least `height` wide (a round dot); otherwise exactly `width * fraction`.
pub fn draw_gauge_bar(
    painter: &mut Painter,
    track: RectF,
    fraction: f64,
    track_color: Color,
    stops: &[(u32, Color)],
    muted: bool,
    min_fill_is_height: bool,
) {
    painter.set_render_hint(RenderHint::Antialiasing, true);
    painter.set_pen(None);
    let radius = track.height / 2.0;
    painter.set_brush(Brush::Color(track_color));
    painter.draw_rounded_rect(track, radius, radius);
    if fraction <= 0.0 || muted {
        return;
    }
    let left = track.x;
    let right = track.x + track.width;
    let mut gradient = LinearGradient::new(PointF::new(left, 0.0), PointF::new(right, 0.0));
    for &(pos, color) in stops {
        gradient.add_stop(pos as f32 / 100.0, color);
    }
    let fraction = fraction.min(1.0) as f32;
    let fill_w = if min_fill_is_height {
        track.height.max(track.width * fraction)
    } else {
        track.width * fraction
    };
    let fill = RectF::new(track.x, track.y, fill_w, track.height);
    painter.set_brush(Brush::LinearGradient(gradient));
    painter.draw_rounded_rect(fill, radius, radius);
}

/// Python `GaugeBar(QWidget)`: a thin progress bar with a rounded track and the gradient revealed
/// up to the value. Drop-in for the bits of `QProgressBar` the cards use (`setRange` / `setValue` /
/// `value` / `setTextVisible`).
pub struct GaugeBar {
    base: ferrokit_widgets::WidgetBase,
    value: i32,
    minimum: i32,
    maximum: i32,
    dark: bool,
    muted: bool,
}

impl GaugeBar {
    /// `GaugeBar(parent=None, height=5)`: `setFixedHeight(height)`, `Expanding x Fixed`.
    pub fn new(height: i32) -> Self {
        use ferrokit_gui::geometry::primitives::{Rect, Size};
        use ferrokit_widgets::{Policy, QSizePolicy, Widget};
        let base = ferrokit_widgets::WidgetBase::with_geometry(Rect::new(0, 0, 0, height));
        base.set_size_policy(QSizePolicy::new(Policy::Expanding, Policy::Fixed));
        let bar = Self {
            base,
            value: 0,
            minimum: 0,
            maximum: 100,
            dark: true,
            muted: false,
        };
        // `setFixedHeight`: both bounds on the height axis only.
        bar.set_minimum_size(Size::new(0, height));
        bar.set_maximum_size(Size::new(16_777_215, height));
        bar
    }

    pub fn set_range(&mut self, minimum: i32, maximum: i32) {
        self.minimum = minimum;
        self.maximum = maximum.max(minimum + 1);
        ferrokit_widgets::Widget::update(self);
    }

    pub fn set_value(&mut self, value: i32) {
        self.value = value;
        ferrokit_widgets::Widget::update(self);
    }

    pub fn value(&self) -> i32 {
        self.value
    }

    /// `setTextVisible`: the gauge never draws text.
    pub fn set_text_visible(&mut self, _visible: bool) {}

    pub fn set_dark(&mut self, dark: bool) {
        if dark != self.dark {
            self.dark = dark;
            ferrokit_widgets::Widget::update(self);
        }
    }

    pub fn dark(&self) -> bool {
        self.dark
    }

    pub fn set_muted(&mut self, muted: bool) {
        if muted != self.muted {
            self.muted = muted;
            ferrokit_widgets::Widget::update(self);
        }
    }

    /// How full the bar is, 0..=1.
    pub fn fraction(&self) -> f64 {
        let span = (self.maximum - self.minimum) as f64;
        ((self.value - self.minimum) as f64 / span).clamp(0.0, 1.0)
    }
}

impl ferrokit_core::object::QObject for GaugeBar {
    fn object_data(&self) -> &ferrokit_core::object::ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> ferrokit_core::object::ObjectDataMut<'_> {
        ferrokit_core::object::ObjectDataMut::new(&mut self.base.object_data)
    }
}

impl ferrokit_widgets::Widget for GaugeBar {
    fn widget_base(&self) -> &ferrokit_widgets::WidgetBase {
        &self.base
    }
    fn id(&self) -> ferrokit_core::object::ObjectId {
        self.base.id()
    }
    fn geometry(&self) -> ferrokit_gui::geometry::primitives::Rect {
        self.base.geometry()
    }
    fn set_geometry(&self, rect: ferrokit_gui::geometry::primitives::Rect) {
        self.base.set_geometry(rect);
    }
    fn is_visible(&self) -> bool {
        self.base.is_visible()
    }
    fn set_visible(&self, visible: bool) {
        self.base.set_visible(visible);
    }
    fn is_enabled(&self) -> bool {
        self.base.is_enabled()
    }
    fn update(&self) {
        self.base.update();
    }
    fn dirty_rect(&self) -> Option<ferrokit_gui::geometry::primitives::Rect> {
        self.base.dirty_rect()
    }
    fn clear_dirty(&self) {
        self.base.clear_dirty();
    }
    fn parent_widget(&self) -> Option<ferrokit_widgets::WidgetWeak> {
        self.base.parent_widget()
    }
    fn set_parent_widget(&self, parent: Option<ferrokit_widgets::WidgetWeak>) {
        self.base.set_parent_widget(parent);
    }
    fn window_id(&self) -> Option<ferrokit_core::object::ObjectId> {
        self.base.window_id()
    }
    fn set_window_id(&self, window_id: Option<ferrokit_core::object::ObjectId>) {
        self.base.set_window_id(window_id);
    }
    fn children(&self) -> Vec<ferrokit_widgets::WidgetRef> {
        self.base.children()
    }
    fn add_child(&mut self, _child: ferrokit_widgets::WidgetRef) {}
    fn remove_child(&mut self, _child_id: ferrokit_core::object::ObjectId) {}

    /// Python `GaugeBar.paintEvent`.
    fn paint_event(&mut self, painter: &mut Painter) {
        let theme = crate::ui::styles::get_theme(self.dark);
        let geo = self.base.geometry();
        let track = RectF::new(0.0, 0.0, geo.width as f32, geo.height as f32);
        let stops = ramp_stops(self.dark, None);
        draw_gauge_bar(
            painter,
            track,
            self.fraction(),
            theme.track,
            &stops,
            self.muted,
            true,
        );
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paints `bar` (200 x height) into a pixmap and returns it.
    fn grab(bar: &mut GaugeBar, width: u32) -> ferrokit_gui::paint::pixmap::Pixmap {
        use ferrokit_gui::geometry::primitives::Rect;
        use ferrokit_widgets::Widget;
        let h = bar.geometry().height.max(1) as u32;
        bar.set_geometry(Rect::new(0, 0, width as i32, h as i32));
        let mut pixmap = ferrokit_gui::paint::pixmap::Pixmap::new(width, h).unwrap();
        let mut painter = Painter::begin(&mut pixmap);
        ferrokit_widgets::Widget::paint_event(bar, &mut painter);
        drop(painter);
        pixmap
    }

    #[test]
    fn api_matches_what_the_cards_use() {
        let mut bar = GaugeBar::new(5);
        bar.set_range(0, 100);
        bar.set_text_visible(false);
        bar.set_value(40);
        assert_eq!(bar.value(), 40);
        assert!((bar.fraction() - 0.4).abs() < 1e-9);
        bar.set_value(250);
        assert_eq!(bar.fraction(), 1.0);
        bar.set_value(-3);
        assert_eq!(bar.fraction(), 0.0);
    }

    #[test]
    fn paints_in_both_themes_and_when_empty() {
        let mut bar = GaugeBar::new(5);
        for dark in [true, false] {
            bar.set_dark(dark);
            for value in [0, 1, 50, 100] {
                bar.set_value(value);
                let pixmap = grab(&mut bar, 120);
                assert_eq!(pixmap.physical_width(), 120);
            }
        }
    }

    #[test]
    fn fill_is_a_revealed_gradient_not_one_flat_colour() {
        let mut bar = GaugeBar::new(6);
        bar.set_value(100);
        let pixmap = grab(&mut bar, 200);
        let left = pixmap.pixel(8, 3).unwrap();
        let right = pixmap.pixel(192, 3).unwrap();
        assert_ne!(
            (left.red(), left.green(), left.blue()),
            (right.red(), right.green(), right.blue())
        );
        assert!(right.red() > left.red(), "warmer on the right");
    }

    #[test]
    fn stops_follow_the_status_bands() {
        for dark in [true, false] {
            let pos: Vec<u32> = ramp_stops(dark, None).iter().map(|(p, _)| *p).collect();
            assert_eq!(pos, vec![0, 50, 75, 90]);
        }
    }

    #[test]
    fn every_gauge_colour_reads_on_its_surfaces() {
        for dark in [true, false] {
            for value in (0..=100).step_by(5) {
                let color = ramp_color(value as f64, dark, None);
                for surface in surfaces(dark) {
                    assert!(contrast(color, surface) >= 3.0, "{value}% dark={dark}");
                }
            }
        }
    }

    #[test]
    fn colour_warms_up_as_usage_rises() {
        for dark in [true, false] {
            let (low, high) = (ramp_color(5.0, dark, None), ramp_color(95.0, dark, None));
            let (lr, hr) = (rgb8(low), rgb8(high));
            assert!(
                hr.0 as i32 - hr.1 as i32 > lr.0 as i32 - lr.1 as i32,
                "dark={dark}"
            );
        }
    }

    #[test]
    fn readable_keeps_good_colours_and_only_moves_lightness() {
        let keep = parse_hex("#1f1f24");
        assert_eq!(to_hex(readable(keep, false, None, MIN_CONTRAST)), "#1f1f24");
        let original = parse_hex("#E3C66A");
        let adjusted = readable(original, false, None, MIN_CONTRAST);
        assert_ne!(to_hex(adjusted), to_hex(original));
        let (oh, _, ol) = to_hsl(original);
        let (mh, _, ml) = to_hsl(adjusted);
        assert!((oh - mh).abs() < 0.04);
        assert!(ml < ol);
    }

    #[test]
    fn hex_roundtrip() {
        for s in ["#000000", "#ffffff", "#8cc79c", "#e07b7b", "#0e1117"] {
            assert_eq!(to_hex(parse_hex(s)), s);
        }
        assert_eq!(to_hex(parse_hex("bogus")), "#000000");
    }

    #[test]
    fn ramp_ends_match_first_and_last_stop() {
        for dark in [true, false] {
            let stops = ramp_stops(dark, None);
            assert_eq!(to_hex(ramp_color(0.0, dark, None)), to_hex(stops[0].1));
            assert_eq!(to_hex(ramp_color(100.0, dark, None)), to_hex(stops[3].1));
        }
    }
}
