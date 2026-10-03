//! Windows glyph backend: DirectWrite rasterizes the glyphs, as Qt's
//! `QWindowsFontEngineDirectWrite` does (qwindowsfontenginedirectwrite.cpp `imageForGlyph` /
//! `renderGlyphRun`), so text looks like the Python/Qt build.
//!
//! What is copied from Qt:
//! * one glyph per `IDWriteGlyphRunAnalysis`, at the origin, em size = the *logical* pixel size and
//!   the device pixel ratio applied as the run's transform;
//! * the rendering mode Qt derives from its default hinting preference (`determineHinting` /
//!   `hintingPreferenceToRenderingMode`): `NATURAL` up to 16 px, `NATURAL_SYMMETRIC` above, with
//!   grid fit default and ClearType antialiasing;
//! * the ClearType 3x1 texture is reduced to one alpha value per pixel by averaging the sub-pixels.
//!
//! Checked against Qt (PySide6) glyph by glyph at 125%, 150% and 200% scaling and 12-24 px
//! (`tools/second_layer_harness/qt_glyph_compare.py`): identical placement and bitmap size, mean
//! coverage difference about 0.5/255, at most 4.
//!
//! The font stays plain bytes in our memory (`SharedFontData`, handed to DirectWrite through an
//! in-memory font file loader); character mapping and advances come from [`OutlineFace`], which
//! reads them straight from the file. Only the coverage bitmap comes from DirectWrite.

use crate::text::font::SharedFontData;
use crate::text::glyph_face::{GlyphFace, GlyphMetrics};
use crate::text::outline_face::OutlineFace;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::sync::LazyLock;
use windows::core::{Interface, BOOL};
use windows::Win32::Graphics::DirectWrite::*;

/// Process-wide DirectWrite objects. `None` when DirectWrite (or the in-memory loader, which needs
/// Windows 10 1703) is unavailable; callers then use [`OutlineFace`].
struct Factory {
    base: IDWriteFactory,
    factory2: IDWriteFactory2,
    loader: IDWriteInMemoryFontFileLoader,
}

// SAFETY: the shared DirectWrite factory and the objects created from it are free-threaded.
unsafe impl Send for Factory {}
unsafe impl Sync for Factory {}

static FACTORY: LazyLock<Option<Factory>> = LazyLock::new(|| unsafe {
    let factory5: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
    let loader = factory5.CreateInMemoryFontFileLoader().ok()?;
    let loader_base: IDWriteFontFileLoader = loader.cast().ok()?;
    factory5.RegisterFontFileLoader(&loader_base).ok()?;
    let base: IDWriteFactory = factory5.cast().ok()?;
    let factory2: IDWriteFactory2 = factory5.cast().ok()?;
    Some(Factory {
        base,
        factory2,
        loader,
    })
});

/// Whether DirectWrite can be used on this machine.
pub fn is_available() -> bool {
    FACTORY.is_some()
}

/// A font face rasterized by DirectWrite.
pub struct DirectWriteFace {
    // Field order matters: DirectWrite's font face reads `base`'s font bytes, so it is dropped first.
    face: IDWriteFontFace,
    base: OutlineFace,
}

// SAFETY: `IDWriteFontFace` is free-threaded, and `OutlineFace` is plain immutable data.
unsafe impl Send for DirectWriteFace {}
unsafe impl Sync for DirectWriteFace {}

impl DirectWriteFace {
    /// Opens face `face_index` of the font file `data` with DirectWrite.
    pub fn new(data: SharedFontData, face_index: u32) -> Result<Self, String> {
        let base = OutlineFace::new(data.clone(), face_index)?;
        let factory = FACTORY.as_ref().ok_or("DirectWrite is unavailable")?;
        let face = unsafe {
            // The bytes stay valid for as long as `base` (which owns a clone of `data`) lives.
            let file = factory
                .loader
                .CreateInMemoryFontFileReference(
                    &factory.base,
                    data.as_slice().as_ptr() as *const c_void,
                    data.len() as u32,
                    None::<&windows::core::IUnknown>,
                )
                .map_err(|e| format!("CreateInMemoryFontFileReference: {e}"))?;

            let mut supported = BOOL(0);
            let mut file_type = DWRITE_FONT_FILE_TYPE_UNKNOWN;
            let mut face_type = DWRITE_FONT_FACE_TYPE_UNKNOWN;
            let mut faces = 0u32;
            file.Analyze(&mut supported, &mut file_type, Some(&mut face_type), &mut faces)
                .map_err(|e| format!("IDWriteFontFile::Analyze: {e}"))?;
            if !supported.as_bool() || face_index >= faces {
                return Err("DirectWrite does not support this font".to_string());
            }
            factory
                .base
                .CreateFontFace(face_type, &[Some(file)], face_index, DWRITE_FONT_SIMULATIONS_NONE)
                .map_err(|e| format!("CreateFontFace: {e}"))?
        };
        Ok(Self { face, base })
    }

