use crate::geometry::primitives::RectF;
use crate::text::font::Font;

/// Font metrics engine (`QFontMetrics` / `QFontMetricsF` equivalent).
///
/// Used for text layout, centering, bounding box measurement, and eliding.
#[derive(Debug, Clone, PartialEq)]
pub struct FontMetrics {
    /// Distance from baseline to highest point of glyphs.
    pub ascent: f32,
    /// Distance from baseline to lowest point of glyphs.
    pub descent: f32,
    /// Leading / line gap.
    pub line_gap: f32,
    /// Total line height = ascent + descent + line_gap.
    pub height: f32,
    /// Average character advance width.
    pub average_char_width: f32,
}

impl FontMetrics {
    /// Creates a new `FontMetrics` with explicit values.
    pub fn new(ascent: f32, descent: f32, line_gap: f32, average_char_width: f32) -> Self {
        let height = ascent + descent + line_gap;
        Self {
            ascent,
            descent,
            line_gap,
            height,
            average_char_width,
        }
    }

    /// Derives standard font metrics from a `Font` instance.
    pub fn from_font(font: &Font) -> Self {
        let size = font.size;
        let ascent = size * 0.8;
        let descent = size * 0.2;
        let line_gap = size * 0.1;
        let avg_width = size * 0.6;
        Self::new(ascent, descent, line_gap, avg_width)
    }
    /// Interline leading spacing (`QFontMetrics::leading`).
    #[inline]
    pub fn leading(&self) -> f32 {
        self.line_gap
    }

    /// Calculates the horizontal advance width of a string (`QFontMetricsF::horizontalAdvance`).
    pub fn horizontal_advance(&self, text: &str, font: &Font) -> f32 {
        let mut total_width = 0.0;

        for ch in text.chars() {
            let w = if font.tabular_numbers && ch.is_ascii_digit() {
                self.average_char_width * 1.05
            } else {
                match ch {
                    ' ' => self.average_char_width * 0.5,
                    '.' | ',' | ':' | ';' | '!' | '|' | '\'' | '`' => {
                        self.average_char_width * 0.35
                    }
                    'i' | 'l' | 'j' | 'I' | 't' => self.average_char_width * 0.45,
                    'w' | 'm' | 'W' | 'M' => self.average_char_width * 1.3,
                    ch if ch.is_ascii() => self.average_char_width,
                    _ => self.average_char_width * 1.8,
                }
            };
            total_width += w;
        }

        total_width
    }

    /// Calculates the bounding rectangle of a string (`QFontMetricsF::boundingRect`).
    pub fn bounding_rect(&self, text: &str, font: &Font) -> RectF {
        let width = self.horizontal_advance(text, font);
        RectF {
            x: 0.0,
            y: -self.ascent,
            width,
            height: self.height,
        }
    }

    /// Returns an elided string ending with "..." if text exceeds max_width (`QFontMetricsF::elidedText`).
    pub fn elided_text(&self, text: &str, max_width: f32, font: &Font) -> String {
        if self.horizontal_advance(text, font) <= max_width {
            return text.to_string();
        }
        let ellipsis = "...";
        let ellipsis_width = self.horizontal_advance(ellipsis, font);
        if ellipsis_width >= max_width {
            return String::new();
        }

        let mut result = String::new();
        for ch in text.chars() {
            let mut candidate = result.clone();
            candidate.push(ch);
            if self.horizontal_advance(&candidate, font) + ellipsis_width > max_width {
                break;
            }
            result.push(ch);
        }
        result.push_str(ellipsis);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::font::Font;

    #[test]
    fn test_font_metrics_basic_properties() {
        let font = Font::new("Segoe UI", 20.0);
        let metrics = FontMetrics::from_font(&font);

        assert_eq!(metrics.ascent, 16.0);
        assert_eq!(metrics.descent, 4.0);
        assert_eq!(metrics.line_gap, 2.0);
        assert_eq!(metrics.height, 22.0);
    }

    #[test]
    fn test_font_metrics_bounding_rect() {
        let font = Font::new("Segoe UI", 10.0);
        let metrics = FontMetrics::from_font(&font);
        let rect = metrics.bounding_rect("Hello", &font);

        assert_eq!(rect.x, 0.0);
        assert_eq!(rect.y, -metrics.ascent);
        assert_eq!(rect.height, metrics.height);
        assert!(rect.width > 0.0);
    }

    #[test]
    fn test_tabular_numbers_stability() {
        let font_tnum = Font::new("Consolas", 12.0).with_tabular_numbers(true);
        let metrics = FontMetrics::from_font(&font_tnum);

        let w1 = metrics.horizontal_advance("1111", &font_tnum);
        let w2 = metrics.horizontal_advance("8888", &font_tnum);
        assert_eq!(
            w1, w2,
            "Tabular numbers advance for 1111 and 8888 must match"
        );
    }

    #[test]
    fn test_elided_text() {
        let font = Font::new("Segoe UI", 12.0);
        let metrics = FontMetrics::from_font(&font);
        let full_text = "Claude HUD Monitor Real-Time Status Notification";

        let elided = metrics.elided_text(full_text, 100.0, &font);
        assert!(elided.ends_with("..."));
        assert!(metrics.horizontal_advance(&elided, &font) <= 100.0);

        let short_text = "OK";
        assert_eq!(metrics.elided_text(short_text, 200.0, &font), "OK");
    }
}
