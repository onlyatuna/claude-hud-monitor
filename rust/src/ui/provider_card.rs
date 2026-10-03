//! Provider metric card component, matching Python ui/provider_card.py.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::Local;
use qtrs_gui::geometry::primitives::Margins;
use qtrs_gui::text::font::{Font, FontWeight};
use qtrs_gui::tiny_skia::Color;
use qtrs_widgets::{BoxLayout, EmptyWidget, Label, Layout, ProgressBar, Widget, WidgetRef};

use crate::providers::base::{format_countdown, UsageMetrics};

pub fn get_progress_color(percent: f64, dark: bool) -> Color {
    if percent >= 90.0 {
        if dark {
            Color::from_rgba8(239, 68, 68, 255) // #ef4444
        } else {
            Color::from_rgba8(185, 28, 28, 255) // #b91c1c
        }
    } else if percent >= 75.0 {
        if dark {
            Color::from_rgba8(245, 158, 11, 255) // #f59e0b
        } else {
            Color::from_rgba8(146, 64, 14, 255) // #92400e
        }
    } else if percent >= 50.0 {
        if dark {
            Color::from_rgba8(59, 130, 246, 255) // #3b82f6
        } else {
            Color::from_rgba8(29, 78, 216, 255) // #1d4ed8
        }
    } else if dark {
        Color::from_rgba8(16, 185, 129, 255) // #10b981
    } else {
        Color::from_rgba8(4, 120, 87, 255) // #047857
    }
}

pub fn provider_accent_color(provider_id: &str, dark: bool) -> Color {
    match provider_id {
        "claude" => {
            if dark {
                Color::from_rgba8(56, 189, 248, 255)
            } else {
                Color::from_rgba8(3, 105, 161, 255)
            }
        }
        "agy" => {
            if dark {
                Color::from_rgba8(16, 185, 129, 255)
            } else {
                Color::from_rgba8(4, 120, 87, 255)
            }
        }
        "codex" => {
            if dark {
                Color::from_rgba8(168, 85, 247, 255)
            } else {
                Color::from_rgba8(126, 34, 206, 255)
            }
        }
        _ => {
            if dark {
                Color::from_rgba8(56, 189, 248, 255)
            } else {
                Color::from_rgba8(3, 105, 161, 255)
            }
        }
    }
}

pub fn default_provider_name(provider_id: &str) -> &'static str {
    match provider_id {
        "claude" => "CLAUDE CODE",
        "agy" => "ANTIGRAVITY",
        "codex" => "OPENAI CODEX",
        _ => "AI AGENT",
    }
}

fn make_widget<W: Widget + 'static>(w: W) -> WidgetRef {
    Rc::new(RefCell::new(Box::new(w)))
}

fn set_label_text(w: &WidgetRef, text: impl Into<String>) {
    if let Some(lbl) = w.borrow_mut().as_any_mut().downcast_mut::<Label>() {
        lbl.set_text(text);
    }
}

fn set_label_color(w: &WidgetRef, color: Color) {
    if let Some(lbl) = w.borrow_mut().as_any_mut().downcast_mut::<Label>() {
        lbl.set_color(color);
    }
}

fn set_progress_val(w: &WidgetRef, val: i32, color: Color) {
    if let Some(bar) = w.borrow_mut().as_any_mut().downcast_mut::<ProgressBar>() {
        bar.set_value(val);
        let qss = format!(
            "QProgressBar::chunk {{ background-color: rgba({}, {}, {}, {}); }}",
            color.red(),
            color.green(),
            color.blue(),
            color.alpha()
        );
        bar.set_style_sheet(&qss);
    }
}

pub struct ProviderCardWidget {
    pub provider_id: String,
    pub dark: bool,
    pub current_metrics: UsageMetrics,
    pub container: WidgetRef,

    pub dot: WidgetRef,
    pub title: WidgetRef,
    pub badge: WidgetRef,
    pub badge2: WidgetRef,

    pub m1_label: WidgetRef,
    pub m1_val: WidgetRef,
    pub m1_bar: WidgetRef,
    pub m1_sub: WidgetRef,

