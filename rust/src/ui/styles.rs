//! HUD visual styling, color themes, and metrics matching Python ui/styles.py.

use qtrs_gui::tiny_skia::Color;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    pub is_dark: bool,
    pub panel_bg: Color,
    pub panel_border: Color,
    pub text: Color,
    pub text2: Color,
    pub text3: Color,
    pub neutral: Color,
    pub separator: Color,
    pub track: Color,
    pub disc: Color,
    pub halo: Color,
    pub hatch: Color,
    pub duo_inner: Color,
    pub duo_outer: Color,
    pub duo_text_inner: Color,
    pub duo_text_outer: Color,
    pub scale_green: Color,
    pub scale_yellow: Color,
    pub scale_orange: Color,
    pub scale_red: Color,
    pub scale_text_green: Color,
    pub scale_text_yellow: Color,
    pub scale_text_orange: Color,
    pub scale_text_red: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            is_dark: true,
            panel_bg: Color::from_rgba8(30, 30, 36, 240),
            panel_border: Color::from_rgba8(255, 255, 255, 31),
            text: Color::from_rgba8(242, 242, 247, 255), // #f2f2f7
            text2: Color::from_rgba8(235, 235, 245, 158),
            text3: Color::from_rgba8(235, 235, 245, 82),
            neutral: Color::from_rgba8(166, 166, 179, 255), // #A6A6B3
            separator: Color::from_rgba8(255, 255, 255, 31),
            track: Color::from_rgba8(255, 255, 255, 28),
            disc: Color::from_rgba8(255, 255, 255, 14),
            halo: Color::from_rgba8(30, 30, 36, 170),
            hatch: Color::from_rgba8(0, 0, 0, 110),
            duo_inner: Color::from_rgba8(154, 166, 228, 255), // #9AA6E4
            duo_outer: Color::from_rgba8(111, 124, 200, 255), // #6F7CC8
            duo_text_inner: Color::from_rgba8(180, 189, 240, 255), // #B4BDF0
            duo_text_outer: Color::from_rgba8(174, 184, 242, 255), // #AEB8F2
            scale_green: Color::from_rgba8(140, 199, 156, 255), // #8CC79C
            scale_yellow: Color::from_rgba8(227, 198, 106, 255), // #E3C66A
            scale_orange: Color::from_rgba8(232, 165, 116, 255), // #E8A574
            scale_red: Color::from_rgba8(224, 123, 123, 255), // #E07B7B
            scale_text_green: Color::from_rgba8(140, 199, 156, 255),
            scale_text_yellow: Color::from_rgba8(227, 198, 106, 255),
            scale_text_orange: Color::from_rgba8(232, 165, 116, 255),
            scale_text_red: Color::from_rgba8(224, 123, 123, 255),
        }
    }

    pub fn light() -> Self {
        Self {
            is_dark: false,
            panel_bg: Color::from_rgba8(246, 244, 250, 240),
            panel_border: Color::from_rgba8(255, 255, 255, 140),
            text: Color::from_rgba8(31, 31, 36, 255), // #1f1f24
            text2: Color::from_rgba8(40, 40, 50, 158),
            text3: Color::from_rgba8(40, 40, 50, 87),
            neutral: Color::from_rgba8(140, 140, 153, 255), // #8C8C99
            separator: Color::from_rgba8(40, 40, 50, 36),
            track: Color::from_rgba8(40, 40, 60, 26),
            disc: Color::from_rgba8(40, 40, 60, 14),
            halo: Color::from_rgba8(246, 244, 250, 190),
            hatch: Color::from_rgba8(30, 30, 60, 85),
            duo_inner: Color::from_rgba8(131, 145, 210, 255), // #8391D2
            duo_outer: Color::from_rgba8(85, 99, 174, 255),   // #5563AE
            duo_text_inner: Color::from_rgba8(96, 112, 190, 255), // #6070BE
            duo_text_outer: Color::from_rgba8(70, 85, 163, 255), // #4655A3
            scale_green: Color::from_rgba8(120, 176, 138, 255), // #78B08A
            scale_yellow: Color::from_rgba8(216, 184, 90, 255), // #D8B85A
            scale_orange: Color::from_rgba8(220, 148, 97, 255), // #DC9461
            scale_red: Color::from_rgba8(212, 104, 104, 255), // #D46868
            scale_text_green: Color::from_rgba8(63, 122, 82, 255), // #3F7A52
            scale_text_yellow: Color::from_rgba8(140, 110, 18, 255), // #8C6E12
            scale_text_orange: Color::from_rgba8(169, 90, 34, 255), // #A95A22
            scale_text_red: Color::from_rgba8(176, 60, 60, 255), // #B03C3C
        }
    }
}

