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
use ferrokit_gui::color::Color;

use crate::pace::{reset_stamp, scale_level, short_window};
use crate::providers::base::{format_countdown, UsageMetrics};
use crate::ui::dock_geometry::{py_round, Rect as GeoRect};
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
    let hex = Color::from;
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
pub const RAMP_STOPS: [(u32, &str); 4] =
    [(0, "green"), (50, "yellow"), (75, "orange"), (90, "red")];
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

pub fn status_color(
    dark: bool,
    scheme: Scheme,
    value: Option<f64>,
    which: Which,
    force_scale: bool,
) -> Color {
    let theme = get_theme(dark);
    if scheme == Scheme::Duo && !force_scale {
        return match which {
            Which::Inner => Color::from(theme.duo_inner),
            Which::Outer => Color::from(theme.duo_outer),
        };
    }
    match scale_level(value) {
        Some("green") => Color::from(theme.scale_green),
        Some("yellow") => Color::from(theme.scale_yellow),
        Some("orange") => Color::from(theme.scale_orange),
        Some("red") => Color::from(theme.scale_red),
        _ => Color::from(theme.neutral),
    }
}

/// Python `_row_values`: `(title, val, reset)` of both main rows, then the extra rows.
pub type RowValue = (String, Option<f64>, Option<DateTime<Utc>>);