    pub m2_label: WidgetRef,
    pub m2_val: WidgetRef,
    pub m2_bar: WidgetRef,
    pub m2_sub: WidgetRef,
}

impl ProviderCardWidget {
    pub fn new(provider_id: &str) -> Self {
        let dark = true;
        let theme_color = provider_accent_color(provider_id, dark);
        let default_name = default_provider_name(provider_id);

        let container = make_widget(EmptyWidget::new());
        let mut root_layout = BoxLayout::vertical();
        root_layout.set_margins(Margins::new(6, 4, 6, 4));
        root_layout.set_spacing(2);

        // 1. Header (dot + title + stretch + badge + badge2)
        let mut header_layout = BoxLayout::horizontal();
        header_layout.set_spacing(4);

        let mut dot_lbl = Label::new("●");
        dot_lbl.set_color(theme_color);
        dot_lbl.set_font(Font::new("Segoe UI", 10.0));
        let dot = make_widget(dot_lbl);
        header_layout.add_widget(dot.clone());

        let mut title_lbl = Label::new(default_name);
        title_lbl.set_color(theme_color);
        title_lbl.set_font(Font::new("Segoe UI", 9.5).with_weight(FontWeight::Bold));
        let title = make_widget(title_lbl);
        title.borrow_mut().set_object_name("CardTitle");
        header_layout.add_widget(title.clone());

        header_layout.add_stretch(1);

        let mut badge_lbl = Label::new("--");
        badge_lbl.set_font(Font::new("Consolas", 9.0).with_weight(FontWeight::Bold));
        let badge = make_widget(badge_lbl);
        badge.borrow_mut().set_object_name("Badge");
        header_layout.add_widget(badge.clone());

        let mut badge2_lbl = Label::new("");
        badge2_lbl.set_font(Font::new("Consolas", 9.0).with_weight(FontWeight::Bold));
        let badge2 = make_widget(badge2_lbl);
        badge2.borrow_mut().set_object_name("Badge");
        badge2.borrow_mut().set_visible(false);
        header_layout.add_widget(badge2.clone());
        let header_widget = make_widget(EmptyWidget::new());
        header_widget
            .borrow_mut()
            .set_layout(Box::new(header_layout));
        root_layout.add_widget(header_widget);

        // 2. Metric 1 (Session 5H)
        let mut m1_layout = BoxLayout::vertical();
        m1_layout.set_spacing(1);

        let mut m1_hdr = BoxLayout::horizontal();
        let mut m1_title_lbl = Label::new("SESSION 5H");
        m1_title_lbl.set_font(Font::new("Segoe UI", 10.0).with_weight(FontWeight::Bold));
        let m1_label = make_widget(m1_title_lbl);
        m1_label.borrow_mut().set_object_name("MetricTitle");
        m1_hdr.add_widget(m1_label.clone());
        m1_hdr.add_stretch(1);
        let mut m1_val_lbl = Label::new("--");
        m1_val_lbl.set_font(Font::new("Consolas", 14.0).with_weight(FontWeight::Bold));
        let m1_val = make_widget(m1_val_lbl);
        m1_val.borrow_mut().set_object_name("MetricValue");
        m1_hdr.add_widget(m1_val.clone());

        let m1_hdr_widget = make_widget(EmptyWidget::new());
        m1_hdr_widget.borrow_mut().set_layout(Box::new(m1_hdr));
        m1_layout.add_widget(m1_hdr_widget);

        let mut bar1 = ProgressBar::new();
        bar1.set_range(0, 100);
        bar1.set_value(0);
        bar1.set_text_visible(false);
        let m1_bar = make_widget(bar1);
        m1_layout.add_widget(m1_bar.clone());

        let mut m1_sub_lbl = Label::new("重設於: --");
        m1_sub_lbl.set_font(Font::new("Segoe UI", 9.5));
        let m1_sub = make_widget(m1_sub_lbl);
        m1_sub.borrow_mut().set_object_name("SubDetail");
        m1_layout.add_widget(m1_sub.clone());

        let m1_widget = make_widget(EmptyWidget::new());
        m1_widget.borrow_mut().set_layout(Box::new(m1_layout));
        root_layout.add_widget(m1_widget);

        // 3. Metric 2 (Weekly 7D)
        let mut m2_layout = BoxLayout::vertical();
        m2_layout.set_spacing(1);

        let mut m2_hdr = BoxLayout::horizontal();
        let mut m2_title_lbl = Label::new("WEEKLY 7D");
        m2_title_lbl.set_font(Font::new("Segoe UI", 10.0).with_weight(FontWeight::Bold));
        let m2_label = make_widget(m2_title_lbl);
        m2_label.borrow_mut().set_object_name("MetricTitle");
        m2_hdr.add_widget(m2_label.clone());
        m2_hdr.add_stretch(1);
        let mut m2_val_lbl = Label::new("--");
        m2_val_lbl.set_font(Font::new("Consolas", 14.0).with_weight(FontWeight::Bold));
        let m2_val = make_widget(m2_val_lbl);
        m2_val.borrow_mut().set_object_name("MetricValue");
        m2_hdr.add_widget(m2_val.clone());

        let m2_hdr_widget = make_widget(EmptyWidget::new());
        m2_hdr_widget.borrow_mut().set_layout(Box::new(m2_hdr));
        m2_layout.add_widget(m2_hdr_widget);

        let mut bar2 = ProgressBar::new();
        bar2.set_range(0, 100);
        bar2.set_value(0);
        bar2.set_text_visible(false);
        let m2_bar = make_widget(bar2);
        m2_layout.add_widget(m2_bar.clone());

        let mut m2_sub_lbl = Label::new("重設於: --");
        m2_sub_lbl.set_font(Font::new("Segoe UI", 9.5));
        let m2_sub = make_widget(m2_sub_lbl);
        m2_sub.borrow_mut().set_object_name("SubDetail");
        m2_layout.add_widget(m2_sub.clone());

        let m2_widget = make_widget(EmptyWidget::new());
        m2_widget.borrow_mut().set_layout(Box::new(m2_layout));
        root_layout.add_widget(m2_widget);

        container.borrow_mut().set_layout(Box::new(root_layout));

        Self {
            provider_id: provider_id.to_string(),
            dark,
            current_metrics: UsageMetrics {
                provider_id: provider_id.to_string(),
                ..Default::default()
            },
            container,
            dot,
            title,
            badge,
            badge2,
            m1_label,
            m1_val,
            m1_bar,
            m1_sub,
            m2_label,
            m2_val,
            m2_bar,
            m2_sub,
        }
    }

