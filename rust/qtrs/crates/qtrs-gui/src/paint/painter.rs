//! Painter primitives and 2D vector painting subsystem (`QPainter` equivalent).
//!
//! Controls stroke styling, fill patterns (gradients, textures, solid), transform stack,
//! composition modes, clipping, and text rendering.

use crate::geometry::primitives::{PointF, RectF};
use crate::geometry::transform::Transform2D;
pub use crate::paint::brush::Brush;
pub use crate::paint::composition::CompositionMode;
use crate::paint::paint_device::PaintDevice;
use crate::paint::path::PainterPath;
use crate::paint::pixmap::Pixmap;
use crate::text::document::TextDocument;
use crate::text::font::Font;
use crate::text::font_database::resolve_font_engines_for_text_global;
use crate::paint::text_blend::{blend_grey, blend_lcd};
use crate::text::glyph_layout::GlyphLayout;
use crate::text::smoothing::{gamma_lut, text_smoothing, TextSmoothing, TrcLut};
use std::sync::Arc;
use tiny_skia::{
    Color, FilterQuality, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pattern, Shader,
    SpreadMode, Stroke, Transform,
};

/// Pen styling (`QPen` equivalent).
#[derive(Debug, Clone, PartialEq)]
pub struct Pen {
    pub color: Color,
    pub width: f32,
    pub cap: LineCap,                   // Butt, Round, Square (Qt::PenCapStyle)
    pub join: LineJoin,                 // Miter, Round, Bevel (Qt::PenJoinStyle)
    pub dash_pattern: Option<Vec<f32>>, // Dash/dot pattern
}

impl Pen {
    // Creates a Pen with color and width, defaulting to round caps.
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
            width,
            cap: LineCap::Round,
            join: LineJoin::Round,
            dash_pattern: None,
        }
    }

    // Creates a Pen with RGBA8 color.
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8, width: f32) -> Self {
        Self::new(Color::from_rgba8(r, g, b, a), width)
    }

    // Sets line cap style.
    pub fn with_cap(mut self, cap: LineCap) -> Self {
        self.cap = cap;
        self
    }

    // Sets line join style.
    pub fn with_join(mut self, join: LineJoin) -> Self {
        self.join = join;
        self
    }

    // Sets dash pattern.
    pub fn with_dash_pattern(mut self, pattern: Vec<f32>) -> Self {
        self.dash_pattern = Some(pattern);
        self
    }

    // Converts to tiny_skia Stroke configuration.
    pub fn to_stroke(&self) -> Stroke {
        let mut stroke = Stroke {
            width: self.width,
            line_cap: self.cap,
            line_join: self.join,
            ..Default::default()
        };
        if let Some(dash) = &self.dash_pattern {
            if let Some(dash_stroke) = tiny_skia::StrokeDash::new(dash.clone(), 0.0) {
                stroke.dash = Some(dash_stroke);
            }
        }
        stroke
    }
}

impl Default for Pen {
    fn default() -> Self {
        Self::new(Color::BLACK, 1.0)
    }
}

/// Painter state snapshot (modeled after Qt `QPainter` state stack).
#[derive(Debug, Clone, PartialEq)]
pub struct PainterState {
    pub pen: Option<Pen>,
    pub brush: Brush,
    pub transform: Transform,
    pub opacity: f32,
    pub antialiasing: bool,
    pub composition_mode: CompositionMode,
    pub clip_rect: Option<tiny_skia::Rect>,
}

impl Default for PainterState {
    fn default() -> Self {
        Self {
            pen: Some(Pen::new(Color::BLACK, 1.0)),
            brush: Brush::NoBrush,
            transform: Transform::identity(),
            opacity: 1.0,
            antialiasing: true,
            composition_mode: CompositionMode::SourceOver,
            clip_rect: None,
        }
    }
}

/// 2D vector painter (`QPainter` equivalent).
///
/// Returns a mutable reference to the active state.
pub struct Painter<'a> {
    device: &'a mut dyn PaintDevice,
    state: PainterState,
    saved_states: Vec<PainterState>,
    cached_mask: Option<Mask>,
    cached_clip_rect: Option<tiny_skia::Rect>,
    /// Gamma table of LCD (ClearType) text; `None` draws grey-scale text. Fixed when painting
    /// starts, like `QRasterPaintEngine::begin` fixes the glyph cache format.
    lcd_lut: Option<Arc<TrcLut>>,
}
impl<'a> Painter<'a> {
    // Begins painting on the specified `PaintDevice`, configuring initial DPR transform.
    pub fn begin(device: &'a mut dyn PaintDevice) -> Self {
        let dpr = device.device_pixel_ratio();
        let mut state = PainterState::default();
        // Configure initial scale transform matching device pixel ratio
        if dpr != 1.0 {
            state.transform = tiny_skia::Transform::from_scale(dpr, dpr);
        }
        let smoothing = text_smoothing();
        Self {
            device,
            state,
            saved_states: Vec::new(),
            cached_mask: None,
            cached_clip_rect: None,
            lcd_lut: smoothing.cleartype.then(|| gamma_lut(smoothing.gamma)),
        }
    }

