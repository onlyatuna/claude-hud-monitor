//! Identity colours per AI service (Python `ui/brand.py`).
//!
//! These mark WHO a tile/card is (name, accent stripe, focus ring, drag line). Usage state stays on the
//! green / amber / red scale so a brand colour is never mistaken for "how full is it".
//! Light variants are darkened until the name reads at >= 4.5:1 on the light panel backgrounds.

use ferrokit_gui::tiny_skia::Color;

/// `(provider id, dark colour, light colour)`.
pub const BRAND: [(&str, &str, &str); 3] = [
    ("claude", "#D97757", "#B4532F"), // Anthropic terracotta
    ("agy", "#8AB4F8", "#1967D2"),    // Google blue (Antigravity / Gemini)
    ("codex", "#10A37F", "#0B7A5E"),  // OpenAI green
];
pub const FALLBACK: (&str, &str) = ("#38bdf8", "#0369a1");

pub fn brand_hex(provider_id: &str, dark: bool) -> &'static str {
    let (d, l) = BRAND
        .iter()
        .find(|(id, _, _)| *id == provider_id)
        .map_or(FALLBACK, |(_, d, l)| (*d, *l));
    if dark {
        d
    } else {
        l
    }
}

pub fn brand_color(provider_id: &str, dark: bool) -> Color {
    let hex = brand_hex(provider_id, dark);
    let channel = |i: usize| u8::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).unwrap_or(0);
    Color::from_rgba8(channel(0), channel(1), channel(2), 255)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_service_has_a_colour_per_theme() {
        assert_eq!(BRAND.len(), 3);
        assert_eq!(brand_hex("claude", true), "#D97757");
        assert_eq!(brand_hex("claude", false), "#B4532F");
        assert_eq!(brand_hex("unknown", true), "#38bdf8");
        assert_eq!(brand_hex("unknown", false), "#0369a1");
    }

    #[test]
    fn hex_is_parsed_into_a_colour() {
        let c = brand_color("codex", true).to_color_u8();
        assert_eq!((c.red(), c.green(), c.blue(), c.alpha()), (0x10, 0xA3, 0x7F, 255));
    }

    #[test]
    fn brand_colours_differ_from_each_other() {
        for dark in [true, false] {
            let mut hexes: Vec<_> = BRAND.iter().map(|(id, _, _)| brand_hex(id, dark)).collect();
            hexes.sort_unstable();
            hexes.dedup();
            assert_eq!(hexes.len(), 3);
        }
    }
}
