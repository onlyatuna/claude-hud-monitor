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
use qtrs_gui::paint::painter::{create_donut_arc_path, create_pie_path, Painter, Pen};
use qtrs_gui::paint::pixmap::Pixmap;
use qtrs_gui::text::font::{Font, FontWeight};
use qtrs_gui::text::font_metrics::FontMetrics;
use qtrs_gui::tiny_skia::{Color, LineCap, LineJoin, PathBuilder};
use qtrs_widgets::{
    BoxLayout, EmptyWidget, GridLayout, Label, Layout, Widget, WidgetBase, WidgetRef,
};

use super::styles::{duo_colors, scale_colors, Theme};
use crate::pace::{
    elapsed_fraction, format_countdown_dhm, format_countdown_hm, format_reset_time, pace_mark,
    runout_text, window_caption, window_seconds, FIVE_HOURS, ONE_WEEK,
};
use crate::providers::base::UsageMetrics;

pub const PROVIDER_ORDER: [&str; 3] = ["claude", "codex", "agy"];

fn make_widget<W: Widget + 'static>(w: W) -> WidgetRef {
    Rc::new(RefCell::new(Box::new(w)))
}

/// Python `setToolTip`: `"\n".join(s for s in parts if s)`.
fn join_tip(parts: &[&str]) -> String {
    parts.iter().copied().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n")
}

fn set_label_text(w: &WidgetRef, text: impl Into<String>) {
    if let Some(lbl) = w.borrow_mut().as_any_mut().downcast_mut::<Label>() {
        lbl.set_text(text);
    }
}

