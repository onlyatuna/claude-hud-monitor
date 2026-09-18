// src/ui/provider_card.rs — Provider metric card rendering matching PySide6 ProviderCardWidget

use crate::providers::base::{UsageMetrics, format_countdown};
use super::styles::{
    provider_color, provider_name, progress_color,
    TEXT_SECONDARY, TEXT_MUTED, COLOR_RED, COLOR_AMBER, TRACK_BG,
};
use egui::{Color32, RichText, Ui};
use chrono::Local;

/// Render one provider card (mirrors PySide6 ProviderCardWidget)
pub fn render_provider_card(ui: &mut Ui, id: &str, data: Option<&UsageMetrics>, target_height: f32) {
    let accent = provider_color(id);
    let title = provider_name(id);

    // Balanced vertical layout matching Qt QVBoxLayout
    // Base content: 16 (hdr) + 33 (m1) + 33 (m2) = 82px.
    let extra = (target_height - 82.0).max(0.0);
    let item_spacing = (3.0 + extra * 0.08).clamp(2.0, 7.0);
    let row_spacing = (0.8 + extra * 0.02).clamp(0.5, 1.5);
    let top_bottom_margin = (2.0 + extra * 0.04).clamp(1.5, 4.0);

    // Card frame with margins (5, top, 5, bottom)
    egui::Frame::none()
        .inner_margin(egui::Margin { left: 5.0, right: 5.0, top: top_bottom_margin, bottom: top_bottom_margin })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, item_spacing);

            // ── 1. Header: ● TITLE       [Badge1] [Badge2] (strictly 16px row, perfectly centered) ──
            let header_h = 16.0;
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), header_h),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_height(header_h);
                    ui.spacing_mut().item_spacing = egui::vec2(4.5, 0.0);

                    // Perfectly aligned vector dot icon (eliminates font glyph baseline skew)
                    let dot_radius = 3.5;
                    let (dot_rect, _) = ui.allocate_exact_size(
                        egui::vec2(dot_radius * 2.0, header_h),
                        egui::Sense::hover(),
                    );
                    let title_resp = ui.label(
                        RichText::new(title)
                            .color(accent)
                            .size(10.0)
                            .strong(),
                    );
                    let dot = dot_color(id, data);
                    let dot_center = egui::pos2(
                        dot_rect.center().x,
                        title_resp.rect.center().y - 0.5,
                    );
                    ui.painter().circle_filled(dot_center, dot_radius, dot);

                    // Right-aligned badges with exact matching vertical center
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.set_height(header_h);
                        ui.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);
                        render_badges_right_to_left(ui, data);
                    });
                },
            );

            // ── 2. Metric rows (Session 5H + Weekly 7D) ──
            match data {
                None => {
                    let (m1_title, m2_title) = default_metric_titles(id);
                    render_metric_row(ui, m1_title, None, "--", None, row_spacing);
                    render_metric_row(ui, m2_title, None, "--", None, row_spacing);
                }
                Some(d) if d.error.is_some() && !d.stale => {
                    render_error_state(ui, d, row_spacing);
                }
                Some(d) => {
                    render_metric_row(
                        ui,
                        &d.metric1_title,
                        d.metric1_val,
                        &d.metric1_text,
                        d.metric1_reset,
                        row_spacing,
                    );
                    render_metric_row(
                        ui,
                        &d.metric2_title,
                        d.metric2_val,
                        &d.metric2_text,
                        d.metric2_reset,
                        row_spacing,
                    );

                    if d.stale {
                        let stamp = d.last_success
                            .map(|dt| dt.with_timezone(&Local).format("%m/%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| "--".to_owned());
                        ui.label(
                            RichText::new(format!("舊資料 {}", stamp))
                                .color(COLOR_AMBER)
                                .size(9.0),
                        );
                    }
                }
            }
        });
}

fn default_metric_titles(id: &str) -> (&'static str, &'static str) {
    match id {
        "codex" => ("WINDOW 30D", "SECONDARY"),
        _ => ("SESSION 5H", "WEEKLY 7D"),
    }
}

fn dot_color(id: &str, data: Option<&UsageMetrics>) -> Color32 {
    let Some(d) = data else { return TEXT_MUTED; };
    if d.error.is_some() && !d.stale {
        COLOR_RED
    } else if d.stale {
        COLOR_AMBER
    } else {
        provider_color(id)
    }
}

fn estimate_badge_w(text: &str) -> f32 {
    let char_w: f32 = text.chars().map(|c| if c as u32 > 0x2E80 { 10.0 } else { 5.6 }).sum();
    char_w + 10.0 // 4px padding each side + 2px border
}

