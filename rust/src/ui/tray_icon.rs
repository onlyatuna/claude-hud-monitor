//! System tray icon component matching Python ui/tray_icon.py.

use std::sync::Arc;
use parking_lot::Mutex;

use qtrs_gui::paint::pixmap::Pixmap;
use qtrs_gui::tiny_skia::Color;
use crate::autostart::{is_autostart_enabled, set_autostart};
use qtrs_platform::menu::{create_platform_menu, PlatformMenu};
use qtrs_platform::tray_icon::TrayIcon;

use super::hud_window::HUDWindow;
use crate::config::Config;


// Menu Action IDs
pub const ACTION_TOGGLE_VISIBILITY: u32 = 1;
pub const ACTION_REFRESH_ALL: u32 = 2;

pub const ACTION_MODE_CARDS: u32 = 10;
pub const ACTION_MODE_TABLE: u32 = 11;

pub const ACTION_LAYOUT_HORIZONTAL: u32 = 20;
pub const ACTION_LAYOUT_VERTICAL: u32 = 21;

pub const ACTION_APPEARANCE_AUTO: u32 = 30;
pub const ACTION_APPEARANCE_LIGHT: u32 = 31;
pub const ACTION_APPEARANCE_DARK: u32 = 32;

pub const ACTION_SCHEME_SCALE: u32 = 40;
pub const ACTION_SCHEME_DUO: u32 = 41;

pub const ACTION_CLICK_THROUGH: u32 = 50;
pub const ACTION_ALWAYS_ON_TOP: u32 = 51;
pub const ACTION_AUTOSTART: u32 = 52;
pub const ACTION_OPEN_LOGS: u32 = 53;

pub const ACTION_LOCK_DRAG: u32 = 60;

pub const ACTION_OPACITY_100: u32 = 70;
pub const ACTION_OPACITY_90: u32 = 71;
pub const ACTION_OPACITY_80: u32 = 72;
pub const ACTION_OPACITY_70: u32 = 73;
pub const ACTION_OPACITY_50: u32 = 74;
pub const ACTION_OPACITY_30: u32 = 75;

pub const ACTION_INTERVAL_30: u32 = 80;
pub const ACTION_INTERVAL_60: u32 = 81;
pub const ACTION_INTERVAL_120: u32 = 82;
pub const ACTION_INTERVAL_300: u32 = 83;

pub const ACTION_RESET_GEOMETRY: u32 = 90;
pub const ACTION_HIDE_HUD: u32 = 91;

pub const ACTION_CLAUDE_PROFILE_AUTO: u32 = 100;
pub const ACTION_CLAUDE_PROFILE_BASE: u32 = 1000;

pub const ACTION_EXIT: u32 = 99;
pub fn create_default_tray_pixmap() -> Pixmap {
    const ICON_PNG_BYTES: &[u8] = include_bytes!("../../assets/app_icon.png");
    if let Ok(img) = image::load_from_memory(ICON_PNG_BYTES) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        if let Some(mut pm) = Pixmap::new(w, h) {
            pm.data_mut().copy_from_slice(rgba.as_raw());
            return pm;
        }
    }
    let mut pm = Pixmap::new(16, 16).unwrap();
    pm.fill(Color::from_rgba8(56, 189, 248, 255));
    pm
}