use crate::ui::set_label_color;

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

        // `QPen(color, 1.6, CustomDashLine)` with `setDashPattern([1.6, 1.4])`: Qt measures the
        // pattern in pen widths and its pen defaults to square caps.
        painter.set_pen(
            Pen::new(color, 1.6)
                .with_cap(LineCap::Square)
                .with_dash_pattern(vec![1.6 * 1.6, 1.4 * 1.6]),
        );
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
    fn widget_base(&self) -> &qtrs_widgets::widget::WidgetBase {
        &self.base
    }

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
        // `UsageDial` is a plain `QWidget` with no size constraint, so the grid row stretches
        // it to fill; a hard minimum here would cap the dial at a fixed square.
        qtrs_gui::geometry::primitives::Size::new(0, 0)
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
        let side = ((w.min(h) - 2.0) as f32).max(10.0);
        let outer_x = (w - side) / 2.0;
        let outer_y = (h - side) / 2.0;
        let center = PointF::new(outer_x + side / 2.0, outer_y + side / 2.0);
        let ring_width = (side * 0.07).max(5.0);

        // 1. Outer ring (Weekly 7D)
        let ring_inset = ring_width / 2.0 + 1.0;
        let ring_rect = RectF::new(
            outer_x + ring_inset,
            outer_y + ring_inset,
            side - ring_inset * 2.0,
            side - ring_inset * 2.0,
        );
        painter.set_brush(Brush::NoBrush);
        painter.set_pen(Pen::new(self.theme.track, ring_width));
        painter.draw_ellipse(ring_rect);

        let (o_pct, o_mark, o_color) = self.outer;
        if let Some(pct) = o_pct {
            if !self.muted && pct > 0.0 {
                painter.set_pen(Pen::new(o_color, ring_width).with_cap(LineCap::Butt));
                painter.draw_arc(ring_rect, 90.0, Self::deg(pct));

                // Hatching when outer pct > mark
                if let Some(mark) = o_mark {
                    if pct > mark {
                        let mark_deg = Self::deg(mark);
                        let span = Self::deg(pct) - mark_deg;
                        if let Some(arc_path) = create_donut_arc_path(ring_rect, ring_width, 90.0 + mark_deg, span) {
                            painter.set_pen(None);
                            painter.set_brush(Brush::Hatched { color: self.theme.hatch });
                            painter.fill_path(&arc_path);
                        }
                    }
                }
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
        let inner_rect = RectF::new(
            outer_x + gap,
            outer_y + gap,
            side - gap * 2.0,
            side - gap * 2.0,
        );
        let inner_side = side - gap * 2.0;

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

                // Hatching when inner pct > mark
                if let Some(mark) = i_mark {
                    if pct > mark {
                        let mark_deg = Self::deg(mark);
                        let span = Self::deg(pct) - mark_deg;
                        if let Some(pie_path) = create_pie_path(inner_rect, 90.0 + mark_deg, span) {
                            painter.set_pen(None);
                            painter.set_brush(Brush::Hatched { color: self.theme.hatch });
                            painter.fill_path(&pie_path);
                        }
                    }
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

        // 3. Centre text with a halo so it reads over the pie and the background alike. Python
        //    builds it as a `QPainterPath` (unhinted outlines at the layout's glyph positions),
        //    strokes the path with the halo and fills it with the text colour.
        let app_font = |px: i32| {
            Font::new(qtrs_widgets::APP_DEFAULT_FAMILY, px as f32).with_weight(FontWeight::Bold)
        };
        let has_pct = self.inner_text.ends_with('%');
        let num = self.inner_text.replace('%', "");
        let num = num.trim();
        let num_px = ((inner_side * if has_pct { 0.30 } else { 0.22 }) as i32).max(8);
        let suf_px = (((num_px as f32) * 0.5) as i32).max(6);
        let num_font = app_font(num_px);
        let suf_font = app_font(suf_px);

        let num_metrics = FontMetrics::from_font(&num_font);
        let num_w = num_metrics.horizontal_advance_exact(num, &num_font);
        let suf_w = if has_pct {
            FontMetrics::from_font(&suf_font).horizontal_advance_exact("%", &suf_font) + 1.0
        } else {
            0.0
        };
        let x = center.x - (num_w + suf_w) / 2.0;
        let cy = center.y - if !self.caption.is_empty() { inner_side * 0.05 } else { 0.0 };
        let cap_height = FontMetrics::cap_height(&num_font).unwrap_or(num_metrics.ascent * 0.7);
        let base_y = cy + cap_height / 2.0;

        let mut builder = PathBuilder::new();
        if let Some(p) = Painter::text_path(PointF::new(x, base_y), num, &num_font) {
            builder.push_path(&p);
        }
        if has_pct {
            if let Some(p) = Painter::text_path(PointF::new(x + num_w + 1.0, base_y), "%", &suf_font) {
                builder.push_path(&p);
            }
        }
        if let Some(path) = builder.finish() {
            if !self.muted {
                painter.set_brush(Brush::NoBrush);
                painter.set_pen(
                    Pen::new(self.theme.halo, 3.0)
                        .with_cap(LineCap::Round)
                        .with_join(LineJoin::Round),
                );
                painter.stroke_path(&path);
            }
            painter.set_pen(None);
            painter.set_brush(Brush::Color(if self.muted {
                // Python fills with `QColor(theme["text3"])`, and `text3` is a CSS `rgba(...)`
                // string, which QColor does not parse: the muted label is opaque black.
                Color::BLACK
            } else {
                self.theme.text
            }));
            painter.fill_path(&path);
        }

        // 4. Caption if provider window differs (e.g. "30D", "5H")
        if !self.caption.is_empty() {
            let cap_px = ((inner_side * 0.13) as i32).max(9);
            let cap_font = app_font(cap_px);
            let cap_w = FontMetrics::from_font(&cap_font).horizontal_advance_exact(&self.caption, &cap_font);
            let origin = PointF::new(center.x - cap_w / 2.0, center.y + inner_side * 0.32);
            if let Some(path) = Painter::text_path(origin, &self.caption, &cap_font) {
                painter.set_brush(Brush::NoBrush);
                painter.set_pen(Pen::new(self.theme.halo, 2.5));
                painter.stroke_path(&path);
                painter.set_pen(None);
                painter.set_brush(Brush::Color(self.theme.text));
                painter.fill_path(&path);
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

/// Provider monochrome vector line icon matching Python `provider_icon`.
pub struct ProviderIconWidget {
    base: WidgetBase,
    provider_id: String,
    color: Color,
    size: f32,
}

impl ProviderIconWidget {
    pub fn new(provider_id: &str, color: Color, size: f32) -> Self {
        let base = WidgetBase::with_geometry(Rect::new(0, 0, size as i32, size as i32));
        base.set_size_policy(qtrs_widgets::QSizePolicy::new(
            qtrs_widgets::Policy::Fixed,
            qtrs_widgets::Policy::Fixed,
        ));
        Self {
            base,
            provider_id: provider_id.to_string(),
            color,
            size,
        }
    }

    pub fn set_color(&mut self, color: Color) {
        self.color = color;
        self.update();
    }
    /// The line glyph in a `size` x `size` logical box (Python `provider_icon`).
    fn draw_glyph(&self, painter: &mut Painter) {
        let s = self.size;
        let c = s / 2.0;
        let pen = Pen::new(self.color, 1.5)
            .with_cap(LineCap::Round)
            .with_join(LineJoin::Round);
        painter.set_pen(pen);
        painter.set_brush(Brush::NoBrush);

        match self.provider_id.as_str() {
            "claude" => {
                // Radiating burst matching Python provider_icon
                for i in 0..8 {
                    let a = (i as f32 * 45.0 + 22.5).to_radians();
                    let r1 = s * 0.14;
                    let r2 = s * if i % 2 == 0 { 0.36 } else { 0.44 };
                    let p1 = PointF::new(c + r1 * a.cos(), c + r1 * a.sin());
                    let p2 = PointF::new(c + r2 * a.cos(), c + r2 * a.sin());
                    painter.draw_line(p1, p2);
                }
            }
            "codex" => {
                // Terminal prompt matching Python provider_icon
                let rect = RectF::new(s * 0.1, s * 0.18, s * 0.8, s * 0.64);
                let r = s * 0.14;
                painter.draw_rounded_rect(rect, r, r);
                // Prompt >
                let p1 = PointF::new(s * 0.28, s * 0.38);
                let p2 = PointF::new(s * 0.42, s * 0.50);
                let p3 = PointF::new(s * 0.28, s * 0.62);
                painter.draw_line(p1, p2);
                painter.draw_line(p2, p3);
                // Underscore _
                let u1 = PointF::new(s * 0.50, s * 0.64);
                let u2 = PointF::new(s * 0.70, s * 0.64);
                painter.draw_line(u1, u2);
            }
            _ => {
                // agy: Arch lifting off matching Python provider_icon
                let mut pb = PathBuilder::new();
                pb.move_to(s * 0.14, s * 0.86);
                pb.cubic_to(
                    s * 0.28, s * 0.10,
                    s * 0.72, s * 0.10,
                    s * 0.86, s * 0.86,
                );
                if let Some(path) = pb.finish() {
                    painter.stroke_path(&path);
                }
                let c1 = PointF::new(s * 0.34, s * 0.62);
                let c2 = PointF::new(s * 0.66, s * 0.62);
                painter.draw_line(c1, c2);
            }
        }
    }

}

impl QObject for ProviderIconWidget {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }
}

impl Widget for ProviderIconWidget {
    fn widget_base(&self) -> &qtrs_widgets::widget::WidgetBase {
        &self.base
    }

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
        qtrs_gui::geometry::primitives::Size::new(self.size as i32, self.size as i32)
    }
    fn minimum_size(&self) -> qtrs_gui::geometry::primitives::Size {
        qtrs_gui::geometry::primitives::Size::new(self.size as i32, self.size as i32)
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
        // Python `provider_icon` draws into a `QPixmap(size * 2)` with a device pixel ratio of 2
        // and `QLabel` paints that pixmap at its logical size, so the glyph is rasterised at 2x
        // and then scaled to the screen's pixel ratio.
        let s = self.size;
        let px = (s * 2.0) as u32;
        let Some(mut pixmap) = Pixmap::with_dpr(px, px, 2.0) else {
            return;
        };
        {
            let mut inner = Painter::begin(&mut pixmap);
            self.draw_glyph(&mut inner);
        }
        painter.draw_pixmap(RectF::new(0.0, 0.0, s, s), &pixmap, None);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Tiny legend swatch glyph matching Python `legend_glyph`.
pub struct GlyphWidget {
    base: WidgetBase,
    kind: String,
    color: Color,
    theme: Theme,
    size: f32,
}

impl GlyphWidget {
    pub fn new(kind: &str, color: Color, theme: Theme, size: f32) -> Self {
        let base = WidgetBase::with_geometry(Rect::new(0, 0, size as i32, size as i32));
        base.set_size_policy(qtrs_widgets::QSizePolicy::new(
            qtrs_widgets::Policy::Fixed,
            qtrs_widgets::Policy::Fixed,
        ));
        Self {
            base,
            kind: kind.to_string(),
            color,
            theme,
            size,
        }
    }

    pub fn set_theme(&mut self, theme: Theme, color: Color) {
        self.theme = theme;
        self.color = color;
        self.update();
    }
}

impl QObject for GlyphWidget {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }
}

impl Widget for GlyphWidget {
    fn widget_base(&self) -> &qtrs_widgets::widget::WidgetBase {
        &self.base
    }

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
        qtrs_gui::geometry::primitives::Size::new(self.size as i32, self.size as i32)
    }
    fn minimum_size(&self) -> qtrs_gui::geometry::primitives::Size {
        qtrs_gui::geometry::primitives::Size::new(self.size as i32, self.size as i32)
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
        let size = self.size;
        let r = RectF::new(1.5, 1.5, (size - 3.0).max(1.0), (size - 3.0).max(1.0));

        match self.kind.as_str() {
            "pie" => {
                painter.set_pen(None);
                painter.set_brush(Brush::Color(self.theme.disc));
                painter.draw_ellipse(r);
                painter.set_brush(Brush::Color(self.color));
                painter.draw_pie(r, 90.0, -250.0);
            }
            "ring" => {
                let inset = RectF::new(r.x + 1.0, r.y + 1.0, (r.width - 2.0).max(1.0), (r.height - 2.0).max(1.0));
                painter.set_brush(Brush::NoBrush);
                painter.set_pen(Pen::new(self.theme.track, 2.2));
                painter.draw_ellipse(inset);
                painter.set_pen(Pen::new(self.color, 2.2));
                painter.draw_arc(inset, 90.0, -250.0);
            }
            "tick" => {
                let pen = Pen::new(self.theme.text, 1.4).with_dash_pattern(vec![1.5, 1.3]);
                painter.set_pen(pen);
                painter.set_brush(Brush::NoBrush);
                painter.draw_line(
                    PointF::new(size / 2.0, 1.0),
                    PointF::new(size / 2.0, size - 1.0),
                );
            }
            "hatch" => {
                painter.set_pen(None);
                painter.set_brush(Brush::Color(self.color));
                painter.draw_rounded_rect(r, 2.0, 2.0);
                painter.set_brush(Brush::Hatched { color: self.theme.hatch });
                painter.draw_rounded_rect(r, 2.0, 2.0);
            }
            _ => {}
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// 1px horizontal separator line matching Python `QFrame#Separator`.
pub struct SeparatorWidget {
    base: WidgetBase,
    color: Color,
}

impl SeparatorWidget {
    pub fn new(color: Color) -> Self {
        let base = WidgetBase::with_geometry(Rect::new(0, 0, 10, 1));
        base.set_size_policy(qtrs_widgets::QSizePolicy::new(
            qtrs_widgets::Policy::Expanding,
            qtrs_widgets::Policy::Fixed,
        ));
        Self { base, color }
    }

    pub fn set_color(&mut self, color: Color) {
        self.color = color;
        self.update();
    }
}

impl QObject for SeparatorWidget {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }
}

impl Widget for SeparatorWidget {
    fn widget_base(&self) -> &qtrs_widgets::widget::WidgetBase {
        &self.base
    }

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
        qtrs_gui::geometry::primitives::Size::new(10, 1)
    }
    fn minimum_size(&self) -> qtrs_gui::geometry::primitives::Size {
        qtrs_gui::geometry::primitives::Size::new(10, 1)
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
        painter.set_pen(None);
        painter.set_brush(Brush::Color(self.color));
        painter.draw_rect(RectF::new(0.0, 0.0, w, 1.0));
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Python `_glyph_row`: a legend glyph and its `QLabel` (styled through `obj`) in a 5px row.
fn make_glyph_row(
    kind: &str,
    color: Color,
    text: &str,
    theme: &Theme,
    size: f32,
    obj: &str,
) -> (WidgetRef, WidgetRef, WidgetRef) {
    let row = make_widget(EmptyWidget::new());
    let mut h = BoxLayout::horizontal();
    h.set_margins(Margins::new(0, 0, 0, 0));
    h.set_spacing(5);

    let glyph = make_widget(GlyphWidget::new(kind, color, theme.clone(), size));
    h.add_widget(glyph.clone());

    let mut lbl = Label::new(text);
    lbl.set_object_name(obj);
    let lbl_ref = make_widget(lbl);
    h.add_widget(lbl_ref.clone());
    h.add_stretch(1);

    row.borrow_mut().set_layout(Box::new(h));
    (row, glyph, lbl_ref)
}

/// Python `sub_label`: a `QLabel#RowLabel`, whose `padding-left: 18px` comes from the sheet.
fn make_sub_label(text: &str) -> WidgetRef {
    let mut lbl = Label::new(text);
    lbl.set_object_name("RowLabel");
    make_widget(lbl)
}

/// Python `_set_state`: the `state` property that `QLabel[state="muted"]` selects on.
fn set_state(w: &WidgetRef, state: &str) {
    w.borrow().set_property("state", state);
}

/// Python `_cell`: a centred `QLabel` styled through its object name.
fn make_cell_label(text: &str, obj: &str) -> WidgetRef {
    let mut l = Label::new(text);
    l.set_object_name(obj);
    l.set_alignment(qtrs_widgets::Alignment::Center);
    make_widget(l)
}

#[allow(dead_code)]
pub struct ProviderColumn {
    pub provider_id: String,
    pub theme: Theme,
    pub scheme: String,
    pub current_metrics: UsageMetrics,

    pub header: WidgetRef,
    pub icon: WidgetRef,
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

        let icon = make_widget(ProviderIconWidget::new(provider_id, theme.text, 18.0));
        top.add_widget(icon.clone());

        let display_name = match provider_id {
            "claude" => "Claude Code",
            "codex" => "Codex",
            "agy" => "Antigravity",
            _ => provider_id,
        };
        let mut name_lbl = Label::new(display_name);
        name_lbl.set_object_name("HeaderName");
        let name = make_widget(name_lbl);
        top.add_widget(name.clone());
        top.add_stretch(1);

        let top_widget = make_widget(EmptyWidget::new());
        top_widget.borrow_mut().set_layout(Box::new(top));
        hv.add_widget(top_widget);

        let badge = make_cell_label(" ", "HeaderBadge");
        hv.add_widget(badge.clone());
        header.borrow_mut().set_layout(Box::new(hv));

        let make_value_cell = |txt: &str, obj: &str| {
            let cell = make_cell_label(txt, obj);
            if let Some(l) = cell.borrow_mut().as_any_mut().downcast_mut::<Label>() {
                // Countdowns must not shift horizontally as digits change.
                l.set_font(l.font().clone().with_tabular_numbers(true));
            }
            cell
        };

        let m1_reset = make_value_cell("--:--", "Cell");
        let m1_countdown = make_value_cell("--:--", "Cell");
        let dial = make_widget(UsageDial::new(theme.clone()));
        let m2_val = make_value_cell("--", "Pill");
        let m2_reset = make_value_cell("--:--", "Cell");
        let m2_countdown = make_value_cell("--:--:--", "Cell");

        Self {
            provider_id: provider_id.to_string(),
            theme,
            scheme: scheme.to_string(),
            current_metrics: UsageMetrics {
                provider_id: provider_id.to_string(),
                ..Default::default()
            },
            header,
            icon,
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
        if let Some(ic) = self.icon.borrow_mut().as_any_mut().downcast_mut::<ProviderIconWidget>() {
            ic.set_color(theme.text);
        }
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

        // `for w in self.value_cells + [self.header]: w.setToolTip(data.error or "")`
        let error_tip = data.error.as_deref().unwrap_or("");
        for w in [
            &self.m1_reset,
            &self.m1_countdown,
            &self.m2_val,
            &self.m2_reset,
            &self.m2_countdown,
            &self.header,
        ] {
            w.borrow().set_tool_tip(error_tip);
        }

        // Python `_set_state`: offline columns dim their value cells and name.
        let state = if is_offline { "muted" } else { "" };
        for w in [
            &self.m1_reset,
            &self.m1_countdown,
            &self.m2_val,
            &self.m2_reset,
            &self.m2_countdown,
            &self.name,
        ] {
            set_state(w, state);
        }

        if is_offline {
            if let Some(ic) = self.icon.borrow_mut().as_any_mut().downcast_mut::<ProviderIconWidget>() {
                ic.set_color(self.theme.text3);
            }
            set_label_text(&self.badge, "OFFLINE");
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
            self.dial.borrow().set_tool_tip(error_tip);
            set_label_text(&self.m2_val, "--");
            // Python `self.m2_val.setStyleSheet("")`: back to the sheet's colour.
            self.m2_val.borrow().set_style_sheet("");
            set_label_text(&self.m1_reset, "--:--");
            set_label_text(&self.m1_countdown, "--:--");
            set_label_text(&self.m2_reset, "--:--");
            set_label_text(&self.m2_countdown, "--:--:--");
            return;
        }

        if let Some(ic) = self.icon.borrow_mut().as_any_mut().downcast_mut::<ProviderIconWidget>() {
            ic.set_color(self.theme.text);
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
            let stamp = match data.last_success {
                Some(t) => t.with_timezone(&chrono::Local).format("%m/%d %H:%M:%S").to_string(),
                None => "--".to_string(),
            };
            self.header
                .borrow()
                .set_tool_tip(&join_tip(&[&format!("舊資料 {stamp}"), error_tip]));
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

        let tip1 = runout_text(data.metric1_val, e1, w1);
        let tip2 = runout_text(data.metric2_val, e2, w2);
        let stale_tip = match (&data.error, data.stale) {
            (Some(e), true) => e.as_str(),
            _ => "",
        };
        self.m2_val.borrow().set_tool_tip(&join_tip(&[&tip2, stale_tip]));
        let dial1 = if tip1.is_empty() { String::new() } else { format!("5 小時：{tip1}") };
        let dial2 = if tip2.is_empty() { String::new() } else { format!("1 週：{tip2}") };
        self.dial.borrow().set_tool_tip(&join_tip(&[&dial1, &dial2, stale_tip]));
    }
}

pub struct UsageTable {
    pub container: WidgetRef,
    pub columns: HashMap<String, ProviderColumn>,
    pub theme: Theme,
    pub scheme: String,
    pub sep1: WidgetRef,
    pub sep2: WidgetRef,
    pub glyphs: Vec<WidgetRef>,
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

        let (inner_c, outer_c, hatch_c) = if scheme == "duo" {
            (theme.duo_inner, theme.duo_outer, theme.duo_inner)
        } else {
            (theme.neutral, theme.neutral, theme.scale_yellow)
        };

        let mut glyphs = Vec::new();

        // Row 1: Separator under header spanning all 4 columns
        let sep1 = make_widget(SeparatorWidget::new(theme.separator));
        grid.add_widget_with_span(sep1.clone(), 1, 0, 1, 4);

        // Row 2: Section title "5 小時" with 13px pie glyph
        let (row2_w, row2_g, _) = make_glyph_row("pie", inner_c, "5 小時", &theme, 13.0, "SectionTitle");
        glyphs.push(row2_g);
        grid.add_widget(row2_w, 2, 0);

        // Rows 3, 4: "重設" / "剩餘" (`QLabel#RowLabel`, padding-left: 18px)
        grid.add_widget(make_sub_label("重設"), 3, 0);
        grid.add_widget(make_sub_label("剩餘"), 4, 0);

        // Row 5: Legend (left)
        let legend = make_widget(EmptyWidget::new());
        let mut lv = BoxLayout::vertical();
        lv.set_margins(Margins::new(0, 0, 0, 0));
        lv.set_spacing(3);
        lv.add_stretch(1);

        for (kind, color, text) in [
            ("pie", inner_c, "內圈 5 小時"),
            ("ring", outer_c, "外環 1 週"),
            ("tick", theme.text, "平均進度"),
            ("hatch", hatch_c, "超出平均"),
        ] {
            let (row_w, row_g, _) = make_glyph_row(kind, color, text, &theme, 11.0, "Legend");
            glyphs.push(row_g);
            lv.add_widget(row_w);
        }
        lv.add_stretch(1);

        legend.borrow_mut().set_layout(Box::new(lv));
        grid.add_widget(legend, 5, 0);

        // Row 6: Separator under dials spanning all 4 columns
        let sep2 = make_widget(SeparatorWidget::new(theme.separator));
        grid.add_widget_with_span(sep2.clone(), 6, 0, 1, 4);

        // Row 7: Section title "1 週" with 13px ring glyph
        let (row7_w, row7_g, _) = make_glyph_row("ring", outer_c, "1 週", &theme, 13.0, "SectionTitle");
        glyphs.push(row7_g);
        grid.add_widget(row7_w, 7, 0);

        // Rows 8, 9: "重設" / "剩餘"
        grid.add_widget(make_sub_label("重設"), 8, 0);
        grid.add_widget(make_sub_label("剩餘"), 9, 0);

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

        grid.set_row_minimum_height(2, 18);
        grid.set_row_stretch(5, 1);

        container.borrow_mut().set_layout(Box::new(grid));
        container.borrow_mut().set_style_sheet(crate::ui::styles::get_table_stylesheet(
            theme.is_dark,
        ));
        qtrs_widgets::widget::adopt_tree(&container);

        Self {
            container,
            columns,
            theme,
            scheme: scheme.to_string(),
            sep1,
            sep2,
            glyphs,
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
        self.container
            .borrow_mut()
            .set_style_sheet(crate::ui::styles::get_table_stylesheet(theme.is_dark));

        if let Some(s) = self.sep1.borrow_mut().as_any_mut().downcast_mut::<SeparatorWidget>() {
            s.set_color(theme.separator);
        }
        if let Some(s) = self.sep2.borrow_mut().as_any_mut().downcast_mut::<SeparatorWidget>() {
            s.set_color(theme.separator);
        }

        let (inner_c, outer_c, hatch_c) = if scheme == "duo" {
            (theme.duo_inner, theme.duo_outer, theme.duo_inner)
        } else {
            (theme.neutral, theme.neutral, theme.scale_yellow)
        };

        let colors = [inner_c, inner_c, outer_c, theme.text, hatch_c, outer_c];
        for (i, g) in self.glyphs.iter().enumerate() {
            if let Some(gw) = g.borrow_mut().as_any_mut().downcast_mut::<GlyphWidget>() {
                let c = colors.get(i).copied().unwrap_or(theme.text);
                gw.set_theme(theme.clone(), c);
            }
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

    #[test]
    fn test_table_layout_contains_icons_separators_and_glyphs() {
        let table = UsageTable::new(Theme::dark(), "scale");
        assert_eq!(table.columns.len(), 3);
        assert!(table.sep1.borrow().as_any().downcast_ref::<SeparatorWidget>().is_some());
        assert!(table.sep2.borrow().as_any().downcast_ref::<SeparatorWidget>().is_some());
        assert_eq!(table.glyphs.len(), 6); // row 2, leg1, leg2, leg3, leg4, row 7
        for pid in PROVIDER_ORDER {
            let col = table.columns.get(pid).unwrap();
            assert!(col.icon.borrow().as_any().downcast_ref::<ProviderIconWidget>().is_some());
        }
    }

    /// A table built under the application style sheet the table-mode HUD installs. Labels take
    /// their sizes from it and from the table's own sheet, so a sheet another test left in the
    /// process-wide slot would change every measurement.
    fn table_mode_table() -> UsageTable {
        qtrs_widgets::application::Application::set_style_sheet(crate::ui::styles::get_hud_stylesheet(true));
        UsageTable::new(Theme::dark(), "scale")
    }

    /// Grid geometry must match the PySide6 `QGridLayout` for a 450x350 table-mode window.
    ///
    /// Reference measured with PySide6 6.11.2 and `QT_QPA_PLATFORM=windows` on the real
    /// `HUDWindow` (the offscreen platform has no font database and reports fabricated
    /// metrics): column x/width `68 | 78/109 | 197/110 | 317/109`, row y/height
    /// `31 | 1 | 18 | 18 | 18 | 134 | 1 | 19 | 18 | 18`.
    // Expected values are PySide6 measurements taken with the Windows fonts (Microsoft JhengHei UI,
    // Segoe UI, Consolas); Linux and macOS have neither those fonts nor reference numbers.
    #[cfg(windows)]
    #[test]
    fn test_grid_matches_qt_geometry() {
        let table = table_mode_table();
        let container = table.widget();
        container
            .borrow_mut()
            .set_geometry(Rect::new(12, 28, 426, 312));
        container.borrow().update_layout();

        // Column 0 is the legend gutter: its width is the widest legend row's size hint,
        // 11px glyph + 5px spacing + 52px "內圈 5 小時" advance, with no trailing stretch spacing.
        let claude = table.columns.get("claude").unwrap();
        let codex = table.columns.get("codex").unwrap();
        let agy = table.columns.get("agy").unwrap();
        let geometry = |col: &ProviderColumn| {
            let g = col.header.borrow().geometry();
            (g.x, g.width)
        };
        assert_eq!(geometry(claude), (78, 109));
        assert_eq!(geometry(codex), (197, 110));
        assert_eq!(geometry(agy), (317, 109));

        // Row heights come from QFontMetrics, so the stretched dial row absorbs the slack.
        assert_eq!(claude.header.borrow().geometry().height, 31);
        assert_eq!(claude.dial.borrow().geometry().y, 106);
        assert_eq!(claude.dial.borrow().geometry().height, 134);
    }

    /// Every HUD label renders in the `QApplication` default family, as the Python table's
    /// style sheet sets no `font-family`. Picking Segoe UI for ASCII text shifts the layout:
    /// its 14px line box is 19px tall against JhengHei UI's 18px.
    // Expected values are PySide6 measurements taken with the Windows fonts (Microsoft JhengHei UI,
    // Segoe UI, Consolas); Linux and macOS have neither those fonts nor reference numbers.
    #[cfg(windows)]
    #[test]
    fn test_labels_use_the_app_default_family() {
        let table = table_mode_table();
        let col = table.columns.get("claude").unwrap();
        let reset = col.m1_reset.borrow();
        let label = reset.as_any().downcast_ref::<Label>().unwrap();
        assert_eq!(label.font().family, qtrs_widgets::APP_DEFAULT_FAMILY);
        // `QLabel#Cell { font-size: 14px; padding: 0px 2px; }` -> an 18px line box.
        assert_eq!(reset.size_hint().height, 18);
    }

    /// A trailing `addStretch()` is a zero-width spacer that takes no spacing either:
    /// `QLayout::sizeHint` sums the non-stretch items and their intervening spacing only.
    #[test]
    fn test_trailing_stretch_adds_no_spacing() {
        let theme = Theme::dark();
        let (row, glyph, label) =
            make_glyph_row("pie", theme.neutral, "內圈 5 小時", &theme, 11.0, "Legend");
        let mut expected = glyph.borrow().size_hint().width + label.borrow().size_hint().width;
        expected += 5; // one spacing, between the two widgets
        assert_eq!(row.borrow().size_hint().width, expected);
    }

    /// Header row: the icon and the name are packed with `addStretch()` spacers on both sides,
    /// which take no spacing, so the 5px spacing falls between the icon and the name only
    /// (18px icon + 5px + 86px "Claude Code" is the 109px hint) and the spare pixel of a 110px
    /// column goes to the left spacer. Measured with PySide6 6.11.2: icon x 1, name x 24, width 86.
    /// Qt's `sizeHint` is 86 whichever way the string is laid out (85.765625 px at 125%, a whole
    /// 86 at 100%), because the width is rounded up.
    // Expected values are PySide6 measurements taken with the Windows fonts (Microsoft JhengHei UI,
    // Segoe UI, Consolas); Linux and macOS have neither those fonts nor reference numbers.
    #[cfg(windows)]
    #[test]
    fn test_header_row_advances_match_qt() {
        let table = table_mode_table();
        let col = table.columns.get("claude").unwrap();
        assert_eq!(col.name.borrow().size_hint().width, 86);
        col.header
            .borrow_mut()
            .set_geometry(Rect::new(0, 0, 110, 31));
        col.header.borrow().update_layout();
        let icon = col.icon.borrow().geometry();
        let name = col.name.borrow().geometry();
        assert_eq!((icon.x, name.x, name.width), (1, 24, 86));
    }

    /// Restores the application device pixel ratio the other tests run under.
    struct DevicePixelRatioGuard;

    impl Drop for DevicePixelRatioGuard {
        fn drop(&mut self) {
            qtrs_gui::text::font_database::set_application_device_pixel_ratio(1.0);
        }
    }

    /// Each column is at least as wide as its widest cell, so a long weekly pill widens its
    /// column at the expense of the others, as in `QGridLayout`: the columns take their minimum
    /// sizes when the equal stretch shares fall below them.
    ///
    /// Reference measured with PySide6 6.11.2 at a device pixel ratio of 1.25 (DirectWrite
    /// advances) with the same cell texts in a 426x312 table: header x/width
    /// `77/109 | 196/121 | 327/99`, pill size hints `71x20 | 121x20 | 89x20`.
    // Expected values are PySide6 measurements taken with the Windows fonts (Microsoft JhengHei UI,
    // Segoe UI, Consolas); Linux and macOS have neither those fonts nor reference numbers.
    #[cfg(windows)]
    #[test]
    fn test_columns_follow_widest_cell_hint() {
        let _guard = DevicePixelRatioGuard;
        qtrs_gui::text::font_database::set_application_device_pixel_ratio(1.25);
        let table = table_mode_table();
        let cells = [("claude", "95 %  ▲1"), ("codex", "-- · SECONDARY"), ("agy", "100 %  ▲24")];
        for (pid, pill) in cells {
            let c = table.columns.get(pid).unwrap();
            set_label_text(&c.m2_val, pill);
            set_label_text(&c.m2_reset, "週二 02:59");
        }
        let container = table.widget();
        container.borrow_mut().set_geometry(Rect::new(0, 0, 426, 312));
        qtrs_widgets::LayoutScheduler::invalidate(&container);
        qtrs_widgets::LayoutScheduler::activate_pending();

        let measured = cells.map(|(pid, _)| {
            let c = table.columns.get(pid).unwrap();
            let header = c.header.borrow().geometry();
            (header.x, header.width, c.m2_val.borrow().size_hint())
        });
        assert_eq!(
            measured.map(|(x, w, _)| (x, w)),
            [(77, 109), (196, 121), (327, 99)]
        );
        assert_eq!(
            measured.map(|(_, _, hint)| (hint.width, hint.height)),
            [(71, 20), (121, 20), (89, 20)]
        );
    }
}