    pub fn widget(&self) -> WidgetRef {
        self.container.clone()
    }

    pub fn set_appearance(&mut self, dark: bool) {
        self.dark = dark;
        let c = provider_accent_color(&self.provider_id, dark);
        set_label_color(&self.title, c);
        let metrics = self.current_metrics.clone();
        self.update_metrics(&metrics);
    }

    pub fn update_metrics(&mut self, data: &UsageMetrics) {
        self.current_metrics = data.clone();
        let default_name = default_provider_name(&self.provider_id);
        if !data.provider_name.is_empty() {
            set_label_text(&self.title, &data.provider_name);
        } else {
            set_label_text(&self.title, default_name);
        }

        let error_color = if self.dark {
            Color::from_rgba8(239, 68, 68, 255)
        } else {
            Color::from_rgba8(185, 28, 28, 255)
        };

        if data.error.is_some() && !data.stale {
            set_label_color(&self.dot, error_color);
            set_label_text(&self.m1_val, "ERR");
            set_label_color(&self.m1_val, error_color);
            set_progress_val(&self.m1_bar, 0, error_color);

            let first_line = data
                .error
                .as_ref()
                .map(|e| e.lines().next().unwrap_or(e))
                .unwrap_or("Error");
            set_label_text(&self.m1_sub, first_line);
            set_label_color(&self.m1_sub, error_color);

            set_label_text(&self.m2_val, "--");
            set_progress_val(&self.m2_bar, 0, error_color);
            set_label_text(&self.m2_sub, "");

            set_label_text(&self.badge, "OFFLINE");
            self.badge.borrow_mut().set_visible(true);
            self.badge2.borrow_mut().set_visible(false);
            return;
        }

        let theme_color = provider_accent_color(&self.provider_id, self.dark);
        set_label_color(&self.dot, theme_color);

        let sub_default_color = Color::from_rgba8(100, 116, 139, 255); // #64748b
        set_label_color(&self.m1_sub, sub_default_color);
        set_label_color(&self.m2_sub, sub_default_color);

        if data.metric1_reset.is_none() {
            set_label_text(&self.m1_sub, "重設於: --");
        }
        if data.metric2_reset.is_none() {
            set_label_text(&self.m2_sub, "重設於: --");
        }

        // Metric 1
        if !data.metric1_title.is_empty() {
            set_label_text(&self.m1_label, &data.metric1_title);
        }
        set_label_text(&self.m1_val, &data.metric1_text);
        let c1 = if let Some(v) = data.metric1_val {
            get_progress_color(v, self.dark)
        } else {
            sub_default_color
        };
        set_label_color(&self.m1_val, c1);
        let v1 = data.metric1_val.unwrap_or(0.0).clamp(0.0, 100.0) as i32;
        set_progress_val(&self.m1_bar, v1, c1);

        // Metric 2
        if !data.metric2_title.is_empty() {
            set_label_text(&self.m2_label, &data.metric2_title);
        }
        set_label_text(&self.m2_val, &data.metric2_text);
        let c2 = if let Some(v) = data.metric2_val {
            get_progress_color(v, self.dark)
        } else {
            sub_default_color
        };
        set_label_color(&self.m2_val, c2);
        let v2 = data.metric2_val.unwrap_or(0.0).clamp(0.0, 100.0) as i32;
        set_progress_val(&self.m2_bar, v2, c2);

        // Badges
        let b1 = data
            .badge1_text
            .replace(" 剩餘:", ":")
            .replace("剩餘:", ":");
        let b2 = data
            .badge2_text
            .replace(" 剩餘:", ":")
            .replace("剩餘:", ":");

        if !b1.is_empty() && !b2.is_empty() {
            set_label_text(&self.badge, &b1);
            self.badge.borrow_mut().set_visible(true);
            set_label_text(&self.badge2, &b2);
            self.badge2.borrow_mut().set_visible(true);
        } else if !b1.is_empty() {
            set_label_text(&self.badge, &b1);
            self.badge.borrow_mut().set_visible(true);
            self.badge2.borrow_mut().set_visible(false);
        } else if !b2.is_empty() {
            set_label_text(&self.badge, &b2);
            self.badge.borrow_mut().set_visible(true);
            self.badge2.borrow_mut().set_visible(false);
        } else {
            set_label_text(&self.badge, "--");
            self.badge.borrow_mut().set_visible(true);
            self.badge2.borrow_mut().set_visible(false);
        }

        self.update_countdown();

        if data.stale {
            let stale_color = if self.dark {
                Color::from_rgba8(245, 158, 11, 255)
            } else {
                Color::from_rgba8(146, 64, 14, 255)
            };
            set_label_color(&self.dot, stale_color);
            set_label_text(&self.badge, "STALE");
        }
        // Re-layout container so labels with updated text lengths receive their exact sizes
        self.container.borrow().update_layout();
    }