pub fn build_tray_menu(native_handle: isize, cfg: &Config) -> Box<dyn PlatformMenu> {
    let mut menu = create_platform_menu(native_handle);

    menu.add_action(ACTION_TOGGLE_VISIBILITY, "👁️ 顯示 / 隱藏 HUD (Alt+C)");
    menu.add_action(ACTION_REFRESH_ALL, "🔄 立即重新整理所有 AI (Refresh All)");
    menu.add_separator();

    // 1. UI Style submenu
    let mut style_sub = create_platform_menu(native_handle);
    style_sub.add_checkable(ACTION_MODE_CARDS, "🗂️ 傳統卡片 (Classic Cards)", cfg.ui_mode == "cards");
    style_sub.add_checkable(ACTION_MODE_TABLE, "📊 儀表表格 (Modern Table)", cfg.ui_mode == "table");
    menu.add_submenu("🎭 介面風格 (UI Style)", style_sub);

    // 2. Layout submenu (only in cards mode)
    if cfg.ui_mode == "cards" {
        let mut layout_sub = create_platform_menu(native_handle);
        layout_sub.add_checkable(
            ACTION_LAYOUT_HORIZONTAL,
            "💻 橫向三欄並排 (Horizontal Triple)",
            cfg.layout_mode == "horizontal",
        );
        layout_sub.add_checkable(
            ACTION_LAYOUT_VERTICAL,
            "📱 直立三層堆疊 (Vertical Stack)",
            cfg.layout_mode == "vertical",
        );
        menu.add_submenu("📐 顯示佈局 (Layout)", layout_sub);
    }

    // 3. Theme submenus
    if cfg.ui_mode == "table" {
        let mut scheme_sub = create_platform_menu(native_handle);
        scheme_sub.add_checkable(ACTION_SCHEME_SCALE, "色階模式 (Scale)", cfg.color_scheme == "scale");
        scheme_sub.add_checkable(ACTION_SCHEME_DUO, "雙色模式 (Duo)", cfg.color_scheme == "duo");
        menu.add_submenu("🎨 配色 (Colors)", scheme_sub);
    }

    let mut app_sub = create_platform_menu(native_handle);
    app_sub.add_checkable(ACTION_APPEARANCE_AUTO, "跟隨系統 (Auto)", cfg.appearance == "auto");
    app_sub.add_checkable(ACTION_APPEARANCE_LIGHT, "淺色模式 (Light)", cfg.appearance == "light");
    app_sub.add_checkable(ACTION_APPEARANCE_DARK, "深色模式 (Dark)", cfg.appearance == "dark");
    menu.add_submenu("🌓 外觀 (Appearance)", app_sub);

    menu.add_separator();

    menu.add_checkable(ACTION_CLICK_THROUGH, "👻 滑鼠點擊穿透 (Alt+Shift+C)", cfg.click_through);
    menu.add_checkable(ACTION_ALWAYS_ON_TOP, "📌 視窗永遠置頂", cfg.always_on_top);
    menu.add_checkable(ACTION_AUTOSTART, "🚀 開機自動啟動", is_autostart_enabled());
    menu.add_action(ACTION_OPEN_LOGS, "📂 開啟記錄檔目錄 (Open Logs)");

    menu.add_separator();
    menu.add_action(ACTION_EXIT, "❌ 結束程式 (Exit)");

    menu
}
pub fn build_hud_context_menu(cfg: &Config) -> qtrs_widgets::menu::Menu {
    use qtrs_core::variant::Variant;
    use qtrs_widgets::action::Action;
    use qtrs_widgets::menu::Menu;

    let mut menu = Menu::new("");
    menu.set_dark_mode(cfg.appearance != "light");

    let add_item = |m: &mut Menu, id: u32, text: &str| {
        let act = Action::new_ref(text);
        act.borrow_mut().set_data(Variant::uint(id as u64));
        m.add_action(act);
    };

    let add_check = |m: &mut Menu, id: u32, text: &str, checked: bool| {
        let act = Action::new_ref(text);
        {
            let mut a = act.borrow_mut();
            a.set_checkable(true);
            a.set_checked(checked);
            a.set_data(Variant::uint(id as u64));
        }
        m.add_action(act);
    };

    // 1. Refresh
    add_item(&mut menu, ACTION_REFRESH_ALL, "🔄 立即重新整理所有 AI (Refresh All)");
    menu.add_separator();

    // 2. Claude Account Submenu (PR 13)
    let claude_sub = Menu::new_ref("✳️ Claude 帳號 (Claude Account)");
    {
        let mut cs = claude_sub.borrow_mut();
        cs.set_dark_mode(cfg.appearance != "light");

        let cur_profile = cfg.claude_profile.as_str();
        let is_auto = cur_profile.is_empty() || cur_profile.eq_ignore_ascii_case("auto");
        let (active_prof, _) = crate::providers::claude::resolve_active_profile(cur_profile);

        let auto_title = if is_auto && active_prof.id != "default" {
            format!("🎯 智慧自動追蹤 (目前: {})", active_prof.short_name)
        } else {
            "🎯 智慧自動追蹤 (最近活躍)".to_string()
        };
        add_check(&mut cs, ACTION_CLAUDE_PROFILE_AUTO, &auto_title, is_auto);

        cs.add_separator();

        let profiles = crate::providers::claude::discover_profiles();
        for (i, prof) in profiles.iter().take(40).enumerate() {
            let is_selected = !is_auto
                && (cur_profile.eq_ignore_ascii_case(&prof.id)
                    || (cur_profile == ".claude" && prof.id == "default"));
            add_check(
                &mut cs,
                ACTION_CLAUDE_PROFILE_BASE + i as u32,
                &prof.display_name,
                is_selected,
            );
        }
    }
    menu.add_menu_ref(&claude_sub);
    menu.add_separator();

    // 3. UI Mode Submenu
    let style_sub = Menu::new_ref("🎭 介面風格 (UI Style)");
    {
        let mut s = style_sub.borrow_mut();
        s.set_dark_mode(cfg.appearance != "light");
        add_check(&mut s, ACTION_MODE_CARDS, "🗂️ 傳統卡片 (Classic Cards)", cfg.ui_mode == "cards");
        add_check(&mut s, ACTION_MODE_TABLE, "📊 儀表表格 (Modern Table)", cfg.ui_mode == "table");
    }
    menu.add_menu_ref(&style_sub);

    // 4. Layout Submenu (Cards mode only)
    if cfg.ui_mode == "cards" {
        let layout_sub = Menu::new_ref("📐 顯示佈局 (Layout)");
        {
            let mut l = layout_sub.borrow_mut();
            l.set_dark_mode(cfg.appearance != "light");
            add_check(
                &mut l,
                ACTION_LAYOUT_HORIZONTAL,
                "💻 橫向三欄並排 (Horizontal Triple)",
                cfg.layout_mode == "horizontal",
            );
            add_check(
                &mut l,
                ACTION_LAYOUT_VERTICAL,
                "📱 直立三層堆疊 (Vertical Stack)",
                cfg.layout_mode == "vertical",
            );
        }
        menu.add_menu_ref(&layout_sub);
    }

    // 5. Themes (Colors if table, Appearance always)
    if cfg.ui_mode == "table" {
        let scheme_sub = Menu::new_ref("🎨 配色 (Colors)");
        {
            let mut sc = scheme_sub.borrow_mut();
            sc.set_dark_mode(cfg.appearance != "light");
            add_check(&mut sc, ACTION_SCHEME_SCALE, "色階模式 (Scale)", cfg.color_scheme == "scale");
            add_check(&mut sc, ACTION_SCHEME_DUO, "雙色模式 (Duo)", cfg.color_scheme == "duo");
        }
        menu.add_menu_ref(&scheme_sub);
    }

    let app_sub = Menu::new_ref("🌓 外觀 (Appearance)");
    {
        let mut a = app_sub.borrow_mut();
        a.set_dark_mode(cfg.appearance != "light");
        add_check(&mut a, ACTION_APPEARANCE_AUTO, "跟隨系統 (Auto)", cfg.appearance == "auto");
        add_check(&mut a, ACTION_APPEARANCE_LIGHT, "淺色模式 (Light)", cfg.appearance == "light");
        add_check(&mut a, ACTION_APPEARANCE_DARK, "深色模式 (Dark)", cfg.appearance == "dark");
    }
    menu.add_menu_ref(&app_sub);

    // 6. Ghost Mode
    add_check(&mut menu, ACTION_CLICK_THROUGH, "👻 滑鼠點擊穿透 (Alt+Shift+C)", cfg.click_through);

    // 7. Always on top
    add_check(&mut menu, ACTION_ALWAYS_ON_TOP, "📌 視窗永遠置頂 (Always on Top)", cfg.always_on_top);

    // 8. Lock Drag
    add_check(&mut menu, ACTION_LOCK_DRAG, "🔒 鎖定視窗位置 (Lock Drag)", cfg.locked);

    // 9. Opacity Submenu
    let opacity_sub = Menu::new_ref("🌗 視窗透明度 (Opacity)");
    {
        let mut op = opacity_sub.borrow_mut();
        op.set_dark_mode(cfg.appearance != "light");
        let current_op = cfg.opacity;
        for (id, pct, val) in [
            (ACTION_OPACITY_100, 100, 1.00),
            (ACTION_OPACITY_90, 90, 0.90),
            (ACTION_OPACITY_80, 80, 0.80),
            (ACTION_OPACITY_70, 70, 0.70),
            (ACTION_OPACITY_50, 50, 0.50),
            (ACTION_OPACITY_30, 30, 0.30),
        ] {
            let is_checked = (current_op - val as f32).abs() < 0.05;
            add_check(&mut op, id, &format!("{}%", pct), is_checked);
        }
    }
    menu.add_menu_ref(&opacity_sub);

    // 10. Interval Submenu
    let interval_sub = Menu::new_ref("⏱️ 更新頻率 (Interval)");
    {
        let mut intv = interval_sub.borrow_mut();
        intv.set_dark_mode(cfg.appearance != "light");
        let cur_int = cfg.refresh_interval_sec;
        for (id, sec) in [
            (ACTION_INTERVAL_30, 30),
            (ACTION_INTERVAL_60, 60),
            (ACTION_INTERVAL_120, 120),
            (ACTION_INTERVAL_300, 300),
        ] {
            add_check(&mut intv, id, &format!("{} 秒", sec), cur_int == sec);
        }
    }
    menu.add_menu_ref(&interval_sub);

    // 11. Autostart
    add_check(&mut menu, ACTION_AUTOSTART, "🚀 開機自動啟動 (Start on Boot)", is_autostart_enabled());

    menu.add_separator();

    // 12. Logs
    add_item(&mut menu, ACTION_OPEN_LOGS, "📂 開啟記錄檔目錄 (Open Logs)");

    // 13. Reset Geometry
    add_item(&mut menu, ACTION_RESET_GEOMETRY, "📐 重設預設尺寸與位置");

    // 14. Hide HUD
    add_item(&mut menu, ACTION_HIDE_HUD, "👁️ 隱藏 HUD (Alt+C 重新喚出)");

    // 15. Exit
    add_item(&mut menu, ACTION_EXIT, "❌ 結束程式 (Exit)");

    menu
}

