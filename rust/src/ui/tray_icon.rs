//! System tray icon component matching Python ui/tray_icon.py.

use parking_lot::Mutex;
use std::sync::Arc;

use crate::autostart::{is_autostart_enabled, set_autostart};
use qtrs_gui::paint::pixmap::Pixmap;
use qtrs_gui::tiny_skia::Color;
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
    // `Pixmap::from_image` premultiplies alpha, as the pixmap format requires.
    if let Some(pm) = qtrs_gui::image::io::ImageReader::read_from_memory(ICON_PNG_BYTES)
        .ok()
        .and_then(|img| Pixmap::from_image(&img))
    {
        return pm;
    }
    let mut pm = Pixmap::new(16, 16).unwrap();
    pm.fill(Color::from_rgba8(56, 189, 248, 255));
    pm
}

#[allow(dead_code)]
pub fn build_tray_menu(native_handle: isize, cfg: &Config) -> Box<dyn PlatformMenu> {
    let mut menu = create_platform_menu(native_handle);

    // 1. Refresh & Toggle
    menu.add_action(ACTION_REFRESH_ALL, "🔄 立即重新整理所有 AI (Refresh All)");
    menu.add_action(ACTION_TOGGLE_VISIBILITY, "👁️ 顯示 / 隱藏 HUD (Alt+C)");
    menu.add_separator();

    // 2. Claude Account Submenu
    let mut claude_sub = create_platform_menu(native_handle);
    let cur_profile = cfg.claude_profile.as_str();
    let is_auto = cur_profile.is_empty() || cur_profile.eq_ignore_ascii_case("auto");
    let (active_prof, _) = crate::providers::claude::resolve_active_profile(cur_profile);

    let auto_title = if is_auto && active_prof.id != "default" {
        format!("🎯 智慧自動追蹤 (目前: {})", active_prof.short_name)
    } else {
        "🎯 智慧自動追蹤 (最近活躍)".to_string()
    };
    claude_sub.add_checkable(ACTION_CLAUDE_PROFILE_AUTO, &auto_title, is_auto);
    claude_sub.add_separator();

    let profiles = crate::providers::claude::discover_profiles();
    for (i, prof) in profiles.iter().take(40).enumerate() {
        let is_selected = !is_auto
            && (cur_profile.eq_ignore_ascii_case(&prof.id)
                || (cur_profile == ".claude" && prof.id == "default"));
        claude_sub.add_checkable(
            ACTION_CLAUDE_PROFILE_BASE + i as u32,
            &prof.display_name,
            is_selected,
        );
    }
    menu.add_submenu("✳️ Claude 帳號 (Claude Account)", claude_sub);
    menu.add_separator();

    // 3. UI Style submenu
    let mut style_sub = create_platform_menu(native_handle);
    style_sub.add_checkable(
        ACTION_MODE_CARDS,
        "🗂️ 傳統卡片 (Classic Cards)",
        cfg.ui_mode == "cards",
    );
    style_sub.add_checkable(
        ACTION_MODE_TABLE,
        "📊 儀表表格 (Modern Table)",
        cfg.ui_mode == "table",
    );
    menu.add_submenu("🎭 介面風格 (UI Style)", style_sub);

    // 4. Layout submenu (only in cards mode)
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

    // 5. Theme submenus
    if cfg.ui_mode == "table" {
        let mut scheme_sub = create_platform_menu(native_handle);
        scheme_sub.add_checkable(
            ACTION_SCHEME_SCALE,
            "色階模式 (Scale)",
            cfg.color_scheme == "scale",
        );
        scheme_sub.add_checkable(
            ACTION_SCHEME_DUO,
            "雙色模式 (Duo)",
            cfg.color_scheme == "duo",
        );
        menu.add_submenu("🎨 配色 (Colors)", scheme_sub);
    }

    let mut app_sub = create_platform_menu(native_handle);
    app_sub.add_checkable(
        ACTION_APPEARANCE_AUTO,
        "跟隨系統 (Auto)",
        cfg.appearance == "auto",
    );
    app_sub.add_checkable(
        ACTION_APPEARANCE_LIGHT,
        "淺色模式 (Light)",
        cfg.appearance == "light",
    );
    app_sub.add_checkable(
        ACTION_APPEARANCE_DARK,
        "深色模式 (Dark)",
        cfg.appearance == "dark",
    );
    menu.add_submenu("🌓 外觀 (Appearance)", app_sub);
    menu.add_separator();

    // 6. Ghost Mode
    menu.add_checkable(
        ACTION_CLICK_THROUGH,
        "👻 滑鼠點擊穿透 (Alt+Shift+C)",
        cfg.click_through,
    );

    // 7. Always on top
    menu.add_checkable(
        ACTION_ALWAYS_ON_TOP,
        "📌 視窗永遠置頂 (Always on Top)",
        cfg.always_on_top,
    );

    // 8. Lock Drag
    menu.add_checkable(ACTION_LOCK_DRAG, "🔒 鎖定視窗位置 (Lock Drag)", cfg.locked);

    // 9. Opacity Submenu
    let mut opacity_sub = create_platform_menu(native_handle);
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
        opacity_sub.add_checkable(id, &format!("{}%", pct), is_checked);
    }
    menu.add_submenu("🌗 視窗透明度 (Opacity)", opacity_sub);

    // 10. Interval Submenu
    let mut interval_sub = create_platform_menu(native_handle);
    let cur_int = cfg.refresh_interval_sec;
    for (id, sec) in [
        (ACTION_INTERVAL_30, 30),
        (ACTION_INTERVAL_60, 60),
        (ACTION_INTERVAL_120, 120),
        (ACTION_INTERVAL_300, 300),
    ] {
        interval_sub.add_checkable(id, &format!("{} 秒", sec), cur_int == sec);
    }
    menu.add_submenu("⏱️ 更新頻率 (Interval)", interval_sub);

    // 11. Autostart
    menu.add_checkable(
        ACTION_AUTOSTART,
        "🚀 開機自動啟動 (Start on Boot)",
        is_autostart_enabled(),
    );
    menu.add_separator();

    // 12. Logs
    menu.add_action(ACTION_OPEN_LOGS, "📂 開啟記錄檔目錄 (Open Logs)");

    // 13. Reset Geometry
    menu.add_action(ACTION_RESET_GEOMETRY, "📐 重設預設尺寸與位置");

    // 14. Hide HUD
    menu.add_action(ACTION_HIDE_HUD, "👁️ 隱藏 HUD (Alt+C 重新喚出)");

    menu.add_separator();

    // 15. Exit
    menu.add_action(ACTION_EXIT, "❌ 結束程式 (Exit)");

    menu
}
/// The `QMenu` rules of the Python HUD's active style sheet: the static cards sheet in cards
/// mode, the theme sheet (`get_hud_stylesheet`) in table mode.
fn hud_menu_style(cfg: &Config) -> qtrs_widgets::menu::MenuStyle {
    use qtrs_gui::text::font::Font;
    let dark = crate::ui::resolve_is_dark(&cfg.appearance);
    let table = cfg.ui_mode == "table";
    // The menu's own font is the system menu font at 9pt (12px); it sizes the rows.
    let family = qtrs_platform::platform()
        .theme()
        .menu_font_family()
        .unwrap_or_else(|| "Segoe UI".to_string());
    let family = format!("{family}, Segoe UI, Segoe UI Emoji");
    let row_font = Font::new(family.clone(), 12.0);
    let rgba = |r: u8, g: u8, b: u8, a: u8| Color::from_rgba8(r, g, b, a);
    if table {
        let (background, border, text, hover, disabled) = if dark {
            (rgba(40, 40, 46, 247), rgba(255, 255, 255, 31), rgba(242, 242, 247, 255), rgba(255, 255, 255, 26), rgba(235, 235, 245, 82))
        } else {
            (rgba(250, 250, 252, 247), rgba(40, 40, 50, 36), rgba(31, 31, 36, 255), rgba(0, 0, 0, 18), rgba(40, 40, 50, 87))
        };
        qtrs_widgets::menu::MenuStyle {
            font: Font::new(family, 12.0),
            row_font,
            background,
            border,
            text,
            disabled_text: disabled,
            hover_background: hover,
            hover_text: text,
            separator: border,
            radius: 8.0,
            padding_v: 5,
            item_padding: [5, 26, 5, 22],
            separator_margin: [5, 10],
        }
    } else {
        let (background, border, text, hover, hover_text, separator) = if dark {
            (rgba(22, 25, 32, 255), rgba(255, 255, 255, 46), rgba(226, 232, 240, 255), rgba(39, 47, 61, 255), rgba(56, 189, 248, 255), rgba(255, 255, 255, 31))
        } else {
            (rgba(248, 250, 252, 255), rgba(15, 23, 42, 46), rgba(23, 32, 51, 255), rgba(226, 232, 240, 255), rgba(3, 105, 161, 255), rgba(15, 23, 42, 31))
        };
        qtrs_widgets::menu::MenuStyle {
            font: Font::new(family, 11.0),
            row_font,
            background,
            border,
            text,
            disabled_text: rgba(100, 116, 139, 255),
            hover_background: hover,
            hover_text,
            separator,
            radius: 6.0,
            padding_v: 4,
            item_padding: [6, 24, 6, 20],
            separator_margin: [4, 8],
        }
    }
}

