use std::sync::Arc;

/// Shared binary font data representation supporting zero-copy slicing and deduplication.
///
/// Encapsulates binary font data behind an `Arc`, ensuring multiple `FontEngine`
/// or `Font` instances share the exact same underlying byte buffer without duplication.
#[derive(Debug, Clone)]
pub struct SharedFontData {
    data: Arc<Vec<u8>>,
}

impl SharedFontData {
    /// Creates a new `SharedFontData` from an owned byte vector.
    pub fn from_vec(vec: Vec<u8>) -> Self {
        Self {
            data: Arc::new(vec),
        }
    }

    /// Creates a `SharedFontData` wrapping an existing `Arc<Vec<u8>>`.
    pub fn from_arc(data: Arc<Vec<u8>>) -> Self {
        Self { data }
    }

    /// Returns the underlying byte slice.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        self.data.as_slice()
    }

    /// Returns the inner `Arc<Vec<u8>>` for backwards compatibility.
    #[inline]
    pub fn to_arc(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.data)
    }

    /// Returns the number of bytes in the font data.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the font data is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Returns the strong reference count of this shared font buffer.
    #[inline]
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.data)
    }

    /// Checks if two `SharedFontData` share the exact same underlying memory buffer.
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }
}

impl std::ops::Deref for SharedFontData {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.data.as_slice()
    }
}

impl AsRef<[u8]> for SharedFontData {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.data.as_slice()
    }
}

impl PartialEq for SharedFontData {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data) || *self.data == *other.data
    }
}

impl From<Vec<u8>> for SharedFontData {
    fn from(v: Vec<u8>) -> Self {
        Self::from_vec(v)
    }
}

impl From<Arc<Vec<u8>>> for SharedFontData {
    fn from(a: Arc<Vec<u8>>) -> Self {
        Self::from_arc(a)
    }
}
/// Font weight values matching Qt `QFont::Weight` and CSS standard numeric values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FontWeight {
    Thin = 100,
    Light = 300,
    #[default]
    Normal = 400,
    Medium = 500,
    SemiBold = 600,
    Bold = 700,
    Black = 900,
}

/// Font style matching Qt `QFont::Style`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

/// Font attribute descriptor matching Qt `QFont`.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// Font family name (e.g. "Segoe UI", "Arial", "Consolas").
    pub family: String,
    /// Font size in points or pixels.
    pub size: f32,
    /// Font weight.
    pub weight: FontWeight,
    /// Font style.
    pub style: FontStyle,
    /// Enables OpenType 'tnum' (tabular numbers) feature.
    pub tabular_numbers: bool,
    /// In-memory binary font data.
    pub font_data: Option<SharedFontData>,
}

impl Font {
    /// Creates a basic font descriptor.
    pub fn new(family: impl Into<String>, size: f32) -> Self {
        Self {
            family: family.into(),
            size,
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
            tabular_numbers: false,
            font_data: None,
        }
    }
    #[inline]
    pub fn family(&self) -> &str {
        &self.family
    }

    #[inline]
    pub fn size(&self) -> f32 {
        self.size
    }

    #[inline]
    pub fn weight(&self) -> FontWeight {
        self.weight
    }

    #[inline]
    pub fn style(&self) -> FontStyle {
        self.style
    }

    #[inline]
    pub fn set_family(&mut self, family: impl Into<String>) {
        self.family = family.into();
    }

    #[inline]
    pub fn set_size(&mut self, size: f32) {
        self.size = size;
    }

    #[inline]
    pub fn set_weight(&mut self, weight: FontWeight) {
        self.weight = weight;
    }

    #[inline]
    pub fn set_style(&mut self, style: FontStyle) {
        self.style = style;
    }

    /// Sets the font weight.
    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Sets the font style.
    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets whether tabular numbers ('tnum') are enabled.
    pub fn with_tabular_numbers(mut self, enabled: bool) -> Self {
        self.tabular_numbers = enabled;
        self
    }

    /// Sets in-memory binary font data.
    pub fn with_font_data(mut self, font_data: impl Into<SharedFontData>) -> Self {
        self.font_data = Some(font_data.into());
        self
    }
}

