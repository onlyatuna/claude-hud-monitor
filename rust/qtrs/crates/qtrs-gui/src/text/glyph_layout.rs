use crate::text::font::{Font, SharedFontData};
use crate::text::glyph_face::{GlyphMetrics, SharedGlyphFace};
use std::str::FromStr;
use std::sync::Arc;

/// A single positioned glyph (`QGlyphRun` item equivalent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    /// Glyph index in font.
    pub glyph_id: u16,
    /// Index into font engine multi list (0 is primary, >0 is fallback).
    pub font_index: u8,
    /// Horizontal offset in points.
    pub x: f32,
    /// Vertical offset in points.
    pub y: f32,
}

/// A single font engine instance combining the glyph source (metrics, coverage bitmaps and
/// character mapping, see [`GlyphFace`]) and optional raw binary data for OpenType layout
/// (`rustybuzz`). Mirrors Qt's `QFontEngine`.
type ColorGlyphCache = Arc<std::sync::Mutex<std::collections::HashMap<(u16, u32), Option<(GlyphMetrics, Arc<tiny_skia::Pixmap>)>>>>;
type MonoGlyphCache = Arc<std::sync::Mutex<std::collections::HashMap<(u16, u32), (GlyphMetrics, Arc<[u8]>)>>>;
/// Upper bound on cached glyph bitmaps per engine (a CJK font could otherwise grow without limit).
const MONO_GLYPH_CACHE_LIMIT: usize = 4096;

#[derive(Clone)]
pub struct FontEngine {
    /// Glyph source for rasterization, metrics, and character-to-glyph mapping.
    pub face: SharedGlyphFace,
    /// Optional binary font file data for HarfBuzz / rustybuzz OpenType shaping.
    pub raw_data: Option<SharedFontData>,
    /// Font face index within a font collection (.ttc / .otc).
    pub face_index: u32,
    /// Cached color glyphs: (glyph_id, px_size_key) -> (Metrics, Arc<Pixmap>)
    color_cache: ColorGlyphCache,
    /// Cached monochrome glyph bitmaps: (glyph_id, px_size bits) -> (Metrics, coverage bitmap)
    mono_cache: MonoGlyphCache,
}

