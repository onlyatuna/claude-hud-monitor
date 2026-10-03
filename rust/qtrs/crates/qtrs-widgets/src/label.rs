use crate::layout::Layout;
use crate::widget::{Widget, WidgetBase, WidgetRef, WidgetWeak};
use qtrs_core::event::Event;
use qtrs_core::object::{ObjectData, ObjectId, QObject};
use qtrs_gui::geometry::primitives::{PointF, Rect, Size};
use qtrs_gui::paint::{Painter, Pen};
use qtrs_gui::text::{Font, FontMetrics};
use qtrs_gui::tiny_skia::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

pub struct Label {
    pub base: WidgetBase,
    text: String,
    font: Font,
    color: Color,
    background_color: Option<Color>,
    alignment: Alignment,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        let text_str = text.into();
        let family = if text_str.chars().any(|c| (c as u32) >= 0x2E80) {
            "Microsoft JhengHei"
        } else {
            "Segoe UI"
        };
        let font = Font::new(family, 13.0);
        let base = WidgetBase::new();
        let metrics = FontMetrics::from_font(&font);
        let text_w = metrics.horizontal_advance(&text_str, &font).ceil() as i32 + 10;
        let text_h = metrics.height.ceil() as i32 + 6;
        base.set_geometry(Rect::new(0, 0, text_w.max(60), text_h.max(24)));
        Self {
            base,
            text: text_str,
            font,
            color: Color::BLACK,
            background_color: None,
            alignment: Alignment::Left,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        if self.text.chars().any(|c| (c as u32) >= 0x2E80) && self.font.family == "Segoe UI" {
            self.font.family = "Microsoft JhengHei".to_string();
        }
        self.request_layout();
        self.update();
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn set_color(&mut self, color: Color) {
        self.color = color;
        self.update();
    }

    pub fn font(&self) -> &Font {
        &self.font
    }

    pub fn set_font(&mut self, font: Font) {
        self.font = font;
        self.update();
    }

    pub fn set_background_color(&mut self, color: Option<Color>) {
        self.background_color = color;
        self.update();
    }

    pub fn alignment(&self) -> Alignment {
        self.alignment
    }

    pub fn set_alignment(&mut self, alignment: Alignment) {
        self.alignment = alignment;
        self.update();
    }
    /// Left, right, top and bottom space `QLabel` keeps around its text under a style sheet:
    /// border plus padding, and, once the label has a box, the advance of `'x'` as indent
    /// (`QLabelPrivate::sizeForWidth`; measured against PySide6: a 9px Consolas badge with
    /// `padding: 1px 4px; border: 1px` is the text plus 15 px wide, 4 px taller than the font).
    /// A label without padding or border has none of it.
    fn box_insets(
        style: &crate::style::stylesheet::ResolvedStyle,
        metrics: &FontMetrics,
        font: &qtrs_gui::text::Font,
    ) -> (f32, f32, f32, f32) {
        let border = style.border_width.unwrap_or(0.0).max(0.0);
        let [pt, pr, pb, pl] = style.padding.unwrap_or([0.0; 4]);
        if border == 0.0 && pt + pr + pb + pl == 0.0 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let indent = metrics.horizontal_advance_exact("x", font).round();
        let left_indent = (indent / 2.0).floor();
        (
            border + pl + left_indent,
            border + pr + (indent - left_indent),
            border + pt,
            border + pb,
        )
    }

    /// The label's font with the stylesheet's size, family, weight and letter spacing applied.
    fn styled_font(&self, style: &crate::style::stylesheet::ResolvedStyle) -> qtrs_gui::text::Font {
        let mut font = self.font.clone();
        if let Some(sz) = style.font_size {
            // `QCss` applies `font-size: Npx` with `QFont::setPixelSize(int)`, so 10.5px is 11px.
            font.size = sz.round();
        }
        if let Some(fam) = &style.font_family {
            font.family = fam.clone();
        }
        if let Some(w) = style.font_weight {
            font.weight = match w {
                800..=900 => qtrs_gui::text::FontWeight::Black,
                700..=799 => qtrs_gui::text::FontWeight::Bold,
                600..=699 => qtrs_gui::text::FontWeight::SemiBold,
                _ => qtrs_gui::text::FontWeight::Normal,
            };
        }
        if let Some(spacing) = style.letter_spacing {
            font.letter_spacing = spacing;
        }
        font
    }

    pub fn resolved_style(&self) -> crate::style::stylesheet::ResolvedStyle {
        let props = self.base.properties.borrow();
        let attrs: Vec<(&str, &str)> = props
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let ctx = crate::style::stylesheet::WidgetStyleContext {
            type_name: "QLabel",
            object_name: self.base.object_data.object_name.as_deref().unwrap_or(""),
            pseudo_states: &[],
            sub_control: None,
            attributes: &attrs,
        };
        let sheet = self.base.style_sheet.borrow();
        crate::style::stylesheet::QStyleSheetStyle::resolve_cascaded(
            sheet.as_ref(),
            crate::application::Application::style_sheet().as_deref(),
            &ctx,
        )
    }
}

impl QObject for Label {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }

    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }

    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn event(&mut self, _event: &mut Event) -> bool {
        false
    }
}