    /// Overrides the text smoothing taken from the platform when painting started.
    pub fn set_text_smoothing(&mut self, smoothing: TextSmoothing) {
        self.lcd_lut = smoothing.cleartype.then(|| gamma_lut(smoothing.gamma));
    }

    // Returns a reference to the active state.
    pub fn state(&self) -> &PainterState {
        &self.state
    }

    // Returns a mutable reference to the active state.
    pub fn state_mut(&mut self) -> &mut PainterState {
        &mut self.state
    }

    // Returns a reference to the underlying PaintDevice.
    pub fn device(&self) -> &dyn PaintDevice {
        &*self.device
    }

    // -------------------------------------------------------------------------
    // State and transform management
    // -------------------------------------------------------------------------

    // Saves the current painter state onto the stack.
    pub fn save(&mut self) {
        self.saved_states.push(self.state.clone());
    }

    // Restores the most recently saved painter state.
    pub fn restore(&mut self) {
        if let Some(prev) = self.saved_states.pop() {
            self.state = prev;
        }
    }

    // Translates coordinates.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.state.transform = self.state.transform.pre_translate(dx, dy);
    }

    // Scales coordinates.
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.state.transform = self.state.transform.pre_scale(sx, sy);
    }

    // Rotates coordinates (in degrees, clockwise).
    pub fn rotate(&mut self, degrees: f32) {
        self.state.transform = self.state.transform.pre_rotate(degrees);
    }
    /// Shears coordinates horizontally by `shx` and vertically by `shy`.
    pub fn shear(&mut self, shx: f32, shy: f32) {
        let t = Transform2D::from_shear(shx, shy);
        self.state.transform = self.state.transform.post_concat(t.to_skia());
    }

    /// Sets the full 2D affine transformation matrix.
    pub fn set_transform(&mut self, transform: &Transform2D) {
        self.state.transform = transform.to_skia();
    }

    /// Returns the current 2D affine transformation matrix.
    pub fn transform(&self) -> Transform2D {
        Transform2D::from_skia(self.state.transform)
    }

    /// Resets the current transformation to identity (scaled by DPR).
    pub fn reset_transform(&mut self) {
        let dpr = self.device.device_pixel_ratio();
        if dpr != 1.0 {
            self.state.transform = Transform::from_scale(dpr, dpr);
        } else {
            self.state.transform = Transform::identity();
        }
    }

    /// Sets the composition / blend mode.
    pub fn set_composition_mode(&mut self, mode: CompositionMode) {
        self.state.composition_mode = mode;
    }

    /// Returns the active composition mode.
    pub fn composition_mode(&self) -> CompositionMode {
        self.state.composition_mode
    }

    pub fn set_pen(&mut self, pen: impl Into<Option<Pen>>) {
        self.state.pen = pen.into();
    }

    pub fn set_brush(&mut self, brush: Brush) {
        self.state.brush = brush;
    }

    // Sets opacity (0.0 to 1.0).
    pub fn set_opacity(&mut self, opacity: f32) {
        self.state.opacity = opacity.clamp(0.0, 1.0);
    }

    // Sets the clip rectangle.
    pub fn set_clip_rect(&mut self, rect: RectF) {
        let tiny_r = tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.width, rect.height);
        self.state.clip_rect = tiny_r;
    }

    // -------------------------------------------------------------------------

    // -------------------------------------------------------------------------

    // Updates or clears the cached clip mask based on current state.
    fn update_clip_mask_if_needed(&mut self) {
        if self.state.clip_rect == self.cached_clip_rect && (self.cached_mask.is_some() || self.state.clip_rect.is_none()) {
            return;
        }
        self.cached_clip_rect = self.state.clip_rect;
        self.cached_mask = if let Some(clip) = self.state.clip_rect {
            if let Some(mut mask) = Mask::new(self.device.physical_width(), self.device.physical_height()) {
                let path = PathBuilder::from_rect(clip);
                let dpr = self.device.device_pixel_ratio();
                let scale = if dpr != 1.0 {
                    tiny_skia::Transform::from_scale(dpr, dpr)
                } else {
                    tiny_skia::Transform::identity()
                };
                mask.fill_path(
                    &path,
                    tiny_skia::FillRule::Winding,
                    false,
                    scale,
                );
                Some(mask)
            } else {
                None
            }
        } else {
            None
        };
    }

    pub fn fill_path(&mut self, path: &Path) {
        self.fill_path_with_rule(path, tiny_skia::FillRule::Winding);
    }

    pub fn fill_path_with_rule(&mut self, path: &Path, fill_rule: tiny_skia::FillRule) {
        let opacity = self.state.opacity;
        let transform = self.state.transform;
        self.update_clip_mask_if_needed();
        let mask_ref = self.cached_mask.as_ref();
        let blend_mode = self.state.composition_mode.into();

        match &self.state.brush {
            Brush::NoBrush => {}
            Brush::Color(color) => {
                let mut c = *color;
                c.apply_opacity(opacity);
                let paint = Paint {
                    shader: Shader::SolidColor(c),
                    blend_mode,
                    anti_alias: self.state.antialiasing,
                    ..Default::default()
                };
                self.device
                    .as_pixmap_mut()
                    .fill_path(path, &paint, fill_rule, transform, mask_ref);
            }
            Brush::Hatched { color } => {
                if let Some(mut pat) = Pixmap::new(8, 8) {
                    let pat_data = pat.data_mut();
                    let r = (color.red() * 255.0).round() as u8;
                    let g = (color.green() * 255.0).round() as u8;
                    let b = (color.blue() * 255.0).round() as u8;
                    let a = (color.alpha() * 255.0).round() as u8;
                    for y in 0..8 {
                        let x = y;
                        let idx = (y * 8 + x) * 4;
                        pat_data[idx] = r;
                        pat_data[idx + 1] = g;
                        pat_data[idx + 2] = b;
                        pat_data[idx + 3] = a;
                    }
                    let paint = Paint {
                        shader: Pattern::new(
                            pat.as_tiny_skia().as_ref(),
                            SpreadMode::Repeat,
                            FilterQuality::Nearest,
                            opacity,
                            Transform::identity(),
                        ),
                        blend_mode,
                        anti_alias: self.state.antialiasing,
                        ..Default::default()
                    };
                    self.device
                        .as_pixmap_mut()
                        .fill_path(path, &paint, fill_rule, transform, mask_ref);
                }
            }
            Brush::LinearGradient(gradient) => {
                if let Some(shader) = gradient.to_shader(Transform::identity()) {
                    let paint = Paint {
                        shader,
                        blend_mode,
                        anti_alias: self.state.antialiasing,
                        ..Default::default()
                    };
                    self.device
                        .as_pixmap_mut()
                        .fill_path(path, &paint, fill_rule, transform, mask_ref);
                }
            }
            Brush::RadialGradient(gradient) => {
                if let Some(shader) = gradient.to_shader(Transform::identity()) {
                    let paint = Paint {
                        shader,
                        blend_mode,
                        anti_alias: self.state.antialiasing,
                        ..Default::default()
                    };
                    self.device
                        .as_pixmap_mut()
                        .fill_path(path, &paint, fill_rule, transform, mask_ref);
                }
            }
            Brush::Texture(pattern) => {
                let paint = Paint {
                    shader: Pattern::new(
                        pattern.pixmap.as_tiny_skia().as_ref(),
                        SpreadMode::Repeat,
                        FilterQuality::Bilinear,
                        opacity,
                        pattern.transform,
                    ),
                    blend_mode,
                    anti_alias: self.state.antialiasing,
                    ..Default::default()
                };
                self.device
                    .as_pixmap_mut()
                    .fill_path(path, &paint, fill_rule, transform, mask_ref);
            }
        }
    }

    pub fn stroke_path(&mut self, path: &Path) {
        let pen = match &self.state.pen {
            Some(p) => p,
            None => return,
        };
        let mut c = pen.color;
        c.apply_opacity(self.state.opacity);
        let blend_mode = self.state.composition_mode.into();
        let paint = Paint {
            shader: Shader::SolidColor(c),
            blend_mode,
            anti_alias: self.state.antialiasing,
            ..Default::default()
        };
        let stroke = pen.to_stroke();
        let transform = self.state.transform;
        self.update_clip_mask_if_needed();
        let mask_ref = self.cached_mask.as_ref();

        self.device
            .as_pixmap_mut()
            .stroke_path(path, &paint, &stroke, transform, mask_ref);
    }

    /// Draws a `PainterPath` (`QPainter::drawPath` equivalent) with filling and outline.
    pub fn draw_path(&mut self, painter_path: &PainterPath) {
        if let Some(path) = painter_path.to_skia_path() {
            self.fill_path_with_rule(&path, painter_path.fill_rule().into());
            self.stroke_path(&path);
        }
    }

    /// Fills a `PainterPath` with current brush and the path's fill rule.
    pub fn fill_painter_path(&mut self, painter_path: &PainterPath) {
        if let Some(path) = painter_path.to_skia_path() {
            self.fill_path_with_rule(&path, painter_path.fill_rule().into());
        }
    }

    /// Strokes a `PainterPath` with current pen.
    pub fn stroke_painter_path(&mut self, painter_path: &PainterPath) {
        if let Some(path) = painter_path.to_skia_path() {
            self.stroke_path(&path);
        }
    }

    // -------------------------------------------------------------------------
    // Basic shapes and HUD geometry
    // -------------------------------------------------------------------------

    pub fn draw_line(&mut self, p1: PointF, p2: PointF) {
        let mut pb = PathBuilder::new();
        pb.move_to(p1.x, p1.y);
        pb.line_to(p2.x, p2.y);
        if let Some(path) = pb.finish() {
            self.stroke_path(&path);
        }
    }

    // Draws a rectangle using current brush and pen.
    pub fn draw_rect(&mut self, rect: RectF) {
        if let Some(r) = tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
            let path = PathBuilder::from_rect(r);
            self.fill_path(&path);
            self.stroke_path(&path);
        }
    }
    /// Fills a rectangle with a solid color, bypassing active brush and pen.
    pub fn fill_rect(&mut self, rect: RectF, color: Color) {
        let old_brush = self.state.brush.clone();
        let old_pen = self.state.pen.clone();
        self.set_brush(Brush::from_color(color));
        self.set_pen(None);
        if let Some(r) = tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
            let path = PathBuilder::from_rect(r);
            self.fill_path(&path);
        }
        self.state.brush = old_brush;
        self.state.pen = old_pen;
    }

    // Draws a rounded rectangle.
    pub fn draw_rounded_rect(&mut self, rect: RectF, rx: f32, ry: f32) {
        let rx = rx.min(rect.width / 2.0).max(0.0);
        let ry = ry.min(rect.height / 2.0).max(0.0);
        if rx <= 0.0 || ry <= 0.0 {
            self.draw_rect(rect);
            return;
        }
        let mut pb = PathBuilder::new();
        let x = rect.x;
        let y = rect.y;
        let w = rect.width;
        let h = rect.height;
        let k = 0.552_284_8; // Bezier circle approximation constant
        let kx = rx * k;
        let ky = ry * k;

        pb.move_to(x + rx, y);
        pb.line_to(x + w - rx, y);
        pb.cubic_to(x + w - rx + kx, y, x + w, y + ry - ky, x + w, y + ry);
        pb.line_to(x + w, y + h - ry);
        pb.cubic_to(
            x + w,
            y + h - ry + ky,
            x + w - rx + kx,
            y + h,
            x + w - rx,
            y + h,
        );
        pb.line_to(x + rx, y + h);
        pb.cubic_to(x + rx - kx, y + h, x, y + h - ry + ky, x, y + h - ry);
        pb.line_to(x, y + ry);
        pb.cubic_to(x, y + ry - ky, x + rx - kx, y, x + rx, y);
        pb.close();

        if let Some(path) = pb.finish() {
            self.fill_path(&path);
            self.stroke_path(&path);
        }
    }

    // Draws an ellipse.
    pub fn draw_ellipse(&mut self, rect: RectF) {
        if let Some(r) = tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
            if let Some(path) = PathBuilder::from_oval(r) {
                self.fill_path(&path);
                self.stroke_path(&path);
            }
        }
    }

    pub fn draw_polyline(&mut self, points: &[PointF]) {
        if points.len() < 2 {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(points[0].x, points[0].y);
        for p in &points[1..] {
            pb.line_to(p.x, p.y);
        }
        if let Some(path) = pb.finish() {
            self.stroke_path(&path);
        }
    }

    // -------------------------------------------------------------------------
    // Returns a mutable reference to the active state.
    // -------------------------------------------------------------------------

    // Draws an arc.
    //
    // `start_deg`: start angle in degrees (90 degrees is top).
    // `span_deg`: sweep angle in degrees (negative for clockwise).
    pub fn draw_arc(&mut self, rect: RectF, start_deg: f32, span_deg: f32) {
        if let Some(path) = create_arc_path(rect, start_deg, span_deg) {
            self.stroke_path(&path);
        }
    }

    // Draws a pie slice.
    //

    pub fn draw_pie(&mut self, rect: RectF, start_deg: f32, span_deg: f32) {
        if let Some(path) = create_pie_path(rect, start_deg, span_deg) {
            self.fill_path(&path);
            self.stroke_path(&path);
        }
    }

    // -------------------------------------------------------------------------
    // Pixmap drawing
    // -------------------------------------------------------------------------

    // Draws a pixmap with optional source rectangle cropping and scaling.
    pub fn draw_pixmap(&mut self, target: RectF, pixmap: &Pixmap, source: Option<RectF>) {
        let src_rect = source.unwrap_or_else(|| {
            RectF::new(
                0.0,
                0.0,
                pixmap.physical_width() as f32,
                pixmap.physical_height() as f32,
            )
        });
        if src_rect.width <= 0.0
            || src_rect.height <= 0.0
            || target.width <= 0.0
            || target.height <= 0.0
        {
            return;
        }
        let scale_x = target.width / src_rect.width;
        let scale_y = target.height / src_rect.height;

        let patt_transform = Transform::from_translate(target.x, target.y)
            .pre_scale(scale_x, scale_y)
            .pre_translate(-src_rect.x, -src_rect.y);

        let paint = Paint {
            shader: Pattern::new(
                pixmap.as_tiny_skia().as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                self.state.opacity,
                patt_transform,
            ),
            anti_alias: self.state.antialiasing,
            ..Default::default()
        };

        if let Some(target_r) =
            tiny_skia::Rect::from_xywh(target.x, target.y, target.width, target.height)
        {
            let path = PathBuilder::from_rect(target_r);
            let transform = self.state.transform;
            self.update_clip_mask_if_needed();
            let mask_ref = self.cached_mask.as_ref();
            self.device.as_pixmap_mut().fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                transform,
                mask_ref,
            );
        }
    }

    // -------------------------------------------------------------------------
    // Text rendering
    // -------------------------------------------------------------------------

    // Returns a mutable reference to the active state.
    //
    // Raster helpers
    // Sets opacity (0.0 to 1.0).
    pub fn draw_text(&mut self, pos: PointF, text: &str, font: &Font) {
        if text.is_empty() {
            return;
        }
        let engines = resolve_font_engines_for_text_global(font, text);
        if engines.is_empty() {
            return;
        }

        // Compute glyph layout using Qt 6 Run-based OpenType fallback
        let layout = GlyphLayout::shape_with_engines(text, font, &engines);

        let text_color = self
            .state
            .pen
            .as_ref()
            .map(|p| p.color)
            .unwrap_or(Color::BLACK);
        let base_alpha = (text_color.alpha() * self.state.opacity).clamp(0.0, 1.0);
        let target_r = (text_color.red() * 255.0).round() as u32;
        let target_g = (text_color.green() * 255.0).round() as u32;
        let target_b = (text_color.blue() * 255.0).round() as u32;

        let target = [target_r, target_g, target_b];
        let lcd_lut = self.lcd_lut.clone();
        let dpr = self.device.device_pixel_ratio();
        let transform = self.state.transform;
        let mut pixmap = self.device.as_pixmap_mut();
        let pw = pixmap.width();
        let ph = pixmap.height();
        let pix_data = pixmap.data_mut();

        for glyph in layout.glyphs {
            let lx = pos.x + glyph.x;
            let ly = pos.y + glyph.y;

            let px = transform.sx * lx + transform.kx * ly + transform.tx;
            let py = transform.ky * lx + transform.sy * ly + transform.ty;

            let engine = match engines.get(glyph.font_index as usize) {
                Some(e) => e,
                None => &engines[0],
            };

            // Qt 6 parity (QWindowsFontEngineDirectWrite::alphaRGBMapForGlyph):
            // Check if this glyph is an OpenType COLRv0/CPAL color glyph (e.g. Segoe UI Emoji)
            if let Some((color_metrics, color_pixmap)) =
                engine.rasterize_color_glyph(glyph.glyph_id, font.size * dpr)
            {
                let start_x = (px + color_metrics.xmin as f32).round() as i32;
                let start_y =
                    (py - color_metrics.ymin as f32 - color_metrics.height as f32).round() as i32;
                let c_w = color_metrics.width;
                let c_h = color_metrics.height;
                let color_data = color_pixmap.data();

                for gy in 0..c_h {
                    let dst_y = start_y + gy as i32;
                    if dst_y < 0 || dst_y >= ph as i32 {
                        continue;
                    }
                    if let Some(clip) = self.state.clip_rect {
                        let cy0 = (clip.top() * dpr).floor() as i32;
                        let cy1 = (clip.bottom() * dpr).ceil() as i32;
                        if dst_y < cy0 || dst_y >= cy1 {
                            continue;
                        }
                    }
                    for gx in 0..c_w {
                        let dst_x = start_x + gx as i32;
                        if dst_x < 0 || dst_x >= pw as i32 {
                            continue;
                        }
                        if let Some(clip) = self.state.clip_rect {
                            let cx0 = (clip.left() * dpr).floor() as i32;
                            let cx1 = (clip.right() * dpr).ceil() as i32;
                            if dst_x < cx0 || dst_x >= cx1 {
                                continue;
                            }
                        }

                        let src_idx = (gy * c_w + gx) * 4;
                        let src_raw_r = color_data[src_idx] as u32;
                        let src_raw_g = color_data[src_idx + 1] as u32;
                        let src_raw_b = color_data[src_idx + 2] as u32;
                        let src_raw_a = color_data[src_idx + 3] as u32;
                        if src_raw_a == 0 {
                            continue;
                        }

                        let a_factor = (src_raw_a as f32 / 255.0) * self.state.opacity;
                        let src_a = (a_factor * 255.0).round() as u32;
                        let inv_a = 255 - src_a;

                        // tiny-skia Pixmap is premultiplied RGBA
                        let src_pr = ((src_raw_r as f32 * self.state.opacity).round() as u32).min(255);
                        let src_pg = ((src_raw_g as f32 * self.state.opacity).round() as u32).min(255);
                        let src_pb = ((src_raw_b as f32 * self.state.opacity).round() as u32).min(255);

                        let dst_idx = ((dst_y as usize) * (pw as usize) + (dst_x as usize)) * 4;
                        let cur_r = pix_data[dst_idx] as u32;
                        let cur_g = pix_data[dst_idx + 1] as u32;
                        let cur_b = pix_data[dst_idx + 2] as u32;
                        let cur_a = pix_data[dst_idx + 3] as u32;

                        pix_data[dst_idx] = ((src_pr + (cur_r * inv_a) / 255).min(255)) as u8;
                        pix_data[dst_idx + 1] = ((src_pg + (cur_g * inv_a) / 255).min(255)) as u8;
                        pix_data[dst_idx + 2] = ((src_pb + (cur_b * inv_a) / 255).min(255)) as u8;
                        pix_data[dst_idx + 3] = ((src_a + (cur_a * inv_a) / 255).min(255)) as u8;
                    }
                }
                continue;
            }

            // Qt draws LCD masks when ClearType is on and the face has them
            // (`QRasterPaintEngine::drawCachedGlyphs`, `alphaRGBMapForGlyph`).
            let lcd_glyph = lcd_lut
                .as_ref()
                .and_then(|_| engine.rasterize_lcd_glyph(glyph.glyph_id, font.size, dpr));
            let (metrics, bitmap) = match &lcd_glyph {
                Some((metrics, bitmap)) => (*metrics, bitmap.clone()),
                None => engine.rasterize_glyph(glyph.glyph_id, font.size, dpr),
            };
            if metrics.width == 0 || metrics.height == 0 {
                continue;
            }

            let start_x = (px + metrics.xmin as f32).round() as i32;
            let start_y = (py - metrics.ymin as f32 - metrics.height as f32).round() as i32;

            if let Some(clip) = self.state.clip_rect {
                let cx0 = (clip.left() * dpr).floor() as i32;
                let cy0 = (clip.top() * dpr).floor() as i32;
                let cx1 = (clip.right() * dpr).ceil() as i32;
                let cy1 = (clip.bottom() * dpr).ceil() as i32;
                let glyph_x1 = start_x + metrics.width as i32;
                let glyph_y1 = start_y + metrics.height as i32;
                if glyph_x1 <= cx0 || start_x >= cx1 || glyph_y1 <= cy0 || start_y >= cy1 {
                    continue;
                }
            }

            for gy in 0..metrics.height {
                let dst_y = start_y + gy as i32;
                if dst_y < 0 || dst_y >= ph as i32 {
                    continue;
                }
                if let Some(clip) = self.state.clip_rect {
                    let cy0 = (clip.top() * dpr).floor() as i32;
                    let cy1 = (clip.bottom() * dpr).ceil() as i32;
                    if dst_y < cy0 || dst_y >= cy1 {
                        continue;
                    }
                }
                for gx in 0..metrics.width {
                    let dst_x = start_x + gx as i32;
                    if dst_x < 0 || dst_x >= pw as i32 {
                        continue;
                    }
                    if let Some(clip) = self.state.clip_rect {
                        let cx0 = (clip.left() * dpr).floor() as i32;
                        let cx1 = (clip.right() * dpr).ceil() as i32;
                        if dst_x < cx0 || dst_x >= cx1 {
                            continue;
                        }
                    }

                    let dst_idx = ((dst_y as usize) * (pw as usize) + (dst_x as usize)) * 4;
                    let px = &mut pix_data[dst_idx..dst_idx + 4];
                    match (&lcd_glyph, &lcd_lut) {
                        (Some(_), Some(lut)) => {
                            let i = (gy * metrics.width + gx) * 3;
                            let cov = [bitmap[i], bitmap[i + 1], bitmap[i + 2]];
                            blend_lcd(px, cov, base_alpha, target, lut);
                        }
                        _ => {
                            let glyph_alpha = bitmap[gy * metrics.width + gx];
                            if glyph_alpha != 0 {
                                blend_grey(px, glyph_alpha, base_alpha, target);
                            }
                        }
                    }
                }
            }
        }
    }
    /// Draws text with a specific color.
    pub fn draw_text_colored(&mut self, pos: PointF, text: &str, font: &Font, color: Color) {
        let old_pen = self.state.pen.clone();
        self.set_pen(Pen::new(color, 1.0));
        self.draw_text(pos, text, font);
        self.state.pen = old_pen;
    }

    /// Renders a multi-line formatted `TextDocument` at the specified position.
    pub fn draw_text_document(&mut self, pos: PointF, doc: &TextDocument) {
        doc.draw(self, pos);
    }
}