pub fn get_theme(dark: bool) -> Theme {
    if dark {
        Theme::dark()
    } else {
        Theme::light()
    }
}

#[allow(dead_code)]
pub fn get_progress_color(percent: f64, dark: bool) -> Color {
    crate::ui::provider_card::get_progress_color(percent, dark)
}

pub fn scale_colors(theme: &Theme, percent: Option<f64>) -> (Color, Color) {
    match crate::pace::scale_level(percent) {
        Some("green") => (theme.scale_green, theme.scale_text_green),
        Some("yellow") => (theme.scale_yellow, theme.scale_text_yellow),
        Some("orange") => (theme.scale_orange, theme.scale_text_orange),
        Some("red") => (theme.scale_red, theme.scale_text_red),
        _ => (theme.neutral, theme.text2),
    }
}

pub fn duo_colors(theme: &Theme, is_inner: bool) -> (Color, Color) {
    if is_inner {
        (theme.duo_inner, theme.duo_text_inner)
    } else {
        (theme.duo_outer, theme.duo_text_outer)
    }
}
pub fn get_cards_stylesheet(dark: bool) -> &'static str {
    if dark {
        r#"
        QWidget#CentralWidget {
            background-color: rgba(14, 17, 23, 0.94);
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
            max-height: 15px;
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
            border: none;
            background-color: rgba(255, 255, 255, 0.12);
            max-width: 1px;
            min-width: 1px;
        }

        QFrame#HorizontalDivider {
            background-color: rgba(255, 255, 255, 0.08);
            border: none;
            min-height: 1px;
            max-height: 1px;
        }
        "#
    } else {
        r#"
        QWidget#CentralWidget {
            background-color: rgba(248, 250, 252, 0.94);
            border: 1px solid rgba(15, 23, 42, 0.18);
            border-radius: 9px;
        }

        QLabel {
            color: #172033;
            font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif;
        }

        QLabel#HeaderTitle {
            font-size: 10.5px;
            font-weight: 800;
            letter-spacing: 1.0px;
            color: #475569;
        }

        QLabel#HeaderStatus {
            font-size: 9.5px;
            color: #64748b;
            font-family: 'Consolas', monospace;
        }

        QLabel#MetricTitle {
            font-size: 10px;
            font-weight: 700;
            color: #475569;
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
            background-color: rgba(15, 23, 42, 0.10);
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
            background-color: rgba(15, 23, 42, 0.05);
            border: 1px solid rgba(15, 23, 42, 0.08);
            border-radius: 3px;
            padding: 1px 4px;
            font-size: 9px;
            max-height: 15px;
            color: #334155;
            font-family: 'Consolas', monospace;
        }

        QPushButton#LayoutToggleBtn {
            background-color: transparent;
            border: 1px solid rgba(15, 23, 42, 0.12);
            border-radius: 4px;
            color: #475569;
            font-size: 11px;
            padding: 1px 4px;
            min-width: 18px;
            max-height: 18px;
        }

        QPushButton#LayoutToggleBtn:hover {
            background-color: rgba(15, 23, 42, 0.12);
            color: #0369a1;
            border-color: #0369a1;
        }

        QFrame#Divider {
            border: none;
            background-color: rgba(15, 23, 42, 0.12);
            max-width: 1px;
            min-width: 1px;
        }

        QFrame#HorizontalDivider {
            background-color: rgba(15, 23, 42, 0.10);
            border: none;
            min-height: 1px;
            max-height: 1px;
        }
        "#
    }
}