impl FontEngine {
    /// Creates a new engine backed by a glyph face.
    pub fn new(face: SharedGlyphFace) -> Self {
        Self {
            face,
            raw_data: None,
            face_index: 0,
            color_cache: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            mono_cache: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Sets the raw binary font data for OpenType shaping.
    pub fn with_raw_data(mut self, raw_data: impl Into<SharedFontData>) -> Self {
        self.raw_data = Some(raw_data.into());
        self
    }

    /// Sets the face index within a font collection file.
    pub fn with_face_index(mut self, face_index: u32) -> Self {
        self.face_index = face_index;
        self
    }
    /// Checks whether `glyph_id` is an OpenType color glyph with COLRv0 layers.
    pub fn is_color_glyph(&self, glyph_id: u16) -> bool {
        if let Some(raw) = &self.raw_data {
            crate::text::color_glyph::parse_colr_v0_layers(raw.as_slice(), self.face_index, glyph_id).is_some()
        } else {
            false
        }
    }

    /// Rasterizes an OpenType color glyph into a 32-bit premultiplied ARGB/RGBA `Pixmap`.
    ///
    /// Mirrors Qt6's `QWindowsFontEngineDirectWrite::alphaRGBMapForGlyph` / `renderColr0GlyphRun`.
    pub fn rasterize_color_glyph(
        &self,
        glyph_id: u16,
        px_size: f32,
    ) -> Option<(GlyphMetrics, Arc<tiny_skia::Pixmap>)> {
        let key = (glyph_id, (px_size * 64.0).round() as u32);
        if let Ok(guard) = self.color_cache.lock() {
            if let Some(entry) = guard.get(&key) {
                return entry.clone();
            }
        }

        let raw = self.raw_data.as_ref()?;
        let rendered = crate::text::color_glyph::rasterize_color_glyph(
            raw.as_slice(),
            self.face_index,
            glyph_id,
            self.face.as_ref(),
            px_size,
        ).map(|(m, p)| (m, Arc::new(p)));

        if let Ok(mut guard) = self.color_cache.lock() {
            guard.insert(key, rendered.clone());
        }

        rendered
    }

    /// Rasterizes a monochrome (alpha coverage) glyph, caching the bitmap per `(glyph, size)`.
    ///
    /// Mirrors Qt's `QFontEngineGlyphCache`: a glyph is rasterized once per size and then reused by
    /// every later paint. The bitmap is identical to the face's own `rasterize_indexed`.
    /// The cache is shared by every clone of this engine.
    pub fn rasterize_glyph(&self, glyph_id: u16, px_size: f32) -> (GlyphMetrics, Arc<[u8]>) {
        let key = (glyph_id, px_size.to_bits());
        if let Ok(guard) = self.mono_cache.lock() {
            if let Some((metrics, bitmap)) = guard.get(&key) {
                return (*metrics, bitmap.clone());
            }
        }
        let (metrics, bitmap) = self.face.rasterize_indexed(glyph_id, px_size);
        let bitmap: Arc<[u8]> = bitmap.into();
        if let Ok(mut guard) = self.mono_cache.lock() {
            if guard.len() >= MONO_GLYPH_CACHE_LIMIT {
                guard.clear();
            }
            guard.insert(key, (metrics, bitmap.clone()));
        }
        (metrics, bitmap)
    }
}

impl From<SharedGlyphFace> for FontEngine {
    fn from(face: SharedGlyphFace) -> Self {
        Self::new(face)
    }
}

/// A contiguous slice of text associated with a single font engine.
/// Mirrors Qt's item boundaries in `QTextEngine::itemBoundaries`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontRun<'a> {
    /// Text slice belonging to this run.
    pub text: &'a str,
    /// Index into the multi-engine list.
    pub engine_index: usize,
}

/// Shaped text layout result (`QGlyphLayout` / `QTextLayout` equivalent).
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphLayout {
    /// List of ordered glyphs and their relative positions.
    pub glyphs: Vec<PositionedGlyph>,
    /// Total advance width of shaped text.
    pub width: f32,
}

impl GlyphLayout {
    /// Creates an empty layout.
    pub fn empty() -> Self {
        Self {
            glyphs: Vec::new(),
            width: 0.0,
        }
    }

    /// Shapes and positions text using Qt 6 Run-based architecture:
    /// partitions text into font runs and uses `rustybuzz` with fallback to face metrics.
    pub fn shape(text: &str, font: &Font, face: SharedGlyphFace) -> Self {
        let mut engine = FontEngine::new(face).with_face_index(0);
        if let Some(raw) = &font.font_data {
            engine = engine.with_raw_data(raw.clone());
        }
        Self::shape_with_engines(text, font, &[engine])
    }

    /// Shapes text using rustybuzz (supporting tnum, kerning, and OpenType features).
    pub fn shape_with_rustybuzz(text: &str, font: &Font, rb_face: &rustybuzz::Face) -> Self {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);

        let mut features = Vec::new();
        if font.tabular_numbers {
            if let Ok(f) = rustybuzz::Feature::from_str("tnum") {
                features.push(f);
            }
        }
        if let Ok(f) = rustybuzz::Feature::from_str("kern") {
            features.push(f);
        }

        let glyph_buffer = rustybuzz::shape(rb_face, &features, buffer);

        let upem = rb_face.units_per_em() as f32;
        let scale = if upem > 0.0 { font.size / upem } else { 1.0 };

        let infos = glyph_buffer.glyph_infos();
        let positions = glyph_buffer.glyph_positions();

        let mut current_x = 0.0;
        let mut current_y = 0.0;
        let mut glyphs = Vec::with_capacity(infos.len());

        for (info, pos) in infos.iter().zip(positions.iter()) {
            glyphs.push(PositionedGlyph {
                glyph_id: info.glyph_id as u16,
                font_index: 0,
                x: current_x + (pos.x_offset as f32) * scale,
                y: current_y + (pos.y_offset as f32) * scale,
            });
            current_x += (pos.x_advance as f32) * scale;
            current_y += (pos.y_advance as f32) * scale;
        }

        Self {
            glyphs,
            width: current_x,
        }
    }

