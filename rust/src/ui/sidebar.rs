#![allow(dead_code)]
//! Scalable quota tiles and reusable provider-card flyouts (port of `python/ui/sidebar.py`).
//!
//! Compact desktop tokens: 4 px rhythm, 12 px labels, native buttons, a visible 2 px focus ring and
//! paired AA text colours; hover is supplementary to keyboard and click activation. Motion honours
//! the system accessibility preference.
//!
//! This file holds the parts of `sidebar.py` that need no widget: the tokens and constants, the
//! row/tile data (`_row_label`, `_color`, `_refresh_tile`, `_refresh_tooltip`, `_row_values`,
//! `_window_reset`) and the geometry the widgets use (`preferred_size`, `_flyout_position`,
//! `_move_reorder`, the resize maths of `HUDWindow`). The widget classes call these.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use ferrokit_gui::tiny_skia::Color;

use crate::pace::{reset_stamp, scale_level, short_window};
use crate::providers::base::{format_countdown, UsageMetrics};
use crate::ui::dock_geometry::{py_round, Rect};
use crate::ui::gauge;
use crate::ui::styles::get_theme;

/// Python `SIDEBAR_TOKENS['dark' | 'light']`.
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub background: Color,
    pub text: Color,
    pub muted: Color,
    pub hover: Color,
    pub pressed: Color,
    pub focus: Color,
    pub separator: Color,
    pub stale: Color,
    pub alert: Color,
    pub font: &'static str,
    pub font_size: i32,
    pub spacing: i32,
    pub radius: i32,
    pub focus_width: i32,
    pub hover_ms: u64,
    pub leave_ms: u64,
    pub animation_ms: u64,
}

impl Tokens {
    /// `providers[key]`: the brand colour of a service in this theme.
    pub fn provider(&self, key: &str, dark: bool) -> Color {
        crate::ui::brand::brand_color(key, dark)
    }
}

pub fn tokens(dark: bool) -> Tokens {
    let hex = gauge::parse_hex;
    if dark {
        Tokens {
            background: hex("#1e1e24"),
            text: hex("#f2f2f7"),
            muted: hex("#a6a6b3"),
            hover: hex("#30303a"),
            pressed: hex("#40404c"),
            focus: hex("#38bdf8"),
            separator: hex("#454550"),
            stale: hex("#f59e0b"),
            alert: hex("#dc2626"),
            font: "Segoe UI",
            font_size: 12,
            spacing: 4,
            radius: 6,
            focus_width: 2,
            hover_ms: 300,
            leave_ms: 600,
            animation_ms: 160,
        }
    } else {
        Tokens {
            background: hex("#f6f4fa"),
            text: hex("#1f1f24"),
            muted: hex("#555563"),
            hover: hex("#e6e4ed"),
            pressed: hex("#d8d6e1"),
            focus: hex("#0369a1"),
            separator: hex("#ceced8"),
            stale: hex("#92400e"),
            alert: hex("#b91c1c"),
            font: "Segoe UI",
            font_size: 12,
            spacing: 4,
            radius: 6,
            focus_width: 2,
            hover_ms: 300,
            leave_ms: 600,
            animation_ms: 160,
        }
    }
}

pub const BASE_BUTTON_W: i32 = 64;
pub const BASE_BUTTON_H: i32 = 64;
pub const BASE_PANEL_W: i32 = 220;
/// Python `_NAMES` (accessible name / tooltip heading) in `DEFAULT_ORDER`.
pub const NAMES: [(&str, &str); 3] = [("claude", "Claude"), ("agy", "AGY"), ("codex", "Codex")];
pub const DEFAULT_ORDER: [&str; 3] = ["claude", "agy", "codex"];
pub const HOLD_MS: u64 = 350;
/// Same bands as the status scale.
pub const RAMP_STOPS: [(u32, &str); 4] = [(0, "green"), (50, "yellow"), (75, "orange"), (90, "red")];
/// A row at or above this used % marks the tile.
pub const ALERT_PCT: f64 = 90.0;
/// How long a tile glows after one of its windows reset.
pub const FLASH_MS: u64 = 1800;
pub const FLASH_STEP_MS: u64 = 60;
/// The flyout is at most this wide (`setFixedSize(min(260, area.width()), ...)`).
pub const FLYOUT_WIDTH: i32 = 260;
/// Python `SidebarWidget.set_scale` bounds.
pub const SCALE_MIN: f64 = 0.6;
pub const SCALE_MAX: f64 = 2.0;

