//! Table-style HUD body matching Python ui/usage_table.py.
//!
//! Providers are arranged in columns with shared row labels on the left.
//! Each column shows the 5-hour window (reset time, countdown) and weekly
//! window (reset time, countdown, percentage) around a central dual dial:
//!   - inner pie  = 5-hour usage
//!   - outer ring = weekly usage
//!   - pace tick  = even-pace benchmark

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::rc::Rc;

use qtrs_core::object::{ObjectData, ObjectId, QObject};
use qtrs_gui::geometry::primitives::{Margins, PointF, Rect, RectF};
use qtrs_gui::paint::brush::Brush;
use qtrs_gui::paint::painter::{Painter, Pen};
use qtrs_gui::text::font::{Font, FontWeight};
use qtrs_gui::text::font_metrics::FontMetrics;
use qtrs_gui::tiny_skia::Color;
use qtrs_widgets::{
    BoxLayout, EmptyWidget, GridLayout, Label, Layout, Widget, WidgetBase, WidgetRef,
};

use super::styles::{duo_colors, scale_colors, Theme};
use crate::pace::{
    elapsed_fraction, format_countdown_dhm, format_countdown_hm, format_reset_time, pace_mark,
    window_caption, window_seconds, FIVE_HOURS, ONE_WEEK,
};
use crate::providers::base::UsageMetrics;

pub const PROVIDER_ORDER: [&str; 3] = ["claude", "codex", "agy"];

fn make_widget<W: Widget + 'static>(w: W) -> WidgetRef {
    Rc::new(RefCell::new(Box::new(w)))
}

fn set_label_text(w: &WidgetRef, text: impl Into<String>) {
    if let Some(lbl) = w.borrow_mut().as_any_mut().downcast_mut::<Label>() {
        lbl.set_text(text);
    }
}

fn set_label_color(w: &WidgetRef, color: Color) {
    if let Some(lbl) = w.borrow_mut().as_any_mut().downcast_mut::<Label>() {
        lbl.set_color(color);
    }
}

/// Custom dial drawing matching Python `UsageDial`.
pub struct UsageDial {
    base: WidgetBase,
    theme: Theme,
    inner: (Option<f64>, Option<f64>, Color), // (percent, mark, color)
    outer: (Option<f64>, Option<f64>, Color),
    inner_text: String,
    caption: String,
    muted: bool,
}

impl UsageDial {
    pub fn new(theme: Theme) -> Self {
        let base = WidgetBase::with_geometry(Rect::new(0, 0, 84, 84));
        base.set_size_policy(qtrs_widgets::QSizePolicy::new(
            qtrs_widgets::Policy::Expanding,
            qtrs_widgets::Policy::Expanding,
        ));

        Self {
            base,
            theme,
            inner: (None, None, Color::TRANSPARENT),
            outer: (None, None, Color::TRANSPARENT),
            inner_text: "--".to_string(),
            caption: String::new(),
            muted: false,
        }
    }

    pub fn set_values(
        &mut self,
        inner: (Option<f64>, Option<f64>, Color),
        inner_text: impl Into<String>,
        outer: (Option<f64>, Option<f64>, Color),
        caption: impl Into<String>,
        muted: bool,
    ) {
        self.inner = inner;
        self.inner_text = inner_text.into();
        self.outer = outer;
        self.caption = caption.into();
        self.muted = muted;
        self.update();
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.update();
    }

    fn deg(percent: f64) -> f32 {
        (-360.0 * (percent.clamp(0.0, 100.0) / 100.0)) as f32
    }

    fn draw_pace_tick(
        painter: &mut Painter,
        center: PointF,
        r_from: f32,
        r_to: f32,
        mark: Option<f64>,
        color: Color,
        muted: bool,
    ) {
        let mark = match mark {
            Some(m) if !muted => m,
            _ => return,
        };

        let angle_rad = ((90.0 - 360.0 * (mark / 100.0)) as f32) * PI / 180.0;
        let p1 = PointF::new(
            center.x + r_from * angle_rad.cos(),
            center.y - r_from * angle_rad.sin(),
        );
        let p2 = PointF::new(
            center.x + r_to * angle_rad.cos(),
            center.y - r_to * angle_rad.sin(),
        );

        painter.set_pen(Pen::new(color, 1.6));
        painter.draw_line(p1, p2);
    }
}