impl Widget for Label {
    fn id(&self) -> ObjectId {
        self.base.object_data.id
    }

    fn geometry(&self) -> Rect {
        self.base.geometry()
    }

    fn set_geometry(&self, rect: Rect) {
        self.base.set_geometry(rect);
    }

    fn size_hint(&self) -> Size {
        let style = self.resolved_style();
        let font = self.styled_font(&style);
        let metrics = FontMetrics::from_font(&font);
        let (box_l, box_r, box_t, box_b) = Self::box_insets(&style, &metrics, &font);
        let text_w = metrics.horizontal_advance_exact(&self.text, &font).ceil() as i32
            + (box_l + box_r).round() as i32;
        let text_h = metrics.height.ceil() as i32 + (box_t + box_b).round() as i32;
        let w = style.min_width.unwrap_or(text_w);
        let h = style.max_height.or(style.min_height).unwrap_or(text_h);
        Size::new(w, h)
    }

    fn minimum_size(&self) -> Size {
        let style = self.resolved_style();
        let w = style.min_width.unwrap_or(0);
        let h = style.min_height.unwrap_or(0);
        Size::new(w, h)
    }

    fn maximum_size(&self) -> Size {
        let style = self.resolved_style();
        let w = style.max_width.unwrap_or(16777215);
        let h = style.max_height.unwrap_or(16777215);
        Size::new(w, h)
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
        Vec::new()
    }

    fn add_child(&mut self, _child: WidgetRef) {}

    fn remove_child(&mut self, _child_id: ObjectId) {}
    fn dpi_changed_event(&mut self, _old_dpr: f32, _new_dpr: f32) {
        self.base.clear_dirty();
        self.base.update();
    }

    fn paint_event(&mut self, painter: &mut Painter) {
        let geom = self.base.geometry();
        let style = self.resolved_style();

        let bg = style.background_color.or(self.background_color);
        let border_r = style.border_radius.unwrap_or(0.0);
        let border_pen = if let Some(bw) = style.border_width {
            if bw > 0.0 && style.border_style.as_deref() != Some("none") {
                Some(Pen::new(style.border_color.unwrap_or(Color::TRANSPARENT), bw))
            } else {
                None
            }
        } else {
            None
        };

        if bg.is_some() || border_pen.is_some() {
            if let Some(c) = bg {
                painter.set_brush(qtrs_gui::paint::Brush::Color(c));
            } else {
                painter.set_brush(qtrs_gui::paint::Brush::Color(Color::TRANSPARENT));
            }
            painter.set_pen(border_pen);
            let h = if let Some(mh) = style.max_height {
                mh as f32
            } else {
                geom.height as f32
            };
            let y = (geom.height as f32 - h) / 2.0;
            let rect_f = qtrs_gui::geometry::primitives::RectF::new(
                0.0,
                y,
                geom.width as f32,
                h,
            );
            if border_r > 0.0 {
                painter.draw_rounded_rect(rect_f, border_r, border_r);
            } else {
                painter.draw_rect(rect_f);
            }
        }

        if self.text.is_empty() {
            return;
        }

        let font = self.styled_font(&style);

        let metrics = FontMetrics::from_font(&font);
        let text_color = style.color.unwrap_or(self.color);
        painter.set_pen(Pen::new(text_color, 1.0));

        let (box_l, box_r, box_t, box_b) = Self::box_insets(&style, &metrics, &font);
        let content_w = (geom.width as f32 - box_l - box_r).max(0.0);
        let content_h = (geom.height as f32 - box_t - box_b).max(0.0);
        let x_for = |text_w: f32| match self.alignment {
            Alignment::Left => box_l,
            Alignment::Center => box_l + ((content_w - text_w) / 2.0).max(0.0),
            Alignment::Right => (geom.width as f32 - box_r - text_w).max(box_l),
        };

        if self.text.contains('\n') {
            let lines: Vec<&str> = self.text.split('\n').collect();
            let line_height = metrics.height;
            let total_text_h = line_height * lines.len() as f32;
            let mut start_y = box_t + ((content_h - total_text_h) / 2.0).max(0.0) + metrics.ascent;

            for line in lines {
                let line_w = metrics.horizontal_advance_exact(line, &font);
                painter.draw_text(PointF::new(x_for(line_w), start_y), line, &font);
                start_y += line_height;
            }
        } else {
            let text_w = metrics.horizontal_advance_exact(&self.text, &font);
            let text_h = metrics.height;
            let baseline_y = box_t + ((content_h - text_h) / 2.0).max(0.0) + metrics.ascent;
            painter.draw_text(PointF::new(x_for(text_w), baseline_y), &self.text, &font);
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_style_sheet(&self, qss: &str) {
        self.base.set_style_sheet(qss);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