pub struct HUDTrayIcon {
    pub tray: Box<TrayIcon>,
    pub config: Arc<Mutex<Config>>,
}

impl HUDTrayIcon {
    pub fn new(config: Arc<Mutex<Config>>) -> Self {
        let pixmap = create_default_tray_pixmap();
        let mut tray = TrayIcon::from_pixmap("AI HUD Monitor (3-in-1)", &pixmap)
            .expect("failed to create TrayIcon");
        let _ = tray.set_tooltip("AI HUD Monitor (3-in-1)\n• Alt+C: 顯隱\n• Alt+Shift+C: 穿透模式");

        let initial_menu = {
            let cfg = config.lock();
            build_tray_menu(tray.hwnd() as isize, &cfg)
        };
        tray.set_menu(initial_menu);

        Self { tray, config }
    }

    pub fn show(&mut self) {
        let _ = self.tray.show();
    }

    pub fn update_menu_state(&mut self) {
        let cfg = self.config.lock();
        let menu = build_tray_menu(self.tray.hwnd() as isize, &cfg);
        self.tray.set_menu(menu);
    }

    pub fn handle_action(&mut self, action_id: u32, hud: &mut HUDWindow) -> bool {
        match action_id {
            ACTION_TOGGLE_VISIBILITY => {
                hud.toggle_visibility();
            }
            ACTION_REFRESH_ALL => {
                hud.trigger_refresh();
            }
            ACTION_MODE_CARDS => {
                hud.apply_ui_mode("cards");
                self.update_menu_state();
            }
            ACTION_MODE_TABLE => {
                hud.apply_ui_mode("table");
                self.update_menu_state();
            }
            ACTION_LAYOUT_HORIZONTAL => {
                {
                    let mut cfg = self.config.lock();
                    cfg.layout_mode = "horizontal".to_string();
                    crate::config::ConfigManager::save(&cfg);
                }
                hud.apply_ui_mode("cards");
                self.update_menu_state();
            }
            ACTION_LAYOUT_VERTICAL => {
                {
                    let mut cfg = self.config.lock();
                    cfg.layout_mode = "vertical".to_string();
                    crate::config::ConfigManager::save(&cfg);
                }
                hud.apply_ui_mode("cards");
                self.update_menu_state();
            }
            ACTION_APPEARANCE_AUTO => {
                {
                    let mut cfg = self.config.lock();
                    cfg.appearance = "auto".to_string();
                    crate::config::ConfigManager::save(&cfg);
                }
                hud.set_theme(true);
                self.update_menu_state();
            }
            ACTION_APPEARANCE_LIGHT => {
                {
                    let mut cfg = self.config.lock();
                    cfg.appearance = "light".to_string();
                    crate::config::ConfigManager::save(&cfg);
                }
                hud.set_theme(false);
                self.update_menu_state();
            }
            ACTION_APPEARANCE_DARK => {
                {
                    let mut cfg = self.config.lock();
                    cfg.appearance = "dark".to_string();
                    crate::config::ConfigManager::save(&cfg);
                }
                hud.set_theme(true);
                self.update_menu_state();
            }
            ACTION_SCHEME_SCALE => {
                {
                    let mut cfg = self.config.lock();
                    cfg.color_scheme = "scale".to_string();
                    crate::config::ConfigManager::save(&cfg);
                }
                let dark = hud.is_dark;
                hud.set_theme(dark);
                self.update_menu_state();
            }
            ACTION_SCHEME_DUO => {
                {
                    let mut cfg = self.config.lock();
                    cfg.color_scheme = "duo".to_string();
                    crate::config::ConfigManager::save(&cfg);
                }
                let dark = hud.is_dark;
                hud.set_theme(dark);
                self.update_menu_state();
            }
            ACTION_CLICK_THROUGH => {
                hud.toggle_click_through();
                self.update_menu_state();
            }
            ACTION_ALWAYS_ON_TOP => {
                hud.toggle_always_on_top();
                self.update_menu_state();
            }
            ACTION_LOCK_DRAG => {
                {
                    let mut cfg = self.config.lock();
                    cfg.locked = !cfg.locked;
                    crate::config::ConfigManager::save(&cfg);
                }
                self.update_menu_state();
            }
            ACTION_OPACITY_100 => {
                hud.set_opacity(1.0);
                self.update_menu_state();
            }
            ACTION_OPACITY_90 => {
                hud.set_opacity(0.9);
                self.update_menu_state();
            }
            ACTION_OPACITY_80 => {
                hud.set_opacity(0.8);
                self.update_menu_state();
            }
            ACTION_OPACITY_70 => {
                hud.set_opacity(0.7);
                self.update_menu_state();
            }
            ACTION_OPACITY_50 => {
                hud.set_opacity(0.5);
                self.update_menu_state();
            }
            ACTION_OPACITY_30 => {
                hud.set_opacity(0.3);
                self.update_menu_state();
            }
            ACTION_INTERVAL_30 => {
                hud.set_refresh_interval(30);
                self.update_menu_state();
            }
            ACTION_INTERVAL_60 => {
                hud.set_refresh_interval(60);
                self.update_menu_state();
            }
            ACTION_INTERVAL_120 => {
                hud.set_refresh_interval(120);
                self.update_menu_state();
            }
            ACTION_INTERVAL_300 => {
                hud.set_refresh_interval(300);
                self.update_menu_state();
            }
            ACTION_AUTOSTART => {
                let enabled = is_autostart_enabled();
                let _ = set_autostart(!enabled);
                self.update_menu_state();
            }
            ACTION_OPEN_LOGS => {
                crate::logger::open_log_dir();
            }
            ACTION_RESET_GEOMETRY => {
                hud.reset_geometry();
            }
            ACTION_HIDE_HUD => {
                hud.hide();
            }
            ACTION_CLAUDE_PROFILE_AUTO => {
                hud.set_claude_profile("auto");
                self.update_menu_state();
            }
            id if id >= ACTION_CLAUDE_PROFILE_BASE && id < ACTION_CLAUDE_PROFILE_BASE + 100 => {
                let idx = (id - ACTION_CLAUDE_PROFILE_BASE) as usize;
                let profiles = crate::providers::claude::discover_profiles();
                if let Some(prof) = profiles.get(idx) {
                    hud.set_claude_profile(&prof.id);
                    self.update_menu_state();
                }
            }
            ACTION_EXIT => {
                return true; // signals app exit
            }
            _ => {}
        }
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_menu_parity_cards_and_table_modes() {
        // 1. Cards mode configuration
        let mut cfg_cards = Config::default();
        cfg_cards.ui_mode = "cards".to_string();
        cfg_cards.layout_mode = "horizontal".to_string();
        cfg_cards.opacity = 0.90;
        cfg_cards.refresh_interval_sec = 60;
        cfg_cards.claude_profile = "auto".to_string();

        let menu_cards = build_hud_context_menu(&cfg_cards);
        let actions = menu_cards.actions();
        assert!(!actions.is_empty(), "Context menu must have actions");
        assert_eq!(actions[0].borrow().text(), "🔄 立即重新整理所有 AI (Refresh All)");

        // 2. Table mode configuration
        let mut cfg_table = Config::default();
        cfg_table.ui_mode = "table".to_string();
        cfg_table.color_scheme = "scale".to_string();

        let menu_table = build_hud_context_menu(&cfg_table);
        let actions_table = menu_table.actions();
        assert!(!actions_table.is_empty());

        let has_refresh = actions_table.iter().any(|a| a.borrow().data().to_u64() == Some(ACTION_REFRESH_ALL as u64));
        assert!(has_refresh);

        let has_lock = actions_table.iter().any(|a| a.borrow().data().to_u64() == Some(ACTION_LOCK_DRAG as u64));
        assert!(has_lock);

        let has_reset = actions_table.iter().any(|a| a.borrow().data().to_u64() == Some(ACTION_RESET_GEOMETRY as u64));
        assert!(has_reset);

        let has_hide = actions_table.iter().any(|a| a.borrow().data().to_u64() == Some(ACTION_HIDE_HUD as u64));
        assert!(has_hide);

        let has_exit = actions_table.iter().any(|a| a.borrow().data().to_u64() == Some(ACTION_EXIT as u64));
        assert!(has_exit);
    }
}