pub fn row_values(data: &UsageMetrics) -> Vec<RowValue> {
    let mut rows = vec![
        (
            data.metric1_title.clone(),
            data.metric1_val,
            data.metric1_reset,
        ),
        (
            data.metric2_title.clone(),
            data.metric2_val,
            data.metric2_reset,
        ),
    ];
    rows.extend(
        data.extra_metrics
            .iter()
            .map(|r| (r.title.clone(), r.val, r.reset)),
    );
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
        let (Some(val), Some(reset), Some(old_val), Some(old_reset)) =
            (val, reset, old_val, old_reset)
        else {
            continue;
        };
        if (*reset - *old_reset).num_milliseconds() > 600_000 && val < old_val {
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
    let labels = [
        row_label(&data.metric1_title),
        row_label(&data.metric2_title),
    ];
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
        let label = if key != "agy" {
            row_label(&title)
        } else {
            title.clone()
        };
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
    gauge::readable(
        color,
        dark,
        Some(&tile_surfaces(dark)[..]),
        gauge::MIN_CONTRAST,
    )
}

/// Python `_Button.ramp_stops`: status colours at their band starts, each readable on the tile.
pub fn ramp_stops(dark: bool) -> Vec<(u32, Color)> {
    gauge::ramp_stops(dark, Some(&tile_surfaces(dark)[..]))
}

/// Python `_Button.ramp(value)`.
pub fn ramp(value: f64, dark: bool) -> Color {
    gauge::ramp_color(value, dark, Some(&tile_surfaces(dark)[..]))
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
pub fn strip_preferred_size(
    scale: f64,
    orientation: Orientation,
    spacing: i32,
    count: i32,
) -> (i32, i32) {
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
pub fn flyout_position(
    tile: GeoRect,
    size: (i32, i32),
    area: GeoRect,
    dock_edge: Option<&str>,
) -> (i32, i32) {
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
pub fn reorder(
    order: &[String],
    key: &str,
    coordinate: i32,
    centers: &HashMap<String, i32>,
) -> Vec<String> {
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

// ---- widgets ---------------------------------------------------------------------------------
//
// `_Indicators`, `_Button`, `_Flyout` and `SidebarWidget` of `sidebar.py`. Python's `owner` back
// references are `Weak<SidebarInner>`; its signals are `Notifier`s. Every call from a button or
// filter into the sidebar that touches layouts or other widgets is queued (`defer`): Python runs
// those synchronously, but here the widget that is being notified is mutably borrowed while its
// handler runs and a layout pass would borrow it again.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Instant;

use ferrokit_core::animation::{EasingCurve, EasingType, PropertyAnimation};
use ferrokit_core::event::{Event, EventKind};
use ferrokit_core::object::{
    register_owned_qobject, ObjectData, ObjectDataMut, ObjectId, OwnedObject, QObject,
};
use ferrokit_core::timer::Timer;
use ferrokit_gui::geometry::primitives::{Margins, Point, PointF, Rect, RectF, Size};
use ferrokit_gui::paint::brush::Brush;
use ferrokit_gui::paint::painter::{Painter, RenderHint};
use ferrokit_gui::text::font::{Font, FontWeight};
use ferrokit_gui::text::font_metrics::FontMetrics;
use ferrokit_gui::tiny_skia::PathBuilder;
use ferrokit_platform::cursor::CursorShape;
use ferrokit_widgets::cursor::Cursor;
use ferrokit_widgets::frame::Frame;
use ferrokit_widgets::layout::Direction;
use ferrokit_widgets::{
    Alignment, BoxLayout, EmptyWidget, Layout, Policy, QSizePolicy, Widget, WidgetBase, WidgetRef,
    WidgetWeak, Window,
};

use crate::providers::base::UsageMetrics as Metrics;
use crate::ui::provider_card::ProviderCardWidget;
use crate::ui::styles::get_cards_stylesheet;
use crate::ui::{round_pen, Notifier, UiThread};

/// Python `QWIDGETSIZE_MAX`.
const QWIDGETSIZE_MAX: i32 = 16_777_215;
/// Python `_Flyout.RADIUS`.
const FLYOUT_RADIUS: f32 = 10.0;
/// Python `Qt.Key_Escape`, `Key_Return`, `Key_Enter`, `Key_Space`.
const KEY_ESCAPE: u32 = 0x0100_0000;
const KEY_RETURN: u32 = 0x0100_0004;
const KEY_ENTER: u32 = 0x0100_0005;
const KEY_SPACE: u32 = 0x20;
/// `Qt::ControlModifier` in `EventKind::Wheel::modifiers`.
const CONTROL_MODIFIER: u32 = 0x0400_0000;

/// Runs `f` once the event that is being dispatched is done and no widget is borrowed.
fn defer(f: impl FnOnce() + 'static) {
    ferrokit_widgets::WidgetCommandQueue::post_task(f);
}

/// Python `SidebarWidget._animations_enabled` (the system's reduced-motion preference).
pub fn animations_enabled() -> bool {
    ferrokit_gui::application::GuiApplication::animation_duration_factor() != 0.0
}

/// `QColor.setAlphaF(alpha)` on a copy.
fn rgba(color: Color, alpha: f32) -> Color {
    let mut c = color;
    c.set_alpha_f(alpha);
    c
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Right,
    Center,
}

/// `QPainter.drawText(rect, align | Qt.AlignVCenter, text)`.
fn draw_text_in(
    painter: &mut Painter,
    rect: RectF,
    align: Align,
    text: &str,
    font: &Font,
    color: Color,
) {
    let metrics = FontMetrics::from_font(font);
    let width = metrics.horizontal_advance_exact(text, font);
    let x = match align {
        Align::Left => rect.x,
        Align::Right => rect.x + rect.width - width,
        Align::Center => rect.x + (rect.width - width) / 2.0,
    };
    let line = metrics.ascent + metrics.descent;
    let baseline = rect.y + (rect.height - line) / 2.0 + metrics.ascent;
    painter.draw_text_colored(PointF::new(x, baseline), text, font, gauge::skia(color));
}

fn font_px(family: &str, px: f32, bold: bool) -> Font {
    let font = Font::new(family, px);
    if bold {
        font.with_weight(FontWeight::Bold)
    } else {
        font
    }
}

/// Python `round(x)` of a pixel size, at least `minimum`.
fn px(value: f64, minimum: i32) -> f32 {
    minimum.max(py_round(value)) as f32
}

// ---- _Button -----------------------------------------------------------------------------------

/// What `_Button` keeps in attributes, shared with its `_Indicators` child and with the owner.
pub struct ButtonData {
    pub kind: String,
    pub tile: RefCell<TileState>,
    pub flash: Cell<f64>,
    pub show_labels: Cell<bool>,
    down: Cell<bool>,
    press_pos: Cell<Option<Point>>,
    dragged: Cell<bool>,
    reordering: Cell<bool>,
    hold: Timer,
    owner: Weak<SidebarInner>,
    /// The button's own handle, for what `_begin_reorder` does to the widget.
    me: RefCell<Option<WidgetWeak>>,
}

impl ButtonData {
    fn is_provider(&self) -> bool {
        name_of(&self.kind).is_some()
    }

    fn widget(&self) -> Option<WidgetRef> {
        self.me.borrow().as_ref().and_then(|w| w.upgrade())
    }

    fn update(&self) {
        if let Some(w) = self.widget() {
            if let Ok(w) = w.try_borrow() {
                w.update();
            }
        }
    }

    /// Drops a press / hold / drag that is in progress (the sidebar changes shape or order under
    /// it): no stuck `reordering`, down state or closed-hand cursor.
    fn reset_interaction(&self) {
        self.hold.stop();
        self.press_pos.set(None);
        self.dragged.set(false);
        let was_reordering = self.reordering.replace(false);
        let was_down = self.down.replace(false);
        if was_reordering {
            if let Some(w) = self.widget() {
                if let Ok(w) = w.try_borrow() {
                    w.set_cursor(CursorShape::PointingHand);
                }
            }
        }
        if was_reordering || was_down {
            self.update();
        }
    }

    /// Python `_begin_reorder` (the hold timer fired).
    fn begin_reorder(&self) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if self.press_pos.get().is_none()
            || self.dragged.get()
            || !self.is_provider()
            || owner.state.get() != State::Strip
        {
            return;
        }
        self.reordering.set(true);
        self.down.set(false);
        if let Some(w) = self.widget() {
            if let Ok(w) = w.try_borrow() {
                w.set_cursor(CursorShape::ClosedHand);
            }
        }
        self.update();
        owner.start_reorder(&self.kind);
    }
}

/// Python `_Indicators`: status painted above the tile without intercepting its input.
pub struct Indicators {
    base: WidgetBase,
    data: Rc<ButtonData>,
    owner: Weak<SidebarInner>,
}

impl Indicators {
    fn new(data: Rc<ButtonData>, owner: Weak<SidebarInner>) -> Self {
        let base = WidgetBase::new();
        // `WA_TransparentForMouseEvents`
        base.set_transparent_for_mouse_events(true);
        Self { base, data, owner }
    }
}

impl QObject for Indicators {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> ObjectDataMut<'_> {
        ObjectDataMut::new(&mut self.base.object_data)
    }
}

impl Widget for Indicators {
    fn widget_base(&self) -> &WidgetBase {
        &self.base
    }
    fn id(&self) -> ObjectId {
        self.base.id()
    }
    fn geometry(&self) -> Rect {
        self.base.geometry()
    }
    fn set_geometry(&self, rect: Rect) {
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
    fn dirty_rect(&self) -> Option<Rect> {
        self.base.dirty_rect()
    }
    fn clear_dirty(&self) {
        self.base.clear_dirty();
    }
    fn parent_widget(&self) -> Option<WidgetWeak> {
        self.base.parent_widget()
    }
    fn set_parent_widget(&self, parent: Option<WidgetWeak>) {
        self.base.set_parent_widget(parent);
    }
    fn window_id(&self) -> Option<ObjectId> {
        self.base.window_id()
    }
    fn set_window_id(&self, window_id: Option<ObjectId>) {
        self.base.set_window_id(window_id);
    }
    fn children(&self) -> Vec<WidgetRef> {
        self.base.children()
    }
    fn add_child(&mut self, child: WidgetRef) {
        self.base.add_child(child);
    }
    fn remove_child(&mut self, child_id: ObjectId) {
        self.base.remove_child(child_id);
    }

    /// Python `_Indicators.paintEvent`.
    fn paint_event(&mut self, p: &mut Painter) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let t = owner.tokens();
        let scale = owner.scale.get() as f32;
        p.set_render_hint(RenderHint::Antialiasing, true);
        let mut right = self.base.geometry().width as f32;
        let tile = self.data.tile.borrow();
        if tile.alert {
            // warning triangle with "!" -- readable without relying on colour
            let size = 11.0 * scale;
            let (x, y) = (right - size - 2.0 * scale, 1.5 * scale);
            let mut path = PathBuilder::new();
            path.move_to(x + size / 2.0, y);
            path.line_to(x + size, y + size * 0.92);
            path.line_to(x, y + size * 0.92);
            path.close();
            p.set_pen(None);
            p.set_brush(Brush::Color(gauge::skia(t.alert)));
            if let Some(path) = path.finish() {
                p.fill_path(&path);
            }
            let font = font_px(t.font, (6.0f32).max((8.0 * scale).round()), true);
            draw_text_in(
                p,
                RectF::new(x, y + size * 0.28, size, size * 0.64),
                Align::Center,
                "!",
                &font,
                Color::from(ferrokit_gui::GlobalColor::White),
            );
            right -= size + 4.0 * scale;
        }
        if tile.stale {
            p.set_pen(None);
            p.set_brush(Brush::Color(gauge::skia(t.stale)));
            p.draw_ellipse(RectF::new(
                right - 10.0 * scale,
                0.0,
                6.0 * scale,
                6.0 * scale,
            ));
        }
        if self.data.kind == "agy" {
            let count = tile.dot_colors.len() as f32;
            for (i, color) in tile.dot_colors.iter().enumerate() {
                p.set_pen(Some(round_pen(gauge::skia(t.background), scale)));
                let fill = if tile.offline { t.muted } else { *color };
                p.set_brush(Brush::Color(gauge::skia(fill)));
                p.draw_ellipse(RectF::new(
                    (60.0 - 8.0 * count + 8.0 * i as f32) * scale,
                    10.0 * scale,
                    6.0 * scale,
                    6.0 * scale,
                ));
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Python `_Button(QPushButton)`.
pub struct SidebarButton {
    base: WidgetBase,
    data: Rc<ButtonData>,
    owner: Weak<SidebarInner>,
    /// QAbstractButton's `clicked`.
    pub clicked: Rc<Notifier<()>>,
    last_move_accepted: bool,
}

impl SidebarButton {
    /// Builds the button and its indicators child; returns the widget and the shared data.
    pub fn create(
        kind: &str,
        owner: &Rc<SidebarInner>,
    ) -> (WidgetRef, Rc<ButtonData>, Rc<Notifier<()>>) {
        let hold = Timer::new();
        hold.set_single_shot(true);
        let data = Rc::new(ButtonData {
            kind: kind.to_owned(),
            tile: RefCell::new(initial_tile()),
            flash: Cell::new(0.0),
            show_labels: Cell::new(owner.scale.get() >= 0.8),
            down: Cell::new(false),
            press_pos: Cell::new(None),
            dragged: Cell::new(false),
            reordering: Cell::new(false),
            hold,
            owner: Rc::downgrade(owner),
            me: RefCell::new(None),
        });
        let weak = UiThread::new(Rc::downgrade(&data));
        data.hold.timeout.connect(move |()| {
            if let Some(data) = weak.get().and_then(|w| w.upgrade()) {
                data.begin_reorder();
            }
        });
        let base = WidgetBase::new();
        base.set_focus_policy(ferrokit_widgets::FocusPolicy::StrongFocus);
        base.set_mouse_tracking(true);
        let clicked = Rc::new(Notifier::new());
        let mut button = SidebarButton {
            base,
            data: Rc::clone(&data),
            owner: Rc::downgrade(owner),
            clicked: Rc::clone(&clicked),
            last_move_accepted: false,
        };
        let indicators: WidgetRef = Rc::new(RefCell::new(Box::new(Indicators::new(
            Rc::clone(&data),
            Rc::downgrade(owner),
        ))));
        button.base.add_child(indicators);
        let widget: WidgetRef = Rc::new(RefCell::new(Box::new(button)));
        *data.me.borrow_mut() = Some(Rc::downgrade(&widget));
        {
            let w = widget.borrow();
            w.set_cursor(CursorShape::PointingHand);
            // Python `setAccessibleName(_NAMES.get(kind, 'Toggle sidebar'))`, `setToolTip(name)`
            let name = name_of(kind).unwrap_or("Toggle sidebar");
            w.set_accessible_name(name);
        }
        widget
            .borrow_mut()
            .set_tool_tip(name_of(kind).unwrap_or("Toggle sidebar"));
        (widget, data, clicked)
    }

    fn indicators(&self) -> Option<WidgetRef> {
        self.base.children().into_iter().next()
    }

    /// `QAbstractButton::click`: queued, because the owner's slot opens a flyout beside this
    /// (borrowed) widget.
    fn click(&self) {
        let clicked = Rc::clone(&self.clicked);
        defer(move || clicked.emit(&()));
    }

    fn set_down(&mut self, down: bool) {
        if self.data.down.get() != down {
            self.data.down.set(down);
            self.update();
        }
    }

    fn escape(&self) {
        if let Some(owner) = self.owner.upgrade() {
            defer(move || owner.close_flyout());
        }
    }
}

fn initial_tile() -> TileState {
    let neutral = Color::from(get_theme(true).neutral);
    TileState {
        rows: vec![
            TileRow {
                label: "5H".into(),
                value: None,
                color: neutral,
            },
            TileRow {
                label: "7D".into(),
                value: None,
                color: neutral,
            },
        ],
        display: ["--".to_owned(), "--".to_owned()],
        dot_colors: vec![neutral; 3],
        alert: false,
        offline: false,
        stale: false,
    }
}

impl QObject for SidebarButton {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> ObjectDataMut<'_> {
        ObjectDataMut::new(&mut self.base.object_data)
    }

    /// Python `_Button.event` plus the `QPushButton` mouse handling.
    fn event(&mut self, event: &mut Event) -> bool {
        match &event.kind {
            EventKind::ToolTip { .. } => {
                // The flyout already shows the details; a native tooltip on top of it only flickers.
                self.owner
                    .upgrade()
                    .is_some_and(|o| o.active.borrow().is_some())
            }
            EventKind::Enter { .. } => {
                self.enter_event(Point::new(0, 0));
                true
            }
            EventKind::Leave => {
                self.leave_event();
                true
            }
            EventKind::MouseButtonPress { x, y, button } => {
                self.mouse_press_event(Point::new(*x, *y), *button, 0);
                if *button != 1 {
                    event.ignore();
                }
                true
            }
            EventKind::MouseMove { x, y } => {
                self.last_move_accepted = false;
                self.mouse_move_event(Point::new(*x, *y));
                self.last_move_accepted
            }
            EventKind::MouseButtonRelease { x, y, button } => {
                self.mouse_release_event(Point::new(*x, *y), *button, 0);
                if *button != 1 {
                    event.ignore();
                }
                true
            }
            EventKind::FocusIn { .. } | EventKind::FocusOut { .. } => {
                self.update();
                true
            }
            EventKind::KeyPress {
                key,
                modifiers,
                is_repeat,
            } => {
                self.key_press_event(*key, *modifiers, *is_repeat);
                true
            }
            EventKind::KeyRelease { key, modifiers } => {
                self.key_release_event(*key, *modifiers);
                true
            }
            _ => false,
        }
    }
}

impl Widget for SidebarButton {
    fn widget_base(&self) -> &WidgetBase {
        &self.base
    }
    fn id(&self) -> ObjectId {
        self.base.id()
    }
    fn geometry(&self) -> Rect {
        self.base.geometry()
    }
    fn set_geometry(&self, rect: Rect) {
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
    fn dirty_rect(&self) -> Option<Rect> {
        self.base.dirty_rect()
    }
    fn clear_dirty(&self) {
        self.base.clear_dirty();
    }
    fn parent_widget(&self) -> Option<WidgetWeak> {
        self.base.parent_widget()
    }
    fn set_parent_widget(&self, parent: Option<WidgetWeak>) {
        self.base.set_parent_widget(parent);
    }
    fn window_id(&self) -> Option<ObjectId> {
        self.base.window_id()
    }
    fn set_window_id(&self, window_id: Option<ObjectId>) {
        self.base.set_window_id(window_id);
        for child in self.base.children() {
            child.borrow().set_window_id(window_id);
        }
    }
    fn children(&self) -> Vec<WidgetRef> {
        self.base.children()
    }
    fn add_child(&mut self, child: WidgetRef) {
        self.base.add_child(child);
    }
    fn remove_child(&mut self, child_id: ObjectId) {
        self.base.remove_child(child_id);
    }
    fn focus_policy(&self) -> ferrokit_widgets::FocusPolicy {
        self.base.focus_policy()
    }
    fn set_focus_policy(&self, policy: ferrokit_widgets::FocusPolicy) {
        self.base.set_focus_policy(policy);
    }
    fn has_focus(&self) -> bool {
        self.base.has_focus()
    }
    fn set_has_focus(&self, focus: bool) {
        self.base.set_has_focus(focus);
    }

    fn enter_event(&mut self, _pos: Point) {
        self.update();
        if self.data.is_provider() {
            if let Some(owner) = self.owner.upgrade() {
                let kind = self.data.kind.clone();
                defer(move || owner.hover(&kind));
            }
        }
    }

    fn leave_event(&mut self) {
        self.update();
        if let Some(owner) = self.owner.upgrade() {
            owner.hover_timer.stop();
            owner.schedule_close();
        }
    }

    /// Python `mousePressEvent`.
    fn mouse_press_event(&mut self, pos: Point, button: u32, _modifiers: u32) {
        if button == 1 {
            self.data.press_pos.set(Some(pos));
            self.data.dragged.set(false);
            if self.data.is_provider() {
                self.data.hold.start_with_interval(HOLD_MS);
            }
            // QAbstractButton::mousePressEvent
            self.set_down(true);
        }
    }

    /// Python `mouseMoveEvent`.
    fn mouse_move_event(&mut self, pos: Point) {
        if self.data.reordering.get() {
            let global = Cursor::pos();
            if let Some(owner) = self.owner.upgrade() {
                let kind = self.data.kind.clone();
                defer(move || owner.move_reorder(&kind, global));
            }
            self.last_move_accepted = true;
            return;
        }
        if let (Some(press), false) = (self.data.press_pos.get(), self.data.dragged.get()) {
            let (dx, dy) = ((pos.x - press.x) as f64, (pos.y - press.y) as f64);
            if dx.hypot(dy) > 6.0 {
                self.data.hold.stop();
                self.data.dragged.set(true);
                self.set_down(false);
                if let Some(owner) = self.owner.upgrade() {
                    owner.hover_timer.stop();
                    defer(move || owner.drag_requested.emit(&()));
                }
            }
        }
    }

    /// Python `mouseReleaseEvent`.
    fn mouse_release_event(&mut self, pos: Point, button: u32, _modifiers: u32) {
        self.data.hold.stop();
        if self.data.reordering.get() {
            self.data.reordering.set(false);
            self.data.press_pos.set(None);
            self.set_down(false);
            self.set_cursor(CursorShape::PointingHand);
            if let Some(owner) = self.owner.upgrade() {
                defer(move || owner.finish_reorder());
            }
            return;
        }
        let dragged = self.data.dragged.get();
        self.data.press_pos.set(None);
        if dragged && button == 1 {
            self.set_down(false);
            return;
        }
        // QAbstractButton::mouseReleaseEvent: `clicked` when it ended inside
        if button == 1 && self.data.down.get() {
            self.set_down(false);
            let g = self.base.geometry();
            if Rect::new(0, 0, g.width, g.height).contains(pos) {
                self.click();
            }
        }
    }

    /// Python `keyPressEvent`.
    fn key_press_event(&mut self, key: u32, _modifiers: u32, _is_repeat: bool) {
        match key {
            KEY_RETURN | KEY_ENTER => self.click(),
            KEY_ESCAPE => self.escape(),
            KEY_SPACE => self.set_down(true),
            _ => {}
        }
    }

    fn key_release_event(&mut self, key: u32, _modifiers: u32) {
        if key == KEY_SPACE && self.data.down.get() {
            self.set_down(false);
            self.click();
        }
    }

    /// Python `_Button.paintEvent`.
    fn paint_event(&mut self, p: &mut Painter) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let t = owner.tokens();
        let dark = owner.dark.get();
        let scale = owner.scale.get();
        let s = scale as f32;
        let geo = self.base.geometry();
        let (w, h) = (geo.width as f32, geo.height as f32);
        let tile = self.data.tile.borrow();
        let radius = t.radius as f32 * s;
        let inner = RectF::new(s, s, w - 2.0 * s, h - 2.0 * s);
        p.set_render_hint(RenderHint::Antialiasing, true);
        p.set_pen(None);
        let fill = if self.data.down.get() {
            t.pressed
        } else if self.under_mouse() {
            t.hover
        } else {
            t.background
        };
        p.set_brush(Brush::Color(gauge::skia(fill)));
        p.draw_rounded_rect(inner, radius, radius);
        let provider = self.data.is_provider();
        let flash = self.data.flash.get();
        if flash > 0.0 && provider {
            let glow = rgba(
                t.provider(&self.data.kind, dark),
                (0.45 * flash).min(0.45) as f32,
            );
            p.set_pen(None);
            p.set_brush(Brush::Color(gauge::skia(glow)));
            p.draw_rounded_rect(inner, radius, radius);
        }
        if provider {
            // identity stripe: the service colour, never a usage colour
            p.set_pen(None);
            let stripe = if tile.offline {
                t.muted
            } else {
                t.provider(&self.data.kind, dark)
            };
            p.set_brush(Brush::Color(gauge::skia(stripe)));
            p.draw_rounded_rect(
                RectF::new(1.5 * s, 7.0 * s, 2.5 * s, h - 14.0 * s),
                1.2 * s,
                1.2 * s,
            );
        }
        if self.has_focus() {
            p.set_brush(Brush::NoBrush);
            let color = if provider {
                t.provider(&self.data.kind, dark)
            } else {
                t.focus
            };
            p.set_pen(Some(round_pen(
                gauge::skia(color),
                t.focus_width as f32 * s,
            )));
            p.draw_rounded_rect(
                RectF::new(2.0 * s, 2.0 * s, w - 4.0 * s, h - 4.0 * s),
                radius,
                radius,
            );
        }
        let family = t.font;
        if self.data.kind == "toggle" {
            let font = font_px(
                family,
                (8.0f32).max((t.font_size as f32 * s).round()),
                false,
            );
            let glyph = if owner.state.get() == State::Strip {
                "\u{2261}"
            } else {
                "\u{25a5}"
            };
            draw_text_in(
                p,
                RectF::new(0.0, 0.0, w, h),
                Align::Center,
                glyph,
                &font,
                t.text,
            );
            return;
        }
        let name_color = if tile.offline {
            t.muted
        } else {
            t.provider(&self.data.kind, dark)
        };
        let title_width = if self.data.kind == "agy" { 30.0 } else { 52.0 };
        let font = font_px(family, (8.0f32).max((t.font_size as f32 * s).round()), true);
        draw_text_in(
            p,
            RectF::new(6.0 * s, 2.0 * s, title_width * s, 16.0 * s),
            Align::Left,
            name_of(&self.data.kind).unwrap_or(""),
            &font,
            name_color,
        );
        let scheme = owner.scheme.get();
        let theme = get_theme(dark);
        for (index, row) in tile.rows.iter().enumerate() {
            let y = (19 + index as i32 * 21) as f32 * s;
            if self.data.show_labels.get() {
                let label_font = font_px(family, (8.0f32).max((10.0 * s).round()), false);
                draw_text_in(
                    p,
                    RectF::new(6.0 * s, y, 30.0 * s, 15.0 * s),
                    Align::Left,
                    &row.label,
                    &label_font,
                    t.muted,
                );
            }
            let shown = row_color(row, dark, scheme);
            let value_font = font_px(family, (8.0f32).max((12.0 * s).round()), true);
            draw_text_in(
                p,
                RectF::new(6.0 * s, y, 52.0 * s, 15.0 * s),
                Align::Right,
                &tile.display[index],
                &value_font,
                if tile.offline { t.muted } else { shown },
            );
            let track = RectF::new(6.0 * s, y + 15.0 * s, 52.0 * s, 4.0 * s);
            let fraction = match row.value {
                Some(value) if !tile.offline => value.clamp(0.0, 100.0) / 100.0,
                _ => 0.0,
            };
            let stops: Vec<(u32, Color)> = if scheme == Scheme::Duo {
                vec![(0, shown), (100, shown)]
            } else {
                // a fixed green -> amber -> red gauge revealed up to the value: the leading edge
                // always carries the same colour as the number
                ramp_stops(dark)
            };
            gauge::draw_gauge_bar(
                p,
                track,
                fraction,
                Color::from(theme.track),
                &stops,
                false,
                false,
            );
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---- SidebarWidget -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Strip,
    Panel,
}

impl State {
    pub fn parse(name: &str) -> Option<State> {
        match name {
            "strip" => Some(State::Strip),
            "panel" => Some(State::Panel),
            _ => None,
        }
    }
}

/// Everything `SidebarWidget` keeps; reached from timers, filters and buttons through `Weak`.
pub struct SidebarInner {
    container: WidgetRef,
    strip: WidgetRef,
    panel: WidgetRef,
    toggle: std::cell::OnceCell<WidgetRef>,
    buttons: std::cell::OnceCell<Vec<(String, WidgetRef, Rc<ButtonData>)>>,
    cards: Vec<(String, Rc<RefCell<ProviderCardWidget>>)>,
    dividers: RefCell<Vec<WidgetRef>>,
    flyout: Rc<RefCell<Window>>,
    flyout_root: WidgetRef,
    /// Name of the screen the HUD window is on (`QWidget.screen()` of a tile); kept current by the
    /// HUD from `Window::screen_changed`. `None` until the HUD has told the sidebar.
    host_screen: Arc<parking_lot::Mutex<Option<String>>>,
    state: Cell<State>,
    orientation: Cell<Orientation>,
    scale: Cell<f64>,
    dock_edge: RefCell<Option<String>>,
    dark: Cell<bool>,
    scheme: Cell<Scheme>,
    metrics: RefCell<std::collections::HashMap<String, Metrics>>,
    active: RefCell<Option<String>>,
    pinned: Cell<bool>,
    pinned_key: RefCell<Option<String>>,
    hover_id: RefCell<Option<String>>,
    row_state: RefCell<std::collections::HashMap<String, RowState>>,
    wheel_remainder: Cell<i32>,
    order: RefCell<Vec<String>>,
    hover_timer: Timer,
    leave_timer: Timer,
    animation: RefCell<PropertyAnimation>,
    animation_timer: Timer,
    animation_clock: Cell<Option<Instant>>,
    filters: RefCell<Vec<OwnedObject<SidebarFilter>>>,
    // Python signals
    pub state_change_requested: Notifier<String>,
    pub context_menu_requested: Notifier<Point>,
    pub scale_step_requested: Notifier<i32>,
    pub drag_requested: Notifier<()>,
    /// provider ids in their new order (press-hold a tile, then drag)
    pub order_changed: Notifier<Vec<String>>,
}

/// Python `SidebarWidget`: a handle to the shared state.
#[derive(Clone)]
pub struct SidebarWidget {
    inner: Rc<SidebarInner>,
}

/// Looks the widget of `kind` up in an `(id, widget, data)` list.
fn find_button<'a>(
    buttons: &'a [(String, WidgetRef, Rc<ButtonData>)],
    key: &str,
) -> Option<&'a (String, WidgetRef, Rc<ButtonData>)> {
    buttons.iter().find(|(k, _, _)| k == key)
}

fn make_widget<W: Widget + 'static>(w: W) -> WidgetRef {
    Rc::new(RefCell::new(Box::new(w)))
}

fn box_layout_widget(direction: Direction) -> WidgetRef {
    let widget = make_widget(EmptyWidget::new());
    let mut layout = BoxLayout::new(direction);
    layout.set_margins(Margins::new(0, 0, 0, 0));
    layout.set_spacing(0);
    widget.borrow_mut().set_layout(Box::new(layout));
    widget
}

fn box_index_of(layout: &BoxLayout, widget: &WidgetRef) -> Option<usize> {
    (0..layout.count())
        .find(|&i| Layout::item_at(layout, i).is_some_and(|w| Rc::ptr_eq(&w, widget)))
}

fn layout_remove(owner: &WidgetRef, widget: &WidgetRef) {
    owner.borrow().with_box_layout(|l| {
        if let Some(index) = box_index_of(l, widget) {
            l.remove_widget(index);
        }
    });
}

fn layout_add(owner: &WidgetRef, widget: &WidgetRef) {
    owner
        .borrow()
        .with_box_layout(|l| Layout::add_widget(l, Rc::clone(widget)));
}

/// Python `re.sub(r'([\d.]+)px', lambda m: f'{float(m[1])*s:g}px', sheet)`.
pub fn scale_px_in_qss(sheet: &str, scale: f64) -> String {
    let bytes = sheet.as_bytes();
    let mut out = String::with_capacity(sheet.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() || bytes[i] == b'.' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if sheet[i..].starts_with("px") {
                if let Ok(value) = sheet[start..i].parse::<f64>() {
                    out.push_str(&format_g(value * scale));
                    continue;
                }
            }
            out.push_str(&sheet[start..i]);
        } else {
            let ch = sheet[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Python's `f'{x:g}'`: six significant digits, no trailing zeros.
pub fn format_g(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let magnitude = value.abs().log10().floor() as i32;
    let decimals = (5 - magnitude).clamp(0, 15) as usize;
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

impl SidebarWidget {
    /// Python `SidebarWidget.__init__(cards)`; `flyout` is the tool window `_Flyout` that is
    /// parented to the HUD window.
    pub fn new(cards: Vec<(String, Rc<RefCell<ProviderCardWidget>>)>, flyout: Window) -> Self {
        let container = make_widget(EmptyWidget::new());
        container.borrow_mut().set_object_name("Sidebar");
        let mut layout = BoxLayout::new(Direction::TopToBottom);
        layout.set_spacing(0);
        container.borrow_mut().set_layout(Box::new(layout));
        let strip = box_layout_widget(Direction::TopToBottom);
        let panel = box_layout_widget(Direction::TopToBottom);
        let flyout_root = flyout.root_widget();
        {
            let mut vbox = BoxLayout::new(Direction::TopToBottom);
            vbox.set_margins(Margins::new(8, 8, 8, 8));
            flyout_root.borrow_mut().set_layout(Box::new(vbox));
        }

        let inner = Rc::new({
            SidebarInner {
                container: Rc::clone(&container),
                strip: Rc::clone(&strip),
                panel: Rc::clone(&panel),
                toggle: std::cell::OnceCell::new(),
                buttons: std::cell::OnceCell::new(),
                cards: cards.clone(),
                dividers: RefCell::new(Vec::new()),
                flyout: Rc::new(RefCell::new(flyout)),
                flyout_root: Rc::clone(&flyout_root),
                host_screen: Arc::new(parking_lot::Mutex::new(None)),
                state: Cell::new(State::Strip),
                orientation: Cell::new(Orientation::Vertical),
                scale: Cell::new(1.0),
                dock_edge: RefCell::new(None),
                dark: Cell::new(true),
                scheme: Cell::new(Scheme::Scale),
                metrics: RefCell::new(Default::default()),
                active: RefCell::new(None),
                pinned: Cell::new(false),
                pinned_key: RefCell::new(None),
                hover_id: RefCell::new(None),
                row_state: RefCell::new(Default::default()),
                wheel_remainder: Cell::new(0),
                order: RefCell::new(DEFAULT_ORDER.iter().map(|s| s.to_string()).collect()),
                hover_timer: Timer::new(),
                leave_timer: Timer::new(),
                animation: RefCell::new(PropertyAnimation::new("windowOpacity")),
                animation_timer: Timer::new(),
                animation_clock: Cell::new(None),
                filters: RefCell::new(Vec::new()),
                state_change_requested: Notifier::new(),
                context_menu_requested: Notifier::new(),
                scale_step_requested: Notifier::new(),
                drag_requested: Notifier::new(),
                order_changed: Notifier::new(),
            }
        });
        // The buttons need `owner`, so they are made once the Rc exists (interior mutability for
        // the two fields that hold them).
        let mut built: Vec<(String, WidgetRef, Rc<ButtonData>)> = Vec::new();
        let (toggle_widget, toggle_data, toggle_clicked) = SidebarButton::create("toggle", &inner);
        built.push(("toggle".to_owned(), Rc::clone(&toggle_widget), toggle_data));
        let mut provider_clicks: Vec<(String, Rc<Notifier<()>>)> = Vec::new();
        for key in DEFAULT_ORDER {
            let (widget, data, clicked) = SidebarButton::create(key, &inner);
            built.push((key.to_owned(), widget, data));
            provider_clicks.push((key.to_owned(), clicked));
        }
        let _ = inner.toggle.set(toggle_widget);
        let _ = inner.buttons.set(
            built
                .into_iter()
                .filter(|(k, _, _)| k != "toggle")
                .collect(),
        );
        let this = SidebarWidget { inner };
        this.wire(toggle_clicked, provider_clicks);
        this
    }

    fn wire(
        &self,
        toggle_clicked: Rc<Notifier<()>>,
        provider_clicks: Vec<(String, Rc<Notifier<()>>)>,
    ) {
        let inner = &self.inner;
        // toggle: stateChangeRequested.emit('panel' if strip else 'strip')
        {
            let weak = Rc::downgrade(inner);
            toggle_clicked.connect(move |()| {
                if let Some(inner) = weak.upgrade() {
                    let next = if inner.state.get() == State::Strip {
                        "panel"
                    } else {
                        "strip"
                    };
                    inner.state_change_requested.emit(&next.to_owned());
                }
            });
        }
        for (key, clicked) in provider_clicks {
            let weak = Rc::downgrade(inner);
            clicked.connect(move |()| {
                if let Some(inner) = weak.upgrade() {
                    inner.click_provider(&key);
                }
            });
        }
        // layout: toggle, strip, panel; the provider tiles in the strip
        let container = Rc::clone(&inner.container);
        layout_add(&container, inner.toggle());
        layout_add(&container, &inner.strip);
        layout_add(&container, &inner.panel);
        for (_, widget, _) in inner.buttons() {
            layout_add(&inner.strip, widget);
        }
        ferrokit_widgets::widget::set_widget_visible(&inner.panel, false);
        // cards: hidden until the panel or the flyout shows them
        for (_, card) in &inner.cards {
            let widget = card.borrow().widget();
            widget.borrow().set_visible(false);
            widget.borrow().set_minimum_size(Size::new(0, 0));
            widget
                .borrow()
                .set_size_policy(QSizePolicy::new(Policy::Ignored, Policy::Preferred));
        }
        // timers
        let weak = UiThread::new(Rc::downgrade(inner));
        inner.hover_timer.set_single_shot(true);
        inner.hover_timer.timeout.connect({
            let weak = UiThread::new(Rc::downgrade(inner));
            move |()| {
                if let Some(inner) = weak.get().and_then(|w| w.upgrade()) {
                    let key = inner.hover_id.borrow().clone();
                    if let Some(key) = key {
                        inner.open_flyout(&key);
                    }
                }
            }
        });
        inner.leave_timer.set_single_shot(true);
        inner.leave_timer.timeout.connect({
            let weak = UiThread::new(Rc::downgrade(inner));
            move |()| {
                if let Some(inner) = weak.get().and_then(|w| w.upgrade()) {
                    inner.maybe_close();
                }
            }
        });
        inner.animation_timer.set_interval(16);
        inner.animation_timer.timeout.connect(move |()| {
            if let Some(inner) = weak.get().and_then(|w| w.upgrade()) {
                inner.animation_tick();
            }
        });
        // `self._animation.setEasingCurve(QEasingCurve.OutCubic)`
        inner
            .animation
            .borrow_mut()
            .animation_mut()
            .set_easing_curve(EasingCurve::new(EasingType::OutCubic));
        // `self._flyout.installEventFilter(self)`, `self.installEventFilter(self)` and the children
        inner.install_filters();
        {
            // `ContextMenu` on the flyout window (not on a tile): the HUD shows its menu
            let weak = Rc::downgrade(inner);
            inner
                .flyout
                .borrow_mut()
                .set_context_menu_handler(move |pos| {
                    if let Some(inner) = weak.upgrade() {
                        inner.context_menu_requested.emit(&pos);
                    }
                });
        }
        {
            // Python `_Flyout.paintEvent`: the rounded surface painted by the flyout itself
            let weak = Rc::downgrade(inner);
            if let Some(root) = inner
                .flyout_root
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<EmptyWidget>()
            {
                root.set_paint_handler(move |p| {
                    let Some(inner) = weak.upgrade() else { return };
                    let t = inner.tokens();
                    let g = p.device();
                    let (w, h) = (g.width(), g.height());
                    p.set_render_hint(RenderHint::Antialiasing, true);
                    p.set_brush(Brush::Color(gauge::skia(t.background)));
                    p.set_pen(Some(round_pen(gauge::skia(t.separator), 1.0)));
                    p.draw_rounded_rect(
                        RectF::new(0.5, 0.5, w - 1.0, h - 1.0),
                        FLYOUT_RADIUS,
                        FLYOUT_RADIUS,
                    );
                });
            }
        }
        self.set_appearance(true, Scheme::Scale);
    }

    /// The `QWidget` to put into the HUD's layout.
    pub fn widget(&self) -> WidgetRef {
        Rc::clone(&self.inner.container)
    }

    pub fn inner(&self) -> &Rc<SidebarInner> {
        &self.inner
    }

    pub fn state(&self) -> State {
        self.inner.state.get()
    }

    pub fn orientation(&self) -> Orientation {
        self.inner.orientation.get()
    }

    pub fn scale(&self) -> f64 {
        self.inner.scale.get()
    }

    pub fn order(&self) -> Vec<String> {
        self.inner.order.borrow().clone()
    }

    /// Python `_active`: the provider whose flyout is open.
    pub fn active(&self) -> Option<String> {
        self.inner.active.borrow().clone()
    }

    pub fn state_change_requested(&self) -> &Notifier<String> {
        &self.inner.state_change_requested
    }

    /// The cell the HUD keeps with the name of the screen its window is on.
    pub fn host_screen(&self) -> Arc<parking_lot::Mutex<Option<String>>> {
        Arc::clone(&self.inner.host_screen)
    }
    pub fn context_menu_requested(&self) -> &Notifier<Point> {
        &self.inner.context_menu_requested
    }
    pub fn scale_step_requested(&self) -> &Notifier<i32> {
        &self.inner.scale_step_requested
    }
    pub fn drag_requested(&self) -> &Notifier<()> {
        &self.inner.drag_requested
    }
    pub fn order_changed(&self) -> &Notifier<Vec<String>> {
        &self.inner.order_changed
    }

    pub fn set_state(&self, state: State) {
        self.inner.set_state(state);
    }

    /// Python `set_order`: `None` when `order` is not a permutation of the providers (Python raises).
    pub fn set_order(&self, order: &[String]) -> Option<()> {
        let order = valid_order(order)?;
        if order == *self.inner.order.borrow() {
            return Some(());
        }
        *self.inner.order.borrow_mut() = order;
        self.inner.apply_order();
        Some(())
    }

    pub fn set_orientation(&self, orientation: Orientation) {
        if orientation == self.inner.orientation.get() {
            return;
        }
        self.inner.close_flyout();
        self.inner.orientation.set(orientation);
        self.inner.relayout();
    }

    /// Python `set_dock_edge(edge)`: the edge the bar is docked to (`None` = floating).
    /// An unknown edge name is refused (`None`; Python raises).
    pub fn set_dock_edge(&self, edge: Option<&str>) -> Option<()> {
        if !matches!(edge, None | Some("left" | "right" | "top" | "bottom")) {
            return None;
        }
        if edge.map(str::to_owned) == *self.inner.dock_edge.borrow() {
            return Some(());
        }
        *self.inner.dock_edge.borrow_mut() = edge.map(str::to_owned);
        let active = self.inner.active.borrow().clone();
        if let Some(key) = active {
            self.inner.animation_timer.stop();
            self.inner.animation.borrow_mut().animation_mut().stop();
            self.inner.place_flyout(&key);
        }
        Some(())
    }

    /// Python `set_scale`; a non-finite scale is refused (Python raises).
    pub fn set_scale(&self, scale: f64) -> Option<()> {
        let scale = clamp_scale(scale)?;
        if scale == self.inner.scale.get() {
            return Some(());
        }
        self.inner.scale.set(scale);
        self.inner.relayout();
        self.update_countdown();
        Some(())
    }

    /// Python `set_appearance(dark, scheme)`.
    pub fn set_appearance(&self, dark: bool, scheme: Scheme) {
        self.inner.set_appearance(dark, scheme);
    }

    pub fn update_metrics(&self, data: &Metrics) {
        self.inner.update_metrics(data);
    }

    pub fn update_countdown(&self) {
        let keys: Vec<String> = self.inner.metrics.borrow().keys().cloned().collect();
        for key in keys {
            self.inner.refresh_tooltip(&key);
        }
    }

    /// Python `preferred_size`.
    pub fn preferred_size(&self) -> (i32, i32) {
        self.inner.preferred_size()
    }

    pub fn close_flyout(&self) {
        self.inner.close_flyout();
    }

    /// Python `_flyout_position` for the HUD's tests and docking.
    pub fn tokens(&self) -> Tokens {
        self.inner.tokens()
    }
}

impl SidebarInner {
    fn tokens(&self) -> Tokens {
        tokens(self.dark.get())
    }

    fn buttons(&self) -> &[(String, WidgetRef, Rc<ButtonData>)] {
        self.buttons.get().map_or(&[], |b| b.as_slice())
    }

    fn toggle(&self) -> &WidgetRef {
        self.toggle
            .get()
            .expect("toggle is set in SidebarWidget::new")
    }

    fn button(&self, key: &str) -> Option<&(String, WidgetRef, Rc<ButtonData>)> {
        find_button(self.buttons(), key)
    }

    fn card(&self, key: &str) -> Option<&Rc<RefCell<ProviderCardWidget>>> {
        self.cards.iter().find(|(k, _)| k == key).map(|(_, c)| c)
    }

    fn card_widget(&self, key: &str) -> Option<WidgetRef> {
        self.card(key).map(|c| c.borrow().widget())
    }

    /// The filters of Python's `eventFilter`: the sidebar and the flyout.
    fn install_filters(self: &Rc<Self>) {
        let mut owners = self.filters.borrow_mut();
        for target in [&self.container, &self.flyout_root] {
            let filter = SidebarFilter {
                data: ObjectData::with_auto_id(),
                inner: UiThread::new(Rc::downgrade(self)),
                flyout_root: self.flyout_root.borrow().id(),
            };
            // SAFETY: the owner is kept in `self.filters` on the UI thread and never reads the
            // object; its callbacks run only from event dispatch.
            let owner = unsafe { register_owned_qobject(filter) };
            target
                .borrow_mut()
                .object_data_mut()
                .install_event_filter(OwnedObject::object_id(&owner));
            owners.push(owner);
        }
    }

    fn set_state(self: &Rc<Self>, state: State) {
        if state == self.state.get() {
            return;
        }
        self.set_state_forced(state);
    }

    /// Python `set_state` after its early return.
    fn set_state_forced(self: &Rc<Self>, state: State) {
        self.close_flyout();
        self.reset_buttons();
        self.state.set(state);
        ferrokit_widgets::widget::set_widget_visible(&self.strip, state == State::Strip);
        ferrokit_widgets::widget::set_widget_visible(&self.panel, state == State::Panel);
        if state == State::Panel {
            let order = self.order.borrow().clone();
            let cards: Vec<&Rc<RefCell<ProviderCardWidget>>> = order
                .iter()
                .filter_map(|k| self.cards.iter().find(|(c, _)| c == k).map(|(_, c)| c))
                .collect();
            for (index, card) in cards.into_iter().enumerate() {
                if index > 0 {
                    let mut divider = Frame::new();
                    divider.set_object_name("SidebarDivider");
                    let line = make_widget(divider);
                    self.dividers.borrow_mut().push(Rc::clone(&line));
                    layout_add(&self.panel, &line);
                }
                let widget = card.borrow().widget();
                layout_add(&self.panel, &widget);
                ferrokit_widgets::widget::set_widget_visible(&widget, true);
            }
        } else {
            // take every widget out of the panel: the cards go back to storage, the dividers go
            loop {
                let taken = self.panel.borrow().with_box_layout(|l| {
                    if l.count() == 0 {
                        None
                    } else {
                        Layout::take_at(l, 0)
                    }
                });
                let Some(Some(widget)) = taken else { break };
                if self
                    .cards
                    .iter()
                    .any(|(_, c)| Rc::ptr_eq(&c.borrow().widget(), &widget))
                {
                    // `_restore_card_style`
                    if let Some((_, card)) = self
                        .cards
                        .iter()
                        .find(|(_, c)| Rc::ptr_eq(&c.borrow().widget(), &widget))
                    {
                        card.borrow_mut().set_font_scale(1.0);
                    }
                    ferrokit_widgets::widget::set_widget_visible(&widget, false);
                }
            }
            self.dividers.borrow_mut().clear();
        }
        self.relayout();
    }

    fn apply_order(self: &Rc<Self>) {
        self.reset_buttons();
        let order = self.order.borrow().clone();
        for (_, widget, _) in self.buttons() {
            layout_remove(&self.strip, widget);
        }
        for key in &order {
            if let Some((_, widget, _)) = self.button(key) {
                layout_add(&self.strip, widget);
            }
        }
        self.strip.borrow().with_box_layout(|l| l.update_layout());
        if self.state.get() == State::Panel {
            self.set_state_forced(State::Strip);
            self.set_state_forced(State::Panel);
        }
    }

    /// `ButtonData::reset_interaction` for every tile.
    fn reset_buttons(&self) {
        for (_, _, data) in self.buttons() {
            data.reset_interaction();
        }
    }

    /// Python `_start_reorder`.
    fn start_reorder(self: &Rc<Self>, _key: &str) {
        let inner = Rc::clone(self);
        defer(move || {
            inner.close_flyout();
            inner.hover_timer.stop();
        });
    }

    /// Python `_move_reorder(key, global_pos)`.
    fn move_reorder(self: &Rc<Self>, key: &str, global: Point) {
        let pos = self.strip.borrow().map_from_global(global);
        let vertical = self.orientation.get() == Orientation::Vertical;
        let coordinate = if vertical { pos.y } else { pos.x };
        let order = self.order.borrow().clone();
        let mut centers = std::collections::HashMap::new();
        for (k, widget, _) in self.buttons() {
            if let Ok(w) = widget.try_borrow() {
                let g = w.geometry();
                centers.insert(
                    k.clone(),
                    if vertical {
                        g.y + g.height / 2
                    } else {
                        g.x + g.width / 2
                    },
                );
            }
        }
        let new_order = reorder(&order, key, coordinate, &centers);
        if new_order != order {
            *self.order.borrow_mut() = new_order.clone();
            for (_, widget, _) in self.buttons() {
                layout_remove(&self.strip, widget);
            }
            for k in &new_order {
                if let Some((_, widget, _)) = self.button(k) {
                    layout_add(&self.strip, widget);
                }
            }
            self.strip.borrow().with_box_layout(|l| l.update_layout());
        }
    }

    /// Python `_finish_reorder`.
    fn finish_reorder(self: &Rc<Self>) {
        let order = self.order.borrow().clone();
        self.order_changed.emit(&order);
    }

    /// Python `_relayout`.
    fn relayout(self: &Rc<Self>) {
        let s = self.scale.get();
        let vertical = self.orientation.get() == Orientation::Vertical;
        let direction = if vertical {
            Direction::TopToBottom
        } else {
            Direction::LeftToRight
        };
        let t = self.tokens();
        let padding = py_round(t.spacing as f64 * s);
        self.container.borrow().with_box_layout(|l| {
            l.set_direction(direction);
            l.set_margins(Margins::new(padding, padding, padding, padding));
            l.set_spacing(0);
        });
        self.strip
            .borrow()
            .with_box_layout(|l| l.set_direction(direction));
        self.panel.borrow().with_box_layout(|l| {
            l.set_direction(direction);
            l.set_spacing(py_round(4.0 * s));
        });
        let handle = scaled(22, s);
        layout_remove(&self.container, self.toggle());
        {
            let toggle = self.toggle().borrow();
            if vertical {
                self.container
                    .borrow()
                    .with_box_layout(|l| l.insert_widget(0, Rc::clone(self.toggle()), 0));
                toggle.set_minimum_size(Size::new(0, 0));
                toggle.set_maximum_size(Size::new(QWIDGETSIZE_MAX, QWIDGETSIZE_MAX));
                toggle.set_fixed_size(Size::new(scaled(BASE_BUTTON_W, s), handle));
            } else {
                self.container
                    .borrow()
                    .with_box_layout(|l| Layout::add_widget(l, Rc::clone(self.toggle())));
                toggle.set_minimum_size(Size::new(handle, handle));
                toggle.set_maximum_size(Size::new(handle, QWIDGETSIZE_MAX));
                toggle.set_size_policy(QSizePolicy::new(Policy::Fixed, Policy::Expanding));
            }
        }
        for (_, widget, data) in self.buttons() {
            let w = widget.borrow();
            let (bw, bh) = (scaled(BASE_BUTTON_W, s), scaled(BASE_BUTTON_H, s));
            w.set_fixed_size(Size::new(bw, bh));
            data.show_labels.set(s >= 0.8);
            if let Some(indicators) = w.children().into_iter().next() {
                indicators.borrow().set_geometry(Rect::new(0, 0, bw, bh));
                indicators.borrow().raise();
                indicators.borrow().update();
            }
            w.update();
        }
        let panel_sheet = get_cards_stylesheet(self.dark.get());
        self.panel
            .borrow()
            .set_style_sheet(&scale_px_in_qss(panel_sheet, s));
        let panel_vertical_width = scaled(BASE_PANEL_W, s) - 2 * padding;
        for (_, card) in &self.cards {
            let widget = card.borrow().widget();
            if self.state.get() == State::Panel {
                // side by side, cards keep their natural height instead of stretching to the tallest one
                self.panel.borrow().with_box_layout(|l| {
                    l.set_alignment(
                        &widget,
                        if vertical {
                            Alignment::NONE
                        } else {
                            Alignment::TOP
                        },
                    );
                });
                card.borrow_mut().set_font_scale(s);
                let width = if vertical {
                    panel_vertical_width
                } else {
                    scaled(BASE_PANEL_W, s)
                };
                let w = widget.borrow();
                w.set_minimum_size(Size::new(width, 0));
                w.set_maximum_size(Size::new(width, QWIDGETSIZE_MAX));
                w.ensure_polished();
                w.update_layout();
            } else {
                let w = widget.borrow();
                w.set_minimum_size(Size::new(0, 0));
                w.set_maximum_size(Size::new(QWIDGETSIZE_MAX, QWIDGETSIZE_MAX));
            }
        }
        for line in self.dividers.borrow().iter() {
            let w = line.borrow();
            w.set_minimum_size(Size::new(0, 0));
            w.set_maximum_size(Size::new(QWIDGETSIZE_MAX, QWIDGETSIZE_MAX));
            let thickness = 1.max(py_round(s));
            if vertical {
                w.set_minimum_size(Size::new(0, thickness));
                w.set_maximum_size(Size::new(QWIDGETSIZE_MAX, thickness));
            } else {
                w.set_minimum_size(Size::new(thickness, 0));
                w.set_maximum_size(Size::new(thickness, QWIDGETSIZE_MAX));
            }
        }
        self.toggle().borrow().update();
        for layout_owner in [&self.strip, &self.panel, &self.container] {
            layout_owner.borrow().with_box_layout(|l| l.update_layout());
        }
        self.container.borrow().update_geometry();
        self.container.borrow().update();
    }

    /// Python `set_appearance`.
    fn set_appearance(self: &Rc<Self>, dark: bool, scheme: Scheme) {
        self.dark.set(dark);
        self.scheme.set(scheme);
        let t = self.tokens();
        let cards_sheet = get_cards_stylesheet(dark);
        self.container.borrow().set_style_sheet(&format!(
            "{cards_sheet}QWidget#Sidebar {{background:{};border-radius:{}px;}} QFrame#SidebarDivider {{background:{};border:none;}}",
            t.background.name(),
            format_g(t.radius as f64 * self.scale.get()),
            t.separator.name(),
        ));
        self.flyout.borrow_mut().set_style_sheet(&format!(
            "{cards_sheet}QWidget {{background-color: transparent;}}"
        ));
        self.relayout();
        let keys: Vec<String> = self.metrics.borrow().keys().cloned().collect();
        for key in &keys {
            self.refresh_tile(key);
        }
        for key in &keys {
            self.refresh_tooltip(key);
        }
    }

    /// Python `update_metrics`.
    fn update_metrics(self: &Rc<Self>, data: &Metrics) {
        let Some((_, _, button)) = self.button(&data.provider_id) else {
            return;
        };
        let previous = self
            .row_state
            .borrow()
            .get(&data.provider_id)
            .cloned()
            .unwrap_or_default();
        self.metrics
            .borrow_mut()
            .insert(data.provider_id.clone(), data.clone());
        let current = row_state(data);
        let offline = is_offline(data);
        if !offline && window_reset(&previous, &current) {
            self.flash(button);
        }
        self.row_state
            .borrow_mut()
            .insert(data.provider_id.clone(), current);
        self.refresh_tile(&data.provider_id);
        self.refresh_tooltip(&data.provider_id);
        if self.state.get() == State::Panel {
            self.relayout();
        }
    }

    /// Python `_refresh_tile`.
    fn refresh_tile(&self, key: &str) {
        let (Some((_, widget, data)), Some(metrics)) =
            (self.button(key), self.metrics.borrow().get(key).cloned())
        else {
            return;
        };
        *data.tile.borrow_mut() = refresh_tile(key, &metrics, self.dark.get(), self.scheme.get());
        if let Ok(w) = widget.try_borrow() {
            if let Some(indicators) = w.children().into_iter().next() {
                indicators.borrow().update();
            }
            w.update();
        }
    }

    /// Python `_refresh_tooltip`.
    fn refresh_tooltip(&self, key: &str) {
        let (Some((_, widget, data)), Some(metrics)) =
            (self.button(key), self.metrics.borrow().get(key).cloned())
        else {
            return;
        };
        let alert = data.tile.borrow().alert;
        let text = tooltip_text(key, &metrics, alert);
        if let Ok(mut w) = widget.try_borrow_mut() {
            w.set_tool_tip(&text);
            w.set_accessible_description(&text);
        }
    }

    /// Python `_flash`.
    fn flash(&self, button: &Rc<ButtonData>) {
        button.flash.set(1.0);
        let steps = ((FLASH_MS / FLASH_STEP_MS).max(1)) as f64;
        let weak = Rc::downgrade(button);
        let handle = Arc::new(UiThread::new(weak));
        if !animations_enabled() {
            let handle = Arc::clone(&handle);
            Timer::single_shot(FLASH_MS, move || {
                if let Some(button) = handle.get().and_then(|w| w.upgrade()) {
                    button.flash.set(0.0);
                    button.update();
                }
            });
            button.update();
            return;
        }
        fn tick(handle: Arc<UiThread<Weak<ButtonData>>>, steps: f64) {
            let Some(button) = handle.get().and_then(|w| w.upgrade()) else {
                return;
            };
            button
                .flash
                .set((button.flash.get() - 1.0 / steps).max(0.0));
            button.update();
            if button.flash.get() > 0.0 {
                Timer::single_shot(FLASH_STEP_MS, move || tick(handle, steps));
            }
        }
        button.update();
        Timer::single_shot(FLASH_STEP_MS, move || tick(handle, steps));
    }

    /// Python `preferred_size`.
    fn preferred_size(&self) -> (i32, i32) {
        let s = self.scale.get();
        let orientation = self.orientation.get();
        let spacing = self.tokens().spacing;
        if self.state.get() == State::Strip {
            return strip_preferred_size(s, orientation, spacing, 3);
        }
        let height = self
            .panel
            .borrow()
            .with_box_layout(|l| {
                l.update_layout();
                l.size_hint().height
            })
            .unwrap_or(0);
        panel_preferred_size(s, orientation, spacing, self.cards.len() as i32, height)
    }

    /// Python `_hover`.
    fn hover(self: &Rc<Self>, key: &str) {
        self.leave_timer.stop();
        if self.state.get() != State::Strip {
            return;
        }
        let active = self.active.borrow().clone();
        if let Some(active) = active {
            if key != active {
                self.hover_timer.stop();
                self.open_flyout(key);
            }
            return;
        }
        *self.hover_id.borrow_mut() = Some(key.to_owned());
        self.hover_timer.start_with_interval(self.tokens().hover_ms);
    }

    /// Python `_click_provider`.
    fn click_provider(self: &Rc<Self>, key: &str) {
        if self.pinned.get() && self.pinned_key.borrow().as_deref() == Some(key) {
            self.close_flyout();
            return;
        }
        self.open_flyout(key);
        self.pinned.set(true);
        *self.pinned_key.borrow_mut() = Some(key.to_owned());
    }

    /// Python `button.screen().availableGeometry()`: the work area of the screen the window the
    /// button is in is on. Qt's `QWidget.screen()` always has an answer; if the named screen is
    /// gone (display change not yet reported) the primary screen stands in.
    fn screen_area(&self) -> Rect {
        let platform = ferrokit_platform::platform();
        let name = self.host_screen.lock().clone();
        name.and_then(|name| platform.screens().into_iter().find(|s| s.name() == name))
            .unwrap_or_else(|| platform.primary_screen())
            .available_geometry()
    }

    /// The button's rectangle in global coordinates (`QRect(origin, button.size())`).
    fn button_rect(&self, key: &str) -> Option<Rect> {
        let (_, widget, _) = self.button(key)?;
        let w = widget.try_borrow().ok()?;
        let origin = w.map_to_global(Point::new(0, 0));
        let g = w.geometry();
        Some(Rect::new(origin.x, origin.y, g.width, g.height))
    }

    /// Python `_flyout_position` for the open flyout: moves it beside its tile.
    fn place_flyout(&self, key: &str) {
        let Some(tile) = self.button_rect(key) else {
            return;
        };
        let area = self.screen_area();
        let g = self.flyout.borrow().geometry();
        let edge = self.dock_edge.borrow().clone();
        let (x, y) = flyout_position(
            (tile.x, tile.y, tile.width, tile.height),
            (g.width, g.height),
            (area.x, area.y, area.width, area.height),
            edge.as_deref(),
        );
        self.flyout
            .borrow_mut()
            .set_geometry(Rect::new(x, y, g.width, g.height));
    }

    /// Python `_open_flyout`.
    fn open_flyout(self: &Rc<Self>, key: &str) {
        if self.state.get() != State::Strip || self.card(key).is_none() {
            return;
        }
        if self.active.borrow().as_deref() == Some(key) {
            self.leave_timer.stop();
            return;
        }
        let (pinned, pinned_key) = (self.pinned.get(), self.pinned_key.borrow().clone());
        self.close_flyout();
        self.pinned.set(pinned);
        *self.pinned_key.borrow_mut() = pinned_key;
        let (Some(card), Some(tile)) = (self.card(key).cloned(), self.button_rect(key)) else {
            return;
        };
        *self.active.borrow_mut() = Some(key.to_owned());
        card.borrow_mut().set_font_scale(1.0); // `_restore_card_style`
        let widget = card.borrow().widget();
        {
            let w = widget.borrow();
            w.set_minimum_size(Size::new(0, 0));
            w.set_maximum_size(Size::new(QWIDGETSIZE_MAX, QWIDGETSIZE_MAX));
        }
        layout_add(&self.flyout_root, &widget);
        widget
            .borrow()
            .set_window_id(Some(self.flyout.borrow().id()));
        ferrokit_widgets::widget::set_widget_visible(&widget, true);
        let area = self.screen_area();
        let hint = self
            .flyout_root
            .borrow()
            .with_box_layout(|l| {
                l.update_layout();
                l.size_hint().height
            })
            .unwrap_or(0);
        let size = (FLYOUT_WIDTH.min(area.width), hint.min(area.height));
        {
            let mut flyout = self.flyout.borrow_mut();
            flyout.set_fixed_size(size.0, size.1);
        }
        let edge = self.dock_edge.borrow().clone();
        let (x, y) = flyout_position(
            (tile.x, tile.y, tile.width, tile.height),
            size,
            (area.x, area.y, area.width, area.height),
            edge.as_deref(),
        );
        {
            let mut flyout = self.flyout.borrow_mut();
            flyout.set_geometry(Rect::new(x, y, size.0, size.1));
            if animations_enabled() {
                flyout.set_opacity(0.0);
            } else {
                flyout.set_opacity(1.0);
            }
            flyout.show();
        }
        if animations_enabled() {
            // Opacity keeps the window inside the screen throughout the animation.
            let mut animation = self.animation.borrow_mut();
            let inner = animation.animation_mut();
            inner.set_duration(self.tokens().animation_ms);
            inner.set_start_value(0.0f64);
            inner.set_end_value(1.0f64);
            inner.start();
            drop(animation);
            self.animation_clock.set(Some(Instant::now()));
            self.animation_timer.start();
        }
    }

    /// One tick of the `QPropertyAnimation` on the flyout's `windowOpacity`.
    fn animation_tick(&self) {
        let now = Instant::now();
        let delta = self
            .animation_clock
            .replace(Some(now))
            .map_or(16, |last| now.duration_since(last).as_millis() as u64);
        let Ok(mut flyout) = self.flyout.try_borrow_mut() else {
            return;
        };
        let running = self
            .animation
            .borrow_mut()
            .step_target(delta.max(1), &mut *flyout);
        if !running {
            self.animation_timer.stop();
        }
    }

    /// Python `close_flyout`.
    fn close_flyout(self: &Rc<Self>) {
        self.hover_timer.stop();
        self.leave_timer.stop();
        self.animation_timer.stop();
        self.animation.borrow_mut().animation_mut().stop();
        let active = self.active.borrow_mut().take();
        if let Some(key) = active {
            if let Some(widget) = self.card_widget(&key) {
                layout_remove(&self.flyout_root, &widget);
                ferrokit_widgets::widget::set_widget_visible(&widget, false);
            }
        }
        if let Ok(mut flyout) = self.flyout.try_borrow_mut() {
            flyout.hide();
        }
        self.pinned.set(false);
        *self.pinned_key.borrow_mut() = None;
    }

    /// Python `_schedule_close`.
    fn schedule_close(&self) {
        if !self.pinned.get() {
            self.leave_timer.start_with_interval(self.tokens().leave_ms);
        }
    }

    /// Python `_maybe_close`.
    fn maybe_close(self: &Rc<Self>) {
        let active = self.active.borrow().clone();
        if self.pinned.get() {
            return;
        }
        let Some(key) = active else { return };
        let pos = Cursor::pos();
        if let Some(tile) = self.button_rect(&key) {
            if tile.contains(pos) {
                return;
            }
        }
        if self.flyout.borrow().geometry().contains(pos) {
            return;
        }
        self.close_flyout();
    }
}

/// Python `SidebarWidget.eventFilter` / `keyPressEvent` for the sidebar and its flyout.
struct SidebarFilter {
    data: ObjectData,
    inner: UiThread<Weak<SidebarInner>>,
    flyout_root: ObjectId,
}

impl QObject for SidebarFilter {
    fn object_data(&self) -> &ObjectData {
        &self.data
    }
    fn object_data_mut(&mut self) -> ObjectDataMut<'_> {
        ObjectDataMut::new(&mut self.data)
    }

    fn event_filter(&mut self, watched: ObjectId, event: &mut Event) -> bool {
        let Some(inner) = self.inner.get().and_then(|w| w.upgrade()) else {
            return false;
        };
        match &event.kind {
            EventKind::Wheel {
                angle_delta_y,
                modifiers,
                ..
            } if modifiers & CONTROL_MODIFIER != 0 => {
                let (steps, remainder) = wheel_steps(inner.wheel_remainder.get(), *angle_delta_y);
                inner.wheel_remainder.set(remainder);
                if !steps.is_empty() {
                    let inner = Rc::clone(&inner);
                    defer(move || {
                        for step in steps {
                            inner.scale_step_requested.emit(&step);
                        }
                    });
                }
                event.accept();
                true
            }
            EventKind::Enter { .. } if watched == self.flyout_root => {
                inner.leave_timer.stop();
                false
            }
            EventKind::Leave if watched == self.flyout_root => {
                inner.schedule_close();
                false
            }
            EventKind::KeyPress { key, .. } if *key == KEY_ESCAPE => {
                let inner = Rc::clone(&inner);
                defer(move || inner.close_flyout());
                true
            }
            _ => false,
        }
    }
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
        assert_eq!(tokens(true).stale, Color::from("#f59e0b"));
        assert_eq!(tokens(false).stale, Color::from("#92400e"));
        assert_eq!(tokens(true).background, Color::from("#1e1e24"));
        assert_eq!(
            (
                tokens(true).hover_ms,
                tokens(true).leave_ms,
                tokens(true).animation_ms
            ),
            (300, 600, 160)
        );
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
        assert_eq!(
            tile.rows[0].color,
            status_color(true, Scheme::Scale, Some(20.0), Which::Inner, false)
        );
        assert_eq!(
            tile.rows[1].color,
            status_color(true, Scheme::Scale, Some(8.0), Which::Outer, false)
        );

        let blank = refresh_tile(
            "claude",
            &UsageMetrics {
                provider_id: "claude".into(),
                ..Default::default()
            },
            true,
            Scheme::Scale,
        );
        assert_eq!(blank.display, ["--", "--"]);
        assert!(blank.rows.iter().all(|r| r.value.is_none()));

        let offline = refresh_tile(
            "claude",
            &UsageMetrics {
                error: Some("offline".into()),
                metric1_val: Some(20.0),
                ..Default::default()
            },
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
        let tip = tooltip_text(
            "claude",
            &UsageMetrics {
                stale: true,
                error: Some("timeout".into()),
                ..Default::default()
            },
            false,
        );
        assert!(tip.contains("STALE"));
    }

    #[test]
    fn agy_dots_follow_the_extra_rows() {
        let data = UsageMetrics {
            provider_id: "agy".into(),
            extra_metrics: vec![
                MetricRow::new("C/G 7D", Some(15.0), "15%"),
                MetricRow::new("ARGON 7D", Some(21.0), "21%"),
            ],
            ..Default::default()
        };
        let tile = refresh_tile("agy", &data, true, Scheme::Scale);
        let green = Color::from(get_theme(true).scale_green);
        assert_eq!(tile.dot_colors, vec![green, green]);
        assert!(tooltip_text("agy", &data, false).contains("C/G 7D 已用 15%"));
        // the duo scheme never changes the dots
        let light = refresh_tile("agy", &data, false, Scheme::Duo);
        assert_eq!(
            light.dot_colors,
            vec![Color::from(get_theme(false).scale_green); 2]
        );
        let none = refresh_tile(
            "agy",
            &UsageMetrics {
                provider_id: "agy".into(),
                ..Default::default()
            },
            false,
            Scheme::Duo,
        );
        assert!(none.dot_colors.is_empty());
        let unknown = refresh_tile(
            "agy",
            &UsageMetrics {
                extra_metrics: vec![MetricRow::new("C/G 7D", None, "--")],
                ..Default::default()
            },
            false,
            Scheme::Duo,
        );
        assert_eq!(
            unknown.dot_colors,
            vec![Color::from(get_theme(false).neutral)]
        );
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
        assert_eq!(
            three
                .rows
                .iter()
                .map(|r| r.label.as_str())
                .collect::<Vec<_>>(),
            ["CG 5H", "Ar 7D"]
        );
        assert_eq!(three.dot_colors.len(), 3);
    }

    #[test]
    fn tooltip_lists_every_row_with_a_countdown() {
        let data = UsageMetrics {
            provider_id: "agy".into(),
            metric1_val: Some(21.0),
            metric2_val: Some(1.0),
            extra_metrics: vec![
                MetricRow::new("C/G 7D", Some(15.0), "15%"),
                MetricRow::new("ARGON 7D", Some(21.0), "21%"),
            ],
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
                let data = UsageMetrics {
                    metric1_val: Some(95.0),
                    metric2_val: Some(10.0),
                    ..Default::default()
                };
                let tile = refresh_tile("claude", &data, dark, scheme);
                assert_eq!(
                    tile.rows[0].color,
                    status_color(dark, scheme, Some(95.0), Which::Inner, false)
                );
                assert_ne!(tile.rows[0].color, tile.rows[1].color);
                for row in &tile.rows {
                    let shown = text_color(row.color, dark);
                    for surface in tile_surfaces(dark) {
                        assert!(gauge::contrast(shown, surface) >= 3.0);
                    }
                }
            }
        }
        let scale = refresh_tile(
            "claude",
            &UsageMetrics {
                metric1_val: Some(95.0),
                metric2_val: Some(10.0),
                ..Default::default()
            },
            true,
            Scheme::Scale,
        );
        let duo = refresh_tile(
            "claude",
            &UsageMetrics {
                metric1_val: Some(95.0),
                metric2_val: Some(10.0),
                ..Default::default()
            },
            true,
            Scheme::Duo,
        );
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
                    let data = UsageMetrics {
                        metric1_val: Some(value),
                        metric2_val: Some(value),
                        ..Default::default()
                    };
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
        assert_eq!(
            stops.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
            [0, 50, 75, 90]
        );
        assert_eq!(ramp(0.0, true), stops[0].1);
        assert_eq!(ramp(100.0, true), stops[3].1);
        let mid = ramp(25.0, true).name();
        assert_ne!(mid, stops[0].1.name());
        assert_ne!(mid, stops[1].1.name());
        let channel = |v: f64, c: fn(&Color) -> i32| c(&ramp(v, true));
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
        assert!(
            !refresh_tile(
                "claude",
                &metrics(95.0, Some("offline")),
                true,
                Scheme::Scale
            )
            .alert
        );
        let agy = UsageMetrics {
            metric1_val: Some(1.0),
            metric2_val: Some(2.0),
            extra_metrics: vec![MetricRow::new("ARGON 7D", Some(93.0), "93%")],
            ..Default::default()
        };
        assert!(
            refresh_tile("agy", &agy, true, Scheme::Scale).alert,
            "extra rows count too"
        );
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
        // 600.5 s later is a reset in Python's float compare
        let edge = row_state(&data(
            1.0,
            now + Duration::hours(5) + Duration::milliseconds(600_500),
        ));
        assert!(window_reset(&rolled, &edge));
    }

    #[test]
    fn geometry_of_the_strip_and_panel() {
        // the Python test: a 1.0 vertical strip is 72 wide (64 + 2 * 4)
        assert_eq!(
            strip_preferred_size(1.0, Orientation::Vertical, 4, 3),
            (72, 22 + 3 * 64 + 8)
        );
        assert_eq!(
            strip_preferred_size(1.0, Orientation::Horizontal, 4, 3),
            (3 * 64 + 22 + 8, 72)
        );
        let (w, h) = strip_preferred_size(1.5, Orientation::Vertical, 4, 3);
        assert!((w as f64 - 72.0 * 1.5).abs() <= 3.0 && (h as f64 - 222.0 * 1.5).abs() <= 3.0);
        assert_eq!(
            panel_preferred_size(1.0, Orientation::Vertical, 4, 3, 500).0,
            220
        );
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
        for edge in [
            Some("right"),
            Some("left"),
            Some("top"),
            Some("bottom"),
            None,
        ] {
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
        let centers: HashMap<String, i32> = [
            ("claude".to_owned(), 55),
            ("agy".to_owned(), 119),
            ("codex".to_owned(), 183),
        ]
        .into();
        // dragged below codex's centre: claude goes last
        assert_eq!(
            reorder(&order, "claude", 200, &centers),
            ["agy", "codex", "claude"]
        );
        assert_eq!(
            reorder(&order, "claude", 0, &centers),
            ["claude", "agy", "codex"]
        );
        assert_eq!(
            reorder(&order, "codex", 100, &centers),
            ["claude", "codex", "agy"]
        );
    }

    #[test]
    fn order_rejects_bad_values() {
        let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            valid_order(&v(&["codex", "claude", "agy"])),
            Some(v(&["codex", "claude", "agy"]))
        );
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

/// Python's `re.sub(r'([\d.]+)px', ...)` of `_relayout` and `f'{x:g}'`.
#[cfg(test)]
mod qss_scale_tests {
    use super::*;

    #[test]
    fn every_px_length_of_a_sheet_is_scaled() {
        let sheet = "QLabel#MetricTitle { font-size: 10px; border: 1px solid red; padding: 1px 4px; } QProgressBar { border-radius: 3px; } QLabel#SubDetail { font-size: 9.5px; color: rgba(255, 255, 255, 0.08); }";
        assert_eq!(
            scale_px_in_qss(sheet, 1.5),
            "QLabel#MetricTitle { font-size: 15px; border: 1.5px solid red; padding: 1.5px 6px; } QProgressBar { border-radius: 4.5px; } QLabel#SubDetail { font-size: 14.25px; color: rgba(255, 255, 255, 0.08); }"
        );
        assert_eq!(
            scale_px_in_qss(sheet, 1.0).replace("1.0", "1"),
            sheet.replace("1.0", "1")
        );
    }

    #[test]
    fn non_length_numbers_and_text_are_untouched() {
        let sheet = "font-family: 'Segoe UI'; font-weight: 800; letter-spacing: 1.0px; 側邊欄 2em";
        assert_eq!(
            scale_px_in_qss(sheet, 2.0),
            "font-family: 'Segoe UI'; font-weight: 800; letter-spacing: 2px; 側邊欄 2em"
        );
    }

    #[test]
    fn g_format_has_six_significant_digits_and_no_trailing_zeros() {
        assert_eq!(format_g(15.0), "15");
        assert_eq!(format_g(14.25), "14.25");
        assert_eq!(format_g(0.5), "0.5");
        assert_eq!(format_g(6.0), "6");
        assert_eq!(format_g(11.875), "11.875");
        assert_eq!(format_g(1.0 / 3.0), "0.333333");
        assert_eq!(format_g(1234.5678), "1234.57");
        assert_eq!(format_g(0.0), "0");
    }
}