fn render_badges_right_to_left(ui: &mut Ui, d: Option<&UsageMetrics>) {
    let Some(d) = d else {
        badge_label(ui, "--");
        return;
    };
    if d.error.is_some() && !d.stale {
        badge_label(ui, "OFFLINE");
        return;
    }
    if d.stale {
        badge_label(ui, "STALE");
        return;
    }

    let b1 = d.badge1_text.trim();
    let b2 = d.badge2_text.trim();

    if b1.is_empty() && b2.is_empty() {
        badge_label(ui, "--");
        return;
    }

    let avail = ui.available_width();

    if !b1.is_empty() && !b2.is_empty() {
        let w_full_b1 = estimate_badge_w(b1);
        let w_b2 = estimate_badge_w(b2);

        let compact_b1 = if b1.contains("剩餘:") {
            b1.replace(" 剩餘:", ":")
        } else {
            b1.to_string()
        };
        let w_compact_b1 = estimate_badge_w(&compact_b1);

        // Tier 1: Wide space (Vertical mode or wide window) -> Both full badges
        if w_full_b1 + w_b2 + 3.0 <= avail {
            badge_label(ui, b2);
            badge_label(ui, b1);
        }
        // Tier 2: Medium space -> Both badges with compact b1 ("C/G: 67%")
        else if w_compact_b1 + w_b2 + 3.0 <= avail {
            badge_label(ui, b2);
            badge_label(ui, &compact_b1);
        }
        // Tier 3: Limited space (tight 3-column horizontal mode) -> Responsive fallback:
        // Do NOT overlap provider title! Show primary badge only:
        else if w_compact_b1 <= avail {
            badge_label(ui, &compact_b1);
        }
        else if w_full_b1 <= avail {
            badge_label(ui, b1);
        }
        else {
            let short = b1.split(':').next_back().unwrap_or(b1).trim();
            badge_label(ui, short);
        }
    } else if !b1.is_empty() {
        let w1 = estimate_badge_w(b1);
        if w1 <= avail {
            badge_label(ui, b1);
        } else {
            let compact = b1.replace(" 剩餘:", ":");
            badge_label(ui, &compact);
        }
    } else if !b2.is_empty() {
        badge_label(ui, b2);
    }
}

fn badge_label(ui: &mut Ui, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let frame = egui::Frame::none()
        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 15))
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 20)))
        .rounding(3.0)
        .inner_margin(egui::Margin { left: 4.0, right: 4.0, top: 1.0, bottom: 1.0 });

    frame.show(ui, |ui| {
        ui.label(
            RichText::new(text)
                .size(9.5)
                .monospace()
                .color(Color32::from_rgb(203, 213, 225)),
        );
    });
}

fn render_error_state(ui: &mut Ui, d: &UsageMetrics, row_spacing: f32) {
    let err_msg = d.error.as_deref().unwrap_or("未知錯誤");
    let first_line = err_msg.lines().next().unwrap_or(err_msg);

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, row_spacing);
        let row_h = 16.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_height(row_h);
                ui.label(RichText::new("SESSION 5H").color(TEXT_SECONDARY).size(9.5).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_height(row_h);
                    ui.label(RichText::new("ERR").color(COLOR_RED).size(14.5).strong().monospace());
                });
            },
        );

        custom_progress_bar(ui, 0.0, COLOR_RED);
        ui.label(RichText::new(first_line).color(COLOR_RED).size(9.0));
    });

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, row_spacing);
        let row_h = 16.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_height(row_h);
                ui.label(RichText::new("WEEKLY 7D").color(TEXT_SECONDARY).size(9.5).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_height(row_h);
                    ui.label(RichText::new("--").color(TEXT_MUTED).size(14.5).monospace());
                });
            },
        );
        custom_progress_bar(ui, 0.0, TEXT_MUTED);
        ui.label(RichText::new("重設於: --").color(TEXT_MUTED).size(9.0));
    });
}

fn render_metric_row(
    ui: &mut Ui,
    title: &str,
    val: Option<f64>,
    text: &str,
    reset: Option<chrono::DateTime<chrono::Utc>>,
    row_spacing: f32,
) {
    // Each metric box: responsive vertical spacing
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, row_spacing);

        // 1. Metric Header: [TITLE] ........... [VALUE%]
        let row_h = 16.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_height(row_h);
                ui.label(RichText::new(title).color(TEXT_SECONDARY).size(9.5).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_height(row_h);
                    let color = val.map(progress_color).unwrap_or(TEXT_MUTED);
                    ui.label(
                        RichText::new(text)
                            .color(color)
                            .size(14.5)
                            .monospace()
                            .strong(),
                    );
                });
            },
        );

        // 2. 4.5px Progress Bar
        let fraction = val.unwrap_or(0.0) as f32 / 100.0;
        let bar_color = val.map(progress_color).unwrap_or(TEXT_MUTED);
        custom_progress_bar(ui, fraction, bar_color);

        // 3. Reset countdown subtext (color: #64748b, font-size: 9.0px)
        let countdown = if reset.is_some() {
            format_countdown(reset)
        } else {
            "--".to_owned()
        };
        ui.label(
            RichText::new(format!("重設於: {}", countdown))
                .color(TEXT_MUTED)
                .size(9.0),
        );
    });
}

/// 4.5px rounded progress bar matching NVIDIA / RivaTuner HUD aesthetic
fn custom_progress_bar(ui: &mut Ui, fraction: f32, color: Color32) {
    let desired_size = egui::vec2(ui.available_width(), 4.5);
    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter();

    // Background track (subtle translucent white rgba(255, 255, 255, 0.08))
    painter.rect_filled(
        rect,
        2.5,
        TRACK_BG,
    );

    // Progress chunk
    let fill_w = (rect.width() * fraction.clamp(0.0, 1.0)).max(0.0);
    if fill_w > 0.0 {
        let chunk_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
        painter.rect_filled(chunk_rect, 2.5, color);
    }
}