/// Python `_NAMES.get(kind)`.
pub fn name_of(kind: &str) -> Option<&'static str> {
    NAMES.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n)
}

/// Python `_row_label`: short row tag. 5H / 7D for single-group providers, a two-letter group
/// prefix for agy's groups.
pub fn row_label(title: &str) -> String {
    let upper = title.to_uppercase();
    for (prefix, tag) in [("GEMINI ", "Ge "), ("C/G ", "CG "), ("ARGON ", "Ar ")] {
        if let Some(rest) = upper.strip_prefix(prefix) {
            return format!("{tag}{rest}");
        }
    }
    short_window(title)
}

/// Python `_color(value, which, force_scale)` on `THEMES[dark|light]`: the status colour of a value
/// (`scheme == "scale"`), or the fixed duo colour of the row (`inner` / `outer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Scale,
    Duo,
}

impl Scheme {
    pub fn parse(name: &str) -> Option<Scheme> {
        match name {
            "scale" => Some(Scheme::Scale),
            "duo" => Some(Scheme::Duo),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Inner,
    Outer,
}

pub fn status_color(dark: bool, scheme: Scheme, value: Option<f64>, which: Which, force_scale: bool) -> Color {
    let theme = get_theme(dark);
    if scheme == Scheme::Duo && !force_scale {
        return match which {
            Which::Inner => theme.duo_inner,
            Which::Outer => theme.duo_outer,
        };
    }
    match scale_level(value) {
        Some("green") => theme.scale_green,
        Some("yellow") => theme.scale_yellow,
        Some("orange") => theme.scale_orange,
        Some("red") => theme.scale_red,
        _ => theme.neutral,
    }
}

/// Python `_row_values`: `(title, val, reset)` of both main rows, then the extra rows.
pub type RowValue = (String, Option<f64>, Option<DateTime<Utc>>);

pub fn row_values(data: &UsageMetrics) -> Vec<RowValue> {
    let mut rows = vec![
        (data.metric1_title.clone(), data.metric1_val, data.metric1_reset),
        (data.metric2_title.clone(), data.metric2_val, data.metric2_reset),
    ];
    rows.extend(data.extra_metrics.iter().map(|r| (r.title.clone(), r.val, r.reset)));
    rows
}

/// What `update_metrics` remembers per provider to spot a window reset: upper-cased title ->
/// `(val, reset)`.
pub type RowState = HashMap<String, (Option<f64>, Option<DateTime<Utc>>)>;

pub fn row_state(data: &UsageMetrics) -> RowState {
    row_values(data)
        .into_iter()
        .map(|(title, val, reset)| (title.to_uppercase(), (val, reset)))
        .collect()
}

/// Python `_window_reset`: true when any row moved to a later reset time with lower usage (its
/// window rolled over).
pub fn window_reset(previous: &RowState, current: &RowState) -> bool {
    for (title, (val, reset)) in current {
        let Some((old_val, old_reset)) = previous.get(title) else {
            continue;
        };
        let (Some(val), Some(reset), Some(old_val), Some(old_reset)) = (val, reset, old_val, old_reset) else {
            continue;
        };
        if (*reset - *old_reset).num_seconds() > 600 && val < old_val {
            return true;
        }
    }
    false
}

/// Python `_Button` display state for a provider tile, as `_refresh_tile` sets it.
#[derive(Debug, Clone, PartialEq)]
pub struct TileRow {
    pub label: String,
    pub value: Option<f64>,
    pub color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TileState {
    pub rows: Vec<TileRow>,
    pub display: [String; 2],
    pub dot_colors: Vec<Color>,
    pub alert: bool,
    pub offline: bool,
    pub stale: bool,
}

/// Python `update_metrics`' first lines: `offline = bool(error and not stale)`.
pub fn is_offline(data: &UsageMetrics) -> bool {
    data.error.is_some() && !data.stale
}

/// Python `_refresh_tile` for one provider: row labels / values / colours, the display strings
/// (`OFF` when offline), agy's per-bucket dots and the alert mark.
pub fn refresh_tile(key: &str, data: &UsageMetrics, dark: bool, scheme: Scheme) -> TileState {
    let offline = is_offline(data);
    let values = if offline {
        [None, None]
    } else {
        [data.metric1_val, data.metric2_val]
    };
    let labels = [row_label(&data.metric1_title), row_label(&data.metric2_title)];
    let rows = labels
        .iter()
        .zip(values)
        .zip([Which::Inner, Which::Outer])
        .map(|((label, value), which)| TileRow {
            label: label.clone(),
            value,
            color: status_color(dark, scheme, value, which, false),
        })
        .collect();
    let text = |v: Option<f64>| v.map_or_else(|| "--".to_owned(), |v| format!("{v:.0}%"));
    let mut display = [text(values[0]), text(values[1])];
    if offline {
        display = ["OFF".to_owned(), "--".to_owned()];
    }
    let dot_colors = if key == "agy" {
        data.extra_metrics
            .iter()
            .map(|row| status_color(dark, scheme, row.val, Which::Inner, true))
            .collect()
    } else {
        Vec::new()
    };
    let alert = !offline
        && row_values(data)
            .iter()
            .any(|(_, val, _)| val.is_some_and(|v| v >= ALERT_PCT));
    TileState {
        rows,
        display,
        dot_colors,
        alert,
        offline,
        stale: data.stale,
    }
}

/// Python `_refresh_tooltip`: the text of a tile's tooltip (and accessible description).
pub fn tooltip_text(key: &str, data: &UsageMetrics, alert: bool) -> String {
    let mut lines = vec![name_of(key).unwrap_or("").to_owned()];
    for (title, value, reset) in row_values(data) {
        let text = value.map_or_else(|| "--".to_owned(), |v| format!("{v:.0}%"));
        let label = if key != "agy" { row_label(&title) } else { title.clone() };
        let stamp = reset_stamp(reset, &title);
        let when = if stamp.is_empty() {
            format_countdown(reset)
        } else {
            format!("{stamp} · {}", format_countdown(reset))
        };
        lines.push(format!("{label} 已用 {text}，重設於 {when}"));
    }
    if alert {
        lines.push(format!("! 接近上限（已用 ≥ {ALERT_PCT:.0}%）"));
    }
    if let Some(error) = &data.error {
        lines.push(error.clone());
    }
    if data.stale {
        lines.push("STALE".to_owned());
    }
    lines.join("\n")
}

/// Python `_Button.text_color`: the row colour, nudged lighter (dark theme) or darker (light theme)
/// only as far as needed to read on the tile. The bar and the number share this one colour.
pub fn tile_surfaces(dark: bool) -> [Color; 3] {
    let t = tokens(dark);
    [t.background, t.hover, t.pressed]
}

pub fn text_color(color: Color, dark: bool) -> Color {
    gauge::readable(color, dark, Some(&tile_surfaces(dark)), gauge::MIN_CONTRAST)
}

/// Python `_Button.ramp_stops`: status colours at their band starts, each readable on the tile.
pub fn ramp_stops(dark: bool) -> Vec<(u32, Color)> {
    gauge::ramp_stops(dark, Some(&tile_surfaces(dark)))
}

/// Python `_Button.ramp(value)`.
pub fn ramp(value: f64, dark: bool) -> Color {
    gauge::ramp_color(value, dark, Some(&tile_surfaces(dark)))
}

/// Python `_Button.row_color(index)`: one colour per row, shared by the bar's leading edge and the
/// number.
pub fn row_color(row: &TileRow, dark: bool, scheme: Scheme) -> Color {
    match row.value {
        Some(value) if scheme != Scheme::Duo => ramp(value, dark),
        _ => text_color(row.color, dark),
    }
}

// ---- geometry --------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

impl Orientation {
    pub fn parse(name: &str) -> Option<Orientation> {
        match name {
            "vertical" => Some(Orientation::Vertical),
            "horizontal" => Some(Orientation::Horizontal),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Orientation::Vertical => "vertical",
            Orientation::Horizontal => "horizontal",
        }
    }
}

/// Python `set_scale`: `min(2.0, max(0.6, scale))`; a non-finite scale is an error there.
pub fn clamp_scale(scale: f64) -> Option<f64> {
    scale.is_finite().then(|| scale.clamp(SCALE_MIN, SCALE_MAX))
}

/// `round(base * scale)` as Python rounds it.
pub fn scaled(base: i32, scale: f64) -> i32 {
    py_round(base as f64 * scale)
}

/// Python `preferred_size` of the strip: `QSize(w, h)`.
pub fn strip_preferred_size(scale: f64, orientation: Orientation, spacing: i32, count: i32) -> (i32, i32) {
    let pad = scaled(spacing, scale) * 2;
    let button_w = scaled(BASE_BUTTON_W, scale);
    let button_h = scaled(BASE_BUTTON_H, scale);
    let handle = scaled(22, scale);
    match orientation {
        Orientation::Vertical => (button_w + pad, handle + count * button_h + pad),
        Orientation::Horizontal => (count * button_w + handle + pad, button_h + pad),
    }
}

/// Python `preferred_size` of the panel, given the height the panel layout asks for with the
/// cards laid out (`_panel_layout.sizeHint().height()`) and the number of cards.
pub fn panel_preferred_size(
    scale: f64,
    orientation: Orientation,
    spacing: i32,
    count: i32,
    layout_height: i32,
) -> (i32, i32) {
    let pad = scaled(spacing, scale) * 2;
    let handle = scaled(22, scale);
    let panel_w = scaled(BASE_PANEL_W, scale);
    match orientation {
        Orientation::Vertical => (panel_w, handle + layout_height + pad),
        Orientation::Horizontal => {
            let gap = 2 * scaled(4, scale) + 1.max(py_round(scale));
            let panel_width = count * panel_w + 0.max(count - 1) * gap;
            (handle + panel_width + pad, handle.max(layout_height) + pad)
        }
    }
}

/// Python `_flyout_position`: where the flyout opens beside a tile (`tile` in global coordinates,
/// the flyout `size`, the screen `area`), kept inside the area.
pub fn flyout_position(tile: Rect, size: (i32, i32), area: Rect, dock_edge: Option<&str>) -> (i32, i32) {
    let (tx, ty, tw, th) = tile;
    let (w, h) = size;
    let (ax, ay, aw, ah) = area;
    // `QRect.right()` / `bottom()`: the last pixel inside.
    let (right, bottom) = (tx + tw - 1, ty + th - 1);
    let (x, y) = match dock_edge {
        Some("left") => (right + 1, ty),
        Some("top") => (tx, bottom + 1),
        Some("bottom") => (tx, ty - h),
        _ => (tx - w, ty),
    };
    let (area_right, area_bottom) = (ax + aw - 1, ay + ah - 1);
    (
        ax.max(x.min(area_right - w + 1)),
        ay.max(y.min(area_bottom - h + 1)),
    )
}

/// Python `_move_reorder`: the provider order after dragging `key` to the cursor coordinate `coordinate`
/// (y when vertical, x when horizontal) in strip coordinates; `centers` gives each key's tile centre
/// along the same axis.
pub fn reorder(order: &[String], key: &str, coordinate: i32, centers: &HashMap<String, i32>) -> Vec<String> {
    let others: Vec<&String> = order.iter().filter(|k| k.as_str() != key).collect();
    let index = others
        .iter()
        .filter(|other| coordinate > centers.get(other.as_str()).copied().unwrap_or(i32::MAX))
        .count();
    let mut result: Vec<String> = others.iter().map(|s| (*s).clone()).collect();
    result.insert(index, key.to_owned());
    result
}

/// Python `set_order`: a permutation of the three providers, else `None` (Python raises).
pub fn valid_order(order: &[String]) -> Option<Vec<String>> {
    let filtered: Vec<String> = order
        .iter()
        .filter(|k| DEFAULT_ORDER.contains(&k.as_str()))
        .cloned()
        .collect();
    let mut a = filtered.clone();
    let mut b: Vec<String> = DEFAULT_ORDER.iter().map(|s| s.to_string()).collect();
    a.sort();
    b.sort();
    (a == b).then_some(filtered)
}

/// Python `eventFilter` wheel handling: accumulates `delta` (1/8 degree units, 120 = one notch)
/// and returns whole steps (+1 / -1 per notch) with the remainder to keep.
pub fn wheel_steps(remainder: i32, delta: i32) -> (Vec<i32>, i32) {
    let mut remainder = remainder + delta;
    let mut steps = Vec::new();
    while remainder.abs() >= 120 {
        let step = if remainder > 0 { 1 } else { -1 };
        remainder -= step * 120;
        steps.push(step);
    }
    (steps, remainder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::MetricRow;
    use chrono::Duration;

    fn dark_themes() -> [bool; 2] {
        [true, false]
    }

    #[test]
    fn row_labels_follow_the_chosen_rows() {
        assert_eq!(row_label("C/G 5H"), "CG 5H");
        assert_eq!(row_label("ARGON 7D"), "Ar 7D");
        assert_eq!(row_label("GEMINI 7D"), "Ge 7D");
        assert_eq!(row_label("SESSION 5H"), "5H");
        assert_eq!(row_label("WEEKLY 7D"), "7D");
        assert_eq!(row_label(""), "");
    }

    #[test]
    fn tokens_are_complete_for_both_themes_and_match_python() {
        assert_eq!(tokens(true).stale, gauge::parse_hex("#f59e0b"));
        assert_eq!(tokens(false).stale, gauge::parse_hex("#92400e"));
        assert_eq!(tokens(true).background, gauge::parse_hex("#1e1e24"));
        assert_eq!((tokens(true).hover_ms, tokens(true).leave_ms, tokens(true).animation_ms), (300, 600, 160));
    }

    #[test]
    fn metrics_and_offline_tiles() {
        let data = UsageMetrics {
            provider_id: "claude".into(),
            metric1_val: Some(20.0),
            metric2_val: Some(8.0),
            ..Default::default()
        };
        let tile = refresh_tile("claude", &data, true, Scheme::Scale);
        assert_eq!(tile.display, ["20%", "8%"]);
        assert_eq!(tile.rows[0].label, "5H");
        assert_eq!(tile.rows[0].color, status_color(true, Scheme::Scale, Some(20.0), Which::Inner, false));
        assert_eq!(tile.rows[1].color, status_color(true, Scheme::Scale, Some(8.0), Which::Outer, false));

        let blank = refresh_tile("claude", &UsageMetrics { provider_id: "claude".into(), ..Default::default() }, true, Scheme::Scale);
        assert_eq!(blank.display, ["--", "--"]);
        assert!(blank.rows.iter().all(|r| r.value.is_none()));

        let offline = refresh_tile(
            "claude",
            &UsageMetrics { error: Some("offline".into()), metric1_val: Some(20.0), ..Default::default() },
            true,
            Scheme::Scale,
        );
        assert!(offline.offline);
        assert_eq!(offline.display, ["OFF", "--"]);
        assert!(offline.rows.iter().all(|r| r.value.is_none()));

        let stale = refresh_tile(
            "claude",
            &UsageMetrics {
                error: Some("timeout".into()),
                stale: true,
                metric1_val: Some(0.0),
                metric1_text: "0%".into(),
                ..Default::default()
            },
            true,
            Scheme::Scale,
        );
        assert!(stale.stale && !stale.offline);
        assert_eq!(stale.display, ["0%", "--"]);
        let tip = tooltip_text("claude", &UsageMetrics { stale: true, error: Some("timeout".into()), ..Default::default() }, false);
        assert!(tip.contains("STALE"));
    }

    #[test]
    fn agy_dots_follow_the_extra_rows() {
        let data = UsageMetrics {
            provider_id: "agy".into(),
            extra_metrics: vec![MetricRow::new("C/G 7D", Some(15.0), "15%"), MetricRow::new("ARGON 7D", Some(21.0), "21%")],
            ..Default::default()
        };
        let tile = refresh_tile("agy", &data, true, Scheme::Scale);
        let green = get_theme(true).scale_green;
        assert_eq!(tile.dot_colors, vec![green, green]);
        assert!(tooltip_text("agy", &data, false).contains("C/G 7D 已用 15%"));
        // the duo scheme never changes the dots
        let light = refresh_tile("agy", &data, false, Scheme::Duo);
        assert_eq!(light.dot_colors, vec![get_theme(false).scale_green; 2]);
        let none = refresh_tile("agy", &UsageMetrics { provider_id: "agy".into(), ..Default::default() }, false, Scheme::Duo);
        assert!(none.dot_colors.is_empty());
        let unknown = refresh_tile(
            "agy",
            &UsageMetrics { extra_metrics: vec![MetricRow::new("C/G 7D", None, "--")], ..Default::default() },
            false,
            Scheme::Duo,
        );
        assert_eq!(unknown.dot_colors, vec![get_theme(false).neutral]);
        let three = refresh_tile(
            "agy",
            &UsageMetrics {
                metric1_title: "C/G 5H".into(),
                metric1_val: Some(3.0),
                metric2_title: "ARGON 7D".into(),
                metric2_val: Some(21.0),
                extra_metrics: vec![
                    MetricRow::new("GEMINI 5H", Some(0.0), "0%"),
                    MetricRow::new("GEMINI 7D", Some(1.0), "1%"),
                    MetricRow::new("C/G 7D", Some(15.0), "15%"),
                ],
                ..Default::default()
            },
            true,
            Scheme::Scale,
        );
        assert_eq!(three.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["CG 5H", "Ar 7D"]);
        assert_eq!(three.dot_colors.len(), 3);
    }

    #[test]
    fn tooltip_lists_every_row_with_a_countdown() {
        let data = UsageMetrics {
            provider_id: "agy".into(),
            metric1_val: Some(21.0),
            metric2_val: Some(1.0),
            extra_metrics: vec![MetricRow::new("C/G 7D", Some(15.0), "15%"), MetricRow::new("ARGON 7D", Some(21.0), "21%")],
            ..Default::default()
        };
        let tip = tooltip_text("agy", &data, false);
        for text in ["已用", "重設於 --", "C/G 7D", "ARGON 7D"] {
            assert!(tip.contains(text), "{text}");
        }
        assert!(!tip.contains('?'));
    }

    #[test]
    fn tile_colors_read_on_every_tile_surface() {
        for dark in dark_themes() {
            for scheme in [Scheme::Scale, Scheme::Duo] {
                let data = UsageMetrics { metric1_val: Some(95.0), metric2_val: Some(10.0), ..Default::default() };
                let tile = refresh_tile("claude", &data, dark, scheme);
                assert_eq!(tile.rows[0].color, status_color(dark, scheme, Some(95.0), Which::Inner, false));
                assert_ne!(tile.rows[0].color, tile.rows[1].color);
                for row in &tile.rows {
                    let shown = text_color(row.color, dark);
                    for surface in tile_surfaces(dark) {
                        assert!(gauge::contrast(shown, surface) >= 3.0);
                    }
                }
            }
        }
        let scale = refresh_tile("claude", &UsageMetrics { metric1_val: Some(95.0), metric2_val: Some(10.0), ..Default::default() }, true, Scheme::Scale);
        let duo = refresh_tile("claude", &UsageMetrics { metric1_val: Some(95.0), metric2_val: Some(10.0), ..Default::default() }, true, Scheme::Duo);
        assert_ne!(
            scale.rows.iter().map(|r| r.color).collect::<Vec<_>>(),
            duo.rows.iter().map(|r| r.color).collect::<Vec<_>>()
        );
    }

    #[test]
    fn bar_and_number_always_share_one_colour() {
        for dark in dark_themes() {
            for scheme in [Scheme::Scale, Scheme::Duo] {
                for value in [0.0, 30.0, 60.0, 85.0, 95.0] {
                    let data = UsageMetrics { metric1_val: Some(value), metric2_val: Some(value), ..Default::default() };
                    let tile = refresh_tile("codex", &data, dark, scheme);
                    for row in &tile.rows {
                        let shown = row_color(row, dark, scheme);
                        for surface in tile_surfaces(dark) {
                            assert!(gauge::contrast(shown, surface) >= 3.0);
                        }
                        if scheme == Scheme::Scale {
                            assert_eq!(shown, ramp(value, dark));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn scale_bars_are_a_green_to_red_gauge() {
        let stops = ramp_stops(true);
        assert_eq!(stops.iter().map(|(p, _)| *p).collect::<Vec<_>>(), [0, 50, 75, 90]);
        assert_eq!(ramp(0.0, true), stops[0].1);
        assert_eq!(ramp(100.0, true), stops[3].1);
        let mid = gauge::to_hex(ramp(25.0, true));
        assert_ne!(mid, gauge::to_hex(stops[0].1));
        assert_ne!(mid, gauge::to_hex(stops[1].1));
        let channel = |v: f64, c: fn(&ferrokit_gui::tiny_skia::Color) -> f32| c(&ramp(v, true));
        assert!(channel(95.0, |c| c.red()) > channel(0.0, |c| c.red()));
        assert!(channel(95.0, |c| c.green()) < channel(0.0, |c| c.green()));
    }

    #[test]
    fn tile_shows_an_alert_mark_from_ninety_percent() {
        let metrics = |v1: f64, error: Option<&str>| UsageMetrics {
            metric1_val: Some(v1),
            metric2_val: Some(10.0),
            error: error.map(str::to_owned),
            ..Default::default()
        };
        assert!(!refresh_tile("claude", &metrics(89.0, None), true, Scheme::Scale).alert);
        let tile = refresh_tile("claude", &metrics(90.0, None), true, Scheme::Scale);
        assert!(tile.alert);
        assert!(tooltip_text("claude", &metrics(90.0, None), tile.alert).contains("90%"));
        assert!(!refresh_tile("claude", &metrics(95.0, Some("offline")), true, Scheme::Scale).alert);
        let agy = UsageMetrics {
            metric1_val: Some(1.0),
            metric2_val: Some(2.0),
            extra_metrics: vec![MetricRow::new("ARGON 7D", Some(93.0), "93%")],
            ..Default::default()
        };
        assert!(refresh_tile("agy", &agy, true, Scheme::Scale).alert, "extra rows count too");
    }

    #[test]
    fn a_window_reset_needs_a_later_reset_and_lower_usage() {
        let now = Utc::now();
        let data = |val: f64, reset: DateTime<Utc>| UsageMetrics {
            provider_id: "codex".into(),
            metric1_title: "WINDOW 5H".into(),
            metric1_val: Some(val),
            metric1_reset: Some(reset),
            metric2_title: "WINDOW 7D".into(),
            metric2_val: Some(5.0),
            metric2_reset: Some(now + Duration::days(5)),
            ..Default::default()
        };
        let first = row_state(&data(80.0, now + Duration::hours(1)));
        assert!(!window_reset(&RowState::new(), &first));
        let growth = row_state(&data(82.0, now + Duration::hours(1)));
        assert!(!window_reset(&first, &growth));
        let rolled = row_state(&data(3.0, now + Duration::hours(5)));
        assert!(window_reset(&growth, &rolled));
        let drift = row_state(&data(40.0, now + Duration::hours(5) + Duration::minutes(2)));
        assert!(!window_reset(&rolled, &drift));
    }

    #[test]
    fn geometry_of_the_strip_and_panel() {
        // the Python test: a 1.0 vertical strip is 72 wide (64 + 2 * 4)
        assert_eq!(strip_preferred_size(1.0, Orientation::Vertical, 4, 3), (72, 22 + 3 * 64 + 8));
        assert_eq!(strip_preferred_size(1.0, Orientation::Horizontal, 4, 3), (3 * 64 + 22 + 8, 72));
        let (w, h) = strip_preferred_size(1.5, Orientation::Vertical, 4, 3);
        assert!((w as f64 - 72.0 * 1.5).abs() <= 3.0 && (h as f64 - 222.0 * 1.5).abs() <= 3.0);
        assert_eq!(panel_preferred_size(1.0, Orientation::Vertical, 4, 3, 500).0, 220);
        let (w, h) = panel_preferred_size(1.0, Orientation::Horizontal, 4, 3, 100);
        assert_eq!((w, h), (22 + 3 * 220 + 2 * (8 + 1) + 8, 22.max(100) + 8));
        assert_eq!(clamp_scale(0.5), Some(0.6));
        assert_eq!(clamp_scale(f64::NAN), None);
        assert_eq!(clamp_scale(3.0), Some(2.0));
    }

    #[test]
    fn flyout_opens_beside_the_tile_and_stays_inside_the_screen() {
        let tile = (500, 400, 64, 64);
        let size = (260, 200);
        let area = (0, 0, 2000, 1500);
        for edge in [Some("right"), Some("left"), Some("top"), Some("bottom"), None] {
            let (x, y) = flyout_position(tile, size, area, edge);
            assert!(x >= 0 && y >= 0 && x + size.0 <= 2000 && y + size.1 <= 1500);
            match edge {
                Some("right") | None => assert_eq!(x + size.0, tile.0),
                Some("left") => assert_eq!(x, tile.0 + tile.2),
                Some("top") => assert_eq!(y, tile.1 + tile.3),
                Some("bottom") => assert_eq!(y + size.1, tile.1),
                _ => {}
            }
        }
        let small = (490, 390, 300, 250);
        let (x, y) = flyout_position(tile, size, small, Some("right"));
        assert!(x >= 490 && y >= 390 && x + size.0 <= 790 && y + size.1 <= 640);
    }

    #[test]
    fn press_hold_drag_reorders() {
        let order: Vec<String> = ["claude", "agy", "codex"].map(String::from).to_vec();
        let centers: HashMap<String, i32> =
            [("claude".to_owned(), 55), ("agy".to_owned(), 119), ("codex".to_owned(), 183)].into();
        // dragged below codex's centre: claude goes last
        assert_eq!(reorder(&order, "claude", 200, &centers), ["agy", "codex", "claude"]);
        assert_eq!(reorder(&order, "claude", 0, &centers), ["claude", "agy", "codex"]);
        assert_eq!(reorder(&order, "codex", 100, &centers), ["claude", "codex", "agy"]);
    }

    #[test]
    fn order_rejects_bad_values() {
        let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(valid_order(&v(&["codex", "claude", "agy"])), Some(v(&["codex", "claude", "agy"])));
        assert_eq!(valid_order(&v(&["claude", "agy"])), None);
        assert_eq!(valid_order(&v(&["claude", "agy", "agy"])), None);
    }

    #[test]
    fn ctrl_wheel_notches_accumulate_into_steps() {
        assert_eq!(wheel_steps(0, 240), (vec![1, 1], 0));
        assert_eq!(wheel_steps(0, -120), (vec![-1], 0));
        assert_eq!(wheel_steps(0, 60), (vec![], 60));
        assert_eq!(wheel_steps(60, 60), (vec![1], 0));
        assert_eq!(wheel_steps(-100, -50), (vec![-1], -30));
    }
}