impl Default for Font {
    fn default() -> Self {
        Self::new("Segoe UI", 12.0)
    }
}

/// Cascading font description matching Qt style inheritance model.
///
/// Fields with `None` inherit their properties from the parent font.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FontDescription {
    pub family: Option<String>,
    pub size: Option<f32>,
    pub weight: Option<FontWeight>,
    pub style: Option<FontStyle>,
    pub tabular_numbers: Option<bool>,
}

impl FontDescription {
    /// Creates an empty font description (inherits all attributes).
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the font family name.
    pub fn with_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }

    /// Sets the font size.
    pub fn with_size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Sets the font weight.
    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Sets the font style.
    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Sets whether tabular numbers are enabled.
    pub fn with_tabular_numbers(mut self, enabled: bool) -> Self {
        self.tabular_numbers = Some(enabled);
        self
    }

    /// Merges this description with parent font, overriding specified properties.
    pub fn merge_with_parent(&self, parent: &Font) -> Font {
        Font {
            family: self.family.clone().unwrap_or_else(|| parent.family.clone()),
            size: self.size.unwrap_or(parent.size),
            weight: self.weight.unwrap_or(parent.weight),
            style: self.style.unwrap_or(parent.style),
            tabular_numbers: self.tabular_numbers.unwrap_or(parent.tabular_numbers),
            font_data: parent.font_data.clone(),
        }
    }
}

impl From<&Font> for FontDescription {
    fn from(font: &Font) -> Self {
        Self {
            family: Some(font.family.clone()),
            size: Some(font.size),
            weight: Some(font.weight),
            style: Some(font.style),
            tabular_numbers: Some(font.tabular_numbers),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_creation_and_builder() {
        let font = Font::new("Consolas", 14.0)
            .with_weight(FontWeight::Bold)
            .with_style(FontStyle::Italic)
            .with_tabular_numbers(true);

        assert_eq!(font.family, "Consolas");
        assert_eq!(font.size, 14.0);
        assert_eq!(font.weight, FontWeight::Bold);
        assert_eq!(font.style, FontStyle::Italic);
        assert!(font.tabular_numbers);
        assert!(font.font_data.is_none());
    }

    #[test]
    fn test_font_with_memory_data() {
        let fake_data = Arc::new(vec![0x00, 0x01, 0x00, 0x00]);
        let font = Font::new("CustomFont", 16.0).with_font_data(fake_data.clone());
        assert_eq!(
            font.font_data.as_ref().unwrap().as_slice(),
            &[0x00, 0x01, 0x00, 0x00]
        );
    }

    #[test]
    fn test_font_description_cascading_merge() {
        let parent = Font::new("Segoe UI", 12.0)
            .with_weight(FontWeight::Normal)
            .with_style(FontStyle::Normal)
            .with_tabular_numbers(false);

        // 1. None inherits parent
        let desc_empty = FontDescription::new();
        let merged_empty = desc_empty.merge_with_parent(&parent);
        assert_eq!(merged_empty, parent);

        // 2. Partial override
        let desc_partial = FontDescription::new()
            .with_size(18.0)
            .with_weight(FontWeight::Bold);
        let merged_partial = desc_partial.merge_with_parent(&parent);

        assert_eq!(merged_partial.family, "Segoe UI");
        assert_eq!(merged_partial.size, 18.0);
        assert_eq!(merged_partial.weight, FontWeight::Bold);
        assert_eq!(merged_partial.style, FontStyle::Normal);
        assert!(!merged_partial.tabular_numbers);

        // 3. Full override
        let desc_full = FontDescription::new()
            .with_family("Consolas")
            .with_size(24.0)
            .with_weight(FontWeight::Black)
            .with_style(FontStyle::Italic)
            .with_tabular_numbers(true);
        let merged_full = desc_full.merge_with_parent(&parent);

        assert_eq!(merged_full.family, "Consolas");
        assert_eq!(merged_full.size, 24.0);
        assert_eq!(merged_full.weight, FontWeight::Black);
        assert_eq!(merged_full.style, FontStyle::Italic);
        assert!(merged_full.tabular_numbers);
    }
}
