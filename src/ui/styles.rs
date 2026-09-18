// src/ui/styles.rs — HUD visual styling matching PySide6 ui/styles.py get_hud_stylesheet()

use egui::Color32;

// CentralWidget: background-color: rgba(14, 17, 23, 0.94); border: 1px solid rgba(255, 255, 255, 0.14); border-radius: 9px;
// Correct premultiplied values: (14*240/255, 17*240/255, 23*240/255, 240) = (13, 16, 21, 240)
pub const BG_DARK: Color32 = Color32::from_rgba_premultiplied(13, 16, 21, 240);
// Border: rgba(255, 255, 255, 0.14) -> 255 * 36 / 255 = 36
pub const BORDER_COLOR: Color32 = Color32::from_rgba_premultiplied(36, 36, 36, 36);

// QLabel: color: #e2e8f0;
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xe2, 0xe8, 0xf0);
// MetricTitle, HeaderTitle: color: #94a3b8;
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x94, 0xa3, 0xb8);
// SubDetail, HeaderStatus: color: #64748b;
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x64, 0x74, 0x8b);
#[allow(dead_code)]
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x64, 0x74, 0x8b);
// Progress bar track: rgba(255, 255, 255, 0.08) -> alpha ~14
pub const TRACK_BG: Color32 = Color32::from_rgba_premultiplied(14, 14, 14, 14);

// Status colors
pub const COLOR_GREEN: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);
pub const COLOR_BLUE: Color32 = Color32::from_rgb(0x38, 0xbd, 0xf8);
pub const COLOR_AMBER: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);
pub const COLOR_RED: Color32 = Color32::from_rgb(0xef, 0x44, 0x44);
pub const COLOR_PURPLE: Color32 = Color32::from_rgb(0xa8, 0x55, 0xf7);

/// Provider accent colors (mirrors PROVIDER_THEMES in Python)
pub fn provider_color(id: &str) -> Color32 {
    match id {
        "claude" => COLOR_BLUE,
        "agy" => COLOR_GREEN,
        "codex" => COLOR_PURPLE,
        _ => COLOR_BLUE,
    }
}

pub fn provider_name(id: &str) -> &'static str {
    match id {
        "claude" => "CLAUDE CODE",
        "agy" => "ANTIGRAVITY",
        "codex" => "OPENAI CODEX",
        _ => "UNKNOWN",
    }
}

/// Progress bar color (mirrors get_progress_color in Python ui/styles.py)
pub fn progress_color(percent: f64) -> Color32 {
    if percent >= 90.0 {
        COLOR_RED // #ef4444
    } else if percent >= 75.0 {
        COLOR_AMBER // #f59e0b
    } else if percent >= 50.0 {
        Color32::from_rgb(0x3b, 0x82, 0xf6) // #3b82f6 (Blue)
    } else {
        COLOR_GREEN // #10b981 (Green)
    }
}

/// Configure the egui visuals for the dark translucent HUD
pub fn apply_hud_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = BG_DARK;
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.window_rounding = egui::Rounding::same(9.0);
    visuals.window_stroke = egui::Stroke::new(1.0_f32, BORDER_COLOR);

    // Layout toggle button styles (matching QPushButton#LayoutToggleBtn)
    visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(30, 30, 30, 30));
    visuals.widgets.inactive.rounding = egui::Rounding::same(4.0);

    visuals.widgets.hovered.bg_fill = Color32::from_rgba_premultiplied(30, 30, 30, 30);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, COLOR_BLUE);
    visuals.widgets.hovered.rounding = egui::Rounding::same(4.0);

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 5.0);
    style.spacing.window_margin = egui::Margin::same(0.0);
    ctx.set_style(style);
}
