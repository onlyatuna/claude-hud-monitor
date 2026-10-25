//! Provider metric card component, matching Python ui/provider_card.py.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{DateTime, Local, Utc};
use ferrokit_core::event::{Event, EventKind};
use ferrokit_core::object::{
    register_owned_qobject, ObjectData, ObjectDataMut, ObjectId, OwnedObject, QObject,
};
use ferrokit_gui::geometry::primitives::{Margins, Point, Rect};
use ferrokit_gui::text::font::{Font, FontWeight};
use ferrokit_gui::tiny_skia::Color;
use ferrokit_platform::cursor::CursorShape;
use ferrokit_widgets::{BoxLayout, EmptyWidget, Label, Layout, Widget, WidgetRef};

use crate::pace::reset_stamp;
use crate::providers::base::{format_countdown, MetricRow, UsageMetrics};
use crate::ui::brand::{brand_color, brand_hex};
use crate::ui::gauge::{parse_hex, ramp_color, to_hex, GaugeBar};
use crate::ui::Notifier;

pub fn get_progress_color(percent: f64, dark: bool) -> Color {
    if percent >= 90.0 {
        if dark {
            Color::from_rgba8(239, 68, 68, 255) // #ef4444
        } else {
            Color::from_rgba8(185, 28, 28, 255) // #b91c1c
        }
    } else if percent >= 75.0 {
        if dark {
            Color::from_rgba8(245, 158, 11, 255) // #f59e0b
        } else {
            Color::from_rgba8(146, 64, 14, 255) // #92400e
        }
    } else if percent >= 50.0 {
        if dark {
            Color::from_rgba8(59, 130, 246, 255) // #3b82f6
        } else {
            Color::from_rgba8(29, 78, 216, 255) // #1d4ed8
        }
    } else if dark {
        Color::from_rgba8(16, 185, 129, 255) // #10b981
    } else {
        Color::from_rgba8(4, 120, 87, 255) // #047857
    }
}

#[allow(dead_code)]
pub fn provider_accent_color(provider_id: &str, dark: bool) -> Color {
    brand_color(provider_id, dark)
}

pub fn default_provider_name(provider_id: &str) -> &'static str {
    match provider_id {
        "claude" => "CLAUDE CODE",
        "agy" => "ANTIGRAVITY",
        "codex" => "OPENAI CODEX",
        _ => "AI AGENT",
    }
}

fn make_widget<W: Widget + 'static>(w: W) -> WidgetRef {
    Rc::new(RefCell::new(Box::new(w)))
}

fn set_label_text(w: &WidgetRef, text: impl Into<String>) {
    if let Some(lbl) = w.borrow_mut().as_any_mut().downcast_mut::<Label>() {
        lbl.set_text(text);
    }
}

fn label_text(w: &WidgetRef) -> String {
    w.borrow()
        .as_any()
        .downcast_ref::<Label>()
        .map(|l| l.text().to_string())
        .unwrap_or_default()
}

use crate::ui::set_label_color;

/// Python `QWidget#RowCell` sheet: a hoverable, rounded block.
const ROW_CELL_QSS: &str = "QWidget#RowCell {background: transparent; border-radius: 6px;} QWidget#RowCell:hover {background: rgba(127, 127, 127, 0.16);}";
/// Colour of an absent value and of the reset sub labels.
const NEUTRAL_HEX: &str = "#64748b";
/// Manhattan distance the pointer has to travel before a press becomes a drag.
const DRAG_THRESHOLD: i32 = 6;

fn error_hex(dark: bool) -> &'static str {
    if dark {
        "#ef4444"
    } else {
        "#b91c1c"
    }
}

fn stale_hex(dark: bool) -> &'static str {
    if dark {
        "#f59e0b"
    } else {
        "#92400e"
    }
}

/// Python `PROVIDER_THEMES[id]["name"]`, `provider_id.upper()` for an unknown provider.
fn theme_name(provider_id: &str) -> String {
    match provider_id {
        "claude" | "agy" | "codex" => default_provider_name(provider_id).to_string(),
        other => other.to_uppercase(),
    }
}

fn title_qss(hex: &str) -> String {
    format!("color: {hex}; font-size: 9.5px; font-weight: 800; letter-spacing: 0.4px;")
}

/// Python `%g`-style number: at most six significant digits, no trailing zeros.
fn fmt_g(v: f64) -> String {
    let rounded: f64 = format!("{:.5e}", v).parse().unwrap_or(v);
    format!("{}", rounded)
}

