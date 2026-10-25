//! System tray icon component matching Python ui/tray_icon.py.

use parking_lot::Mutex;
use std::sync::Arc;

use crate::autostart::{is_autostart_enabled, set_autostart};
use ferrokit_core::variant::Variant;
use ferrokit_gui::paint::pixmap::Pixmap;
use ferrokit_gui::tiny_skia::Color;
use ferrokit_platform::platform_tray::TrayMessageIcon;
use ferrokit_platform::tray_icon::TrayIcon;
use ferrokit_widgets::action::Action;
use ferrokit_widgets::menu::{Menu, MenuRef};

use super::hud_window::HUDWindow;
use crate::config::Config;

// Menu Action IDs
pub const ACTION_TOGGLE_VISIBILITY: u32 = 1;
pub const ACTION_REFRESH_ALL: u32 = 2;

pub const ACTION_MODE_CARDS: u32 = 10;
pub const ACTION_MODE_TABLE: u32 = 11;
pub const ACTION_MODE_SIDEBAR: u32 = 12;

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

// Sidebar style (Python `hud_menu.py`: SIDEBAR_FORMS, SIDEBAR_SCALES, IDLE_LEVELS, IDLE_DELAYS_MS).
pub const ACTION_SIDEBAR_STRIP: u32 = 110;
pub const ACTION_SIDEBAR_PANEL: u32 = 111;
pub const ACTION_SIDEBAR_SCALE_BASE: u32 = 4000; // + percent
pub const ACTION_IDLE_FADE_TOGGLE: u32 = 130;
pub const ACTION_IDLE_LEVEL_BASE: u32 = 2000; // + percent
pub const ACTION_IDLE_DELAY_BASE: u32 = 100_000; // + milliseconds
pub const ACTION_AGY_ORDER_RESET: u32 = 150;

pub const SIDEBAR_SCALES: [u32; 6] = [60, 80, 100, 125, 150, 200];
pub const IDLE_LEVELS: [u32; 4] = [80, 60, 40, 25];
pub const IDLE_DELAYS_MS: [u32; 4] = [500, 900, 2000, 5000];

/// Python `SCHEME_LABELS` (`hud_window.py:25`) in `SCHEMES` order (`styles.py:20`).
const SCHEME_CHOICES: [(&str, u32, &str); 2] = [
    ("scale", ACTION_SCHEME_SCALE, "色階 (Scale)"),
    ("duo", ACTION_SCHEME_DUO, "雙色 (Duo)"),
];

/// Python `APPEARANCE_LABELS` (`hud_window.py:26`) in `APPEARANCES` order (`styles.py:21`).
const APPEARANCE_CHOICES: [(&str, u32, &str); 3] = [
    ("auto", ACTION_APPEARANCE_AUTO, "跟隨系統 (Auto)"),
    ("light", ACTION_APPEARANCE_LIGHT, "淺色 (Light)"),
    ("dark", ACTION_APPEARANCE_DARK, "深色 (Dark)"),
];
pub fn create_default_tray_pixmap() -> Pixmap {
    const ICON_PNG_BYTES: &[u8] = include_bytes!("../../assets/app_icon.png");
    // `Pixmap::from_image` premultiplies alpha, as the pixmap format requires.
    if let Some(pm) = ferrokit_gui::image::io::ImageReader::read_from_memory(ICON_PNG_BYTES)
        .ok()
        .and_then(|img| Pixmap::from_image(&img))
    {
        return pm;
    }
    let mut pm = Pixmap::new(16, 16).unwrap();
    pm.fill(Color::from_rgba8(56, 189, 248, 255));
    pm
}