pub fn create_arc_path(rect: RectF, start_deg: f32, span_deg: f32) -> Option<Path> {
    if span_deg.abs() < 1e-4 || rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    let rx = rect.width / 2.0;
    let ry = rect.height / 2.0;
    let cx = rect.x + rx;
    let cy = rect.y + ry;

    let mut pb = PathBuilder::new();
    let num_segments = (span_deg.abs() / 45.0).ceil().max(1.0) as usize;
    let step_deg = span_deg / num_segments as f32;

    for i in 0..num_segments {
        let a1_deg = start_deg + i as f32 * step_deg;
        let a2_deg = a1_deg + step_deg;
        // Returns a mutable reference to the active state.
        let a1 = a1_deg.to_radians();
        let a2 = a2_deg.to_radians();

        let p1 = (cx + rx * a1.cos(), cy - ry * a1.sin());
        let p2 = (cx + rx * a2.cos(), cy - ry * a2.sin());

        let delta = (a2 - a1) / 2.0;
        let k = (4.0 / 3.0) * (delta.sin() / (1.0 + delta.cos()));

        let c1x = p1.0 - k * rx * a1.sin();
        let c1y = p1.1 - k * ry * a1.cos();
        let c2x = p2.0 + k * rx * a2.sin();
        let c2y = p2.1 + k * ry * a2.cos();

        if i == 0 {
            pb.move_to(p1.0, p1.1);
        }
        pb.cubic_to(c1x, c1y, c2x, c2y, p2.0, p2.1);
    }

    pb.finish()
}