impl QObject for UsageDial {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }
}

impl Widget for UsageDial {
    fn id(&self) -> ObjectId {
        self.base.object_data.id
    }
    fn geometry(&self) -> Rect {
        self.base.geometry()
    }
    fn set_geometry(&self, rect: Rect) {
        self.base.set_geometry(rect);
    }
    fn size_hint(&self) -> qtrs_gui::geometry::primitives::Size {
        qtrs_gui::geometry::primitives::Size::new(84, 84)
    }
    fn minimum_size(&self) -> qtrs_gui::geometry::primitives::Size {
        qtrs_gui::geometry::primitives::Size::new(84, 84)
    }
    fn size_policy(&self) -> qtrs_widgets::QSizePolicy {
        self.base.size_policy()
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
    fn set_enabled(&self, enabled: bool) {
        self.base.set_enabled(enabled);
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
    fn layout(&self) -> Option<&dyn Layout> {
        None
    }
    fn layout_mut(&mut self) -> Option<&mut Box<dyn Layout>> {
        None
    }
    fn set_layout(&mut self, _layout: Box<dyn Layout>) {}
    fn parent_widget(&self) -> Option<qtrs_widgets::WidgetWeak> {
        self.base.parent_widget()
    }
    fn set_parent_widget(&self, parent: Option<qtrs_widgets::WidgetWeak>) {
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
    fn add_child(&mut self, _child: WidgetRef) {}
    fn remove_child(&mut self, _child_id: ObjectId) {}

    fn paint_event(&mut self, painter: &mut Painter) {
        let geo = self.base.geometry();
        let w = geo.width as f32;
        let h = geo.height as f32;
        let side = (w.min(h) - 4.0).max(10.0);
        let center = PointF::new(w / 2.0, h / 2.0);
        let ring_width = (side * 0.07).max(5.0);

        // 1. Outer ring (Weekly 7D)
        let outer_rect = RectF::new(center.x - side / 2.0, center.y - side / 2.0, side, side);
        painter.set_brush(Brush::NoBrush);
        painter.set_pen(Pen::new(self.theme.track, ring_width));
        painter.draw_ellipse(outer_rect);

        let (o_pct, o_mark, o_color) = self.outer;
        if let Some(pct) = o_pct {
            if !self.muted && pct > 0.0 {
                painter.set_pen(Pen::new(o_color, ring_width));
                painter.draw_arc(outer_rect, 90.0, Self::deg(pct));
            }
        }
        Self::draw_pace_tick(
            painter,
            center,
            side / 2.0 - ring_width - 2.5,
            side / 2.0 + 0.5,
            o_mark,
            self.theme.text,
            self.muted,
        );

        // 2. Inner pie (Session 5H)
        let gap = ring_width + side * 0.06;
        let inner_side = (side - gap * 2.0).max(4.0);
        let inner_rect = RectF::new(
            center.x - inner_side / 2.0,
            center.y - inner_side / 2.0,
            inner_side,
            inner_side,
        );

        painter.set_pen(None);
        painter.set_brush(Brush::Color(self.theme.disc));
        painter.draw_ellipse(inner_rect);

        let (i_pct, i_mark, i_color) = self.inner;
        if let Some(pct) = i_pct {
            if !self.muted && pct > 0.0 {
                painter.set_brush(Brush::Color(i_color));
                if pct >= 100.0 {
                    painter.draw_ellipse(inner_rect);
                } else {
                    painter.draw_pie(inner_rect, 90.0, Self::deg(pct));
                }
            }
        }
        let r_in = inner_side / 2.0;
        Self::draw_pace_tick(
            painter,
            center,
            r_in * 0.6,
            r_in,
            i_mark,
            self.theme.text,
            self.muted,
        );

        // 3. Center percentage text
        let font_size = (inner_side * 0.28).max(9.0);
        let font = Font::new("Segoe UI", font_size)
            .with_weight(FontWeight::Bold)
            .with_tabular_numbers(true);
        let metrics = FontMetrics::from_font(&font);
        let text_w = metrics.horizontal_advance(&self.inner_text, &font);
        let text_x = center.x - text_w / 2.0;
        let text_y = center.y + metrics.ascent / 2.0;

        let text_color = if self.muted {
            self.theme.text3
        } else {
            self.theme.text
        };
        painter.set_pen(Pen::new(text_color, 1.0));
        painter.draw_text(PointF::new(text_x, text_y), &self.inner_text, &font);

        // 4. Caption if provider window differs (e.g. "1D")
        if !self.caption.is_empty() {
            let cap_font =
                Font::new("Segoe UI", (inner_side * 0.14).max(8.0)).with_weight(FontWeight::Bold);
            let cap_metrics = FontMetrics::from_font(&cap_font);
            let cap_w = cap_metrics.horizontal_advance(&self.caption, &cap_font);
            painter.set_pen(Pen::new(self.theme.text2, 1.0));
            painter.draw_text(
                PointF::new(center.x - cap_w / 2.0, center.y + inner_side * 0.35),
                &self.caption,
                &cap_font,
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

#[allow(dead_code)]
pub struct ProviderColumn {
    pub provider_id: String,
    pub theme: Theme,
    pub scheme: String,
    pub current_metrics: UsageMetrics,

    pub header: WidgetRef,
    pub name: WidgetRef,
    pub badge: WidgetRef,
    pub m1_reset: WidgetRef,
    pub m1_countdown: WidgetRef,
    pub dial: WidgetRef,
    pub m2_val: WidgetRef,
    pub m2_reset: WidgetRef,
    pub m2_countdown: WidgetRef,
}

impl ProviderColumn {
    pub fn new(provider_id: &str, theme: Theme, scheme: &str) -> Self {
        let header = make_widget(EmptyWidget::new());
        let mut hv = BoxLayout::vertical();
        hv.set_margins(Margins::new(0, 0, 0, 0));
        hv.set_spacing(0);

        let mut top = BoxLayout::horizontal();
        top.set_spacing(5);
        top.add_stretch(1);

        let display_name = match provider_id {
            "claude" => "Claude Code",
            "codex" => "Codex",
            "agy" => "Antigravity",
            _ => provider_id,
        };
        let mut name_lbl = Label::new(display_name);
        name_lbl.set_color(theme.text);
        name_lbl.set_font(Font::new("Segoe UI", 12.5).with_weight(FontWeight::Bold));
        let name = make_widget(name_lbl);
        top.add_widget(name.clone());
        top.add_stretch(1);

        let top_widget = make_widget(EmptyWidget::new());
        top_widget.borrow_mut().set_layout(Box::new(top));
        hv.add_widget(top_widget);

        let mut badge_lbl = Label::new(" ");
        badge_lbl.set_color(theme.text2);
        badge_lbl.set_font(Font::new("Segoe UI", 9.5).with_weight(FontWeight::Bold));
        let badge = make_widget(badge_lbl);
        hv.add_widget(badge.clone());
        header.borrow_mut().set_layout(Box::new(hv));

        let make_cell_label = |txt: &str, sz: f32, bold: bool| {
            let mut l = Label::new(txt);
            let mut f = Font::new("Segoe UI", sz).with_tabular_numbers(true);
            if bold {
                f.weight = FontWeight::Bold;
            }
            l.set_font(f);
            make_widget(l)
        };

        let m1_reset = make_cell_label("--:--", 12.5, false);
        let m1_countdown = make_cell_label("--:--", 12.5, false);
        let dial = make_widget(UsageDial::new(theme.clone()));
        let m2_val = make_cell_label("--", 11.5, true);
        let m2_reset = make_cell_label("--:--", 12.5, false);
        let m2_countdown = make_cell_label("--:--:--", 12.5, false);

        Self {
            provider_id: provider_id.to_string(),
            theme,
            scheme: scheme.to_string(),
            current_metrics: UsageMetrics {
                provider_id: provider_id.to_string(),
                ..Default::default()
            },
            header,
            name,
            badge,
            m1_reset,
            m1_countdown,
            dial,
            m2_val,
            m2_reset,
            m2_countdown,
        }
    }

    pub fn set_theme(&mut self, theme: Theme, scheme: &str) {
        self.theme = theme.clone();
        self.scheme = scheme.to_string();
        if let Some(d) = self
            .dial
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<UsageDial>()
        {
            d.set_theme(theme);
        }
        let metrics = self.current_metrics.clone();
        self.update_metrics(&metrics);
    }

    pub fn update_metrics(&mut self, data: &UsageMetrics) {
        self.current_metrics = data.clone();
        let is_offline = data.error.is_some() && !data.stale;

        if is_offline {
            set_label_text(&self.badge, "OFFLINE");
            set_label_color(&self.badge, self.theme.scale_red);
            if let Some(d) = self
                .dial
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<UsageDial>()
            {
                d.set_values(
                    (None, None, Color::TRANSPARENT),
                    "ERR",
                    (None, None, Color::TRANSPARENT),
                    "",
                    true,
                );
            }
            set_label_text(&self.m2_val, "--");
            set_label_text(&self.m1_reset, "--:--");
            set_label_text(&self.m1_countdown, "--:--");
            set_label_text(&self.m2_reset, "--:--");
            set_label_text(&self.m2_countdown, "--:--:--");
            return;
        }

        let badge_text = if data.stale {
            "⏱ STALE"
        } else if !data.badge1_text.is_empty() {
            &data.badge1_text
        } else if !data.badge2_text.is_empty() {
            &data.badge2_text
        } else {
            " "
        };
        set_label_text(&self.badge, badge_text);
        if data.stale {
            set_label_color(&self.badge, self.theme.scale_yellow);
        } else {
            set_label_color(&self.badge, self.theme.text2);
        }

        self.update_countdown();
    }

    pub fn update_countdown(&mut self) {
        let is_offline = self.current_metrics.error.is_some() && !self.current_metrics.stale;
        if is_offline {
            return;
        }

        let data = &self.current_metrics;
        set_label_text(&self.m1_reset, format_reset_time(data.metric1_reset, false));
        set_label_text(&self.m1_countdown, format_countdown_hm(data.metric1_reset));
        set_label_text(&self.m2_reset, format_reset_time(data.metric2_reset, true));
        set_label_text(&self.m2_countdown, format_countdown_dhm(data.metric2_reset));

        let w1 = window_seconds(&data.metric1_title, FIVE_HOURS);
        let w2 = window_seconds(&data.metric2_title, ONE_WEEK);
        let e1 = elapsed_fraction(data.metric1_reset, w1);
        let e2 = elapsed_fraction(data.metric2_reset, w2);
        let mark1 = pace_mark(e1);
        let mark2 = pace_mark(e2);

        let (fill1, _) = if self.scheme == "duo" {
            duo_colors(&self.theme, true)
        } else {
            scale_colors(&self.theme, data.metric1_val)
        };
        let (fill2, text2) = if self.scheme == "duo" {
            duo_colors(&self.theme, false)
        } else {
            scale_colors(&self.theme, data.metric2_val)
        };

        let cap1 = window_caption(&data.metric1_title, "5H");
        if let Some(d) = self
            .dial
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<UsageDial>()
        {
            d.set_values(
                (data.metric1_val, mark1, fill1),
                &data.metric1_text,
                (data.metric2_val, mark2, fill2),
                cap1,
                false,
            );
        }

        let mut pct_text = data.metric2_text.replace("%", " %");
        let cap2 = window_caption(&data.metric2_title, "7D");
        if !cap2.is_empty() {
            pct_text = format!("{} · {}", pct_text, cap2);
        }
        if let (Some(v2), Some(m2)) = (data.metric2_val, mark2) {
            if v2 - m2 > 0.5 {
                pct_text = format!("{}  ▲{:.0}", pct_text, v2 - m2);
            }
        }
        set_label_text(&self.m2_val, pct_text);
        set_label_color(&self.m2_val, text2);
    }
}

pub struct UsageTable {
    pub container: WidgetRef,
    pub columns: HashMap<String, ProviderColumn>,
    pub theme: Theme,
    pub scheme: String,
}

impl UsageTable {
    pub fn new(theme: Theme, scheme: &str) -> Self {
        let container = make_widget(EmptyWidget::new());
        let mut grid = GridLayout::new();
        grid.set_margins(Margins::new(0, 0, 0, 0));
        grid.set_horizontal_spacing(10);
        grid.set_vertical_spacing(4);

        let mut columns = HashMap::new();
        for pid in PROVIDER_ORDER {
            columns.insert(
                pid.to_string(),
                ProviderColumn::new(pid, theme.clone(), scheme),
            );
        }

        // Row 1: Section title "5 小時"
        let mut five_h_lbl = Label::new("5 小時");
        five_h_lbl.set_color(theme.text);
        five_h_lbl.set_font(Font::new("Segoe UI", 11.5).with_weight(FontWeight::Bold));
        grid.add_widget(make_widget(five_h_lbl), 2, 0);

        // Row 3: "重設"
        let mut r1_lbl = Label::new("重設");
        r1_lbl.set_color(theme.text2);
        r1_lbl.set_font(Font::new("Segoe UI", 11.0));
        grid.add_widget(make_widget(r1_lbl), 3, 0);

        // Row 4: "剩餘"
        let mut l1_lbl = Label::new("剩餘");
        l1_lbl.set_color(theme.text2);
        l1_lbl.set_font(Font::new("Segoe UI", 11.0));
        grid.add_widget(make_widget(l1_lbl), 4, 0);

        // Row 5: Legend (left)
        let mut leg_lbl = Label::new("內圈 5H\n外環 1週");
        leg_lbl.set_color(theme.text3);
        leg_lbl.set_font(Font::new("Segoe UI", 9.0));
        grid.add_widget(make_widget(leg_lbl), 5, 0);

        // Row 7: Section title "1 週"
        let mut week_lbl = Label::new("1 週");
        week_lbl.set_color(theme.text);
        week_lbl.set_font(Font::new("Segoe UI", 11.5).with_weight(FontWeight::Bold));
        grid.add_widget(make_widget(week_lbl), 7, 0);

        // Row 8: "重設"
        let mut r2_lbl = Label::new("重設");
        r2_lbl.set_color(theme.text2);
        r2_lbl.set_font(Font::new("Segoe UI", 11.0));
        grid.add_widget(make_widget(r2_lbl), 8, 0);

        // Row 9: "剩餘"
        let mut l2_lbl = Label::new("剩餘");
        l2_lbl.set_color(theme.text2);
        l2_lbl.set_font(Font::new("Segoe UI", 11.0));
        grid.add_widget(make_widget(l2_lbl), 9, 0);

        // Populate provider columns
        for (col_idx, pid) in PROVIDER_ORDER.iter().enumerate() {
            let col = col_idx + 1;
            if let Some(c) = columns.get(*pid) {
                grid.add_widget(c.header.clone(), 0, col);
                grid.add_widget(c.m1_reset.clone(), 3, col);
                grid.add_widget(c.m1_countdown.clone(), 4, col);
                grid.add_widget(c.dial.clone(), 5, col);
                grid.add_widget(c.m2_val.clone(), 7, col);
                grid.add_widget(c.m2_reset.clone(), 8, col);
                grid.add_widget(c.m2_countdown.clone(), 9, col);
                grid.set_column_stretch(col, 1);
            }
        }
        grid.set_row_stretch(5, 1);

        container.borrow_mut().set_layout(Box::new(grid));

        Self {
            container,
            columns,
            theme,
            scheme: scheme.to_string(),
        }
    }

    pub fn widget(&self) -> WidgetRef {
        self.container.clone()
    }

    pub fn set_theme(&mut self, theme: Theme, scheme: &str) {
        self.theme = theme.clone();
        self.scheme = scheme.to_string();
        for col in self.columns.values_mut() {
            col.set_theme(theme.clone(), scheme);
        }
    }

    pub fn update_metrics(&mut self, data: &UsageMetrics) {
        if let Some(col) = self.columns.get_mut(&data.provider_id) {
            col.update_metrics(data);
        }
    }

    pub fn update_countdowns(&mut self) {
        for col in self.columns.values_mut() {
            col.update_countdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_error_recovery_clears_offline_state() {
        let mut table = UsageTable::new(Theme::dark(), "scale");
        table.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            error: Some("timeout".to_string()),
            ..Default::default()
        });
        let col = table.columns.get("agy").unwrap();
        assert_eq!(
            col.badge
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "OFFLINE"
        );

        table.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            metric1_val: Some(20.0),
            metric1_text: "20%".to_string(),
            ..Default::default()
        });
        let col = table.columns.get("agy").unwrap();
        assert_ne!(
            col.badge
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "OFFLINE"
        );
        assert_eq!(
            col.m2_val
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
        let mut table = UsageTable::new(Theme::dark(), "scale");
        table.update_metrics(&UsageMetrics {
            provider_id: "claude".to_string(),
            metric2_reset: Some(Utc::now() + Duration::days(2)),
            ..Default::default()
        });
        let col = table.columns.get("claude").unwrap();
        assert_ne!(
            col.m2_countdown
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "--:--:--"
        );

        table.update_metrics(&UsageMetrics {
            provider_id: "claude".to_string(),
            ..Default::default()
        });
        let col = table.columns.get("claude").unwrap();
        assert_eq!(
            col.m2_countdown
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "--:--:--"
        );
        assert_eq!(
            col.m1_reset
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "--:--"
        );
    }

    #[test]
    fn test_stale_data_remains_visible_and_labelled() {
        let mut table = UsageTable::new(Theme::dark(), "scale");
        table.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            metric1_val: Some(25.0),
            metric1_text: "25%".to_string(),
            metric2_val: Some(10.0),
            metric2_text: "10%".to_string(),
            error: Some("timeout".to_string()),
            stale: true,
            last_success: Some(Utc::now()),
            ..Default::default()
        });
        let col = table.columns.get("agy").unwrap();
        assert_eq!(
            col.m2_val
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "10 %"
        );
        assert!(col
            .badge
            .borrow()
            .as_any()
            .downcast_ref::<Label>()
            .unwrap()
            .text()
            .contains("STALE"));
    }

    #[test]
    fn test_ahead_of_pace_shows_overage_on_weekly_pill() {
        let mut table = UsageTable::new(Theme::dark(), "scale");
        table.update_metrics(&UsageMetrics {
            provider_id: "codex".to_string(),
            metric2_val: Some(62.0),
            metric2_text: "62%".to_string(),
            metric2_reset: Some(Utc::now() + Duration::days(6)),
            ..Default::default()
        });
        let col = table.columns.get("codex").unwrap();
        assert!(col
            .m2_val
            .borrow()
            .as_any()
            .downcast_ref::<Label>()
            .unwrap()
            .text()
            .contains("▲48"));
    }

    #[test]
    fn test_non_default_window_gets_caption() {
        let mut table = UsageTable::new(Theme::dark(), "scale");
        table.update_metrics(&UsageMetrics {
            provider_id: "codex".to_string(),
            metric1_title: "WINDOW 1D".to_string(),
            metric1_val: Some(5.0),
            metric1_text: "5%".to_string(),
            metric2_title: "WINDOW 30D".to_string(),
            metric2_val: Some(10.0),
            metric2_text: "10%".to_string(),
            ..Default::default()
        });
        let col = table.columns.get("codex").unwrap();
        assert!(col
            .m2_val
            .borrow()
            .as_any()
            .downcast_ref::<Label>()
            .unwrap()
            .text()
            .contains("30D"));
    }
}