/// The `QMenu` rules of the Python HUD's active style sheet: the static cards sheet in cards
/// mode, the theme sheet (`get_hud_stylesheet`) in table mode.
fn hud_menu_style(cfg: &Config) -> ferrokit_widgets::menu::MenuStyle {
    use ferrokit_gui::text::font::Font;
    let dark = crate::ui::resolve_is_dark(&cfg.appearance);
    let table = cfg.ui_mode == "table";
    // The menu's own font is the system menu font at 9pt (12px); it sizes the rows.
    let family = ferrokit_platform::platform()
        .theme()
        .menu_font_family()
        .unwrap_or_else(|| "Segoe UI".to_string());
    let family = format!("{family}, Segoe UI, Segoe UI Emoji");
    let row_font = Font::new(family.clone(), 12.0);
    let rgba = |r: u8, g: u8, b: u8, a: u8| Color::from_rgba8(r, g, b, a);
    if table {
        let (background, border, text, hover, disabled) = if dark {
            (
                rgba(40, 40, 46, 247),
                rgba(255, 255, 255, 31),
                rgba(242, 242, 247, 255),
                rgba(255, 255, 255, 26),
                rgba(235, 235, 245, 82),
            )
        } else {
            (
                rgba(250, 250, 252, 247),
                rgba(40, 40, 50, 36),
                rgba(31, 31, 36, 255),
                rgba(0, 0, 0, 18),
                rgba(40, 40, 50, 87),
            )
        };
        ferrokit_widgets::menu::MenuStyle {
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
            (
                rgba(22, 25, 32, 255),
                rgba(255, 255, 255, 46),
                rgba(226, 232, 240, 255),
                rgba(39, 47, 61, 255),
                rgba(56, 189, 248, 255),
                rgba(255, 255, 255, 31),
            )
        } else {
            (
                rgba(248, 250, 252, 255),
                rgba(15, 23, 42, 46),
                rgba(23, 32, 51, 255),
                rgba(226, 232, 240, 255),
                rgba(3, 105, 161, 255),
                rgba(15, 23, 42, 31),
            )
        };
        ferrokit_widgets::menu::MenuStyle {
            font: Font::new(family, 11.0),
            row_font,
            background,
            border,
            text,
            disabled_text: text,
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

#[cfg(test)]
fn test_hud_root() -> ferrokit_widgets::widget::WidgetRef {
    std::rc::Rc::new(std::cell::RefCell::new(Box::new(
        ferrokit_widgets::widget::EmptyWidget::new(),
    )))
}

pub fn build_hud_context_menu(cfg: &Config, root: &ferrokit_widgets::widget::WidgetRef) -> Menu {
    let mut menu = Menu::new("");
    menu.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
    menu.set_style(hud_menu_style(cfg));
    ferrokit_widgets::Widget::set_parent_widget(&menu, Some(std::rc::Rc::downgrade(root)));

    // Refresh
    add_item(
        &mut menu,
        ACTION_REFRESH_ALL,
        "🔄 立即重新整理所有 AI (Refresh All)",
    );
    menu.add_separator();

    // Claude Account Submenu (PR 13)
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

    // UI Style, Layout, Colors/Appearance (shared with the tray menu)
    add_ui_style_menu(&mut menu, cfg);
    add_layout_menu(&mut menu, cfg);
    add_sidebar_menu(&mut menu, cfg);
    add_theme_menus(&mut menu, cfg);

    // Ghost Mode
    add_check(
        &mut menu,
        ACTION_CLICK_THROUGH,
        "👻 滑鼠點擊穿透 (Alt+Shift+C)",
        cfg.click_through,
    );

    // Always on top
    add_check(
        &mut menu,
        ACTION_ALWAYS_ON_TOP,
        "📌 視窗永遠置頂 (Always on Top)",
        cfg.always_on_top,
    );

    // Lock Drag
    add_check(
        &mut menu,
        ACTION_LOCK_DRAG,
        "🔒 鎖定視窗位置 (Lock Drag)",
        cfg.locked,
    );

    // Opacity Submenu
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

    // Interval Submenu
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

    // Autostart
    add_check(
        &mut menu,
        ACTION_AUTOSTART,
        "🚀 開機自動啟動 (Start on Boot)",
        is_autostart_enabled(),
    );

    menu.add_separator();

    // Logs, Reset Geometry, Hide HUD, Exit
    add_item(&mut menu, ACTION_OPEN_LOGS, "📂 開啟記錄檔目錄 (Open Logs)");
    add_item(&mut menu, ACTION_RESET_GEOMETRY, "📐 重設預設尺寸與位置");
    add_item(&mut menu, ACTION_HIDE_HUD, "👁️ 隱藏 HUD (Alt+C 重新喚出)");
    add_item(&mut menu, ACTION_EXIT, "❌ 結束程式 (Exit)");

    menu
}

/// Python `HUDTrayIcon` right-click menu (`tray_icon.py:43-122`). The tray and the HUD body
/// are different menus: this one never contains the body-only account, lock, opacity,
/// interval, reset or hide items.
pub fn build_hud_tray_menu(cfg: &Config, root: &ferrokit_widgets::widget::WidgetRef) -> Menu {
    let mut menu = Menu::new("");
    menu.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
    menu.set_style(hud_menu_style(cfg));
    ferrokit_widgets::Widget::set_parent_widget(&menu, Some(std::rc::Rc::downgrade(root)));

    add_item(
        &mut menu,
        ACTION_TOGGLE_VISIBILITY,
        "👁️ 顯示 / 隱藏 HUD (Alt+C)",
    );
    add_item(
        &mut menu,
        ACTION_REFRESH_ALL,
        "🔄 立即重新整理所有 AI (Refresh All)",
    );
    menu.add_separator();
    add_ui_style_menu(&mut menu, cfg);
    add_layout_menu(&mut menu, cfg);
    add_sidebar_menu(&mut menu, cfg);
    add_theme_menus(&mut menu, cfg);
    add_check(
        &mut menu,
        ACTION_CLICK_THROUGH,
        "👻 滑鼠點擊穿透 (Alt+Shift+C)",
        cfg.click_through,
    );
    add_check(
        &mut menu,
        ACTION_ALWAYS_ON_TOP,
        "📌 視窗永遠置頂",
        cfg.always_on_top,
    );
    add_check(
        &mut menu,
        ACTION_AUTOSTART,
        "🚀 開機自動啟動",
        is_autostart_enabled(),
    );
    add_item(&mut menu, ACTION_OPEN_LOGS, "📂 開啟記錄檔目錄 (Open Logs)");
    menu.add_separator();
    add_item(&mut menu, ACTION_EXIT, "❌ 結束程式 (Exit)");
    menu
}

/// A submenu styled like its parent (dark or light appearance).
fn new_submenu(title: &str, cfg: &Config) -> MenuRef {
    let sub = Menu::new_ref(title);
    sub.borrow_mut()
        .set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
    sub
}

/// Plain action whose data is `id`, the key `handle_action` dispatches on.
fn add_item(menu: &mut Menu, id: u32, text: &str) {
    let act = Action::new_ref(text);
    act.borrow_mut().set_data(Variant::uint(id as u64));
    menu.add_action(act);
}

/// Checkable action with its Python `setChecked` state.
fn add_check(menu: &mut Menu, id: u32, text: &str, checked: bool) {
    let act = Action::new_ref(text);
    {
        let mut a = act.borrow_mut();
        a.set_checkable(true);
        a.set_checked(checked);
        a.set_data(Variant::uint(id as u64));
    }
    menu.add_action(act);
}

/// Python `ui_style_menu` (`tray_icon.py:66-77`, `hud_window.py:710-721`).
fn add_ui_style_menu(menu: &mut Menu, cfg: &Config) {
    let sub = new_submenu("🎭 介面風格 (UI Style)", cfg);
    {
        let mut s = sub.borrow_mut();
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
        add_check(
            &mut s,
            ACTION_MODE_SIDEBAR,
            "📍 側邊欄 (Sidebar)",
            cfg.ui_mode == "sidebar",
        );
    }
    menu.add_menu_ref(&sub);
}

/// A disabled plain-text row (Python `QAction.setEnabled(False)`).
fn add_disabled(menu: &mut Menu, text: &str) {
    let act = Action::new_ref(text);
    act.borrow_mut().set_enabled(false);
    menu.add_action(act);
}

/// Python `hud_menu._build_interface` (the sidebar entries), `_build_agy_rows` and
/// `_build_idle_fade`: visible only in the sidebar style.
fn add_sidebar_menu(menu: &mut Menu, cfg: &Config) {
    let sub = new_submenu("📍 側邊欄 (Sidebar)", cfg);
    {
        let mut s = sub.borrow_mut();
        for (value, id, label) in [
            ("strip", ACTION_SIDEBAR_STRIP, "細條 (Strip)"),
            ("panel", ACTION_SIDEBAR_PANEL, "固定窄欄 (Panel)"),
        ] {
            add_check(&mut s, id, label, cfg.sidebar_state == value);
        }
        s.add_separator();
        let size_menu = new_submenu("大小 (Ctrl＋滾輪)", cfg);
        {
            let mut sz = size_menu.borrow_mut();
            let current = (cfg.sidebar_scale_setting() * 100.0).round() as u32;
            for pct in SIDEBAR_SCALES {
                add_check(&mut sz, ACTION_SIDEBAR_SCALE_BASE + pct, &format!("{pct}%"), pct == current);
            }
        }
        s.add_menu_ref(&size_menu);
        s.add_separator();
        // AGY row order: shown here, changed by dragging rows in the detail page
        let order = crate::agy_rows::normalize_order(&cfg.agy_order);
        let agy = new_submenu("AGY 項目順序 (Row order)", cfg);
        {
            let mut a = agy.borrow_mut();
            add_disabled(&mut a, "在詳細頁面拖拉標題可調整，前兩條為主要顯示");
            a.add_separator();
            for (index, title) in order.iter().enumerate() {
                add_disabled(&mut a, &format!("{}. {title}", index + 1));
            }
            a.add_separator();
            let reset = Action::new_ref("還原預設順序");
            {
                let mut r = reset.borrow_mut();
                r.set_data(Variant::uint(ACTION_AGY_ORDER_RESET as u64));
                r.set_enabled(order.iter().map(String::as_str).ne(crate::agy_rows::DEFAULT_ORDER));
            }
            a.add_action(reset);
        }
        s.add_menu_ref(&agy);
        s.add_separator();
        // idle fade
        let fade = new_submenu("閒置時淡化 (Idle fade)", cfg);
        {
            let mut f = fade.borrow_mut();
            add_check(&mut f, ACTION_IDLE_FADE_TOGGLE, "啟用", cfg.sidebar_idle_fade);
            f.add_separator();
            let level_menu = new_submenu("淡化到 (Dim to)", cfg);
            {
                let mut l = level_menu.borrow_mut();
                let level = (cfg.idle_fade_level_setting() * 100.0).round() as u32;
                let mut levels: Vec<u32> = IDLE_LEVELS.to_vec();
                if !levels.contains(&level) {
                    levels.push(level);
                }
                levels.sort_unstable_by(|a, b| b.cmp(a));
                for pct in levels {
                    add_check(&mut l, ACTION_IDLE_LEVEL_BASE + pct, &format!("{pct}%"), pct == level);
                }
            }
            f.add_menu_ref(&level_menu);
            let delay_menu = new_submenu("多久後淡化 (Delay)", cfg);
            {
                let mut d = delay_menu.borrow_mut();
                let delay = cfg.idle_fade_delay_setting() as u32;
                let mut delays: Vec<u32> = IDLE_DELAYS_MS.to_vec();
                if !delays.contains(&delay) {
                    delays.push(delay);
                }
                delays.sort_unstable();
                for ms in delays {
                    add_check(
                        &mut d,
                        ACTION_IDLE_DELAY_BASE + ms,
                        &format!("{} 秒", crate::ui::sidebar::format_g(ms as f64 / 1000.0)),
                        ms == delay,
                    );
                }
            }
            f.add_menu_ref(&delay_menu);
        }
        s.add_menu_ref(&fade);
    }
    let action = menu.add_menu_ref(&sub);
    action.borrow_mut().set_visible(cfg.ui_mode == "sidebar");
}

/// Python `layout_menu` (`tray_icon.py:79-90`). Visible only in cards mode, as
/// `update_menu_state` sets it (`tray_icon.py:134`).
fn add_layout_menu(menu: &mut Menu, cfg: &Config) {
    let sub = new_submenu("📐 顯示佈局 (Layout)", cfg);
    {
        let mut l = sub.borrow_mut();
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
    let action = menu.add_menu_ref(&sub);
    action.borrow_mut().set_visible(cfg.ui_mode == "cards");
}

/// Python `add_theme_menus` (`hud_window.py:652-675`): Colors, visible only in table mode,
/// then Appearance. The tray uses the same pair.
fn add_theme_menus(menu: &mut Menu, cfg: &Config) {
    let scheme_sub = new_submenu("🎨 配色 (Colors)", cfg);
    {
        let mut sc = scheme_sub.borrow_mut();
        for (value, id, label) in SCHEME_CHOICES {
            add_check(&mut sc, id, label, cfg.color_scheme == value);
        }
    }
    let scheme_action = menu.add_menu_ref(&scheme_sub);
    scheme_action
        .borrow_mut()
        .set_visible(cfg.ui_mode == "table");

    let appearance_sub = new_submenu("🌓 外觀 (Appearance)", cfg);
    {
        let mut a = appearance_sub.borrow_mut();
        for (value, id, label) in APPEARANCE_CHOICES {
            add_check(&mut a, id, label, cfg.appearance == value);
        }
    }
    menu.add_menu_ref(&appearance_sub);
}

/// Balloon sink: Python `tray_icon.showMessage(title, text, icon, msecs)`. The shell sink is
/// `TrayIcon::show_message`; tests record the calls instead.
pub(crate) type ShowMessage<'a> = &'a mut dyn FnMut(&str, &str, TrayMessageIcon, u32);

/// Python `HUDWindow.toggle_click_through` (`hud_window.py:534-536`). The tray action and the
/// global hotkey both reach it through `HUDTrayIcon::toggle_click_through`.
pub(crate) fn run_click_through_toggle(hud: &mut HUDWindow, show_message: ShowMessage<'_>) {
    hud.toggle_click_through();
    if hud.is_click_through {
        show_message(
            "👻 滑鼠穿透模式已啟用",
            "點擊將直接穿透 HUD。\n如需調整設定或移動，請按 Alt+Shift+C 或右鍵點擊系統匣圖示取消。",
            TrayMessageIcon::Information,
            4000,
        );
    }
}

/// Python `_toggle_autostart` (`tray_icon.py:144-151`).
pub(crate) fn run_autostart_toggle(
    config: &Mutex<Config>,
    is_enabled: impl FnOnce() -> bool,
    set_enabled: impl FnOnce(bool) -> bool,
    show_message: ShowMessage<'_>,
) {
    let new_val = !is_enabled();
    if set_enabled(new_val) {
        crate::config::ConfigManager::update_and_save(config, |cfg| cfg.autostart = new_val);
    } else {
        show_message(
            "開機啟動",
            "設定失敗，請檢查系統權限",
            TrayMessageIcon::Information,
            10000,
        );
    }
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

    /// The one place a click-through toggle happens: the tray action and the global hotkey both
    /// call this, so the balloon follows the new state the same way for both.
    pub fn toggle_click_through(&mut self, hud: &mut HUDWindow) {
        let tray = &mut self.tray;
        run_click_through_toggle(hud, &mut |title, msg, icon, ms| {
            let _ = tray.show_message(title, msg, icon, ms);
        });
        self.update_menu_state();
    }

    fn toggle_autostart(&mut self) {
        let tray = &mut self.tray;
        run_autostart_toggle(
            &self.config,
            is_autostart_enabled,
            set_autostart,
            &mut |title, msg, icon, ms| {
                let _ = tray.show_message(title, msg, icon, ms);
            },
        );
        self.update_menu_state();
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
            ACTION_MODE_SIDEBAR => {
                hud.apply_ui_mode("sidebar");
                self.update_menu_state();
            }
            ACTION_SIDEBAR_STRIP => {
                hud.set_sidebar_state("strip");
                self.update_menu_state();
            }
            ACTION_SIDEBAR_PANEL => {
                hud.set_sidebar_state("panel");
                self.update_menu_state();
            }
            id if SIDEBAR_SCALES
                .iter()
                .any(|pct| ACTION_SIDEBAR_SCALE_BASE + pct == id) =>
            {
                hud.set_sidebar_scale((id - ACTION_SIDEBAR_SCALE_BASE) as f64 / 100.0);
                self.update_menu_state();
            }
            ACTION_IDLE_FADE_TOGGLE => {
                hud.toggle_idle_fade();
                self.update_menu_state();
            }
            id if (ACTION_IDLE_LEVEL_BASE + 20..=ACTION_IDLE_LEVEL_BASE + 95).contains(&id) => {
                hud.set_idle_fade_level((id - ACTION_IDLE_LEVEL_BASE) as i32);
                self.update_menu_state();
            }
            id if (ACTION_IDLE_DELAY_BASE + 200..=ACTION_IDLE_DELAY_BASE + 30_000).contains(&id) => {
                hud.set_idle_fade_delay((id - ACTION_IDLE_DELAY_BASE) as i64);
                self.update_menu_state();
            }
            ACTION_AGY_ORDER_RESET => {
                hud.reset_agy_order();
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
                self.toggle_click_through(hud);
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
                self.toggle_autostart();
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
    use std::cell::{Cell, RefCell};

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

        let menu_cards = build_hud_context_menu(&cfg_cards, &test_hud_root());
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

        let menu_table = build_hud_context_menu(&cfg_table, &test_hud_root());
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
    fn tray_and_hud_menus_share_dark_mode() {
        // `auto` follows the operating system, so the test fixes the appearance.
        for (appearance, dark) in [("dark", true), ("light", false)] {
            let config = Config {
                appearance: appearance.to_string(),
                ..Config::default()
            };
            assert_eq!(
                build_hud_tray_menu(&config, &test_hud_root()).is_dark_mode(),
                dark,
                "tray {appearance}"
            );
            assert_eq!(
                build_hud_context_menu(&config, &test_hud_root()).is_dark_mode(),
                dark,
                "body {appearance}"
            );
        }
    }

    /// Restores the application device pixel ratio the other tests run under.
    struct DevicePixelRatioGuard;

    impl Drop for DevicePixelRatioGuard {
        fn drop(&mut self) {
            ferrokit_gui::text::font_database::set_application_device_pixel_ratio(1.0);
        }
    }

    /// The popup window grows leftwards when a sub-menu opens, so the menu is painted at a
    /// different offset inside it. The offset is whole device pixels, so the menu's own pixels
    /// must be the same, only shifted: a fractional logical offset (1 logical px = 1.25 device
    /// px) rounds every glyph differently and the letter spacing jumps.
    #[test]
    fn test_menu_text_does_not_depend_on_window_offset() {
        use ferrokit_gui::paint::{Painter, Pixmap};
        use ferrokit_widgets::Widget;
        let _guard = DevicePixelRatioGuard;
        ferrokit_gui::text::font_database::set_application_device_pixel_ratio(1.25);
        let mut menu = build_hud_context_menu(&Config::default(), &test_hud_root());
        menu.set_visible(true);
        let size = menu.size_hint();
        menu.set_geometry(ferrokit_gui::geometry::primitives::Rect::new(
            0,
            0,
            size.width,
            size.height,
        ));

        let (w, h) = (
            (size.width as f32 * 1.25).ceil() as u32,
            (size.height as f32 * 1.25).ceil() as u32,
        );
        let mut render = |offset: i32| {
            let mut pm = Pixmap::with_dpr(w + 16, h, 1.25).unwrap();
            pm.fill(ferrokit_gui::tiny_skia::Color::TRANSPARENT);
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

    /// Python's balloon texts, byte for byte (`hud_window.py:525-526`, `tray_icon.py:150`).
    const GHOST_NOTICE_TITLE: &str = "👻 滑鼠穿透模式已啟用";
    const GHOST_NOTICE_TEXT: &str =
        "點擊將直接穿透 HUD。\n如需調整設定或移動，請按 Alt+Shift+C 或右鍵點擊系統匣圖示取消。";
    const AUTOSTART_FAILED_TITLE: &str = "開機啟動";
    const AUTOSTART_FAILED_TEXT: &str = "設定失敗，請檢查系統權限";

    /// One balloon as the shell sink receives it: title, text, icon, duration in ms.
    type Balloon = (String, String, TrayMessageIcon, u32);

    #[test]
    fn tray_click_through_action_balloons_only_when_enabling() {
        let cfg = Arc::new(Mutex::new(Config {
            click_through: false,
            ..Config::default()
        }));
        let refresh = Arc::new(Mutex::new(
            crate::refresh_controller::RefreshController::new(60),
        ));
        let mut hud = HUDWindow::with_providers(
            Arc::clone(&cfg),
            refresh,
            crate::providers::stub::stub_providers(),
        )
        .expect("HUDWindow::with_providers");
        let balloons = RefCell::new(Vec::<Balloon>::new());
        let mut sink = |title: &str, text: &str, icon: TrayMessageIcon, ms: u32| {
            balloons
                .borrow_mut()
                .push((title.to_string(), text.to_string(), icon, ms));
        };
        let ghost_notice = (
            GHOST_NOTICE_TITLE.to_string(),
            GHOST_NOTICE_TEXT.to_string(),
            TrayMessageIcon::Information,
            4000,
        );

        run_click_through_toggle(&mut hud, &mut sink);
        assert!(
            hud.is_click_through,
            "the first toggle enables click-through"
        );
        assert_eq!(*balloons.borrow(), vec![ghost_notice]);

        run_click_through_toggle(&mut hud, &mut sink);
        assert!(
            !hud.is_click_through,
            "the second toggle disables click-through"
        );
        assert_eq!(balloons.borrow().len(), 1, "disabling must not balloon");
    }

    #[test]
    fn tray_autostart_action_balloons_and_keeps_config_when_setting_fails() {
        let cfg = Arc::new(Mutex::new(Config {
            autostart: false,
            ..Config::default()
        }));
        let balloons = RefCell::new(Vec::<Balloon>::new());
        let mut sink = |title: &str, text: &str, icon: TrayMessageIcon, ms: u32| {
            balloons
                .borrow_mut()
                .push((title.to_string(), text.to_string(), icon, ms));
        };
        let requested = Cell::new(None);

        run_autostart_toggle(
            &cfg,
            || false,
            |enable| {
                requested.set(Some(enable));
                false
            },
            &mut sink,
        );

        assert_eq!(requested.get(), Some(true));
        assert_eq!(
            *balloons.borrow(),
            vec![(
                AUTOSTART_FAILED_TITLE.to_string(),
                AUTOSTART_FAILED_TEXT.to_string(),
                TrayMessageIcon::Information,
                10000,
            )]
        );
        assert!(
            !cfg.lock().autostart,
            "a failed switch must not change config.autostart"
        );
    }

    #[test]
    fn tray_autostart_action_persists_new_value_without_balloon_when_setting_succeeds() {
        let cfg = Arc::new(Mutex::new(Config {
            autostart: false,
            ..Config::default()
        }));
        let balloons = RefCell::new(Vec::<Balloon>::new());
        let mut sink = |title: &str, text: &str, icon: TrayMessageIcon, ms: u32| {
            balloons
                .borrow_mut()
                .push((title.to_string(), text.to_string(), icon, ms));
        };
        let requested = Cell::new(None);

        run_autostart_toggle(
            &cfg,
            || false,
            |enable| {
                requested.set(Some(enable));
                true
            },
            &mut sink,
        );

        assert_eq!(requested.get(), Some(true));
        assert!(
            balloons.borrow().is_empty(),
            "a successful switch shows no balloon"
        );
        assert!(
            cfg.lock().autostart,
            "config.autostart must follow the new value"
        );
    }
}

#[cfg(test)]
mod menu_parity_tests {
    use super::*;
    use ferrokit_widgets::action::ActionRef;
    use ferrokit_widgets::menu::Menu;

    /// Visible labels of the submenu whose parent text contains `title_part`
    /// (Qt skips hidden actions in the popup, `qmenu.cpp:374, 397`).
    fn submenu_visible_labels(menu: &Menu, title_part: &str) -> Vec<String> {
        let sub = menu
            .actions()
            .iter()
            .find(|a| a.borrow().text().contains(title_part))
            .and_then(|a| a.borrow().menu())
            .expect("submenu must exist");
        let labels: Vec<String> = sub
            .borrow()
            .actions()
            .iter()
            .filter(|a| a.borrow().is_visible())
            .map(|a| a.borrow().text().to_string())
            .collect();
        labels
    }

    /// Python `SCHEME_LABELS` (`hud_window.py:25`) in the Colors submenu.
    #[test]
    fn body_scheme_labels_match_python_scheme_labels() {
        let cfg = Config {
            ui_mode: "table".to_string(),
            color_scheme: "scale".to_string(),
            ..Config::default()
        };
        let menu = build_hud_context_menu(&cfg, &test_hud_root());
        assert_eq!(
            submenu_visible_labels(&menu, "(Colors)"),
            vec!["色階 (Scale)".to_string(), "雙色 (Duo)".to_string()]
        );
    }

    /// Python `APPEARANCE_LABELS` (`hud_window.py:26`) in the Appearance submenu.
    #[test]
    fn body_appearance_labels_match_python_appearance_labels() {
        let cfg = Config {
            ui_mode: "table".to_string(),
            ..Config::default()
        };
        let menu = build_hud_context_menu(&cfg, &test_hud_root());
        assert_eq!(
            submenu_visible_labels(&menu, "(Appearance)"),
            vec![
                "跟隨系統 (Auto)".to_string(),
                "淺色 (Light)".to_string(),
                "深色 (Dark)".to_string(),
            ]
        );
    }

    /// Python's cards sheet has no `QMenu::item:disabled` rule, so Qt keeps the normal item colour
    /// on disabled rows (`qstylesheetstyle.cpp` configurePalette writes every group).
    #[test]
    fn cards_disabled_menu_rows_keep_the_normal_text_colour() {
        let cfg = Config {
            ui_mode: "cards".to_string(),
            appearance: "dark".to_string(),
            ..Config::default()
        };
        let style = hud_menu_style(&cfg);
        assert_eq!(style.disabled_text, style.text);
    }

    /// Visible top-level items in popup order; `---` marks a separator.
    fn visible_top_level(menu: &Menu) -> Vec<String> {
        menu.actions()
            .iter()
            .filter(|a| a.borrow().is_visible())
            .map(|a| {
                if a.borrow().is_separator() {
                    "---".to_string()
                } else {
                    a.borrow().text().to_string()
                }
            })
            .collect()
    }

    fn action_with_text(menu: &Menu, text: &str) -> ActionRef {
        menu.actions()
            .iter()
            .find(|a| a.borrow().text() == text)
            .cloned()
            .expect("top-level action must exist")
    }

    fn submenu_action(menu: &Menu, title_part: &str, text: &str) -> ActionRef {
        let actions = menu.actions();
        let sub = actions
            .iter()
            .find(|a| a.borrow().text().contains(title_part))
            .and_then(|a| a.borrow().menu())
            .expect("submenu must exist");
        let children = sub.borrow().actions();
        children
            .iter()
            .find(|a| a.borrow().text() == text)
            .cloned()
            .expect("submenu item must exist")
    }

    /// Python `tray_icon.py:43-122` item order in cards mode: Layout is shown, Colors is hidden.
    #[test]
    fn tray_menu_cards_mode_matches_python_tray_items() {
        let cfg = Config {
            ui_mode: "cards".to_string(),
            ..Config::default()
        };
        let menu = build_hud_tray_menu(&cfg, &test_hud_root());
        let expected: Vec<String> = [
            "👁️ 顯示 / 隱藏 HUD (Alt+C)",
            "🔄 立即重新整理所有 AI (Refresh All)",
            "---",
            "🎭 介面風格 (UI Style)",
            "📐 顯示佈局 (Layout)",
            "🌓 外觀 (Appearance)",
            "👻 滑鼠點擊穿透 (Alt+Shift+C)",
            "📌 視窗永遠置頂",
            "🚀 開機自動啟動",
            "📂 開啟記錄檔目錄 (Open Logs)",
            "---",
            "❌ 結束程式 (Exit)",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(visible_top_level(&menu), expected);
    }

    /// Python `tray_icon.py` table mode: Colors is shown between UI Style and Appearance; Layout
    /// is hidden, and the Colors children are the Python `SCHEME_LABELS`.
    #[test]
    fn tray_menu_table_mode_shows_colors_hides_layout() {
        let cfg = Config {
            ui_mode: "table".to_string(),
            ..Config::default()
        };
        let menu = build_hud_tray_menu(&cfg, &test_hud_root());
        let expected: Vec<String> = [
            "👁️ 顯示 / 隱藏 HUD (Alt+C)",
            "🔄 立即重新整理所有 AI (Refresh All)",
            "---",
            "🎭 介面風格 (UI Style)",
            "🎨 配色 (Colors)",
            "🌓 外觀 (Appearance)",
            "👻 滑鼠點擊穿透 (Alt+Shift+C)",
            "📌 視窗永遠置頂",
            "🚀 開機自動啟動",
            "📂 開啟記錄檔目錄 (Open Logs)",
            "---",
            "❌ 結束程式 (Exit)",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(visible_top_level(&menu), expected);
        assert_eq!(
            submenu_visible_labels(&menu, "(Colors)"),
            vec!["色階 (Scale)".to_string(), "雙色 (Duo)".to_string()]
        );
    }

    /// Checked state follows the config for the tray items (Python `update_menu_state`).
    #[test]
    fn tray_menu_checked_state_follows_config() {
        let cfg = Config {
            ui_mode: "cards".to_string(),
            layout_mode: "vertical".to_string(),
            appearance: "dark".to_string(),
            click_through: true,
            always_on_top: false,
            ..Config::default()
        };
        let menu = build_hud_tray_menu(&cfg, &test_hud_root());
        assert!(action_with_text(&menu, "👻 滑鼠點擊穿透 (Alt+Shift+C)")
            .borrow()
            .is_checked());
        assert!(!action_with_text(&menu, "📌 視窗永遠置頂")
            .borrow()
            .is_checked());
        assert!(
            submenu_action(&menu, "(Layout)", "📱 直立三層堆疊 (Vertical Stack)")
                .borrow()
                .is_checked()
        );
        assert!(submenu_action(&menu, "(Appearance)", "深色 (Dark)")
            .borrow()
            .is_checked());
        assert!(!submenu_action(&menu, "(Appearance)", "淺色 (Light)")
            .borrow()
            .is_checked());
    }

    /// Each tray item dispatches the id `handle_action` routes on.
    #[test]
    fn tray_menu_items_dispatch_their_handler_ids() {
        let menu = build_hud_tray_menu(&Config::default(), &test_hud_root());
        for (text, id) in [
            ("👁️ 顯示 / 隱藏 HUD (Alt+C)", ACTION_TOGGLE_VISIBILITY),
            ("🔄 立即重新整理所有 AI (Refresh All)", ACTION_REFRESH_ALL),
            ("📂 開啟記錄檔目錄 (Open Logs)", ACTION_OPEN_LOGS),
            ("❌ 結束程式 (Exit)", ACTION_EXIT),
        ] {
            assert_eq!(
                action_with_text(&menu, text).borrow().data().to_u64(),
                Some(id as u64),
                "{text}"
            );
        }
    }

    /// The sidebar entries exist only in the sidebar style (Python `hud_menu._build_interface`).
    #[test]
    fn sidebar_submenu_is_visible_only_in_the_sidebar_style() {
        let sidebar = Config {
            ui_mode: "sidebar".to_string(),
            ..Config::default()
        };
        let menu = build_hud_context_menu(&sidebar, &test_hud_root());
        assert!(visible_top_level(&menu).contains(&"📍 側邊欄 (Sidebar)".to_string()));
        let tray = build_hud_tray_menu(&sidebar, &test_hud_root());
        assert!(visible_top_level(&tray).contains(&"📍 側邊欄 (Sidebar)".to_string()));
        for mode in ["cards", "table"] {
            let cfg = Config {
                ui_mode: mode.to_string(),
                ..Config::default()
            };
            let menu = build_hud_context_menu(&cfg, &test_hud_root());
            assert!(!visible_top_level(&menu).contains(&"📍 側邊欄 (Sidebar)".to_string()), "{mode}");
        }
    }

    #[test]
    fn sidebar_submenu_lists_forms_sizes_row_order_and_idle_fade() {
        let cfg = Config {
            ui_mode: "sidebar".to_string(),
            sidebar_state: "strip".to_string(),
            sidebar_scale: 1.25,
            sidebar_idle_level: 0.5,
            sidebar_idle_delay_ms: 1200,
            ..Config::default()
        };
        let menu = build_hud_context_menu(&cfg, &test_hud_root());
        let labels = submenu_visible_labels(&menu, "(Sidebar)");
        assert_eq!(labels[0], "細條 (Strip)");
        assert_eq!(labels[1], "固定窄欄 (Panel)");
        assert!(labels.iter().any(|l| l.contains("大小")));
        assert!(labels.iter().any(|l| l.contains("AGY 項目順序")));
        assert!(labels.iter().any(|l| l.contains("閒置時淡化")));
        // the chosen form and size are the checked ones
        assert!(submenu_action(&menu, "(Sidebar)", "細條 (Strip)").borrow().is_checked());
        assert!(!submenu_action(&menu, "(Sidebar)", "固定窄欄 (Panel)").borrow().is_checked());
        let sidebar_menu = menu
            .actions()
            .iter()
            .find(|a| a.borrow().text().contains("(Sidebar)"))
            .and_then(|a| a.borrow().menu())
            .expect("sidebar submenu");
        let size = sidebar_menu
            .borrow()
            .actions()
            .iter()
            .find(|a| a.borrow().text().contains("大小"))
            .and_then(|a| a.borrow().menu())
            .expect("size submenu");
        let checked: Vec<String> = size
            .borrow()
            .actions()
            .iter()
            .filter(|a| a.borrow().is_checked())
            .map(|a| a.borrow().text().to_string())
            .collect();
        assert_eq!(checked, vec!["125%".to_string()]);
        // a value that is not one of the presets is added to its list (Python `set(IDLE_LEVELS) | {level}`)
        let fade = sidebar_menu
            .borrow()
            .actions()
            .iter()
            .find(|a| a.borrow().text().contains("閒置時淡化"))
            .and_then(|a| a.borrow().menu())
            .expect("idle fade submenu");
        let delay = fade
            .borrow()
            .actions()
            .iter()
            .find(|a| a.borrow().text().contains("多久後淡化"))
            .and_then(|a| a.borrow().menu())
            .expect("delay submenu");
        let delays: Vec<String> = delay.borrow().actions().iter().map(|a| a.borrow().text().to_string()).collect();
        assert_eq!(delays, ["0.5 秒", "0.9 秒", "1.2 秒", "2 秒", "5 秒"]);
    }
}
