"""
HUD themes and stylesheets supporting both Classic Cards and Modern Table layouts.
"""

# ==================== Common / Card Colors ====================

def get_progress_color(percent: float) -> str:
    if percent >= 90:
        return "#ef4444"  # Neon Red
    elif percent >= 75:
        return "#f59e0b"  # Amber Warning
    elif percent >= 50:
        return "#3b82f6"  # Tech Blue
    else:
        return "#10b981"  # GeForce Emerald Green


# ==================== Table Theme Definitions ====================

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


def get_cards_stylesheet(vibrant: bool = False) -> str:
    """Classic Cards layout stylesheet (NVIDIA / RivaTuner Aesthetic)."""
    bg = "rgba(14, 17, 23, 0.94)"
    template = """
    QWidget#CentralWidget {
        background-color: __BG__;
        border: 1px solid rgba(255, 255, 255, 0.14);
        border-radius: 9px;
    }
    
    QLabel {
        color: #e2e8f0;
        font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif;
    }
    
    QLabel#HeaderTitle {
        font-size: 10.5px;
        font-weight: 800;
        letter-spacing: 1.0px;
        color: #94a3b8;
    }
    
    QLabel#HeaderStatus {
        font-size: 9.5px;
        color: #64748b;
        font-family: 'Consolas', monospace;
    }
    
    QLabel#MetricTitle {
        font-size: 10px;
        font-weight: 700;
        color: #94a3b8;
        letter-spacing: 0.6px;
    }
    
    QLabel#MetricValue {
        font-size: 16px;
        font-weight: 800;
        font-family: 'Consolas', 'Courier New', monospace;
    }
    
    QLabel#SubDetail {
        font-size: 9.5px;
        color: #64748b;
    }
    
    QProgressBar {
        background-color: rgba(255, 255, 255, 0.08);
        border: none;
        border-radius: 3px;
        text-align: right;
        min-height: 5px;
        max-height: 5px;
    }
    
    QProgressBar::chunk {
        border-radius: 3px;
    }
    
    QLabel#Badge {
        background-color: rgba(255, 255, 255, 0.06);
        border: 1px solid rgba(255, 255, 255, 0.08);
        border-radius: 3px;
        padding: 1px 4px;
        font-size: 9px;
        color: #cbd5e1;
        font-family: 'Consolas', monospace;
    }
    
    QPushButton#LayoutToggleBtn {
        background-color: transparent;
        border: 1px solid rgba(255, 255, 255, 0.12);
        border-radius: 4px;
        color: #94a3b8;
        font-size: 11px;
        padding: 1px 4px;
        min-width: 18px;
        max-height: 18px;
    }
    
    QPushButton#LayoutToggleBtn:hover {
        background-color: rgba(255, 255, 255, 0.12);
        color: #38bdf8;
        border-color: #38bdf8;
    }
    
    QFrame#Divider {
        background-color: rgba(255, 255, 255, 0.12);
        max-width: 1px;
        min-width: 1px;
    }
    
    QMenu {
        background-color: #161920;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 6px;
        padding: 4px 0px;
    }
    
    QMenu::item {
        color: #e2e8f0;
        padding: 6px 24px 6px 20px;
        font-size: 11px;
    }
    
    QMenu::item:selected {
        background-color: #272f3d;
        color: #38bdf8;
    }
    
    QMenu::separator {
        height: 1px;
        background-color: rgba(255, 255, 255, 0.12);
        margin: 4px 8px;
    }
    """
    return template.replace("__BG__", bg)


def get_hud_stylesheet(theme: dict = None, vibrant: bool = False) -> str:
    """Return appropriate stylesheet for HUD window."""
    if theme is None:
        return get_cards_stylesheet()

    panel = theme["panel"] if vibrant else theme["panel_solid"]
    return f"""
    QWidget#CentralWidget {{
        background-color: {panel};
        border: 1px solid {theme['panel_border']};
        border-radius: {theme['radius']}px;
    }}
    QLabel {{ color: {theme['text']}; }}
    QLabel#HeaderTitle {{ font-size: 11px; font-weight: 700; color: {theme['text2']}; }}
    QLabel#HeaderStatus {{ font-size: 10px; color: {theme['text2']}; }}
    QPushButton#LayoutToggleBtn {{
        background-color: transparent;
        border: 1px solid {theme['separator']};
        border-radius: 4px;
        color: {theme['text2']};
        font-size: 11px;
        padding: 1px 4px;
        min-width: 18px;
        max-height: 18px;
    }}
    QPushButton#LayoutToggleBtn:hover {{
        background-color: {theme['menu_hover']};
        color: {theme['text']};
    }}
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
    """Stylesheet for UsageTable elements."""
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