    /// Fallback shaping using per-glyph face metrics.
    pub fn shape_with_face(text: &str, font: &Font, face: SharedGlyphFace) -> Self {
        let engine = FontEngine::new(face);
        Self::shape_with_engines(text, font, &[engine])
    }

    /// True Qt `QFontEngineMulti` parity: shapes text using primary font engine,
    /// dynamically falling back to secondary font engines on a per-glyph basis.
    pub fn shape_with_faces(text: &str, font: &Font, faces: &[SharedGlyphFace]) -> Self {
        let engines: Vec<FontEngine> = faces
            .iter()
            .enumerate()
            .map(|(idx, f)| {
                let mut engine = FontEngine::new(Arc::clone(f));
                if idx == 0 {
                    if let Some(data) = &font.font_data {
                        engine.raw_data = Some(data.clone());
                    }
                }
                engine
            })
            .collect();

        Self::shape_with_engines(text, font, &engines)
    }

    /// Step 1 & 2 (Qt parity: `QFontEngineMulti::stringToCMap` + `QTextEngine::itemBoundaries`):
    /// Maps characters to font engines and partitions the string into contiguous runs sharing the same engine.
    pub fn partition_into_runs<'a>(text: &'a str, engines: &[FontEngine]) -> Vec<FontRun<'a>> {
        if text.is_empty() || engines.is_empty() {
            return Vec::new();
        }

        let primary = &engines[0].face;
        let mut char_engine_indices = Vec::with_capacity(text.len());
        let mut last_fallback_idx = 0usize;

        for ch in text.chars() {
            let mut resolved_idx = 0usize;
            let gid = primary.glyph_index(ch);

            // Qt parity: if primary engine has no glyph (0) and character is not whitespace/ignorable,
            // query fallback engines.
            if gid == 0 && ch != ' ' && ch != '\t' && ch != '\n' && ch != '\r' && ch != '\u{200b}' {
                let mut found = false;

                // Locality heuristic (Qt `lastFallback` in `stringToCMap`):
                // consecutive characters in multi-language text (CJK/Emoji) usually share the same font.
                if last_fallback_idx > 0
                    && last_fallback_idx < engines.len()
                    && engines[last_fallback_idx].face.glyph_index(ch) != 0
                {
                    resolved_idx = last_fallback_idx;
                    found = true;
                }

                if !found {
                    for (idx, fb_engine) in engines.iter().enumerate().skip(1) {
                        if idx == last_fallback_idx {
                            continue;
                        }
                        if fb_engine.face.glyph_index(ch) != 0 {
                            resolved_idx = idx;
                            last_fallback_idx = idx;
                            break;
                        }
                    }
                }
            }

            char_engine_indices.push((ch, resolved_idx));
        }

        // Qt parity (`QTextEngine::itemBoundaries`): group adjacent characters with the same engine into runs.
        let mut runs = Vec::new();
        let mut current_engine = char_engine_indices[0].1;
        let mut current_start = 0;
        let mut current_byte = 0;

        for &(ch, engine_idx) in &char_engine_indices {
            let ch_len = ch.len_utf8();
            if engine_idx != current_engine {
                runs.push(FontRun {
                    text: &text[current_start..current_byte],
                    engine_index: current_engine,
                });
                current_start = current_byte;
                current_engine = engine_idx;
            }
            current_byte += ch_len;
        }
        runs.push(FontRun {
            text: &text[current_start..text.len()],
            engine_index: current_engine,
        });

        runs
    }

    /// Full Qt 6 `QTextEngine::shapeTextWithHarfbuzzNG` parity:
    /// Shapes text across multiple font engines by partitioning into runs,
    /// batch-shaping each run with `rustybuzz` (with full OpenType GSUB/GPOS feature support),
    /// and falling back to the face's own per-glyph metrics when binary font data is unavailable.
    pub fn shape_with_engines(text: &str, font: &Font, engines: &[FontEngine]) -> Self {
        if text.is_empty() || engines.is_empty() {
            return Self::empty();
        }

        // Pre-parse rustybuzz faces once per engine (zero-copy table header parsing).
        let rb_faces: Vec<Option<rustybuzz::Face>> = engines
            .iter()
            .map(|e| {
                e.raw_data.as_ref().and_then(|data| {
                    rustybuzz::Face::from_slice(data.as_slice(), e.face_index)
                })
            })
            .collect();

        let runs = Self::partition_into_runs(text, engines);
        let mut glyphs = Vec::with_capacity(text.len());
        let mut current_x = 0.0;
        let mut current_y = 0.0;

        for run in runs {
            let engine = &engines[run.engine_index];
            let maybe_rb_face = rb_faces.get(run.engine_index).and_then(|f| f.as_ref());

            if let Some(rb_face) = maybe_rb_face {
                // Qt `shapeTextWithHarfbuzzNG` path:
                let mut buffer = rustybuzz::UnicodeBuffer::new();
                buffer.push_str(run.text);

                let mut features = Vec::new();
                if font.tabular_numbers {
                    if let Ok(f) = rustybuzz::Feature::from_str("tnum") {
                        features.push(f);
                    }
                }
                if let Ok(f) = rustybuzz::Feature::from_str("kern") {
                    features.push(f);
                }

                let glyph_buffer = rustybuzz::shape(rb_face, &features, buffer);
                let upem = rb_face.units_per_em() as f32;
                let scale = if upem > 0.0 { font.size / upem } else { 1.0 };

                let infos = glyph_buffer.glyph_infos();
                let positions = glyph_buffer.glyph_positions();

                for (info, pos) in infos.iter().zip(positions.iter()) {
                    glyphs.push(PositionedGlyph {
                        glyph_id: info.glyph_id as u16,
                        font_index: run.engine_index as u8,
                        x: current_x + (pos.x_offset as f32) * scale,
                        y: current_y + (pos.y_offset as f32) * scale,
                    });
                    current_x += (pos.x_advance as f32) * scale;
                    current_y += (pos.y_advance as f32) * scale;
                }
            } else {
                // Fallback path (per-glyph face metrics) for runs without raw binary font data:
                let tnum_width = if font.tabular_numbers {
                    Some(
                        engine
                            .face
                            .metrics('0', font.size)
                            .advance_width
                            .max(engine.face.metrics('8', font.size).advance_width),
                    )
                } else {
                    None
                };

                for ch in run.text.chars() {
                    let gid = engine.face.glyph_index(ch);
                    let metrics = engine.face.metrics(ch, font.size);
                    let adv_x = if ch.is_ascii_digit() && run.engine_index == 0 {
                        tnum_width.unwrap_or(metrics.advance_width)
                    } else {
                        metrics.advance_width
                    };

                    glyphs.push(PositionedGlyph {
                        glyph_id: gid,
                        font_index: run.engine_index as u8,
                        x: current_x,
                        y: current_y,
                    });
                    current_x += adv_x;
                }
            }
        }

        Self {
            glyphs,
            width: current_x,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::text::glyph_face::parse_face;

    fn get_test_font() -> (Arc<Vec<u8>>, SharedGlyphFace) {
        let path = std::path::Path::new("C:/Windows/Fonts/arial.ttf");
        let data = if path.exists() {
            std::fs::read(path).unwrap()
        } else {
            let p2 = std::path::Path::new("C:/Windows/Fonts/segoeui.ttf");
            if p2.exists() {
                std::fs::read(p2).unwrap()
            } else {
                vec![]
            }
        };

        let face = parse_face(&SharedFontData::from_vec(data.clone()), 0).unwrap();
        (Arc::new(data), face)
    }

    #[test]
    fn test_glyph_layout_fontdue_shaping() {
        let (data, font_face) = get_test_font();
        let font = Font::new("Arial", 16.0).with_font_data(data);

        let layout = GlyphLayout::shape("HUD 100%", &font, font_face);
        assert!(!layout.glyphs.is_empty());
        assert!(layout.width > 0.0);
        assert_eq!(layout.glyphs[0].x, 0.0);
        assert!(layout.glyphs[1].x > 0.0);
    }

    #[test]
    fn test_glyph_layout_rustybuzz_tnum() {
        let (data, font_face) = get_test_font();
        let font_tnum = Font::new("Arial", 16.0)
            .with_tabular_numbers(true)
            .with_font_data(data);

        let layout_1111 = GlyphLayout::shape("1111", &font_tnum, font_face.clone());
        let layout_8888 = GlyphLayout::shape("8888", &font_tnum, font_face);

        assert_eq!(layout_1111.glyphs.len(), 4);
        assert_eq!(layout_8888.glyphs.len(), 4);

        assert!((layout_1111.width - layout_8888.width).abs() < 1.0);
    }
    #[test]
    fn test_glyph_layout_emoji_fallback() {
        let (_data, font_face) = get_test_font();
        let font = Font::new("Arial", 16.0);
        let emoji_path = std::path::Path::new("C:/Windows/Fonts/seguiemj.ttf");
        if emoji_path.exists() {
            let emoji_data = std::fs::read(emoji_path).unwrap();
            let emoji_font = parse_face(&SharedFontData::from_vec(emoji_data), 0).unwrap();
            let fonts = vec![font_face, emoji_font];
            let layout = GlyphLayout::shape_with_faces("🔄 立即", &font, &fonts);
            assert_eq!(layout.glyphs[0].font_index, 1, "Emoji should resolve to fallback engine 1");
            let (metrics, bitmap) = fonts[1].rasterize_indexed(layout.glyphs[0].glyph_id, 16.0);
            assert!(metrics.width > 0);
            assert!(metrics.height > 0);
            assert!(!bitmap.is_empty());
        }
    }

    #[test]
    fn test_glyph_layout_empty_and_whitespace() {
        let (data, font_face) = get_test_font();
        let font = Font::new("Arial", 16.0).with_font_data(data);

        let layout_empty = GlyphLayout::shape("", &font, font_face.clone());
        assert!(layout_empty.glyphs.is_empty());
        assert_eq!(layout_empty.width, 0.0);

        let layout_space = GlyphLayout::shape("   ", &font, font_face);
        assert_eq!(layout_space.glyphs.len(), 3);
        assert!(layout_space.width > 0.0);
    }

    #[test]
    fn test_glyph_layout_partition_into_runs() {
        let (_data, font_face) = get_test_font();
        let emoji_path = std::path::Path::new("C:/Windows/Fonts/seguiemj.ttf");
        if emoji_path.exists() {
            let emoji_data = std::fs::read(emoji_path).unwrap();
            let emoji_font = parse_face(&SharedFontData::from_vec(emoji_data), 0).unwrap();
            let engines = vec![
                FontEngine::new(font_face),
                FontEngine::new(emoji_font),
            ];

            // "OK 🔄 OK" -> Run 0: "OK ", Run 1: "🔄", Run 2: " OK"
            let runs = GlyphLayout::partition_into_runs("OK 🔄 OK", &engines);
            assert_eq!(runs.len(), 3);
            assert_eq!(runs[0].text, "OK ");
            assert_eq!(runs[0].engine_index, 0);
            assert_eq!(runs[1].text, "🔄");
            assert_eq!(runs[1].engine_index, 1);
            assert_eq!(runs[2].text, " OK");
            assert_eq!(runs[2].engine_index, 0);
        }
    }

    #[test]
    fn test_glyph_layout_multi_engine_raw_opentype_shaping() {
        let (data, font_face) = get_test_font();
        let font = Font::new("Arial", 16.0);
        let emoji_path = std::path::Path::new("C:/Windows/Fonts/seguiemj.ttf");
        if emoji_path.exists() {
            let emoji_data = std::fs::read(emoji_path).unwrap();
            let emoji_font = parse_face(&SharedFontData::from_vec(emoji_data.clone()), 0).unwrap();
            let engines = vec![
                FontEngine::new(font_face).with_raw_data(data),
                FontEngine::new(emoji_font).with_raw_data(Arc::new(emoji_data)),
            ];

            let layout = GlyphLayout::shape_with_engines("Status: 🔄 Active", &font, &engines);
            assert!(!layout.glyphs.is_empty());
            assert!(layout.width > 0.0);

            // Verify that glyph coordinates advance monotonically
            for w in layout.glyphs.windows(2) {
                assert!(w[1].x >= w[0].x, "Glyph coordinates must advance monotonically");
            }

            // Locate the emoji glyph and verify it has engine_index = 1
            let emoji_glyph = layout.glyphs.iter().find(|g| g.font_index == 1);
            assert!(emoji_glyph.is_some(), "Emoji should have been shaped with engine 1");
        }
    }
}