    /// Qt's `determineHinting` + `hintingPreferenceToRenderingMode` for default hinting, plus the
    /// measuring mode from `renderModeToMeasureMode`.
    ///
    /// With a device pixel ratio other than 1, Qt's default hinting resolves to vertical hinting up
    /// to 16 px (`NATURAL`, "asymmetric") and none above (`NATURAL_SYMMETRIC`), the sizes Microsoft
    /// recommends. At ratio 1 Qt does not use DirectWrite by default (its GDI engine draws instead),
    /// so there is nothing to copy: `GDI_CLASSIC` up to 16 px is the DirectWrite mode that imitates
    /// GDI. That branch has not been compared against Qt (this machine runs at 125%).
    fn rendering_mode(size: f32, scale: f32) -> (DWRITE_RENDERING_MODE, DWRITE_MEASURING_MODE) {
        let scaled = (scale - 1.0).abs() > f32::EPSILON;
        match (scaled, size > 16.0) {
            (true, false) => (DWRITE_RENDERING_MODE_NATURAL, DWRITE_MEASURING_MODE_NATURAL),
            (_, true) => (DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC, DWRITE_MEASURING_MODE_NATURAL),
            (false, false) => (DWRITE_RENDERING_MODE_GDI_CLASSIC, DWRITE_MEASURING_MODE_GDI_CLASSIC),
        }
    }

    /// Qt's `imageForGlyph` + `alphaMapForGlyph`. `None` when DirectWrite fails for this glyph.
    fn rasterize_directwrite(&self, glyph_id: u16, size: f32, scale: f32) -> Option<(GlyphMetrics, Vec<u8>)> {
        let factory = FACTORY.as_ref()?;
        let advance = self.base.metrics_indexed(glyph_id, size * scale);
        let blank = GlyphMetrics {
            advance_width: advance.advance_width,
            advance_height: advance.advance_height,
            ..GlyphMetrics::default()
        };

        let glyph_index = glyph_id;
        let glyph_advance = 0.0f32;
        let glyph_offset = DWRITE_GLYPH_OFFSET::default();
        let mut run = DWRITE_GLYPH_RUN {
            fontFace: ManuallyDrop::new(Some(self.face.clone())),
            fontEmSize: size,
            glyphCount: 1,
            glyphIndices: &glyph_index,
            glyphAdvances: &glyph_advance,
            glyphOffsets: &glyph_offset,
            isSideways: BOOL(0),
            bidiLevel: 0,
        };
        let transform = DWRITE_MATRIX {
            m11: scale,
            m12: 0.0,
            m21: 0.0,
            m22: scale,
            dx: 0.0,
            dy: 0.0,
        };
        let (mode, measuring) = Self::rendering_mode(size, scale);
        let analysis = unsafe {
            factory.factory2.CreateGlyphRunAnalysis(
                &run,
                Some(&transform),
                mode,
                measuring,
                DWRITE_GRID_FIT_MODE_DEFAULT,
                DWRITE_TEXT_ANTIALIAS_MODE_CLEARTYPE,
                0.0,
                0.0,
            )
        };
        // Release the reference taken for the run now that the analysis owns what it needs.
        drop(unsafe { ManuallyDrop::take(&mut run.fontFace) });
        let analysis = analysis.ok()?;

        let bounds = unsafe { analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_CLEARTYPE_3x1) }.ok()?;
        let (width, height) = ((bounds.right - bounds.left).max(0) as usize, (bounds.bottom - bounds.top).max(0) as usize);
        if width == 0 || height == 0 {
            return Some((blank, Vec::new()));
        }

        let mut texture = vec![0u8; width * height * 3];
        unsafe { analysis.CreateAlphaTexture(DWRITE_TEXTURE_CLEARTYPE_3x1, &bounds, &mut texture) }.ok()?;

        // Qt's destination-with-alpha branch of `renderGlyphRun`: the three ClearType sub-pixel
        // coverages are averaged (rounded) into one alpha value. (Measured against Qt output this
        // matches to within 3/255; `qGray` weights, as used for opaque destinations, do not.)
        let bitmap = texture
            .chunks_exact(3)
            .map(|p| ((p[0] as u32 + p[1] as u32 + p[2] as u32 + 1) / 3) as u8)
            .collect();
        // Texture bounds are relative to the glyph origin on the baseline, rows growing downwards;
        // `ymin` is the bitmap's bottom edge above the baseline.
        let metrics = GlyphMetrics {
            xmin: bounds.left,
            ymin: -bounds.bottom,
            width,
            height,
            ..blank
        };
        Some((metrics, bitmap))
    }
}

impl GlyphFace for DirectWriteFace {
    fn glyph_index(&self, ch: char) -> u16 {
        self.base.glyph_index(ch)
    }

    /// Outline metrics (advance widths are unhinted, as in Qt's natural layout). The placement of
    /// the DirectWrite bitmap itself is returned by the rasterize methods.
    fn metrics_indexed(&self, glyph_id: u16, px: f32) -> GlyphMetrics {
        self.base.metrics_indexed(glyph_id, px)
    }

    fn rasterize_indexed(&self, glyph_id: u16, px: f32) -> (GlyphMetrics, Vec<u8>) {
        self.rasterize_scaled(glyph_id, px, 1.0)
    }

    fn rasterize_scaled(&self, glyph_id: u16, size: f32, scale: f32) -> (GlyphMetrics, Vec<u8>) {
        if size <= 0.0 || scale <= 0.0 {
            return (GlyphMetrics::default(), Vec::new());
        }
        self.rasterize_directwrite(glyph_id, size, scale)
            .unwrap_or_else(|| self.base.rasterize_indexed(glyph_id, size * scale))
    }
}
