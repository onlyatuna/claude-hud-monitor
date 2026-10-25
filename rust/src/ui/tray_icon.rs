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

pub const ACTION_MODE_TABLE: u32 = 11;
pub const ACTION_MODE_SIDEBAR: u32 = 12;

pub const ACTION_APPEARANCE_AUTO: u32 = 30;
pub const ACTION_APPEARANCE_LIGHT: u32 = 31;
pub const ACTION_APPEARANCE_DARK: u32 = 32;

pub const ACTION_SCHEME_SCALE: u32 = 40;
pub const ACTION_SCHEME_DUO: u32 = 41;
/// The `table_blur` switch (Python `toggle_table_blur`), in the Appearance submenu of the table style.
pub const ACTION_TABLE_BLUR: u32 = 42;

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

/// Python `hud_menu._build_appearance`: the blur check item.
const TABLE_BLUR_LABEL: &str = "毛玻璃模糊 (Blur)";

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

/// The `UI_MODES` choices of Python `hud_menu` (the retired Classic Cards style is gone).
const UI_MODE_CHOICES: [(&str, u32, &str); 2] = [
    ("table", ACTION_MODE_TABLE, "儀表表格 (Modern Table)"),
    ("sidebar", ACTION_MODE_SIDEBAR, "側邊欄 (Sidebar)"),
];

/// Python `SIDEBAR_FORMS`.
const SIDEBAR_FORM_CHOICES: [(&str, u32, &str); 2] = [
    ("strip", ACTION_SIDEBAR_STRIP, "細條 (Strip)"),
    ("panel", ACTION_SIDEBAR_PANEL, "固定窄欄 (Panel)"),
];

/// Python `hud_menu.build_hud_menu`: the one menu structure shared by the HUD right-click menu and
/// the tray icon. The top level stays short and everything else lives in four groups (Interface /
/// Appearance / Behaviour / More); a setting appears exactly once. `tray` adds show/hide on top and
/// drops the More group. The menu is built each time it is shown, so the checks are always current.
pub fn build_hud_menu(
    cfg: &Config,
    root: &ferrokit_widgets::widget::WidgetRef,
    tray: bool,
) -> Menu {
    let mut menu = Menu::new("");
    menu.set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
    menu.set_style(hud_menu_style(cfg));
    ferrokit_widgets::Widget::set_parent_widget(&menu, Some(std::rc::Rc::downgrade(root)));

    if tray {
        add_item(&mut menu, ACTION_TOGGLE_VISIBILITY, "顯示 / 隱藏 HUD (Alt+C)");
    }
    add_item(&mut menu, ACTION_REFRESH_ALL, "立即重新整理所有 AI (Refresh All)");
    menu.add_separator();
    build_interface(&mut menu, cfg);
    build_appearance(&mut menu, cfg);
    build_behaviour(&mut menu, cfg);
    if !tray {
        build_more(&mut menu, cfg);
    }
    menu.add_separator();
    add_item(&mut menu, ACTION_EXIT, "結束程式 (Exit)");
    menu
}

/// A submenu styled like its parent (dark or light appearance).
fn new_submenu(title: &str, cfg: &Config) -> MenuRef {
    let sub = Menu::new_ref(title);
    sub.borrow_mut()
        .set_dark_mode(crate::ui::resolve_is_dark(&cfg.appearance));
    sub
}