pub fn create_pie_path(rect: RectF, start_deg: f32, span_deg: f32) -> Option<Path> {
    if span_deg.abs() < 1e-4 || rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    let rx = rect.width / 2.0;
    let ry = rect.height / 2.0;
    let cx = rect.x + rx;
    let cy = rect.y + ry;

    let mut pb = PathBuilder::new();
    // Start from center
    pb.move_to(cx, cy);

    let num_segments = (span_deg.abs() / 45.0).ceil().max(1.0) as usize;
    let step_deg = span_deg / num_segments as f32;

    for i in 0..num_segments {
        let a1_deg = start_deg + i as f32 * step_deg;
        let a2_deg = a1_deg + step_deg;
        let a1 = a1_deg.to_radians();
        let a2 = a2_deg.to_radians();

        let p1 = (cx + rx * a1.cos(), cy - ry * a1.sin());
        let p2 = (cx + rx * a2.cos(), cy - ry * a2.sin());

        let delta = (a2 - a1) / 2.0;
        let k = (4.0 / 3.0) * (delta.sin() / (1.0 + delta.cos()));

        let c1x = p1.0 - k * rx * a1.sin();
        let c1y = p1.1 - k * ry * a1.cos();
        let c2x = p2.0 + k * rx * a2.sin();
        let c2y = p2.1 + k * ry * a2.cos();

        if i == 0 {
            pb.line_to(p1.0, p1.1);
        }
        pb.cubic_to(c1x, c1y, c2x, c2y, p2.0, p2.1);
    }

    // Close back to center
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pen_creation_and_customization() {
        let pen = Pen::new(Color::from_rgba8(255, 0, 0, 255), 2.5);
        assert_eq!(pen.width, 2.5);
        assert_eq!(pen.cap, LineCap::Round);
        assert_eq!(pen.join, LineJoin::Round);
        assert!(pen.dash_pattern.is_none());

        let customized = pen
            .with_cap(LineCap::Square)
            .with_join(LineJoin::Miter)
            .with_dash_pattern(vec![5.0, 5.0]);

        assert_eq!(customized.cap, LineCap::Square);
        assert_eq!(customized.join, LineJoin::Miter);
        assert_eq!(customized.dash_pattern, Some(vec![5.0, 5.0]));

        let stroke = customized.to_stroke();
        assert_eq!(stroke.width, 2.5);
        assert_eq!(stroke.line_cap, LineCap::Square);
        assert_eq!(stroke.line_join, LineJoin::Miter);
        assert!(stroke.dash.is_some());
    }

    #[test]
    fn test_brush_variants_and_default() {
        assert_eq!(Brush::default(), Brush::NoBrush);

        let solid = Brush::from_rgba8(10, 20, 30, 255);
        assert_eq!(solid, Brush::Color(Color::from_rgba8(10, 20, 30, 255)));

        let hatched = Brush::hatched(Color::from_rgba8(255, 255, 0, 180));
        assert_eq!(
            hatched,
            Brush::Hatched {
                color: Color::from_rgba8(255, 255, 0, 180)
            }
        );
    }

    #[test]
    fn test_painter_state_default_and_clone() {
        let state = PainterState::default();
        assert!(state.pen.is_some());
        assert_eq!(state.brush, Brush::NoBrush);
        assert_eq!(state.opacity, 1.0);
        assert!(state.antialiasing);
        assert!(state.clip_rect.is_none());
        assert_eq!(state.transform, tiny_skia::Transform::identity());

        let mut cloned = state.clone();
        cloned.opacity = 0.5;
        cloned.pen = None;
        cloned.brush = Brush::from_rgba8(255, 0, 0, 255);
        assert_ne!(state, cloned);
        assert_eq!(cloned.opacity, 0.5);
        assert!(cloned.pen.is_none());
    }

    #[test]
    fn test_painter_begin_dpr_transform() {
        use crate::paint::pixmap::Pixmap;

        // DPR 1.0
        let mut pm1 = Pixmap::new(100, 100).unwrap();
        let painter1 = Painter::begin(&mut pm1);
        assert_eq!(painter1.state().transform, tiny_skia::Transform::identity());

        // DPR 2.0
        let mut pm2 = Pixmap::with_dpr(200, 100, 2.0).unwrap();
        let painter2 = Painter::begin(&mut pm2);
        assert_eq!(
            painter2.state().transform,
            tiny_skia::Transform::from_scale(2.0, 2.0)
        );
    }

    #[test]
    fn test_painter_transforms_and_state_stack() {
        let mut pm = Pixmap::new(100, 100).unwrap();
        let mut p = Painter::begin(&mut pm);

        p.set_opacity(0.8);
        p.translate(10.0, 20.0);
        p.save();

        p.scale(2.0, 2.0);
        p.set_opacity(0.4);
        assert_eq!(p.state().opacity, 0.4);

        p.restore();
        assert_eq!(p.state().opacity, 0.8);

        // Returns a mutable reference to the active state.
    }

    #[test]
    fn test_painter_draw_primitives_headless() {
        let mut pm = Pixmap::new(200, 200).unwrap();
        let mut p = Painter::begin(&mut pm);

        // Line
        p.set_pen(Pen::from_rgba8(255, 0, 0, 255, 2.0));
        p.draw_line(PointF::new(0.0, 0.0), PointF::new(50.0, 50.0));

        // Rect
        p.set_brush(Brush::from_rgba8(0, 255, 0, 255));
        p.draw_rect(RectF::new(10.0, 10.0, 40.0, 40.0));

        // Rounded rect
        p.draw_rounded_rect(RectF::new(60.0, 10.0, 40.0, 40.0), 5.0, 5.0);

        // Ellipse
        p.draw_ellipse(RectF::new(10.0, 60.0, 40.0, 40.0));

        let points = [
            PointF::new(0.0, 0.0),
            PointF::new(10.0, 20.0),
            PointF::new(20.0, 10.0),
        ];
        p.draw_polyline(&points);

        assert!(pm.data().iter().any(|&b| b > 0));
    }

    #[test]
    fn test_painter_draw_arc_and_pie() {
        let mut pm = Pixmap::new(200, 200).unwrap();
        let mut p = Painter::begin(&mut pm);

        p.set_pen(Pen::from_rgba8(255, 255, 0, 255, 3.0));
        p.set_brush(Brush::from_rgba8(0, 0, 255, 128));

        // Arc (HUD outer ring)
        p.draw_arc(RectF::new(20.0, 20.0, 100.0, 100.0), 90.0, -180.0);

        p.draw_pie(RectF::new(20.0, 20.0, 100.0, 100.0), 0.0, 90.0);

        assert!(pm.data().iter().any(|&b| b > 0));
    }

    #[test]
    fn test_painter_draw_pixmap_and_clip() {
        let mut dest = Pixmap::new(100, 100).unwrap();
        let mut src = Pixmap::new(50, 50).unwrap();
        src.fill(Color::from_rgba8(255, 0, 0, 255));

        let mut p = Painter::begin(&mut dest);
        p.set_clip_rect(RectF::new(10.0, 10.0, 80.0, 80.0));
        p.draw_pixmap(RectF::new(0.0, 0.0, 50.0, 50.0), &src, None);

        assert!(dest.data().iter().any(|&b| b > 0));
    }

    // -------------------------------------------------------------------------
    // Returns a mutable reference to the active state.
    // -------------------------------------------------------------------------

    #[test]
    fn test_painter_draw_rect_and_fill() {
        let mut surface = Pixmap::new(100, 100).unwrap();
        // Fill with white
        surface.fill(tiny_skia::Color::WHITE);

        {
            let mut painter = Painter::begin(&mut surface);
            painter.set_pen(None);
            painter.set_brush(Brush::Color(tiny_skia::Color::from_rgba8(255, 0, 0, 255)));

            painter.draw_rect(RectF::new(40.0, 40.0, 20.0, 20.0));
        }

        let data = surface.data();

        let get_pixel = |x: usize, y: usize| -> (u8, u8, u8, u8) {
            let idx = (y * 100 + x) * 4;
            (data[idx], data[idx + 1], data[idx + 2], data[idx + 3])
        };

        assert_eq!(get_pixel(10, 10), (255, 255, 255, 255));

        assert_eq!(get_pixel(50, 50), (255, 0, 0, 255));
    }

    // -------------------------------------------------------------------------

    // -------------------------------------------------------------------------

    #[test]
    fn test_render_claude_hud_dial_to_png() {
        let mut surface = Pixmap::new(300, 300).unwrap();
        surface.fill(tiny_skia::Color::TRANSPARENT);

        {
            let mut painter = Painter::begin(&mut surface);

            // Returns a mutable reference to the active state.
            let bounds = RectF::new(20.0, 20.0, 260.0, 260.0);
            painter.set_pen(Pen::new(
                tiny_skia::Color::from_rgba8(40, 40, 40, 255),
                14.0,
            ));
            painter.set_brush(Brush::NoBrush);
            painter.draw_ellipse(bounds);

            painter.set_pen(Pen::new(
                tiny_skia::Color::from_rgba8(50, 205, 50, 255),
                14.0,
            ));
            painter.draw_arc(bounds, 90.0, -270.0);

            let inner_bounds = RectF::new(60.0, 60.0, 180.0, 180.0);
            painter.set_pen(None);
            painter.set_brush(Brush::Color(tiny_skia::Color::from_rgba8(255, 140, 0, 200)));
            painter.draw_pie(inner_bounds, 90.0, -120.0);

            let font = Font::new("Arial", 18.0)
                .with_weight(crate::text::font::FontWeight::Bold)
                .with_tabular_numbers(true);
            painter.set_pen(Pen::from_rgba8(255, 255, 255, 255, 1.0));
            painter.draw_text(PointF::new(105.0, 155.0), "75%", &font);
        }

        // Save PNG
        let out_dir = std::path::Path::new("target");
        if !out_dir.exists() {
            let _ = std::fs::create_dir_all(out_dir);
        }
        let output_path = out_dir.join("test_hud_dial.png");
        surface.save_png(&output_path).expect("failed to save PNG");
        assert!(output_path.exists());
    }

    #[test]
    fn test_painter_draw_text_headless_probe() {
        let mut surface = Pixmap::new(100, 100).unwrap();
        surface.fill(tiny_skia::Color::TRANSPARENT);

        {
            let mut painter = Painter::begin(&mut surface);
            painter.set_pen(Pen::from_rgba8(0, 255, 0, 255, 1.0));
            let font = Font::new("Arial", 16.0);
            painter.draw_text(PointF::new(20.0, 50.0), "HUD", &font);
        }

        // Verify rendered text contains green pixels
        let data = surface.data();
        let has_green = data
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[1] > 50 && p[3] > 0);
        assert!(has_green, "Canvas must contain rendered green text pixels");
    }
}
