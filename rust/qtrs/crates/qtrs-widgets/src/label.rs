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




/// `QApplication`'s default font family on Windows (`Microsoft JhengHei UI`).
///
/// A `QLabel` whose style sheet does not set `font-family` keeps this family, so it is the
/// face every HUD label renders with unless a rule names one explicitly.
pub const APP_DEFAULT_FAMILY: &str = "Microsoft JhengHei UI";

/// Family the `QWidget`-scoped style-sheet rule resolves to: the first installed family in the
/// `'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif` stack.
pub const SHEET_FAMILY: &str = "Segoe UI";

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        let text_str = text.into();
        // `QLabel` uses the `QApplication` default font unless a style-sheet rule names a
        // family. That default is Microsoft JhengHei UI on Windows, and the table style sheet
        // sets no `font-family`, so both its ASCII and CJK labels render with it.
        let font = Font::new(APP_DEFAULT_FAMILY, 13.0);
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
        self.update_geometry();
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
        // FontChange: update(); updateGeometry() (qwidget.cpp:9502).
        self.update_geometry();
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
        // `QLabel::setAlignment` -> updateLabel(): the text indent follows the alignment.
        self.update_geometry();
        self.update();
    }
    /// Left, right, top and bottom space `QLabel` keeps around its text under a style sheet:
    /// the border plus the padding of each edge (`QStyleSheetStyle::subElementRect`).
    fn box_insets(style: &crate::style::stylesheet::ResolvedStyle) -> (f32, f32, f32, f32) {
        let border = style.border_width.unwrap_or(0.0).max(0.0);
        let [pt, pr, pb, pl] = style.padding.unwrap_or([0.0; 4]);
        (border + pl, border + pr, border + pt, border + pb)
    }

    /// `QLabel`'s text indent: the advance of `'x'`, which `QLabelPrivate::sizeForWidth` adds to
    /// the width of a left- or right-aligned label that has a frame (`frameWidth() != 0`, which a
    /// style sheet makes the widest edge of border plus padding). A centred label has none, and
    /// neither has a label without a box. (Measured against PySide6: `padding-left: 5px` adds
    /// 5 + 6 to a left-aligned 12px label and 5 to a centred one; `padding: 0px 2px` adds 4 to
    /// the centred table cells.) `QLabelPrivate::documentRect` insets the text by half of it on
    /// the aligned side.
    fn indent(
        style: &crate::style::stylesheet::ResolvedStyle,
        metrics: &FontMetrics,
        font: &qtrs_gui::text::Font,
        alignment: Alignment,
    ) -> f32 {
        if alignment == Alignment::Center {
            return 0.0;
        }
        let border = style.border_width.unwrap_or(0.0).max(0.0);
        let padding = style.padding.unwrap_or([0.0; 4]);
        let frame = padding.iter().map(|p| border + p).fold(0.0, f32::max);
        if frame <= 0.0 {
            return 0.0;
        }
        metrics.horizontal_advance_exact("x", font).round()
    }

    /// The label's font with the stylesheet's size, family, weight and letter spacing applied.
    fn styled_font(&self, style: &crate::style::stylesheet::ResolvedStyle) -> qtrs_gui::text::Font {
        let mut font = self.font.clone();
        if let Some(sz) = style.font_size {
            // `QCss` applies `font-size: Npx` with `QFont::setPixelSize(int)`, so 10.5px is 11px.
            font.size = sz.round();
        }
        if let Some(fam) = style.font_family() {
            font.family = fam;
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
        let mut buf = [""; 4];
        let ctx = crate::style::stylesheet::WidgetStyleContext {
            type_name: "QLabel",
            object_name: self.base.object_data.object_name.as_deref().unwrap_or(""),
            pseudo_states: self.base.style_pseudo_states(false, false, &mut buf),
            sub_control: None,
            attributes: &attrs,
        };
        self.base.resolve_style(&ctx)
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
    fn widget_base(&self) -> &crate::widget::WidgetBase {
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

    fn size_hint(&self) -> Size {
        let style = self.resolved_style();
        let font = self.styled_font(&style);
        let metrics = FontMetrics::from_font(&font);
        let (box_l, box_r, box_t, box_b) = Self::box_insets(&style);
        let indent = Self::indent(&style, &metrics, &font, self.alignment);
        let text_w = metrics.horizontal_advance_exact(&self.text, &font).ceil() as i32
            + (box_l + box_r + indent).round() as i32;
        let text_h = FontMetrics::layout_height(&font).ceil() as i32 + (box_t + box_b).round() as i32;
        // `QLabelPrivate::sizeForWidth`: the text size plus the box, expanded to `minimumSize()`
        // (`qlabel.cpp:620`). `max-height` is not a hint.
        let (min_w, min_h) = style.min_box_size();
        Size::new(text_w.max(min_w), text_h.max(min_h))
    }

    fn minimum_size(&self) -> Size {
        let style = self.resolved_style();
        let (w, h) = style.min_box_size();
        Size::new(w, h)
    }

    /// `QLabel::minimumSizeHint` of a label that does not wrap: the text's own size.
    fn minimum_size_hint(&self) -> Size {
        self.size_hint()
    }

    fn maximum_size(&self) -> Size {
        let style = self.resolved_style();
        let (w, h) = style.max_box_size();
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

    fn has_focus(&self) -> bool {
        self.base.has_focus()
    }

    fn set_has_focus(&self, focus: bool) {
        self.base.set_has_focus(focus);
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
            // Qt paints the rule's background and border over the whole widget rect; `max-height`
            // already limited the geometry (as a box size), it is not a paint height.
            let rect_f = qtrs_gui::geometry::primitives::RectF::new(
                0.0,
                0.0,
                geom.width as f32,
                geom.height as f32,
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

        let (mut box_l, mut box_r, box_t, box_b) = Self::box_insets(&style);
        let half_indent = (Self::indent(&style, &metrics, &font, self.alignment) / 2.0).floor();
        match self.alignment {
            Alignment::Left => box_l += half_indent,
            Alignment::Right => box_r += half_indent,
            Alignment::Center => {}
        }
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

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