/// Adds the submenu `title` to `menu`, filled by `fill`.
fn add_submenu(menu: &mut Menu, title: &str, cfg: &Config, fill: impl FnOnce(&mut Menu)) {
    let sub = new_submenu(title, cfg);
    fill(&mut sub.borrow_mut());
    menu.add_menu_ref(&sub);
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

/// A disabled plain-text row (Python `QAction.setEnabled(False)`).
fn add_disabled(menu: &mut Menu, text: &str) {
    let act = Action::new_ref(text);
    act.borrow_mut().set_enabled(false);
    menu.add_action(act);
}

/// Python `hud_menu._build_interface`: the style, and in the sidebar style its form and size.
fn build_interface(menu: &mut Menu, cfg: &Config) {
    add_submenu(menu, "介面 (Interface)", cfg, |sub| {
        for (value, id, label) in UI_MODE_CHOICES {
            add_check(sub, id, label, cfg.ui_mode == value);
        }
        if cfg.ui_mode == "sidebar" {
            sub.add_separator();
            for (value, id, label) in SIDEBAR_FORM_CHOICES {
                add_check(sub, id, label, cfg.sidebar_state == value);
            }
            sub.add_separator();
            add_submenu(sub, "大小 (Ctrl＋滾輪)", cfg, |size| {
                let current = (cfg.sidebar_scale_setting() * 100.0).round() as u32;
                for pct in SIDEBAR_SCALES {
                    add_check(size, ACTION_SIDEBAR_SCALE_BASE + pct, &format!("{pct}%"), pct == current);
                }
            });
        }
        sub.add_separator();
        build_agy_rows(sub, cfg);
    });
}

/// Python `hud_menu._build_agy_rows`: shows the current agy row order; the order itself is changed by
/// dragging rows in the detail page.
fn build_agy_rows(menu: &mut Menu, cfg: &Config) {
    let order = crate::agy_rows::normalize_order(&cfg.agy_order);
    add_submenu(menu, "AGY 項目順序 (Row order)", cfg, |sub| {
        add_disabled(sub, "在詳細頁面拖拉標題可調整，前兩條為主要顯示");
        sub.add_separator();
        for (index, title) in order.iter().enumerate() {
            add_disabled(sub, &format!("{}. {title}", index + 1));
        }
        sub.add_separator();
        let reset = Action::new_ref("還原預設順序");
        {
            let mut r = reset.borrow_mut();
            r.set_data(Variant::uint(ACTION_AGY_ORDER_RESET as u64));
            r.set_enabled(order.iter().map(String::as_str).ne(crate::agy_rows::DEFAULT_ORDER));
        }
        sub.add_action(reset);
    });
}

/// Python `hud_menu._build_appearance`: light / dark, in the table style its colour scheme and the
/// blur switch, then the window opacity.
fn build_appearance(menu: &mut Menu, cfg: &Config) {
    add_submenu(menu, "外觀 (Appearance)", cfg, |sub| {
        for (value, id, label) in APPEARANCE_CHOICES {
            add_check(sub, id, label, cfg.appearance == value);
        }
        if cfg.ui_mode == "table" {
            sub.add_separator();
            for (value, id, label) in SCHEME_CHOICES {
                add_check(sub, id, label, cfg.color_scheme == value);
            }
            add_check(sub, ACTION_TABLE_BLUR, TABLE_BLUR_LABEL, cfg.table_blur);
        }
        sub.add_separator();
        add_submenu(sub, "視窗透明度 (Opacity)", cfg, |opacity| {
            for (id, pct, val) in [
                (ACTION_OPACITY_100, 100, 1.00),
                (ACTION_OPACITY_90, 90, 0.90),
                (ACTION_OPACITY_80, 80, 0.80),
                (ACTION_OPACITY_70, 70, 0.70),
                (ACTION_OPACITY_50, 50, 0.50),
                (ACTION_OPACITY_30, 30, 0.30),
            ] {
                let checked = (cfg.opacity - val as f32).abs() < 0.05;
                add_check(opacity, id, &format!("{pct}%"), checked);
            }
        });
    });
}

/// Python `hud_menu._build_idle_fade`.
fn build_idle_fade(menu: &mut Menu, cfg: &Config) {
    add_submenu(menu, "閒置時淡化 (Idle fade)", cfg, |sub| {
        add_check(sub, ACTION_IDLE_FADE_TOGGLE, "啟用", cfg.sidebar_idle_fade);
        sub.add_separator();
        add_submenu(sub, "淡化到 (Dim to)", cfg, |dim| {
            let level = (cfg.idle_fade_level_setting() * 100.0).round() as u32;
            let mut levels: Vec<u32> = IDLE_LEVELS.to_vec();
            if !levels.contains(&level) {
                levels.push(level);
            }
            levels.sort_unstable_by(|a, b| b.cmp(a));
            for pct in levels {
                add_check(dim, ACTION_IDLE_LEVEL_BASE + pct, &format!("{pct}%"), pct == level);
            }
        });
        add_submenu(sub, "多久後淡化 (Delay)", cfg, |delays| {
            let delay = cfg.idle_fade_delay_setting() as u32;
            let mut all: Vec<u32> = IDLE_DELAYS_MS.to_vec();
            if !all.contains(&delay) {
                all.push(delay);
            }
            all.sort_unstable();
            for ms in all {
                add_check(
                    delays,
                    ACTION_IDLE_DELAY_BASE + ms,
                    &format!("{} 秒", crate::ui::sidebar::format_g(ms as f64 / 1000.0)),
                    ms == delay,
                );
            }
        });
    });
}

/// Python `hud_menu._build_behaviour`.
fn build_behaviour(menu: &mut Menu, cfg: &Config) {
    add_submenu(menu, "行為 (Behaviour)", cfg, |sub| {
        add_check(sub, ACTION_CLICK_THROUGH, "滑鼠點擊穿透 (Alt+Shift+C)", cfg.click_through);
        add_check(sub, ACTION_ALWAYS_ON_TOP, "視窗永遠置頂 (Always on Top)", cfg.always_on_top);
        add_check(sub, ACTION_LOCK_DRAG, "鎖定視窗位置 (Lock Drag)", cfg.locked);
        build_idle_fade(sub, cfg);
        add_check(sub, ACTION_AUTOSTART, "開機自動啟動 (Start on Boot)", is_autostart_enabled());
        sub.add_separator();
        add_submenu(sub, "⏱️ 更新頻率 (Interval)", cfg, |interval| {
            for (id, sec) in [
                (ACTION_INTERVAL_30, 30),
                (ACTION_INTERVAL_60, 60),
                (ACTION_INTERVAL_120, 120),
                (ACTION_INTERVAL_300, 300),
            ] {
                add_check(interval, id, &format!("{sec} 秒"), cfg.refresh_interval_sec == sec);
            }
        });
    });
}

/// Python `hud_menu._build_claude_accounts`.
fn build_claude_accounts(menu: &mut Menu, cfg: &Config) {
    add_submenu(menu, "Claude 帳號 (Claude Account)", cfg, |sub| {
        let current = cfg.claude_profile.as_str();
        let is_auto = current.is_empty() || current.eq_ignore_ascii_case("auto");
        let (active, _) = crate::providers::claude::resolve_active_profile(current);
        let title = if is_auto && active.id != "default" {
            format!("智慧自動追蹤 (目前: {})", active.short_name)
        } else {
            "智慧自動追蹤 (最近活躍)".to_string()
        };
        add_check(sub, ACTION_CLAUDE_PROFILE_AUTO, &title, is_auto);
        sub.add_separator();
        for (i, prof) in crate::providers::claude::discover_profiles().iter().take(40).enumerate() {
            let selected = !is_auto
                && (current.eq_ignore_ascii_case(&prof.id)
                    || (current == ".claude" && prof.id == "default"));
            add_check(sub, ACTION_CLAUDE_PROFILE_BASE + i as u32, &prof.display_name, selected);
        }
    });
}

/// Python `hud_menu._build_more`.
fn build_more(menu: &mut Menu, cfg: &Config) {
    add_submenu(menu, "更多 (More)", cfg, |sub| {
        build_claude_accounts(sub, cfg);
        sub.add_separator();
        add_item(sub, ACTION_OPEN_LOGS, "開啟記錄檔目錄 (Open Logs)");
        add_item(sub, ACTION_RESET_GEOMETRY, "重設預設尺寸與位置");
        add_item(sub, ACTION_HIDE_HUD, "隱藏 HUD (Alt+C 重新喚出)");
    });
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
        // Dynamic on-demand menu: `build_hud_menu` builds it whenever it is requested, ensuring
        // perfect parity between the HUD right-click menu and the tray without Win32 GDI caching.
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
            ACTION_TABLE_BLUR => {
                hud.toggle_table_blur();
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
        let mut menu = build_hud_menu(&Config::default(), &test_hud_root(), false);
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

    fn body(cfg: &Config) -> Menu {
        build_hud_menu(cfg, &test_hud_root(), false)
    }

    fn tray(cfg: &Config) -> Menu {
        build_hud_menu(cfg, &test_hud_root(), true)
    }

    fn config(ui_mode: &str) -> Config {
        Config {
            ui_mode: ui_mode.to_string(),
            ..Config::default()
        }
    }

    /// Items of a menu in popup order; `---` marks a separator.
    fn labels(menu: &Menu) -> Vec<String> {
        menu.actions()
            .iter()
            .map(|a| {
                if a.borrow().is_separator() {
                    "---".to_string()
                } else {
                    a.borrow().text().to_string()
                }
            })
            .collect()
    }

    /// The submenu reached by following the titles in `path` from `menu`.
    fn submenu(menu: &Menu, path: &[&str]) -> MenuRef {
        let mut current: Option<MenuRef> = None;
        for title in path {
            let actions = match &current {
                Some(m) => m.borrow().actions(),
                None => menu.actions(),
            };
            let next = actions
                .iter()
                .find(|a| a.borrow().text() == *title)
                .and_then(|a| a.borrow().menu())
                .unwrap_or_else(|| panic!("submenu {title} must exist"));
            current = Some(next);
        }
        current.expect("a path")
    }

    fn sub_labels(menu: &Menu, path: &[&str]) -> Vec<String> {
        labels(&submenu(menu, path).borrow())
    }

    /// The action `item` inside the submenu at `path` (the top level when `path` is empty).
    fn item(menu: &Menu, path: &[&str], text: &str) -> ActionRef {
        let actions = if path.is_empty() {
            menu.actions()
        } else {
            submenu(menu, path).borrow().actions()
        };
        actions
            .iter()
            .find(|a| a.borrow().text() == text)
            .cloned()
            .unwrap_or_else(|| panic!("{text} must exist"))
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Python `hud_menu.build_hud_menu` for the HUD body: refresh, the four groups, exit.
    #[test]
    fn body_menu_has_the_python_top_level() {
        for mode in ["table", "sidebar"] {
            assert_eq!(
                labels(&body(&config(mode))),
                strings(&[
                    "立即重新整理所有 AI (Refresh All)",
                    "---",
                    "介面 (Interface)",
                    "外觀 (Appearance)",
                    "行為 (Behaviour)",
                    "更多 (More)",
                    "---",
                    "結束程式 (Exit)",
                ]),
                "{mode}"
            );
        }
    }

    /// `tray=True` adds show/hide on top and drops the More group.
    #[test]
    fn tray_menu_adds_show_hide_and_drops_more() {
        for mode in ["table", "sidebar"] {
            assert_eq!(
                labels(&tray(&config(mode))),
                strings(&[
                    "顯示 / 隱藏 HUD (Alt+C)",
                    "立即重新整理所有 AI (Refresh All)",
                    "---",
                    "介面 (Interface)",
                    "外觀 (Appearance)",
                    "行為 (Behaviour)",
                    "---",
                    "結束程式 (Exit)",
                ]),
                "{mode}"
            );
        }
    }

    #[test]
    fn interface_group_offers_two_styles_and_the_sidebar_entries_only_in_the_sidebar_style() {
        let table = body(&config("table"));
        assert_eq!(
            sub_labels(&table, &["介面 (Interface)"]),
            strings(&[
                "儀表表格 (Modern Table)",
                "側邊欄 (Sidebar)",
                "---",
                "AGY 項目順序 (Row order)",
            ])
        );
        assert!(item(&table, &["介面 (Interface)"], "儀表表格 (Modern Table)").borrow().is_checked());
        assert!(!item(&table, &["介面 (Interface)"], "側邊欄 (Sidebar)").borrow().is_checked());

        let sidebar = body(&config("sidebar"));
        assert_eq!(
            sub_labels(&sidebar, &["介面 (Interface)"]),
            strings(&[
                "儀表表格 (Modern Table)",
                "側邊欄 (Sidebar)",
                "---",
                "細條 (Strip)",
                "固定窄欄 (Panel)",
                "---",
                "大小 (Ctrl＋滾輪)",
                "---",
                "AGY 項目順序 (Row order)",
            ])
        );
        assert!(item(&sidebar, &["介面 (Interface)"], "側邊欄 (Sidebar)").borrow().is_checked());
    }

    /// The colour scheme and the blur switch are table-only; the opacity is always there.
    #[test]
    fn appearance_group_shows_schemes_and_blur_only_in_the_table_style() {
        let table = body(&config("table"));
        assert_eq!(
            sub_labels(&table, &["外觀 (Appearance)"]),
            strings(&[
                "跟隨系統 (Auto)",
                "淺色 (Light)",
                "深色 (Dark)",
                "---",
                "色階 (Scale)",
                "雙色 (Duo)",
                "毛玻璃模糊 (Blur)",
                "---",
                "視窗透明度 (Opacity)",
            ])
        );
        let sidebar = body(&config("sidebar"));
        assert_eq!(
            sub_labels(&sidebar, &["外觀 (Appearance)"]),
            strings(&[
                "跟隨系統 (Auto)",
                "淺色 (Light)",
                "深色 (Dark)",
                "---",
                "視窗透明度 (Opacity)",
            ])
        );
    }

    #[test]
    fn behaviour_and_more_groups_list_the_python_items() {
        let menu = body(&config("sidebar"));
        assert_eq!(
            sub_labels(&menu, &["行為 (Behaviour)"]),
            strings(&[
                "滑鼠點擊穿透 (Alt+Shift+C)",
                "視窗永遠置頂 (Always on Top)",
                "鎖定視窗位置 (Lock Drag)",
                "閒置時淡化 (Idle fade)",
                "開機自動啟動 (Start on Boot)",
                "---",
                "⏱️ 更新頻率 (Interval)",
            ])
        );
        assert_eq!(
            sub_labels(&menu, &["更多 (More)"]),
            strings(&[
                "Claude 帳號 (Claude Account)",
                "---",
                "開啟記錄檔目錄 (Open Logs)",
                "重設預設尺寸與位置",
                "隱藏 HUD (Alt+C 重新喚出)",
            ])
        );
    }

    /// Python's cards sheet (the sidebar's) has no `QMenu::item:disabled` rule, so Qt keeps the normal
    /// item colour on disabled rows (`qstylesheetstyle.cpp` configurePalette writes every group).
    #[test]
    fn sidebar_disabled_menu_rows_keep_the_normal_text_colour() {
        let cfg = Config {
            ui_mode: "sidebar".to_string(),
            appearance: "dark".to_string(),
            ..Config::default()
        };
        let style = hud_menu_style(&cfg);
        assert_eq!(style.disabled_text, style.text);
    }

    #[test]
    fn tray_and_body_menus_share_dark_mode() {
        // `auto` follows the operating system, so the test fixes the appearance.
        for (appearance, dark) in [("dark", true), ("light", false)] {
            let cfg = Config {
                appearance: appearance.to_string(),
                ..Config::default()
            };
            assert_eq!(tray(&cfg).is_dark_mode(), dark, "tray {appearance}");
            assert_eq!(body(&cfg).is_dark_mode(), dark, "body {appearance}");
        }
    }

    /// Checked state follows the config (Python builds the menu again each time it is shown).
    #[test]
    fn checked_state_follows_config() {
        let cfg = Config {
            ui_mode: "sidebar".to_string(),
            sidebar_state: "strip".to_string(),
            appearance: "dark".to_string(),
            click_through: true,
            always_on_top: false,
            locked: true,
            ..Config::default()
        };
        let menu = tray(&cfg);
        let behaviour = ["行為 (Behaviour)"];
        assert!(item(&menu, &behaviour, "滑鼠點擊穿透 (Alt+Shift+C)").borrow().is_checked());
        assert!(!item(&menu, &behaviour, "視窗永遠置頂 (Always on Top)").borrow().is_checked());
        assert!(item(&menu, &behaviour, "鎖定視窗位置 (Lock Drag)").borrow().is_checked());
        assert!(item(&menu, &["外觀 (Appearance)"], "深色 (Dark)").borrow().is_checked());
        assert!(!item(&menu, &["外觀 (Appearance)"], "淺色 (Light)").borrow().is_checked());
        assert!(item(&menu, &["介面 (Interface)"], "細條 (Strip)").borrow().is_checked());
        assert!(!item(&menu, &["介面 (Interface)"], "固定窄欄 (Panel)").borrow().is_checked());
    }

    /// Each item dispatches the id `handle_action` routes on.
    #[test]
    fn items_dispatch_their_handler_ids() {
        let id = |menu: &Menu, path: &[&str], text: &str| {
            item(menu, path, text).borrow().data().to_u64()
        };
        let tray_menu = tray(&config("table"));
        assert_eq!(
            id(&tray_menu, &[], "顯示 / 隱藏 HUD (Alt+C)"),
            Some(ACTION_TOGGLE_VISIBILITY as u64)
        );
        let menu = body(&config("table"));
        for (path, text, expected) in [
            (vec![], "立即重新整理所有 AI (Refresh All)", ACTION_REFRESH_ALL),
            (vec![], "結束程式 (Exit)", ACTION_EXIT),
            (vec!["介面 (Interface)"], "儀表表格 (Modern Table)", ACTION_MODE_TABLE),
            (vec!["介面 (Interface)"], "側邊欄 (Sidebar)", ACTION_MODE_SIDEBAR),
            (vec!["外觀 (Appearance)"], "毛玻璃模糊 (Blur)", ACTION_TABLE_BLUR),
            (vec!["行為 (Behaviour)"], "滑鼠點擊穿透 (Alt+Shift+C)", ACTION_CLICK_THROUGH),
            (vec!["行為 (Behaviour)"], "視窗永遠置頂 (Always on Top)", ACTION_ALWAYS_ON_TOP),
            (vec!["行為 (Behaviour)"], "鎖定視窗位置 (Lock Drag)", ACTION_LOCK_DRAG),
            (vec!["行為 (Behaviour)"], "開機自動啟動 (Start on Boot)", ACTION_AUTOSTART),
            (vec!["更多 (More)"], "開啟記錄檔目錄 (Open Logs)", ACTION_OPEN_LOGS),
            (vec!["更多 (More)"], "重設預設尺寸與位置", ACTION_RESET_GEOMETRY),
            (vec!["更多 (More)"], "隱藏 HUD (Alt+C 重新喚出)", ACTION_HIDE_HUD),
        ] {
            assert_eq!(id(&menu, &path, text), Some(expected as u64), "{text}");
        }
    }

    #[test]
    fn sidebar_entries_list_the_size_the_row_order_and_the_idle_fade() {
        let cfg = Config {
            ui_mode: "sidebar".to_string(),
            sidebar_state: "strip".to_string(),
            sidebar_scale: 1.25,
            sidebar_idle_level: 0.5,
            sidebar_idle_delay_ms: 1200,
            ..Config::default()
        };
        let menu = body(&cfg);
        let checked = |path: &[&str]| -> Vec<String> {
            submenu(&menu, path)
                .borrow()
                .actions()
                .iter()
                .filter(|a| a.borrow().is_checked())
                .map(|a| a.borrow().text().to_string())
                .collect()
        };
        assert_eq!(checked(&["介面 (Interface)", "大小 (Ctrl＋滾輪)"]), strings(&["125%"]));
        // a value that is not one of the presets is added to its list (Python `set(IDLE_LEVELS) | {level}`)
        let fade = ["行為 (Behaviour)", "閒置時淡化 (Idle fade)"];
        assert_eq!(
            sub_labels(&menu, &[fade[0], fade[1], "多久後淡化 (Delay)"]),
            ["0.5 秒", "0.9 秒", "1.2 秒", "2 秒", "5 秒"]
        );
        assert_eq!(
            checked(&[fade[0], fade[1], "淡化到 (Dim to)"]),
            strings(&["50%"])
        );
        // the AGY row order is listed, and only the reset entry can be chosen
        let rows = sub_labels(&menu, &["介面 (Interface)", "AGY 項目順序 (Row order)"]);
        assert!(rows.iter().any(|l| l.starts_with("1. ")));
        assert!(rows.contains(&"還原預設順序".to_string()));
    }
}