    pub fn update_countdown(&mut self) {
        let data = &self.current_metrics;
        if data.error.is_some() && !data.stale {
            return;
        }

        if let Some(reset) = data.metric1_reset {
            set_label_text(&self.m1_sub, format!("重設於: {}", format_countdown(Some(reset))));
        } else {
            set_label_text(&self.m1_sub, "重設於: --");
        }

        if let Some(reset) = data.metric2_reset {
            set_label_text(&self.m2_sub, format!("重設於: {}", format_countdown(Some(reset))));
        } else {
            set_label_text(&self.m2_sub, "重設於: --");
        }

        if data.stale {
            let stale_color = if self.dark {
                Color::from_rgba8(245, 158, 11, 255)
            } else {
                Color::from_rgba8(146, 64, 14, 255)
            };
            let stamp = if let Some(last) = data.last_success {
                let local: chrono::DateTime<Local> = chrono::DateTime::from(last);
                local.format("%m/%d %H:%M:%S").to_string()
            } else {
                "--".to_string()
            };
            set_label_text(&self.m1_sub, format!("舊資料 {}", stamp));
            set_label_color(&self.m1_sub, stale_color);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_error_recovery_clears_text() {
        let mut card = ProviderCardWidget::new("agy");
        card.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            error: Some("timeout".to_string()),
            ..Default::default()
        });
        assert_eq!(card.badge.borrow().as_any().downcast_ref::<Label>().unwrap().text(), "OFFLINE");
        assert_eq!(card.m1_val.borrow().as_any().downcast_ref::<Label>().unwrap().text(), "ERR");

        card.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            metric1_val: Some(20.0),
            metric1_text: "20%".to_string(),
            ..Default::default()
        });
        assert_ne!(card.badge.borrow().as_any().downcast_ref::<Label>().unwrap().text(), "OFFLINE");
        assert_eq!(card.m1_val.borrow().as_any().downcast_ref::<Label>().unwrap().text(), "20%");
        assert_eq!(card.m2_val.borrow().as_any().downcast_ref::<Label>().unwrap().text(), "--");
    }

    #[test]
    fn test_absent_timestamp_clears_previous_countdown() {
        let mut card = ProviderCardWidget::new("claude");
        card.update_metrics(&UsageMetrics {
            provider_id: "claude".to_string(),
            metric2_reset: Some(Utc::now() + Duration::days(2)),
            ..Default::default()
        });
        assert!(card.m2_sub.borrow().as_any().downcast_ref::<Label>().unwrap().text().contains("重設於:"));

        card.update_metrics(&UsageMetrics {
            provider_id: "claude".to_string(),
            ..Default::default()
        });
        assert_eq!(card.m2_sub.borrow().as_any().downcast_ref::<Label>().unwrap().text(), "重設於: --");
    }

    #[test]
    fn test_stale_data_remains_visible_and_labelled() {
        let mut card = ProviderCardWidget::new("agy");
        card.update_metrics(&UsageMetrics {
            provider_id: "agy".to_string(),
            metric1_val: Some(25.0),
            metric1_text: "25%".to_string(),
            error: Some("timeout".to_string()),
            stale: true,
            last_success: Some(Utc::now()),
            ..Default::default()
        });
        assert_eq!(card.m1_val.borrow().as_any().downcast_ref::<Label>().unwrap().text(), "25%");
        assert_eq!(card.badge.borrow().as_any().downcast_ref::<Label>().unwrap().text(), "STALE");
    }
    #[test]
    fn test_hud_layout_proportions() {
        let config = std::sync::Arc::new(parking_lot::Mutex::new(crate::config::ConfigManager::load()));
        let refresh_ctrl = std::sync::Arc::new(parking_lot::Mutex::new(crate::refresh_controller::RefreshController::new(60)));
        let mut hud = crate::ui::hud_window::HUDWindow::new(config, refresh_ctrl).unwrap();

        // 1. Horizontal mode
        hud.window.set_geometry(qtrs_gui::geometry::primitives::Rect::new(0, 0, 690, 152));
        hud.apply_cards_layout_mode("horizontal");
        let stack_geom = hud.stack.borrow().geometry();
        assert!(stack_geom.y <= 35, "Stack must start below header, not pushed down (got {})", stack_geom.y);
        let claude_geom = hud.cards["claude"].container.borrow().geometry();
        assert!(claude_geom.width >= 200, "Card width must be at least 200px (got {})", claude_geom.width);
        assert_eq!(hud.cards["claude"].m1_bar.borrow().geometry().height, 5, "Progress bar height must be 5px");

        // 2. Vertical mode
        hud.window.set_geometry(qtrs_gui::geometry::primitives::Rect::new(0, 0, 280, 490));
        hud.apply_cards_layout_mode("vertical");
        let stack_geom_v = hud.stack.borrow().geometry();
        assert!(stack_geom_v.y <= 35, "Stack must start below header in vertical mode (got {})", stack_geom_v.y);
        let c_claude = hud.cards["claude"].container.borrow().geometry();
        let c_agy = hud.cards["agy"].container.borrow().geometry();
        assert!(c_claude.height >= 120, "Vertical card must have reasonable height (got {})", c_claude.height);
        assert!(c_agy.y + c_agy.height <= stack_geom_v.height + 2, "Last card must not overflow stack container");

        // 3. Table mode
        hud.window.set_geometry(qtrs_gui::geometry::primitives::Rect::new(0, 0, 450, 350));
        hud.apply_ui_mode("table");
        let dial_claude = hud.table.columns["claude"].dial.borrow().geometry();
        let dial_codex = hud.table.columns["codex"].dial.borrow().geometry();
        assert!(dial_claude.width >= 80 && dial_claude.height >= 80, "Dial must meet minimum size");
        assert_eq!(dial_claude.height, dial_codex.height, "Dials must have uniform height");
    }

    #[test]
    fn test_inspect_card_layout_with_data() {
        let config = std::sync::Arc::new(parking_lot::Mutex::new(crate::config::ConfigManager::load()));
        let refresh_ctrl = std::sync::Arc::new(parking_lot::Mutex::new(crate::refresh_controller::RefreshController::new(60)));
        let mut hud = crate::ui::hud_window::HUDWindow::new(config, refresh_ctrl).unwrap();
        hud.window.set_geometry(qtrs_gui::geometry::primitives::Rect::new(0, 0, 883, 574));
        hud.apply_cards_layout_mode("vertical");

        let claude_data = crate::providers::base::UsageMetrics {
            provider_id: "claude".to_string(),
            badge1_text: "Chat: 0%".to_string(),
            badge2_text: "Code: 10%".to_string(),
            metric1_title: "SESSION 5H".to_string(),
            metric1_text: "10%".to_string(),
            metric1_val: Some(10.0),
            metric2_title: "WEEKLY 7D".to_string(),
            metric2_text: "49%".to_string(),
            metric2_val: Some(49.0),
            ..Default::default()
        };
        hud.on_data_fetched(&claude_data);

        let agy_data = crate::providers::base::UsageMetrics {
            provider_id: "agy".to_string(),
            provider_name: "Antigravity".to_string(),
            badge1_text: "C/G: 10%".to_string(),
            badge2_text: "Gemini Models".to_string(),
            metric1_title: "SESSION 5H".to_string(),
            metric1_text: "40%".to_string(),
            metric1_val: Some(40.0),
            metric2_title: "WEEKLY 7D".to_string(),
            metric2_text: "58%".to_string(),
            metric2_val: Some(58.0),
            ..Default::default()
        };
        hud.on_data_fetched(&agy_data);

        let codex_data = crate::providers::base::UsageMetrics {
            provider_id: "codex".to_string(),
            error: Some("登入憑證已失效，請使用原 CLI 重新登入".to_string()),
            ..Default::default()
        };
        hud.on_data_fetched(&codex_data);

        let c = &hud.cards["claude"];
        println!("CLAUDE title: {:?}", c.title.borrow().geometry());
        let title_geom = c.title.borrow().geometry();
        let badge_geom = c.badge.borrow().geometry();
        let badge2_geom = c.badge2.borrow().geometry();

        // Regression: Badge and Badge2 must never overlap title at (0, 0)
        assert!(badge_geom.x >= title_geom.x + title_geom.width, "Badge must be right of title");
        assert!(badge2_geom.x >= badge_geom.x + badge_geom.width, "Badge2 must be right of badge");
        assert!(badge2_geom.width > 0, "Badge2 must have positive width");

        let a = &hud.cards["agy"];
        let a_title = a.title.borrow().geometry();
        let a_badge = a.badge.borrow().geometry();
        let a_badge2 = a.badge2.borrow().geometry();

        assert!(a_badge.x >= a_title.x + a_title.width, "Agy badge must be right of title");
        assert!(a_badge2.x >= a_badge.x + a_badge.width, "Agy badge2 must be right of badge");
        assert!(a_badge2.width > 0, "Agy badge2 must have positive width");
    }
}

