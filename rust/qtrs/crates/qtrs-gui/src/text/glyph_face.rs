//! Glyph source of a font engine: character -> glyph mapping, glyph metrics and coverage bitmaps.
//!
//! Mirrors the glyph half of Qt's `QFontEngine` interface. Qt keeps one backend per platform
//! (DirectWrite/GDI on Windows, CoreText on macOS, FreeType elsewhere) behind that interface and
//! asks it for one glyph at a time; [`parse_face`] is the single place that chooses the backend
//! (Qt's `QPlatformFontDatabase::fontEngine` / `createEngine`).

use std::sync::Arc;

/// Placement and size of a rasterized glyph, in pixels at the requested size.
///
/// `xmin`/`ymin` are the offset of the bitmap's left edge and bottom edge from the pen position
/// (y grows upwards), so the bitmap's top-left lands at `(x + xmin, baseline - ymin - height)`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GlyphMetrics {
    pub xmin: i32,
    pub ymin: i32,
    pub width: usize,
    pub height: usize,
    pub advance_width: f32,
    pub advance_height: f32,
}

/// One face of a font as seen by the rasterizer.
pub trait GlyphFace: Send + Sync {
    /// Glyph index for `ch`, or 0 when the face has no glyph for it.
    fn glyph_index(&self, ch: char) -> u16;

    /// Metrics of `glyph_id` at `px` pixels per em, without rasterizing.
    fn metrics_indexed(&self, glyph_id: u16, px: f32) -> GlyphMetrics;

    /// Metrics and 8-bit coverage bitmap (row-major, top row first) of `glyph_id` at `px`.
    fn rasterize_indexed(&self, glyph_id: u16, px: f32) -> (GlyphMetrics, Vec<u8>);

    /// Metrics of the glyph for `ch` at `px`.
    fn metrics(&self, ch: char, px: f32) -> GlyphMetrics {
        self.metrics_indexed(self.glyph_index(ch), px)
    }
}

/// Shared handle to a face (Qt's ref-counted `QFontEngine*`).
pub type SharedGlyphFace = Arc<dyn GlyphFace>;

impl From<fontdue::Metrics> for GlyphMetrics {
    fn from(m: fontdue::Metrics) -> Self {
        Self {
            xmin: m.xmin,
            ymin: m.ymin,
            width: m.width,
            height: m.height,
            advance_width: m.advance_width,
            advance_height: m.advance_height,
        }
    }
}

impl GlyphFace for fontdue::Font {
    fn glyph_index(&self, ch: char) -> u16 {
        self.lookup_glyph_index(ch)
    }

    fn metrics_indexed(&self, glyph_id: u16, px: f32) -> GlyphMetrics {
        fontdue::Font::metrics_indexed(self, glyph_id, px).into()
    }

    fn rasterize_indexed(&self, glyph_id: u16, px: f32) -> (GlyphMetrics, Vec<u8>) {
        let (m, bitmap) = fontdue::Font::rasterize_indexed(self, glyph_id, px);
        (m.into(), bitmap)
    }
}

/// Builds the glyph face for face `face_index` of the font file `data`.
///
/// This is the only place that selects a rasterizer backend.
pub fn parse_face(data: &[u8], face_index: u32) -> Result<SharedGlyphFace, String> {
    let settings = fontdue::FontSettings {
        collection_index: face_index,
        ..fontdue::FontSettings::default()
    };
    fontdue::Font::from_bytes(data, settings)
        .map(|f| Arc::new(f) as SharedGlyphFace)
        .map_err(|e| format!("Failed to parse font: {e}"))
}