/// Python `SidebarWidget._scale_card_style`: every `font-size: Npx` becomes `N * scale` px.
pub fn scale_font_sizes(qss: &str, scale: f64) -> String {
    const KEY: &str = "font-size:";
    let mut out = String::with_capacity(qss.len());
    let mut rest = qss;
    while let Some(i) = rest.find(KEY) {
        let after = &rest[i + KEY.len()..];
        let trimmed = after.trim_start();
        let num_len = trimmed
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(trimmed.len());
        let number = trimmed[..num_len].parse::<f64>().ok();
        match number {
            Some(n) if trimmed[num_len..].starts_with("px") => {
                out.push_str(&rest[..i]);
                out.push_str(&format!("font-size: {}px", fmt_g(n * scale)));
                rest = &trimmed[num_len + 2..];
            }
            _ => {
                out.push_str(&rest[..i + KEY.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The widget-local style sheets of the card, kept in their original (scale 1) text so the sidebar's
/// font scale (`set_font_scale`) applies to every later style as well.
struct LocalStyles {
    scale: Cell<f64>,
    specs: RefCell<Vec<(WidgetRef, String)>>,
}

impl LocalStyles {
    fn new() -> Self {
        Self {
            scale: Cell::new(1.0),
            specs: RefCell::new(Vec::new()),
        }
    }

    /// `widget.setStyleSheet(qss)` (and the label's palette colour) with the current scale.
    fn apply(&self, w: &WidgetRef, qss: String, color: Option<Color>) {
        {
            let mut specs = self.specs.borrow_mut();
            match specs.iter_mut().find(|(s, _)| Rc::ptr_eq(s, w)) {
                Some(entry) => entry.1 = qss.clone(),
                None => specs.push((w.clone(), qss.clone())),
            }
        }
        let mut b = w.borrow_mut();
        if let Some(c) = color {
            if let Some(lbl) = b.as_any_mut().downcast_mut::<Label>() {
                lbl.set_color(c);
            }
        }
        b.set_style_sheet(&scale_font_sizes(&qss, self.scale.get()));
    }

    fn set_scale(&self, scale: f64) {
        self.scale.set(scale);
        for (w, qss) in self.specs.borrow().iter() {
            w.borrow().set_style_sheet(&scale_font_sizes(qss, scale));
        }
    }
}

/// Python `_reset_text`: "重設於: 週五 18:00 · 6天 5時 55分" for weekly rows, just the countdown for
/// short windows; the provider's own `subtext` (or "重設於: --") when there is no reset time.
pub fn reset_text(reset: Option<DateTime<Utc>>, title: &str, subtext: &str) -> String {
    if reset.is_none() {
        return if subtext.is_empty() {
            "重設於: --".to_string()
        } else {
            subtext.to_string()
        };
    }
    let countdown = format_countdown(reset);
    let stamp = reset_stamp(reset, title);
    if stamp.is_empty() {
        format!("重設於: {countdown}")
    } else {
        format!("重設於: {stamp} · {countdown}")
    }
}

/// Python `_drop_index`: `rows` are `(top, bottom)` in card coordinates. Returns the indices of the
/// rows other than `source` and the position (among them) the dragged row would drop at for pointer
/// height `y`: the number of those rows whose centre lies above `y`.
pub fn drop_index(rows: &[(i32, i32)], source: usize, y: i32) -> (Vec<usize>, usize) {
    let others: Vec<usize> = (0..rows.len()).filter(|&i| i != source).collect();
    let index = others
        .iter()
        .filter(|&&i| y > (rows[i].0 + rows[i].1) / 2)
        .count();
    (others, index)
}

/// Python `_show_drop_line` height: just above the first remaining row, or just below the row
/// before the drop position.
pub fn drop_line_y(rows: &[(i32, i32)], others: &[usize], index: usize) -> i32 {
    if index == 0 {
        rows[others[0]].0 - 2
    } else {
        rows[others[index - 1]].1 + 1
    }
}

/// The titles after moving row `source` to position `target` (Python `pop` + `insert`).
pub fn reordered(titles: &[String], source: usize, target: usize) -> Vec<String> {
    let mut out = titles.to_vec();
    if source < out.len() {
        let moved = out.remove(source);
        out.insert(target.min(out.len()), moved);
    }
    out
}

fn make_bar() -> WidgetRef {
    let mut bar = GaugeBar::new(5);
    bar.set_range(0, 100);
    bar.set_value(0);
    make_widget(bar)
}

/// `bar.set_dark(dark)` (when given) and `bar.setValue(value)`.
fn set_bar(w: &WidgetRef, value: i32, dark: Option<bool>) {
    if let Some(bar) = w.borrow_mut().as_any_mut().downcast_mut::<GaugeBar>() {
        if let Some(dark) = dark {
            bar.set_dark(dark);
        }
        bar.set_value(value);
    }
}

/// Python `min(100, max(0, val or 0))`.
fn bar_value(val: Option<f64>) -> i32 {
    val.unwrap_or(0.0).clamp(0.0, 100.0) as i32
}

/// Python `ramp_color(val, dark) if val is not None else "#64748b"`.
fn value_hex(val: Option<f64>, dark: bool) -> String {
    match val {
        Some(v) => to_hex(ramp_color(v, dark, None)),
        None => NEUTRAL_HEX.to_string(),
    }
}

/// `data.error` as Python truthiness: an empty message is no error.
fn error_text(data: &UsageMetrics) -> Option<&str> {
    data.error.as_deref().filter(|e| !e.is_empty())
}

/// Python `_make_cell`: a metric block as one hoverable, draggable cell.
fn make_cell(mut layout: BoxLayout) -> WidgetRef {
    let cell = make_widget(EmptyWidget::new());
    cell.borrow_mut().set_object_name("RowCell");
    cell.borrow().set_style_sheet(ROW_CELL_QSS);
    layout.set_margins(Margins::new(4, 2, 4, 2));
    cell.borrow_mut().set_layout(Box::new(layout));
    cell
}

/// Python `_arm_cell`: the open hand over the cell. (The event filter that starts a drag is one
/// filter on the card, because a press on any descendant propagates up to it.)
fn arm_cell(cell: &WidgetRef) {
    cell.borrow().set_cursor(CursorShape::OpenHand);
}

struct MetricBlock {
    cell: WidgetRef,
    label: WidgetRef,
    val: WidgetRef,
    bar: WidgetRef,
    sub: WidgetRef,
}

/// Python `_init_ui` metric 1 / metric 2 block.
fn build_metric_block(title: &str, styles: &LocalStyles) -> MetricBlock {
    let mut layout = BoxLayout::vertical();
    layout.set_spacing(1);

    let mut hdr = BoxLayout::horizontal();
    let mut title_lbl = Label::new(title);
    title_lbl.set_font(Font::new("Segoe UI", 10.0).with_weight(FontWeight::Bold));
    let label = make_widget(title_lbl);
    label.borrow_mut().set_object_name("MetricTitle");
    hdr.add_widget(label.clone());
    hdr.add_stretch(1);
    let mut val_lbl = Label::new("--");
    val_lbl.set_font(Font::new("Consolas", 14.0).with_weight(FontWeight::Bold));
    let val = make_widget(val_lbl);
    val.borrow_mut().set_object_name("MetricValue");
    styles.apply(&val, "font-size: 14px;".to_string(), None);
    hdr.add_widget(val.clone());
    layout.add_layout(Box::new(hdr), 0);

    let bar = make_bar();
    layout.add_widget(bar.clone());

    let mut sub_lbl = Label::new("重設於: --");
    sub_lbl.set_font(Font::new("Segoe UI", 9.5));
    let sub = make_widget(sub_lbl);
    sub.borrow_mut().set_object_name("SubDetail");
    layout.add_widget(sub.clone());

    let cell = make_cell(layout);
    MetricBlock {
        cell,
        label,
        val,
        bar,
        sub,
    }
}

/// Python `extra_rows` entry `(widget, title, value, bar, sub)`.
struct ExtraRow {
    cell: WidgetRef,
    title: WidgetRef,
    value: WidgetRef,
    bar: WidgetRef,
    sub: WidgetRef,
}

/// Python `_row_drag`.
struct RowDrag {
    cell: WidgetRef,
    source: usize,
    active: bool,
    target: Option<usize>,
    /// Press position in card coordinates (Python: the global position; the card does not move
    /// during a drag).
    start: (i32, i32),
}

/// Python `_drag_rows` entry: title, the row's rectangle in card coordinates (full card width) and
/// the cell's own horizontal extent.
struct DragRow {
    title: String,
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
    cell: WidgetRef,
}

/// The card state shared with the event filter.
struct CardCore {
    container: WidgetRef,
    m1_cell: WidgetRef,
    m2_cell: WidgetRef,
    m1_label: WidgetRef,
    m2_label: WidgetRef,
    extra_container: RefCell<Option<WidgetRef>>,
    extra_rows: RefCell<Vec<ExtraRow>>,
    /// `self.width()`, kept by the container's resize handler (the container is mutably borrowed
    /// while the filter runs).
    width: Rc<Cell<i32>>,
    /// Not `data.error and not data.stale`.
    reorderable: Cell<bool>,
    /// `len(data.extra_metrics)`.
    extra_count: Cell<usize>,
    drop_line: WidgetRef,
    /// `self.theme['color']`.
    theme_hex: RefCell<String>,
    drag: RefCell<Option<RowDrag>>,
    row_order_changed: Notifier<Vec<String>>,
}

impl CardCore {
    /// Python `_drag_rows` titles.
    fn row_titles(&self) -> Vec<String> {
        self.drag_rows().into_iter().map(|r| r.title).collect()
    }

    /// Python `_drag_rows`: rows in display order; empty when there is nothing to reorder.
    fn drag_rows(&self) -> Vec<DragRow> {
        let m1_title = label_text(&self.m1_label);
        if !self.reorderable.get() || m1_title.is_empty() {
            return Vec::new();
        }
        let mut cells = vec![
            (m1_title, self.m1_cell.clone()),
            (label_text(&self.m2_label), self.m2_cell.clone()),
        ];
        for row in self.extra_rows.borrow().iter().take(self.extra_count.get()) {
            cells.push((label_text(&row.title), row.cell.clone()));
        }
        cells
            .into_iter()
            .filter_map(|(title, cell)| {
                let (origin, height, width) = {
                    let c = cell.try_borrow().ok()?;
                    let origin = c.map_to(&self.container, Point::new(0, 0));
                    let g = c.geometry();
                    (origin, g.height, g.width)
                };
                Some(DragRow {
                    title,
                    top: origin.y,
                    bottom: origin.y + height.max(1) - 1,
                    left: origin.x,
                    right: origin.x + width,
                    cell,
                })
            })
            .collect()
    }

    /// Python `_show_drop_line`.
    fn show_drop_line(&self, rows: &[(i32, i32)], others: &[usize], index: usize) {
        if others.is_empty() {
            return;
        }
        let line = self.drop_line.borrow();
        line.set_style_sheet(&format!(
            "background: {}; border-radius: 1px;",
            self.theme_hex.borrow()
        ));
        let y = drop_line_y(rows, others, index);
        line.set_geometry(Rect::new(4, y, (self.width.get() - 8).max(10), 3));
        line.set_visible(true);
        line.raise();
    }

    /// Python `eventFilter` `MouseButtonPress` with the left button.
    fn press(&self, x: i32, y: i32) -> bool {
        let rows = self.drag_rows();
        for (index, row) in rows.iter().enumerate() {
            if y >= row.top && y <= row.bottom && x >= row.left && x < row.right {
                *self.drag.borrow_mut() = Some(RowDrag {
                    cell: row.cell.clone(),
                    source: index,
                    active: false,
                    target: None,
                    start: (x, y),
                });
                return true;
            }
        }
        false
    }

    /// Python `eventFilter` `MouseMove` of the dragged cell.
    fn drag_move(&self, x: i32, y: i32) -> bool {
        let source = {
            let mut guard = self.drag.borrow_mut();
            let Some(drag) = guard.as_mut() else {
                return false;
            };
            let manhattan = (x - drag.start.0).abs() + (y - drag.start.1).abs();
            if !drag.active && manhattan > DRAG_THRESHOLD {
                drag.active = true;
                drag.cell.borrow().set_cursor(CursorShape::ClosedHand);
            }
            drag.active.then_some(drag.source)
        };
        if let Some(source) = source {
            let rows = self.drag_rows();
            if !rows.is_empty() {
                let spans: Vec<(i32, i32)> = rows.iter().map(|r| (r.top, r.bottom)).collect();
                let (others, index) = drop_index(&spans, source, y);
                if let Some(drag) = self.drag.borrow_mut().as_mut() {
                    drag.target = Some(index);
                }
                self.show_drop_line(&spans, &others, index);
            }
        }
        true
    }

    /// Python `eventFilter` `MouseButtonRelease` of the dragged cell.
    fn release(self: &Rc<Self>) -> bool {
        let Some(drag) = self.drag.borrow_mut().take() else {
            return false;
        };
        drag.cell.borrow().set_cursor(CursorShape::OpenHand);
        self.drop_line.borrow().set_visible(false);
        if let (true, Some(target)) = (drag.active, drag.target) {
            let titles = self.row_titles();
            if !titles.is_empty() {
                let order = reordered(&titles, drag.source, target);
                if order != titles {
                    // The card is mutably borrowed while its filter runs; a slot that touches the
                    // card (the sidebar re-applies the order) has to wait until the event is done.
                    let core = Rc::clone(self);
                    ferrokit_widgets::command::WidgetCommandQueue::post_task(move || {
                        core.row_order_changed.emit(&order)
                    });
                }
            }
        }
        true
    }
}

/// The card's `eventFilter`, installed on the container: presses, moves and releases of any
/// descendant propagate up to it in container coordinates.
struct CardFilter {
    data: ObjectData,
    core: Rc<CardCore>,
}

impl QObject for CardFilter {
    fn object_data(&self) -> &ObjectData {
        &self.data
    }
    fn object_data_mut(&mut self) -> ObjectDataMut<'_> {
        ObjectDataMut::new(&mut self.data)
    }
    fn event_filter(&mut self, _watched: ObjectId, event: &mut Event) -> bool {
        match event.kind {
            EventKind::MouseButtonPress { x, y, button: 1 } => self.core.press(x, y),
            EventKind::MouseMove { x, y } => self.core.drag_move(x, y),
            EventKind::MouseButtonRelease { .. } => self.core.release(),
            _ => false,
        }
    }
}

pub struct ProviderCardWidget {
    pub provider_id: String,
    pub dark: bool,
    pub current_metrics: UsageMetrics,
    pub container: WidgetRef,

    pub dot: WidgetRef,
    pub title: WidgetRef,
    pub badge: WidgetRef,
    pub badge2: WidgetRef,

    pub m1_label: WidgetRef,
    pub m1_val: WidgetRef,
    pub m1_bar: WidgetRef,
    pub m1_sub: WidgetRef,

    pub m2_label: WidgetRef,
    pub m2_val: WidgetRef,
    pub m2_bar: WidgetRef,
    pub m2_sub: WidgetRef,

    styles: LocalStyles,
    core: Rc<CardCore>,
    /// Keeps the event filter registered for as long as the card lives.
    _filter: OwnedObject<CardFilter>,
}

impl ProviderCardWidget {
    pub fn new(provider_id: &str) -> Self {
        let dark = true;
        let theme_hex = brand_hex(provider_id, true).to_string();
        let theme_color = brand_color(provider_id, true);
        let styles = LocalStyles::new();

        let width = Rc::new(Cell::new(0));
        let container_widget = EmptyWidget::new();
        {
            let width = Rc::clone(&width);
            container_widget.set_resize_handler(move |size, _old| width.set(size.width));
        }
        let container = make_widget(container_widget);
        let mut root_layout = BoxLayout::vertical();
        root_layout.set_margins(Margins::new(6, 4, 6, 4));
        root_layout.set_spacing(5);

        // 1. Header (dot + title + stretch + badge + badge2)
        let mut header_layout = BoxLayout::horizontal();
        header_layout.set_spacing(4);

        let mut dot_lbl = Label::new("●");
        dot_lbl.set_font(Font::new("Segoe UI", 10.0));
        let dot = make_widget(dot_lbl);
        styles.apply(
            &dot,
            format!("color: {theme_hex}; font-size: 10px;"),
            Some(theme_color),
        );
        header_layout.add_widget(dot.clone());

        let mut title_lbl = Label::new(theme_name(provider_id));
        title_lbl.set_font(Font::new("Segoe UI", 9.5).with_weight(FontWeight::Bold));
        let title = make_widget(title_lbl);
        title.borrow_mut().set_object_name("CardTitle");
        // `self.title.setSizePolicy(Minimum, Preferred)` (`provider_card.py:39`)
        title
            .borrow_mut()
            .set_size_policy(ferrokit_widgets::QSizePolicy::new(
                ferrokit_widgets::Policy::Minimum,
                ferrokit_widgets::Policy::Preferred,
            ));
        styles.apply(&title, title_qss(&theme_hex), Some(theme_color));
        header_layout.add_widget(title.clone());

        header_layout.add_stretch(1);

        let mut badge_lbl = Label::new("--");
        badge_lbl.set_font(Font::new("Consolas", 9.0));
        let badge = make_widget(badge_lbl);
        badge.borrow_mut().set_object_name("Badge");
        header_layout.add_widget(badge.clone());

        let mut badge2_lbl = Label::new("");
        badge2_lbl.set_font(Font::new("Consolas", 9.0));
        let badge2 = make_widget(badge2_lbl);
        badge2.borrow_mut().set_object_name("Badge");
        badge2.borrow_mut().set_visible(false);
        header_layout.add_widget(badge2.clone());
        root_layout.add_layout(Box::new(header_layout), 0);

        // 2. Metric 1 (Session 5H) and 3. Metric 2 (Weekly 7D)
        let m1 = build_metric_block("SESSION 5H", &styles);
        root_layout.add_widget(m1.cell.clone());
        let m2 = build_metric_block("WEEKLY 7D", &styles);
        root_layout.add_widget(m2.cell.clone());

        container.borrow_mut().set_layout(Box::new(root_layout));

        // `self._drop_line`: Python creates it on the first drag; the container is mutably
        // borrowed while the filter runs, so it exists from the start (hidden).
        let drop_line = make_widget(EmptyWidget::with_geometry(Rect::new(0, 0, 0, 3)));
        drop_line
            .borrow()
            .widget_base()
            .set_transparent_for_mouse_events(true);
        drop_line.borrow().set_visible(false);
        drop_line
            .borrow()
            .set_parent_widget(Some(Rc::downgrade(&container)));
        container.borrow_mut().add_child(drop_line.clone());

        arm_cell(&m1.cell);
        arm_cell(&m2.cell);

        let core = Rc::new(CardCore {
            container: container.clone(),
            m1_cell: m1.cell,
            m2_cell: m2.cell,
            m1_label: m1.label.clone(),
            m2_label: m2.label.clone(),
            extra_container: RefCell::new(None),
            extra_rows: RefCell::new(Vec::new()),
            width,
            reorderable: Cell::new(true),
            extra_count: Cell::new(0),
            drop_line,
            theme_hex: RefCell::new(theme_hex),
            drag: RefCell::new(None),
            row_order_changed: Notifier::new(),
        });
        // SAFETY: the owner lives in the card, on the UI thread; nothing else touches the filter
        // object while one of its callbacks runs.
        let filter = unsafe {
            register_owned_qobject(CardFilter {
                data: ObjectData::with_auto_id(),
                core: Rc::clone(&core),
            })
        };
        container
            .borrow_mut()
            .object_data_mut()
            .install_event_filter(OwnedObject::object_id(&filter));

        Self {
            provider_id: provider_id.to_string(),
            dark,
            current_metrics: UsageMetrics {
                provider_id: provider_id.to_string(),
                ..Default::default()
            },
            container,
            dot,
            title,
            badge,
            badge2,
            m1_label: m1.label,
            m1_val: m1.val,
            m1_bar: m1.bar,
            m1_sub: m1.sub,
            m2_label: m2.label,
            m2_val: m2.val,
            m2_bar: m2.bar,
            m2_sub: m2.sub,
            styles,
            core,
            _filter: filter,
        }
    }

    pub fn widget(&self) -> WidgetRef {
        self.container.clone()
    }

    /// Python `rowOrderChanged`: the new order of all row titles after a drag.
    pub fn row_order_changed(&self) -> &Notifier<Vec<String>> {
        &self.core.row_order_changed
    }

    /// Python `_drag_rows` titles: `[m1, m2, extras...]`, empty when there is nothing to reorder.
    pub fn row_titles(&self) -> Vec<String> {
        self.core.row_titles()
    }

    /// Python `SidebarWidget._scale_card_style`: every local `font-size` of the card times `scale`.
    pub fn set_font_scale(&mut self, scale: f64) {
        self.styles.set_scale(scale);
    }

    /// Python `_create_extra_row`.
    fn create_extra_row(&self) {
        if self.core.extra_container.borrow().is_none() {
            let mut layout = BoxLayout::vertical();
            layout.set_margins(Margins::new(0, 0, 0, 0));
            layout.set_spacing(1);
            let extra_container = make_widget(EmptyWidget::new());
            extra_container.borrow_mut().set_layout(Box::new(layout));
            self.container
                .borrow()
                .with_box_layout(|l| l.add_widget(extra_container.clone()));
            *self.core.extra_container.borrow_mut() = Some(extra_container);
        }
        let mut layout = BoxLayout::vertical();
        layout.set_spacing(0);
        let mut header = BoxLayout::horizontal();
        header.set_margins(Margins::new(0, 0, 0, 0));
        header.set_spacing(1);
        let mut title_lbl = Label::new("");
        title_lbl.set_font(Font::new("Segoe UI", 10.0).with_weight(FontWeight::Bold));
        let title = make_widget(title_lbl);
        title.borrow_mut().set_object_name("MetricTitle");
        let mut value_lbl = Label::new("");
        value_lbl.set_font(Font::new("Consolas", 14.0).with_weight(FontWeight::Bold));
        let value = make_widget(value_lbl);
        value.borrow_mut().set_object_name("MetricValue");
        header.add_widget(title.clone());
        header.add_stretch(1);
        header.add_widget(value.clone());
        layout.add_layout(Box::new(header), 0);
        let bar = make_bar();
        layout.add_widget(bar.clone());
        let mut sub_lbl = Label::new("");
        sub_lbl.set_font(Font::new("Segoe UI", 9.5));
        let sub = make_widget(sub_lbl);
        sub.borrow_mut().set_object_name("SubDetail");
        layout.add_widget(sub.clone());
        let cell = make_cell(layout);
        arm_cell(&cell);
        if let Some(extra_container) = self.core.extra_container.borrow().as_ref() {
            extra_container
                .borrow()
                .with_box_layout(|l| l.add_widget(cell.clone()));
        }
        self.core.extra_rows.borrow_mut().push(ExtraRow {
            cell,
            title,
            value,
            bar,
            sub,
        });
    }

    /// Python `_update_extra_metrics`.
    fn update_extra_metrics(&self, metrics: &[MetricRow]) {
        while self.core.extra_rows.borrow().len() < metrics.len() {
            self.create_extra_row();
        }
        if let Some(extra_container) = self.core.extra_container.borrow().as_ref() {
            extra_container.borrow().set_visible(!metrics.is_empty());
        }
        for (index, row) in self.core.extra_rows.borrow().iter().enumerate() {
            row.cell.borrow().set_visible(index < metrics.len());
            let Some(metric) = metrics.get(index) else {
                continue;
            };
            set_label_text(&row.title, &metric.title);
            set_label_text(&row.value, &metric.text);
            let color = value_hex(metric.val, self.dark);
            self.styles.apply(
                &row.value,
                format!("color: {color}; font-size: 14px;"),
                Some(parse_hex(&color)),
            );
            set_bar(&row.bar, bar_value(metric.val), Some(self.dark));
            set_label_color(&row.sub, parse_hex(NEUTRAL_HEX));
            set_label_text(
                &row.sub,
                reset_text(metric.reset, &metric.title, &metric.subtext),
            );
        }
    }

    pub fn set_appearance(&mut self, dark: bool) {
        self.dark = dark;
        let hex = brand_hex(&self.provider_id, dark).to_string();
        self.styles
            .apply(&self.title, title_qss(&hex), Some(brand_color(&self.provider_id, dark)));
        *self.core.theme_hex.borrow_mut() = hex;
        let metrics = self.current_metrics.clone();
        self.update_metrics(&metrics);
    }

    pub fn update_metrics(&mut self, data: &UsageMetrics) {
        self.current_metrics = data.clone();
        let error = error_text(data);
        let offline = error.is_some() && !data.stale;
        self.core.reorderable.set(!offline);
        self.core.extra_count.set(data.extra_metrics.len());
        self.update_extra_metrics(if offline { &[] } else { &data.extra_metrics });
        if !data.provider_name.is_empty() {
            set_label_text(&self.title, &data.provider_name);
        } else {
            set_label_text(&self.title, theme_name(&self.provider_id));
        }

        // `self.setToolTip(data.error or "")` (`provider_card.py:125`); the card is `container`.
        self.container
            .borrow_mut()
            .set_tool_tip(error.unwrap_or(""));
        let error_hex = error_hex(self.dark);
        let theme_hex = self.core.theme_hex.borrow().clone();
        if let (true, Some(error)) = (offline, error) {
            self.styles.apply(
                &self.dot,
                format!("color: {error_hex}; font-size: 10px;"),
                Some(parse_hex(error_hex)),
            );
            set_label_text(&self.m1_val, "ERR");
            self.styles.apply(
                &self.m1_val,
                format!("color: {error_hex}; font-size: 13px;"),
                Some(parse_hex(error_hex)),
            );
            set_bar(&self.m1_bar, 0, None);
            set_label_text(&self.m1_sub, error.split('\n').next().unwrap_or(""));
            set_label_color(&self.m1_sub, parse_hex(error_hex));

            set_label_text(&self.m2_val, "--");
            set_bar(&self.m2_bar, 0, None);
            set_label_text(&self.m2_sub, "");
            set_label_text(&self.badge, "OFFLINE");
            self.badge.borrow_mut().set_visible(true);
            self.badge2.borrow_mut().set_visible(false);
            self.container.borrow().update_layout();
            return;
        }

        self.styles.apply(
            &self.dot,
            format!("color: {theme_hex}; font-size: 10px;"),
            Some(parse_hex(&theme_hex)),
        );
        set_label_color(&self.m1_sub, parse_hex(NEUTRAL_HEX));
        set_label_color(&self.m2_sub, parse_hex(NEUTRAL_HEX));

        // Reset sub labels before updating to prevent residual error messages
        if !data.metric1_subtext.is_empty() {
            set_label_text(&self.m1_sub, &data.metric1_subtext);
        } else if data.metric1_reset.is_none() {
            set_label_text(&self.m1_sub, "重設於: --");
        }

        if !data.metric2_subtext.is_empty() {
            set_label_text(&self.m2_sub, &data.metric2_subtext);
        } else if data.metric2_reset.is_none() {
            set_label_text(&self.m2_sub, "重設於: --");
        }

        // Metric 1
        set_label_text(&self.m1_label, &data.metric1_title);
        set_label_text(&self.m1_val, &data.metric1_text);
        let c1 = value_hex(data.metric1_val, self.dark);
        self.styles.apply(
            &self.m1_val,
            format!("color: {c1}; font-size: 14px;"),
            Some(parse_hex(&c1)),
        );
        set_bar(&self.m1_bar, bar_value(data.metric1_val), Some(self.dark));

        // Metric 2
        set_label_text(&self.m2_label, &data.metric2_title);
        set_label_text(&self.m2_val, &data.metric2_text);
        let c2 = value_hex(data.metric2_val, self.dark);
        self.styles.apply(
            &self.m2_val,
            format!("color: {c2}; font-size: 14px;"),
            Some(parse_hex(&c2)),
        );
        set_bar(&self.m2_bar, bar_value(data.metric2_val), Some(self.dark));

        // Badges (compact formatting to prevent squeezing title)
        let b1 = data
            .badge1_text
            .replace(" 剩餘:", ":")
            .replace("剩餘:", ":");
        let b2 = data
            .badge2_text
            .replace(" 剩餘:", ":")
            .replace("剩餘:", ":");

        if !b1.is_empty() && !b2.is_empty() {
            set_label_text(&self.badge, &b1);
            self.badge.borrow_mut().set_visible(true);
            set_label_text(&self.badge2, &b2);
            self.badge2.borrow_mut().set_visible(true);
        } else if !b1.is_empty() {
            set_label_text(&self.badge, &b1);
            self.badge.borrow_mut().set_visible(true);
            self.badge2.borrow_mut().set_visible(false);
        } else if !b2.is_empty() {
            set_label_text(&self.badge, &b2);
            self.badge.borrow_mut().set_visible(true);
            self.badge2.borrow_mut().set_visible(false);
        } else {
            set_label_text(&self.badge, "--");
            self.badge.borrow_mut().set_visible(true);
            self.badge2.borrow_mut().set_visible(false);
        }

        self.update_countdown();
        if data.stale {
            let stale = stale_hex(self.dark);
            self.styles.apply(
                &self.dot,
                format!("color: {stale}; font-size: 10px;"),
                Some(parse_hex(stale)),
            );
            set_label_text(&self.badge, "STALE");
        }
        // Re-layout container so labels with updated text lengths receive their exact sizes
        self.container.borrow().update_layout();
    }

    pub fn update_countdown(&mut self) {
        let data = &self.current_metrics;
        if error_text(data).is_some() && !data.stale {
            return;
        }
        set_label_text(
            &self.m1_sub,
            reset_text(data.metric1_reset, &data.metric1_title, &data.metric1_subtext),
        );
        set_label_text(
            &self.m2_sub,
            reset_text(data.metric2_reset, &data.metric2_title, &data.metric2_subtext),
        );
        for (metric, row) in data.extra_metrics.iter().zip(self.core.extra_rows.borrow().iter()) {
            set_label_text(
                &row.sub,
                reset_text(metric.reset, &metric.title, &metric.subtext),
            );
        }
        if data.stale {
            let stamp = if let Some(last) = data.last_success {
                let local: DateTime<Local> = DateTime::from(last);
                local.format("%m/%d %H:%M:%S").to_string()
            } else {
                "--".to_string()
            };
            set_label_text(&self.m1_sub, format!("舊資料 {}", stamp));
            set_label_color(&self.m1_sub, parse_hex(stale_hex(self.dark)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_error_recovery_clears_text() {
        let mut card = ProviderCardWidget::new("agy");
        card.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            error: Some("timeout".to_string()),
            ..Default::default()
        });
        assert_eq!(
            card.badge
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "OFFLINE"
        );
        assert_eq!(
            card.m1_val
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "ERR"
        );

        card.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            metric1_val: Some(20.0),
            metric1_text: "20%".to_string(),
            ..Default::default()
        });
        assert_ne!(
            card.badge
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "OFFLINE"
        );
        assert_eq!(
            card.m1_val
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "20%"
        );
        assert_eq!(
            card.m2_val
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "--"
        );
    }

    #[test]
    fn test_absent_timestamp_clears_previous_countdown() {
        let mut card = ProviderCardWidget::new("claude");
        card.update_metrics(&UsageMetrics {
            provider_id: "claude".to_string(),
            metric2_reset: Some(Utc::now() + Duration::days(2)),
            ..Default::default()
        });
        assert!(card
            .m2_sub
            .borrow()
            .as_any()
            .downcast_ref::<Label>()
            .unwrap()
            .text()
            .contains("重設於:"));

        card.update_metrics(&UsageMetrics {
            provider_id: "claude".to_string(),
            ..Default::default()
        });
        assert_eq!(
            card.m2_sub
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "重設於: --"
        );
    }

    #[test]
    fn test_stale_data_remains_visible_and_labelled() {
        let mut card = ProviderCardWidget::new("agy");
        card.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            metric1_val: Some(25.0),
            metric1_text: "25%".to_string(),
            error: Some("timeout".to_string()),
            stale: true,
            last_success: Some(Utc::now()),
            ..Default::default()
        });
        assert_eq!(
            card.m1_val
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "25%"
        );
        assert_eq!(
            card.badge
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "STALE"
        );
    }
    /// RC-14 / G12.8.a. Expected values are PySide6 measurements (`rust/tools/geometry_audit/results/
    /// py_horizontal.json`, DPR 1.25, Windows fonts): the card's `sizeHint` height is 109 and the metric
    /// value label's is 17, because the Python label sets a widget-local `font-size: 14px` that wins
    /// over the application sheet's 16px. Since the metric blocks became RowCells (margins 4, 2, 4, 2,
    /// `provider_card.py` `_make_cell`) the card is 8 px taller: PySide6 now measures 117 (probe:
    /// `ProviderCardWidget('claude')` polished under the cards sheet at DPR 1.25, same fonts).
    #[cfg(windows)]
    #[test]
    fn test_card_size_hint_matches_pyside6() {
        let _setup = crate::ui::test_support::CardsOracleSetup::new();
        let card = ProviderCardWidget::new("claude");

        let value_height = card.m1_val.borrow().size_hint().height;
        assert_eq!(value_height, 17, "metric value label height (PySide6: 17)");
        let card_height = card.widget().borrow().size_hint().height;
        assert_eq!(card_height, 117, "card height (PySide6: 117)");
    }

    /// RC-15. PySide6's `QLabel#Badge` has no `max-height`; its `sizeHint` height is 14 at DPR 1.0
    /// and 15 at DPR 1.25 (`py_*_dpr1.json`, `py_*.json`). A `max-height: 15px` in the sheet forces 15
    /// at both ratios.
    #[cfg(windows)]
    #[test]
    fn test_badge_size_hint_matches_pyside6_at_both_ratios() {
        for (dpr, expected) in [(1.0, 14), (1.25, 15)] {
            let _setup = crate::ui::test_support::CardsOracleSetup::at_dpr(dpr);
            let card = ProviderCardWidget::new("claude");
            let hint = card.badge.borrow().size_hint();
            assert_eq!(
                hint.height, expected,
                "badge height at DPR {dpr} (PySide6: {expected}); hint {hint:?}"
            );
        }
    }

    #[test]
    fn test_badge_font_weight_matches_python() {
        let card = ProviderCardWidget::new("claude");
        let b = card.badge.borrow();
        let lbl = b.as_any().downcast_ref::<Label>().unwrap();
        assert_eq!(
            lbl.font().weight,
            FontWeight::Normal,
            "Badge font weight must be Normal (400) matching Python (G12.8.e)"
        );
    }

    // Expected values are PySide6 measurements taken with the Windows fonts (Microsoft JhengHei UI,
    // Segoe UI, Consolas); Linux and macOS have neither those fonts nor reference numbers.
    #[cfg(windows)]
    #[test]
    fn test_hud_layout_proportions() {
        let config =
            std::sync::Arc::new(parking_lot::Mutex::new(crate::config::ConfigManager::load()));
        let refresh_ctrl = std::sync::Arc::new(parking_lot::Mutex::new(
            crate::refresh_controller::RefreshController::new(60),
        ));
        let mut hud = crate::ui::hud_window::HUDWindow::with_providers(
            config,
            refresh_ctrl,
            crate::providers::stub::stub_providers(),
        )
        .unwrap();

        // 1. Horizontal mode
        hud.window
            .set_geometry(ferrokit_gui::geometry::primitives::Rect::new(
                0, 0, 690, 152,
            ));
        hud.apply_cards_layout_mode("horizontal");
        let stack_geom = hud.stack.borrow().geometry();
        assert!(
            stack_geom.y <= 35,
            "Stack must start below header, not pushed down (got {})",
            stack_geom.y
        );
        let claude_geom = hud.cards["claude"].container.borrow().geometry();
        assert!(
            claude_geom.width >= 200,
            "Card width must be at least 200px (got {})",
            claude_geom.width
        );
        assert_eq!(
            hud.cards["claude"].m1_bar.borrow().geometry().height,
            5,
            "Progress bar height must be 5px"
        );

        // 2. Vertical mode. The step above stored the 690x152 window as the vertical size, which no
        // user has; use the default vertical size (PySide6 280x410: cards keep their hint there).
        {
            let mut cfg = hud.config.lock();
            cfg.vertical_width = crate::config::DEFAULT_VERTICAL_WIDTH;
            // 24 px (3 cards x 8) taller than the old default: the card is 117 high now
            cfg.vertical_height = crate::config::DEFAULT_VERTICAL_HEIGHT + 24;
        }
        hud.apply_cards_layout_mode("vertical");
        let stack_geom_v = hud.stack.borrow().geometry();
        // PySide6: the extra height of a taller vertical window belongs to the header row (RC-16),
        // so the stack starts right under it (root spacing 6) instead of at a fixed y.
        let header_geom = hud.window.root_widget().borrow().children()[0]
            .borrow()
            .geometry();
        assert_eq!(
            stack_geom_v.y,
            header_geom.y + header_geom.height + 6,
            "Stack must start right below the header in vertical mode"
        );
        let c_claude = hud.cards["claude"].container.borrow().geometry();
        let c_agy = hud.cards["agy"].container.borrow().geometry();
        // PySide6 keeps each vertical card at its size hint (117 at DPR 1.25, 116 at 1.0).
        assert_eq!(
            c_claude.height,
            hud.cards["claude"].container.borrow().size_hint().height,
            "Vertical card keeps its size hint (got {})",
            c_claude.height
        );
        assert!(
            c_agy.y + c_agy.height <= stack_geom_v.height + 2,
            "Last card must not overflow stack container"
        );

        // 3. Table mode
        hud.window
            .set_geometry(ferrokit_gui::geometry::primitives::Rect::new(
                0, 0, 450, 350,
            ));
        hud.apply_ui_mode("table");
        let dial_claude = hud.table.columns["claude"].dial.borrow().geometry();
        let dial_codex = hud.table.columns["codex"].dial.borrow().geometry();
        assert!(
            dial_claude.width >= 80 && dial_claude.height >= 80,
            "Dial must meet minimum size"
        );
        assert_eq!(
            dial_claude.height, dial_codex.height,
            "Dials must have uniform height"
        );
    }
    // Expected values are PySide6 measurements taken with the Windows fonts (Microsoft JhengHei UI,
    // Segoe UI, Consolas); Linux and macOS have neither those fonts nor reference numbers.
    #[cfg(windows)]
    #[test]
    fn test_horizontal_cards_fit_the_window_with_populated_badges() {
        let mut cfg = crate::config::Config {
            layout_mode: "horizontal".to_string(),
            ..Default::default()
        };
        crate::config::ConfigManager::sanitize(&mut cfg);
        let config = std::sync::Arc::new(parking_lot::Mutex::new(cfg));
        let refresh_ctrl = std::sync::Arc::new(parking_lot::Mutex::new(
            crate::refresh_controller::RefreshController::new(60),
        ));
        let mut hud = crate::ui::hud_window::HUDWindow::with_providers(
            config,
            refresh_ctrl,
            crate::providers::stub::stub_providers(),
        )
        .unwrap();
        hud.apply_cards_layout_mode("horizontal");
        for (pid, name, b1, b2) in [
            ("claude", "Claude Code", "Code: 100%", "Chat: 0%"),
            ("agy", "Antigravity", "C/G: 55%", "Gemini Models"),
            ("codex", "OpenAI Codex", "Plan: Free", ""),
        ] {
            let m = UsageMetrics {
                provider_id: pid.to_string(),
                provider_name: name.to_string(),
                badge1_text: b1.to_string(),
                badge2_text: b2.to_string(),
                metric1_title: "SESSION 5H".to_string(),
                metric1_text: "68%".to_string(),
                metric1_val: Some(68.0),
                metric2_title: "WEEKLY 7D".to_string(),
                metric2_text: "100%".to_string(),
                metric2_val: Some(100.0),
                ..Default::default()
            };
            hud.cards.get_mut(pid).unwrap().update_metrics(&m);
        }
        hud.cards_container.borrow().update_layout();
        let container_w = hud.cards_container.borrow().geometry().width;
        let mut prev_right = i32::MIN;
        for pid in ["claude", "agy", "codex"] {
            let c = &hud.cards[pid];
            let g = c.container.borrow().geometry();
            assert!(g.x >= prev_right, "{pid} overlaps the previous card: {g:?}");
            assert!(
                g.x + g.width <= container_w,
                "{pid} card ({g:?}) is pushed out of the {container_w}px container"
            );
            assert!(
                g.width >= c.container.borrow().minimum_size().width,
                "{pid} card is narrower than its content"
            );
            prev_right = g.x + g.width;
        }
    }
    #[test]
    fn test_horizontal_triple_side_by_side_parity_and_switching() {
        let mut cfg = crate::config::Config {
            layout_mode: "vertical".to_string(),
            horizontal_height: 463,
            horizontal_width: 540,
            ..Default::default()
        };
        crate::config::ConfigManager::sanitize(&mut cfg);
        let config = std::sync::Arc::new(parking_lot::Mutex::new(cfg));
        let refresh_ctrl = std::sync::Arc::new(parking_lot::Mutex::new(
            crate::refresh_controller::RefreshController::new(60),
        ));
        let mut hud = crate::ui::hud_window::HUDWindow::with_providers(
            config,
            refresh_ctrl,
            crate::providers::stub::stub_providers(),
        )
        .unwrap();

        // 1. Initial state is vertical
        assert_eq!(hud.config.lock().layout_mode, "vertical");
        let c_claude_v = hud.cards["claude"].container.borrow().geometry();
        let c_codex_v = hud.cards["codex"].container.borrow().geometry();
        let c_agy_v = hud.cards["agy"].container.borrow().geometry();
        assert_eq!(c_claude_v.x, c_codex_v.x);
        assert_eq!(c_claude_v.x, c_agy_v.x);
        assert!(c_claude_v.y < c_agy_v.y);
        assert!(c_agy_v.y < c_codex_v.y);

        // 2. Switch to Horizontal Triple mode
        hud.apply_cards_layout_mode("horizontal");
        assert_eq!(hud.config.lock().layout_mode, "horizontal");

        let win_geom = hud.window.geometry();
        assert_eq!(
            win_geom.width,
            crate::config::DEFAULT_HORIZONTAL_WIDTH as i32
        );
        assert_eq!(
            win_geom.height,
            crate::config::DEFAULT_HORIZONTAL_HEIGHT as i32
        );

        let c_claude_h = hud.cards["claude"].container.borrow().geometry();
        let c_codex_h = hud.cards["codex"].container.borrow().geometry();
        let c_agy_h = hud.cards["agy"].container.borrow().geometry();

        // IN HORIZONTAL MODE: Y positions must be identical (side-by-side)
        assert_eq!(
            c_claude_h.y, c_codex_h.y,
            "Horizontal cards must have same Y offset"
        );
        assert_eq!(
            c_claude_h.y, c_agy_h.y,
            "Horizontal cards must have same Y offset"
        );

        // IN HORIZONTAL MODE: X positions must be strictly increasing across the 3 columns
        assert!(
            c_claude_h.x < c_agy_h.x,
            "Claude must be to the left of Antigravity"
        );
        assert!(
            c_agy_h.x < c_codex_h.x,
            "Antigravity must be to the left of Codex"
        );

        // Card widths must each occupy a substantial column (>= 200px)
        assert!(c_claude_h.width >= 200);
        assert!(c_codex_h.width >= 200);
        assert!(c_agy_h.width >= 200);

        // 3. Toggle back to vertical via toggle_cards_layout()
        hud.toggle_cards_layout();
        assert_eq!(hud.config.lock().layout_mode, "vertical");
        let c_claude_v2 = hud.cards["claude"].container.borrow().geometry();
        let c_codex_v2 = hud.cards["codex"].container.borrow().geometry();
        assert_eq!(c_claude_v2.x, c_codex_v2.x);
        assert!(c_claude_v2.y < c_codex_v2.y);
    }

    #[test]
    fn test_inspect_card_layout_with_data() {
        let el = ferrokit_core::event_loop::EventLoop::new();
        let config =
            std::sync::Arc::new(parking_lot::Mutex::new(crate::config::ConfigManager::load()));
        let refresh_ctrl = std::sync::Arc::new(parking_lot::Mutex::new(
            crate::refresh_controller::RefreshController::new(60),
        ));
        let mut hud = crate::ui::hud_window::HUDWindow::with_providers(
            config,
            refresh_ctrl,
            crate::providers::stub::stub_providers(),
        )
        .unwrap();
        hud.window
            .set_geometry(ferrokit_gui::geometry::primitives::Rect::new(
                0, 0, 883, 574,
            ));
        hud.apply_cards_layout_mode("vertical");

        let claude_data = crate::providers::base::UsageMetrics {
            provider_id: "claude".to_string(),
            badge1_text: "Chat: 0%".to_string(),
            badge2_text: "Code: 10%".to_string(),
            metric1_title: "SESSION 5H".to_string(),
            metric1_text: "10%".to_string(),
            metric1_val: Some(10.0),
            metric2_title: "WEEKLY 7D".to_string(),
            metric2_text: "49%".to_string(),
            metric2_val: Some(49.0),
            ..Default::default()
        };
        hud.on_data_fetched(&claude_data);

        let agy_data = crate::providers::base::UsageMetrics {
            provider_id: "agy".to_string(),
            provider_name: "Antigravity".to_string(),
            badge1_text: "C/G: 10%".to_string(),
            badge2_text: "Gemini Models".to_string(),
            metric1_title: "SESSION 5H".to_string(),
            metric1_text: "40%".to_string(),
            metric1_val: Some(40.0),
            metric2_title: "WEEKLY 7D".to_string(),
            metric2_text: "58%".to_string(),
            metric2_val: Some(58.0),
            ..Default::default()
        };
        hud.on_data_fetched(&agy_data);

        let codex_data = crate::providers::base::UsageMetrics {
            provider_id: "codex".to_string(),
            error: Some("登入憑證已失效，請使用原 CLI 重新登入".to_string()),
            ..Default::default()
        };
        hud.on_data_fetched(&codex_data);
        // Qt lays out on the LayoutRequest / UpdateRequest pass of the event loop, not inside the
        // setters; let that pass run before reading geometry.
        el.process_events(false);

        let c = &hud.cards["claude"];
        println!("CLAUDE title: {:?}", c.title.borrow().geometry());
        let title_geom = c.title.borrow().geometry();
        let badge_geom = c.badge.borrow().geometry();
        let badge2_geom = c.badge2.borrow().geometry();

        // Regression: Badge and Badge2 must never overlap title at (0, 0)
        assert!(
            badge_geom.x >= title_geom.x + title_geom.width,
            "Badge must be right of title"
        );
        assert!(
            badge2_geom.x >= badge_geom.x + badge_geom.width,
            "Badge2 must be right of badge"
        );
        assert!(badge2_geom.width > 0, "Badge2 must have positive width");

        let a = &hud.cards["agy"];
        let a_title = a.title.borrow().geometry();
        let a_badge = a.badge.borrow().geometry();
        let a_badge2 = a.badge2.borrow().geometry();

        assert!(
            a_badge.x >= a_title.x + a_title.width,
            "Agy badge must be right of title"
        );
        assert!(
            a_badge2.x >= a_badge.x + a_badge.width,
            "Agy badge2 must be right of badge"
        );
        assert!(a_badge2.width > 0, "Agy badge2 must have positive width");
    }
}

#[cfg(test)]
mod card_logic_tests {
    use super::*;
    use chrono::Duration;

    fn local_qss(card: &ProviderCardWidget, w: &WidgetRef) -> String {
        card.styles
            .specs
            .borrow()
            .iter()
            .find(|(s, _)| Rc::ptr_eq(s, w))
            .map(|(_, q)| q.clone())
            .unwrap_or_default()
    }

    #[test]
    fn reset_text_without_reset_uses_subtext_or_placeholder() {
        assert_eq!(reset_text(None, "SESSION 5H", ""), "重設於: --");
        assert_eq!(reset_text(None, "SESSION 5H", "已用完"), "已用完");
    }

    #[test]
    fn reset_text_has_a_stamp_only_for_long_windows() {
        let reset = Some(Utc::now() + Duration::hours(3));
        let short = reset_text(reset, "SESSION 5H", "");
        assert!(short.starts_with("重設於: ") && !short.contains('·'), "{short}");
        let weekly = reset_text(reset, "WEEKLY 7D", "");
        assert!(weekly.starts_with("重設於: 週") && weekly.contains(" · "), "{weekly}");
    }

    #[test]
    fn drop_index_counts_row_centres_above_the_pointer() {
        // Three rows of height 10: centres 4, 14, 24 (Qt `QRect::center` of top..=bottom).
        let rows = [(0, 9), (10, 19), (20, 29)];
        assert_eq!(drop_index(&rows, 2, 0), (vec![0, 1], 0));
        assert_eq!(drop_index(&rows, 2, 5), (vec![0, 1], 1));
        assert_eq!(drop_index(&rows, 2, 100), (vec![0, 1], 2));
        assert_eq!(drop_index(&rows, 0, 14), (vec![1, 2], 0));
        assert_eq!(drop_index(&rows, 0, 15), (vec![1, 2], 1));
    }

    #[test]
    fn drop_line_sits_above_the_first_row_or_below_the_previous_one() {
        let rows = [(0, 9), (10, 19), (20, 29)];
        let (others, _) = drop_index(&rows, 2, 0);
        assert_eq!(drop_line_y(&rows, &others, 0), -2);
        assert_eq!(drop_line_y(&rows, &others, 1), 10);
        assert_eq!(drop_line_y(&rows, &others, 2), 20);
    }

    #[test]
    fn reordered_moves_the_dragged_title() {
        let titles: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        // Dragging the last row above the first one (python test: 'C/G 7D' to the top).
        assert_eq!(reordered(&titles, 3, 0), ["d", "a", "b", "c"]);
        assert_eq!(reordered(&titles, 0, 2), ["b", "c", "a", "d"]);
        assert_eq!(reordered(&titles, 1, 1), titles);
    }

    #[test]
    fn font_sizes_scale_and_one_restores() {
        let qss = "color: #fff; font-size: 9.5px; font-weight: 800; letter-spacing: 0.4px;";
        assert_eq!(
            scale_font_sizes(qss, 2.0),
            "color: #fff; font-size: 19px; font-weight: 800; letter-spacing: 0.4px;"
        );
        assert_eq!(scale_font_sizes(qss, 1.0), qss);
        assert_eq!(scale_font_sizes("font-size:14px;", 1.5), "font-size: 21px;");
        assert_eq!(scale_font_sizes("color: red;", 3.0), "color: red;");
    }

    /// python `CardUsesGaugeTests`.
    #[test]
    fn card_bars_are_gauges_and_numbers_match_the_leading_colour() {
        use crate::providers::base::MetricRow;
        let mut card = ProviderCardWidget::new("agy");
        card.set_appearance(true);
        card.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            metric1_title: "C/G 5H".to_string(),
            metric1_val: Some(82.0),
            metric1_text: "82%".to_string(),
            metric2_title: "ARGON 7D".to_string(),
            metric2_val: Some(12.0),
            metric2_text: "12%".to_string(),
            extra_metrics: vec![MetricRow::new("GEMINI 5H", Some(55.0), "55%")],
            ..Default::default()
        });
        let value = |w: &WidgetRef| {
            w.borrow()
                .as_any()
                .downcast_ref::<GaugeBar>()
                .map(|b| (b.value(), b.dark()))
        };
        assert_eq!(value(&card.m1_bar), Some((82, true)));
        assert!(local_qss(&card, &card.m1_val).contains(&to_hex(ramp_color(82.0, true, None))));
        assert!(local_qss(&card, &card.m2_val).contains(&to_hex(ramp_color(12.0, true, None))));
        let (extra_value, extra_bar) = {
            let rows = card.core.extra_rows.borrow();
            (rows[0].value.clone(), rows[0].bar.clone())
        };
        assert_eq!(value(&extra_bar), Some((55, true)));
        assert!(local_qss(&card, &extra_value).contains(&to_hex(ramp_color(55.0, true, None))));
        assert_eq!(
            card.row_titles(),
            ["C/G 5H", "ARGON 7D", "GEMINI 5H"].map(String::from)
        );

        card.set_appearance(false);
        assert!(local_qss(&card, &card.m1_val).contains(&to_hex(ramp_color(82.0, false, None))));
        assert_eq!(value(&card.m1_bar), Some((82, false)));
    }

    #[test]
    fn rows_are_not_reorderable_while_offline() {
        let mut card = ProviderCardWidget::new("claude");
        assert_eq!(card.row_titles(), ["SESSION 5H", "WEEKLY 7D"].map(String::from));
        card.update_metrics(&UsageMetrics {
            provider_id: "claude".to_string(),
            error: Some("timeout".to_string()),
            ..Default::default()
        });
        assert!(card.row_titles().is_empty());
    }

    #[test]
    fn font_scale_applies_to_later_styles() {
        let mut card = ProviderCardWidget::new("claude");
        card.set_font_scale(2.0);
        card.update_metrics(&UsageMetrics {
            provider_id: "claude".to_string(),
            metric1_val: Some(10.0),
            metric1_text: "10%".to_string(),
            ..Default::default()
        });
        // The stored style stays at scale 1 so that `1.0` restores it.
        assert!(local_qss(&card, &card.m1_val).contains("font-size: 14px"));
        assert_eq!(card.styles.scale.get(), 2.0);
    }
}
