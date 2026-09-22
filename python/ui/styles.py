"""
HUD themes and stylesheets.

Frosted-glass look modelled on macOS utility panels: no boxed cells, hairline
section separators, one quiet accent. Two appearances (light / dark) × two
colour schemes:
  "scale" - muted green / yellow / orange / red by usage (see core.pace.SCALE_THRESHOLDS)
  "duo"   - fixed periwinkle pair: inner pie = 5h window, outer ring = weekly window
"""

SCHEMES = ("scale", "duo")
APPEARANCES = ("auto", "light", "dark")

THEMES = {
    "light": {
        # "panel" sits on native vibrancy; "panel_solid" is used where no blur is available
        "panel": "rgba(246, 244, 250, 0.50)", "panel_solid": "rgba(246, 244, 250, 0.94)",
        "panel_border": "rgba(255, 255, 255, 0.55)", "radius": 12,
        "text": "#1f1f24", "text2": "rgba(40, 40, 50, 0.62)", "text3": "rgba(40, 40, 50, 0.34)",
        "neutral": "#8C8C99",
        "separator": "rgba(40, 40, 50, 0.14)",
        "track": (40, 40, 60, 26), "disc": (40, 40, 60, 14),
        "halo": (246, 244, 250, 190), "hatch": (30, 30, 60, 85),
        "duo": {"inner": "#8391D2", "outer": "#5563AE"},
        "duo_text": {"inner": "#6070BE", "outer": "#4655A3"},
        "scale": {"green": "#78B08A", "yellow": "#D8B85A", "orange": "#DC9461", "red": "#D46868"},
        # Darker text variants keep contrast on light glass
        "scale_text": {"green": "#3F7A52", "yellow": "#8C6E12", "orange": "#A95A22", "red": "#B03C3C"},
        "menu_bg": "rgba(250, 250, 252, 0.97)", "menu_hover": "rgba(0, 0, 0, 0.07)",
    },
    "dark": {
        "panel": "rgba(34, 34, 40, 0.55)", "panel_solid": "rgba(30, 30, 36, 0.94)",
        "panel_border": "rgba(255, 255, 255, 0.12)", "radius": 12,
        "text": "#f2f2f7", "text2": "rgba(235, 235, 245, 0.62)", "text3": "rgba(235, 235, 245, 0.32)",
        "neutral": "#A6A6B3",
        "separator": "rgba(255, 255, 255, 0.12)",
        "track": (255, 255, 255, 28), "disc": (255, 255, 255, 14),
        "halo": (30, 30, 36, 170), "hatch": (0, 0, 0, 110),
        "duo": {"inner": "#9AA6E4", "outer": "#6F7CC8"},
        "duo_text": {"inner": "#B4BDF0", "outer": "#AEB8F2"},
        "scale": {"green": "#8CC79C", "yellow": "#E3C66A", "orange": "#E8A574", "red": "#E07B7B"},
        "scale_text": {"green": "#8CC79C", "yellow": "#E3C66A", "orange": "#E8A574", "red": "#E07B7B"},
        "menu_bg": "rgba(40, 40, 46, 0.97)", "menu_hover": "rgba(255, 255, 255, 0.10)",
    },
}


def get_hud_stylesheet(theme: dict, vibrant: bool) -> str:
    panel = theme["panel"] if vibrant else theme["panel_solid"]
    return f"""
    QWidget#CentralWidget {{
        background-color: {panel};
        border: 1px solid {theme['panel_border']};
        border-radius: {theme['radius']}px;
    }}
    QLabel {{ color: {theme['text']}; }}
    QLabel#HeaderTitle {{ font-size: 12px; font-weight: 600; color: {theme['text2']}; }}
    QLabel#HeaderStatus {{ font-size: 11px; color: {theme['text2']}; }}
    QMenu {{
        background-color: {theme['menu_bg']};
        border: 1px solid {theme['separator']};
        border-radius: 8px;
        padding: 5px 0px;
    }}
    QMenu::item {{ color: {theme['text']}; padding: 5px 26px 5px 22px; font-size: 12px; }}
    QMenu::item:selected {{ background-color: {theme['menu_hover']}; }}
    QMenu::item:disabled {{ color: {theme['text3']}; }}
    QMenu::separator {{ height: 1px; background-color: {theme['separator']}; margin: 5px 10px; }}
    """


def get_table_stylesheet(theme: dict) -> str:
    return f"""
    QLabel {{ color: {theme['text']}; }}
    QLabel#SectionTitle {{ font-size: 13px; font-weight: 600; }}
    QLabel#RowLabel {{ color: {theme['text2']}; font-size: 12px; padding-left: 18px; }}
    QLabel#Legend {{ color: {theme['text2']}; font-size: 10px; }}
    QLabel#Cell {{ font-size: 14px; padding: 0px 2px; }}
    QLabel#Pill {{ font-size: 15px; font-weight: 600; padding: 0px 2px; }}
    QLabel#HeaderName {{ font-size: 14px; font-weight: 600; }}
    QLabel#HeaderBadge {{ color: {theme['text3']}; font-size: 9.5px; font-weight: 600; letter-spacing: 0.6px; }}
    QFrame#Separator {{ background-color: {theme['separator']}; border: none; min-height: 1px; max-height: 1px; }}
    QLabel[state="muted"] {{ color: {theme['text3']}; }}
    """
