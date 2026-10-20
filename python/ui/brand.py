"""Identity colours per AI service.

These mark WHO a tile/card is (name, accent stripe, focus ring, drag line). Usage state stays on the
green / amber / red scale so a brand colour is never mistaken for "how full is it".
Light variants are darkened until the name reads at >= 4.5:1 on the light panel backgrounds.
"""

BRAND = {
    "claude": {"dark": "#D97757", "light": "#B4532F"},   # Anthropic terracotta
    "agy": {"dark": "#8AB4F8", "light": "#1967D2"},      # Google blue (Antigravity / Gemini)
    "codex": {"dark": "#10A37F", "light": "#0B7A5E"},    # OpenAI green
}
FALLBACK = {"dark": "#38bdf8", "light": "#0369a1"}


def brand_color(provider_id: str, dark: bool) -> str:
    return BRAND.get(provider_id, FALLBACK)["dark" if dark else "light"]