pub fn build_hud_context_menu(cfg: &Config) -> qtrs_widgets::menu::Menu {
    use qtrs_core::variant::Variant;
    use qtrs_widgets::action::Action;
    use qtrs_widgets::menu::Menu;

    let mut menu = Menu::new("");
    menu.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
    menu.set_style(hud_menu_style(cfg));

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
    add_item(
        &mut menu,
        ACTION_REFRESH_ALL,
        "🔄 立即重新整理所有 AI (Refresh All)",
    );
    menu.add_separator();

    // 2. Claude Account Submenu (PR 13)
    let claude_sub = Menu::new_ref("✳️ Claude 帳號 (Claude Account)");
    {
        let mut cs = claude_sub.borrow_mut();
        cs.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));

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
        s.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
        add_check(
            &mut s,
            ACTION_MODE_CARDS,
            "🗂️ 傳統卡片 (Classic Cards)",
            cfg.ui_mode == "cards",
        );
        add_check(
            &mut s,
            ACTION_MODE_TABLE,
            "📊 儀表表格 (Modern Table)",
            cfg.ui_mode == "table",
        );
    }
    menu.add_menu_ref(&style_sub);

    // 4. Layout Submenu (Cards mode only)
    if cfg.ui_mode == "cards" {
        let layout_sub = Menu::new_ref("📐 顯示佈局 (Layout)");
        {
            let mut l = layout_sub.borrow_mut();
            l.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
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
            sc.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
            add_check(
                &mut sc,
                ACTION_SCHEME_SCALE,
                "色階模式 (Scale)",
                cfg.color_scheme == "scale",
            );
            add_check(
                &mut sc,
                ACTION_SCHEME_DUO,
                "雙色模式 (Duo)",
                cfg.color_scheme == "duo",
            );
        }
        menu.add_menu_ref(&scheme_sub);
    }

    let app_sub = Menu::new_ref("🌓 外觀 (Appearance)");
    {
        let mut a = app_sub.borrow_mut();
        a.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
        add_check(
            &mut a,
            ACTION_APPEARANCE_AUTO,
            "跟隨系統 (Auto)",
            cfg.appearance == "auto",
        );
        add_check(
            &mut a,
            ACTION_APPEARANCE_LIGHT,
            "淺色模式 (Light)",
            cfg.appearance == "light",
        );
        add_check(
            &mut a,
            ACTION_APPEARANCE_DARK,
            "深色模式 (Dark)",
            cfg.appearance == "dark",
        );
    }
    menu.add_menu_ref(&app_sub);

    // 6. Ghost Mode
    add_check(
        &mut menu,
        ACTION_CLICK_THROUGH,
        "👻 滑鼠點擊穿透 (Alt+Shift+C)",
        cfg.click_through,
    );

    // 7. Always on top
    add_check(
        &mut menu,
        ACTION_ALWAYS_ON_TOP,
        "📌 視窗永遠置頂 (Always on Top)",
        cfg.always_on_top,
    );

    // 8. Lock Drag
    add_check(
        &mut menu,
        ACTION_LOCK_DRAG,
        "🔒 鎖定視窗位置 (Lock Drag)",
        cfg.locked,
    );

    // 9. Opacity Submenu
    let opacity_sub = Menu::new_ref("🌗 視窗透明度 (Opacity)");
    {
        let mut op = opacity_sub.borrow_mut();
        op.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
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
        intv.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
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
    add_check(
        &mut menu,
        ACTION_AUTOSTART,
        "🚀 開機自動啟動 (Start on Boot)",
        is_autostart_enabled(),
    );

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

        Self { tray, config }
    }

    pub fn show(&mut self) {
        let _ = self.tray.show();
    }

    pub fn update_menu_state(&mut self) {
        // Dynamic on-demand menu: build_hud_context_menu(&cfg) is built whenever
        // requested, ensuring perfect parity with the HUD right-click menu without Win32 GDI caching.
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
                hud.apply_cards_layout_mode("horizontal");
                self.update_menu_state();
            }
            ACTION_LAYOUT_VERTICAL => {
                hud.apply_cards_layout_mode("vertical");
                self.update_menu_state();
            }
            ACTION_APPEARANCE_AUTO => {
                crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
                    cfg.appearance = "auto".to_string();
                });
                hud.set_theme(crate::ui::resolve_is_dark("auto"));
                self.update_menu_state();
            }
            ACTION_APPEARANCE_LIGHT => {
                crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
                    cfg.appearance = "light".to_string();
                });
                hud.set_theme(false);
                self.update_menu_state();
            }
            ACTION_APPEARANCE_DARK => {
                crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
                    cfg.appearance = "dark".to_string();
                });
                hud.set_theme(true);
                self.update_menu_state();
            }
            ACTION_SCHEME_SCALE => {
                crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
                    cfg.color_scheme = "scale".to_string();
                });
                let dark = hud.is_dark;
                hud.set_theme(dark);
                self.update_menu_state();
            }
            ACTION_SCHEME_DUO => {
                crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
                    cfg.color_scheme = "duo".to_string();
                });
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
                crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
                    cfg.locked = !cfg.locked;
                });
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
            id if (ACTION_CLAUDE_PROFILE_BASE..ACTION_CLAUDE_PROFILE_BASE + 100).contains(&id) => {
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
        let cfg_cards = Config {
            ui_mode: "cards".to_string(),
            layout_mode: "horizontal".to_string(),
            opacity: 0.90,
            refresh_interval_sec: 60,
            claude_profile: "auto".to_string(),
            ..Config::default()
        };

        let menu_cards = build_hud_context_menu(&cfg_cards);
        let actions = menu_cards.actions();
        assert!(!actions.is_empty(), "Context menu must have actions");
        assert_eq!(
            actions[0].borrow().text(),
            "🔄 立即重新整理所有 AI (Refresh All)"
        );

        // 2. Table mode configuration
        let cfg_table = Config {
            ui_mode: "table".to_string(),
            color_scheme: "scale".to_string(),
            ..Config::default()
        };

        let menu_table = build_hud_context_menu(&cfg_table);
        let actions_table = menu_table.actions();
        assert!(!actions_table.is_empty());

        let has_refresh = actions_table
            .iter()
            .any(|a| a.borrow().data().to_u64() == Some(ACTION_REFRESH_ALL as u64));
        assert!(has_refresh);

        let has_lock = actions_table
            .iter()
            .any(|a| a.borrow().data().to_u64() == Some(ACTION_LOCK_DRAG as u64));
        assert!(has_lock);

        let has_reset = actions_table
            .iter()
            .any(|a| a.borrow().data().to_u64() == Some(ACTION_RESET_GEOMETRY as u64));
        assert!(has_reset);

        let has_hide = actions_table
            .iter()
            .any(|a| a.borrow().data().to_u64() == Some(ACTION_HIDE_HUD as u64));
        assert!(has_hide);

        let has_exit = actions_table
            .iter()
            .any(|a| a.borrow().data().to_u64() == Some(ACTION_EXIT as u64));
        assert!(has_exit);
    }

    #[test]
    fn test_tray_and_hud_menu_unified_parity() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let mut tray = HUDTrayIcon::new(Arc::clone(&cfg));
        tray.update_menu_state();

        // Verify the menu used by the tray is the modern styled Menu
        let menu = build_hud_context_menu(&cfg.lock());
        assert!(!menu.actions().is_empty());
        assert!(menu.is_dark_mode());
    }

    /// Restores the application device pixel ratio the other tests run under.
    struct DevicePixelRatioGuard;

    impl Drop for DevicePixelRatioGuard {
        fn drop(&mut self) {
            qtrs_gui::text::font_database::set_application_device_pixel_ratio(1.0);
        }
    }

    /// The popup window grows leftwards when a sub-menu opens, so the menu is painted at a
    /// different offset inside it. The offset is whole device pixels, so the menu's own pixels
    /// must be the same, only shifted: a fractional logical offset (1 logical px = 1.25 device
    /// px) rounds every glyph differently and the letter spacing jumps.
    #[test]
    fn test_menu_text_does_not_depend_on_window_offset() {
        use qtrs_gui::paint::{Painter, Pixmap};
        use qtrs_widgets::Widget;
        let _guard = DevicePixelRatioGuard;
        qtrs_gui::text::font_database::set_application_device_pixel_ratio(1.25);
        let mut menu = build_hud_context_menu(&Config::default());
        menu.set_visible(true);
        let size = menu.size_hint();
        menu.set_geometry(qtrs_gui::geometry::primitives::Rect::new(0, 0, size.width, size.height));

        let (w, h) = ((size.width as f32 * 1.25).ceil() as u32, (size.height as f32 * 1.25).ceil() as u32);
        let mut render = |offset: i32| {
            let mut pm = Pixmap::with_dpr(w + 16, h, 1.25).unwrap();
            pm.fill(qtrs_gui::tiny_skia::Color::TRANSPARENT);
            {
                let mut p = Painter::begin(&mut pm);
                p.translate_device(offset, 0);
                menu.paint_event(&mut p);
            }
            pm
        };
        let base = render(0);
        let stride = (w + 16) as usize * 4;
        for offset in 1..=7 {
            let shifted = render(offset);
            for y in 0..h as usize {
                let row = &base.data()[y * stride..(y + 1) * stride];
                let moved = &shifted.data()[y * stride..(y + 1) * stride];
                let n = offset as usize * 4;
                assert_eq!(
                    &row[..stride - n - 8 * 4],
                    &moved[n..stride - 8 * 4],
                    "menu row {y} changed when painted {offset} device px to the right"
                );
            }
        }
    }
}
