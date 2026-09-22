// src/ui/hud_app.rs — Main egui HUD window application

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, RichText};
use log::info;

use super::native_menu::{self, MenuAction};
use super::provider_card::render_provider_card;
use super::styles::{
    apply_hud_visuals, BG_DARK, BORDER_COLOR, COLOR_AMBER, COLOR_BLUE, COLOR_GREEN, TEXT_MUTED,
    TEXT_SECONDARY,
};
use crate::config::{
    Config, ConfigManager, DEFAULT_VERTICAL_HEIGHT, HUD_BODY_SPACING, MIN_HORIZONTAL_HEIGHT,
    MIN_HORIZONTAL_WIDTH, MIN_VERTICAL_HEIGHT, MIN_VERTICAL_WIDTH, VERTICAL_CARD_MIN_HEIGHT,
    VERTICAL_DIVIDER_LINE_HEIGHT, VERTICAL_DIVIDER_SPACING,
};
use crate::hotkey::HotkeyManager;
use crate::providers::{
    AgyProvider, ClaudeProvider, CodexProvider, Provider, UsageMetrics, PROVIDER_IDS,
};
use crate::refresh_controller::RefreshController;

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[cfg(target_os = "windows")]
pub static WAKE_MSG: AtomicU32 = AtomicU32::new(0);

#[cfg(target_os = "windows")]
static WAKE_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug)]
struct ActiveResize {
    direction: egui::viewport::ResizeDirection,
    start_cursor: egui::Pos2,
    start_rect: [i32; 4], // left, top, width, height (physical pixels)
}

pub struct HudApp {
    config: Arc<Mutex<Config>>,
    metrics: HashMap<String, UsageMetrics>,
    providers_arc: HashMap<String, Arc<dyn Provider + Send + Sync>>,
    refresh_ctrl: Arc<Mutex<RefreshController>>,

    /// Last heartbeat for sleep-resume detection
    last_heartbeat: Instant,

    hotkey: Option<HotkeyManager>,
    #[cfg(target_os = "windows")]
    active_resize: Option<ActiveResize>,
    #[cfg(target_os = "windows")]
    hwnd: isize,

    is_visible: bool,
    toggle_btn_rect: egui::Rect,
    #[cfg(not(target_os = "linux"))]
    tray_attempts: usize,
    frame_count: u64,
    last_poll_time: Instant,
    last_repaint: Instant,

    // Ghost icon texture handle
    ghost_texture: Option<egui::TextureHandle>,
    // Tray icon kept alive (Windows/macOS)
    #[cfg(not(target_os = "linux"))]
    _tray: Option<tray_icon::TrayIcon>,
}

impl HudApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: Arc<Mutex<Config>>,
        refresh_ctrl: Arc<Mutex<RefreshController>>,
    ) -> Self {
        apply_hud_visuals(&cc.egui_ctx);

        // Build Arc-wrapped providers and register them with the controller
        let providers_arc: HashMap<String, Arc<dyn Provider + Send + Sync>> = HashMap::from([
            (
                "claude".to_owned(),
                Arc::new(ClaudeProvider::with_config(Some(Arc::clone(&config))))
                    as Arc<dyn Provider + Send + Sync>,
            ),
            (
                "agy".to_owned(),
                Arc::new(AgyProvider::new()) as Arc<dyn Provider + Send + Sync>,
            ),
            (
                "codex".to_owned(),
                Arc::new(CodexProvider::new()) as Arc<dyn Provider + Send + Sync>,
            ),
        ]);

        {
            let mut ctrl = refresh_ctrl.lock().unwrap();
            ctrl.set_egui_ctx(cc.egui_ctx.clone());
            for (id, provider) in &providers_arc {
                ctrl.states.insert(id.clone(), Default::default());
                ctrl.launch(id, Arc::clone(provider));
            }
        }

        // Hotkey manager
        let (hotkey_enabled, hotkey_str) = {
            let cfg = config.lock().unwrap();
            (cfg.hotkey_enabled, cfg.hotkey.clone())
        };
        let hotkey = if hotkey_enabled {
            match HotkeyManager::start(&hotkey_str) {
                Ok(hk) => Some(hk),
                Err(e) => {
                    log::warn!("[HudApp] Hotkey registration failed: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Tray icon is built lazily in update() when the event loop is active
        #[cfg(not(target_os = "linux"))]
        let _tray = None;

        Self {
            config,
            metrics: HashMap::new(),
            providers_arc,
            refresh_ctrl,
            last_heartbeat: Instant::now(),
            hotkey,
            #[cfg(target_os = "windows")]
            active_resize: None,
            #[cfg(target_os = "windows")]
            hwnd: 0,
            is_visible: true,
            toggle_btn_rect: egui::Rect::NOTHING,
            #[cfg(not(target_os = "linux"))]
            tray_attempts: 0,
            frame_count: 0,
            last_poll_time: Instant::now(),
            last_repaint: Instant::now(),
            ghost_texture: None,
            #[cfg(not(target_os = "linux"))]
            _tray,
        }
    }

    fn request_repaint_coalesced(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if now.duration_since(self.last_repaint) >= Duration::from_millis(33) {
            self.last_repaint = now;
            ctx.request_repaint();
        }
    }

    /// Toggle layout between horizontal and vertical, updating window size immediately
    pub fn toggle_layout(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "windows")]
        {
            self.active_resize = None;
        }
        let mut cfg = self.config.lock().unwrap();
        let new_mode = if cfg.layout_mode == "horizontal" {
            "vertical"
        } else {
            "horizontal"
        };
        cfg.layout_mode = new_mode.to_owned();

        let (w, h, min_w, min_h) = if new_mode == "horizontal" {
            (
                (cfg.horizontal_width as f32).max(MIN_HORIZONTAL_WIDTH as f32),
                (cfg.horizontal_height as f32).max(MIN_HORIZONTAL_HEIGHT as f32),
                MIN_HORIZONTAL_WIDTH as f32,
                MIN_HORIZONTAL_HEIGHT as f32,
            )
        } else {
            (
                (cfg.vertical_width as f32).max(MIN_VERTICAL_WIDTH as f32),
                (cfg.vertical_height as f32).max(MIN_VERTICAL_HEIGHT as f32),
                MIN_VERTICAL_WIDTH as f32,
                MIN_VERTICAL_HEIGHT as f32,
            )
        };

        ConfigManager::save(&cfg);
        drop(cfg);

        #[cfg(target_os = "windows")]
        if self.hwnd != 0 {
            if let Some([left, top, _, _]) = get_window_rect(self.hwnd) {
                let ppp = ctx.pixels_per_point();
                let phys_w = (w * ppp).round() as i32;
                let phys_h = (h * ppp).round() as i32;
                set_window_rect(self.hwnd, left, top, phys_w, phys_h);
                ensure_window_within_monitor(self.hwnd, &self.config, ppp);
            }
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            min_w, min_h,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
    }

    /// Toggle HUD window visibility, releasing memory working set on hide.
    pub fn toggle_visibility(&mut self, ctx: &egui::Context) {
        self.is_visible = !self.is_visible;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.is_visible));
        #[cfg(target_os = "windows")]
        if self.hwnd != 0 {
            apply_no_native_titlebar(self.hwnd);
        }
        if self.is_visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.request_repaint_coalesced(ctx);
        } else {
            #[cfg(target_os = "windows")]
            trim_working_set();
        }
    }

    /// Handle menu actions dispatched from the native Win32 context menu
    pub fn handle_menu_action(&mut self, action: MenuAction, ctx: &egui::Context) {
        match action {
            MenuAction::RefreshAll => {
                self.refresh_ctrl
                    .lock()
                    .unwrap()
                    .refresh(&self.providers_arc);
            }
            MenuAction::SetLayoutHorizontal => {
                let cur = self.config.lock().unwrap().layout_mode.clone();
                if cur != "horizontal" {
                    self.toggle_layout(ctx);
                }
            }
            MenuAction::SetLayoutVertical => {
                let cur = self.config.lock().unwrap().layout_mode.clone();
                if cur != "vertical" {
                    self.toggle_layout(ctx);
                }
            }
            MenuAction::ToggleClickThrough => {
                let mut cfg = self.config.lock().unwrap();
                if !cfg.click_through && !cfg.hotkey_enabled {
                    log::info!(
                        "[HudApp] Enabling hotkeys because ghost mode requires keyboard toggle"
                    );
                    cfg.hotkey_enabled = true;
                    if self.hotkey.is_none() {
                        if let Ok(hk) = HotkeyManager::start(&cfg.hotkey) {
                            hk.set_context(ctx.clone());
                            self.hotkey = Some(hk);
                        }
                    }
                }
                cfg.click_through = !cfg.click_through;
                let ct = cfg.click_through;
                ConfigManager::save(&cfg);
                drop(cfg);
                #[cfg(not(target_os = "windows"))]
                ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(ct));
                #[cfg(target_os = "windows")]
                {
                    apply_win32_click_through(self.hwnd, ct);
                    apply_no_native_titlebar(self.hwnd);
                }
            }
            MenuAction::ToggleAlwaysOnTop => {
                let mut cfg = self.config.lock().unwrap();
                cfg.always_on_top = !cfg.always_on_top;
                let aot = cfg.always_on_top;
                ConfigManager::save(&cfg);
                let level = if aot {
                    egui::viewport::WindowLevel::AlwaysOnTop
                } else {
                    egui::viewport::WindowLevel::Normal
                };
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            }
            MenuAction::ToggleLock => {
                let mut cfg = self.config.lock().unwrap();
                cfg.locked = !cfg.locked;
                ConfigManager::save(&cfg);
            }
            MenuAction::SetOpacity(pct) => {
                let mut cfg = self.config.lock().unwrap();
                cfg.opacity = pct as f32 / 100.0;
                ConfigManager::save(&cfg);
                ctx.request_repaint();
            }
            MenuAction::SetInterval(sec) => {
                let mut cfg = self.config.lock().unwrap();
                cfg.refresh_interval_sec = sec;
                ConfigManager::save(&cfg);
                self.refresh_ctrl.lock().unwrap().set_interval(sec);
            }
            MenuAction::SetClaudeProfile(profile_id) => {
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.claude_profile = profile_id;
                    ConfigManager::save(&cfg);
                }
                self.refresh_ctrl
                    .lock()
                    .unwrap()
                    .refresh(&self.providers_arc);
                ctx.request_repaint();
            }
            MenuAction::ToggleAutostart => {
                let cur = crate::autostart::is_autostart_enabled();
                if crate::autostart::set_autostart(!cur) {
                    self.config.lock().unwrap().autostart = !cur;
                    ConfigManager::save(&self.config.lock().unwrap());
                }
            }
            MenuAction::ResetGeometry => {
                let mode = self.config.lock().unwrap().layout_mode.clone();
                let (w, h) = if mode == "horizontal" {
                    (690.0, 152.0)
                } else {
                    (280.0, DEFAULT_VERTICAL_HEIGHT as f32)
                };
                let (def_x, def_y) = get_primary_monitor_default_pos(w as i32, h as i32);
                {
                    let mut cfg = self.config.lock().unwrap();
                    if mode == "horizontal" {
                        cfg.horizontal_width = 690;
                        cfg.horizontal_height = 152;
                    } else {
                        cfg.vertical_width = 280;
                        cfg.vertical_height = DEFAULT_VERTICAL_HEIGHT;
                    }
                    cfg.window_x = Some(def_x);
                    cfg.window_y = Some(def_y);
                    ConfigManager::save(&cfg);
                }
                #[cfg(target_os = "windows")]
                if self.hwnd != 0 {
                    let ppp = ctx.pixels_per_point();
                    let phys_x = (def_x as f32 * ppp).round() as i32;
                    let phys_y = (def_y as f32 * ppp).round() as i32;
                    let phys_w = (w * ppp).round() as i32;
                    let phys_h = (h * ppp).round() as i32;
                    set_window_rect(self.hwnd, phys_x, phys_y, phys_w, phys_h);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    def_x as f32,
                    def_y as f32,
                )));
            }
            MenuAction::OpenLogs => {
                crate::logger::open_log_dir();
            }
            MenuAction::ToggleHide => {
                self.toggle_visibility(ctx);
            }
            MenuAction::Exit => {
                std::process::exit(0);
            }
        }
    }
}

impl eframe::App for HudApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // transparent window margin
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count = self.frame_count.wrapping_add(1);
        if self.frame_count == 1 {
            if let Some(hk) = &self.hotkey {
                hk.set_context(ctx.clone());
            }
            self.refresh_ctrl.lock().unwrap().set_egui_ctx(ctx.clone());
        }
        #[cfg(target_os = "windows")]
        if self.frame_count == 10 {
            trim_working_set();
        }

        #[cfg(target_os = "windows")]
        if WAKE_REQUESTED.swap(false, Ordering::Relaxed) {
            self.is_visible = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.request_repaint_coalesced(ctx);
        }

        // Drain worker results
        let updates = {
            let mut ctrl = self.refresh_ctrl.lock().unwrap();
            ctrl.drain_results(&self.providers_arc)
        };
        for m in updates {
            self.metrics.insert(m.provider_id.clone(), m);
        }

        // Poll for scheduled refreshes at a reduced cadence to avoid per-frame scheduler churn.
        let now = Instant::now();
        if now.duration_since(self.last_poll_time) >= Duration::from_millis(250) {
            let mut ctrl = self.refresh_ctrl.lock().unwrap();
            ctrl.poll(&self.providers_arc);
            self.last_poll_time = now;
        }

        // Sleep-resume detection
        if now.duration_since(self.last_heartbeat) > Duration::from_secs(15) {
            info!("[HudApp] Wake from sleep detected, triggering refresh");
            let mut ctrl = self.refresh_ctrl.lock().unwrap();
            ctrl.refresh(&self.providers_arc);
        }
        self.last_heartbeat = now;

        // Hotkey polling
        let (hk_toggle, hk_ct) = if let Some(hk) = &self.hotkey {
            (hk.poll_toggle(), hk.poll_clickthrough())
        } else {
            (false, false)
        };

        if hk_toggle {
            self.toggle_visibility(ctx);
        }
        if hk_ct {
            let mut cfg = self.config.lock().unwrap();
            cfg.click_through = !cfg.click_through;
            let ct = cfg.click_through;
            ConfigManager::save(&cfg);
            drop(cfg);
            #[cfg(not(target_os = "windows"))]
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(ct));
            #[cfg(target_os = "windows")]
            {
                apply_win32_click_through(self.hwnd, ct);
                apply_no_native_titlebar(self.hwnd);
            }
        }

        // One-shot: apply stored click_through on first rendered frame (frame_count==2 means
        // hwnd is also initialised above). Only fire once via the flag.
        if self.frame_count == 2 {
            let mut cfg = self.config.lock().unwrap();
            if cfg.click_through && !cfg.hotkey_enabled {
                log::info!("[HudApp] Restoring hotkeys because click-through is enabled in config");
                cfg.hotkey_enabled = true;
                if self.hotkey.is_none() {
                    if let Ok(hk) = HotkeyManager::start(&cfg.hotkey) {
                        hk.set_context(ctx.clone());
                        self.hotkey = Some(hk);
                    }
                }
                ConfigManager::save(&cfg);
            }
            let ct = cfg.click_through;
            drop(cfg);
            #[cfg(not(target_os = "windows"))]
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(ct));
            #[cfg(target_os = "windows")]
            {
                apply_win32_click_through(self.hwnd, ct);
                apply_no_native_titlebar(self.hwnd);
            }
        }

        #[cfg(target_os = "windows")]
        if self.hwnd == 0 {
            self.hwnd = get_window_hwnd();
            if self.hwnd != 0 {
                init_win32_window_frame(self.hwnd);
                let ppp = ctx.pixels_per_point();
                ensure_window_within_monitor(self.hwnd, &self.config, ppp);
            }
        }
        #[cfg(target_os = "windows")]
        if self.hwnd != 0 {
            apply_no_native_titlebar(self.hwnd);
        }

        // Lazy tray icon initialization — attempt exactly once when the window handle is ready.
        // If Shell_NotifyIcon fails (e.g. Explorer not yet ready), tray-icon's built-in
        // TaskbarCreated broadcast handler will automatically re-register the icon.
        #[cfg(not(target_os = "linux"))]
        if self._tray.is_none() && self.tray_attempts == 0 {
            #[cfg(target_os = "windows")]
            let ready = self.hwnd != 0;
            #[cfg(not(target_os = "windows"))]
            let ready = true;

            if ready {
                self.tray_attempts = 1;
                self._tray = build_tray_icon();
            }
        }

        // System tray event polling
        #[cfg(not(target_os = "linux"))]
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            match event {
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                } => {
                    self.toggle_visibility(ctx);
                }
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Right,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                } => {
                    let cfg = self.config.lock().unwrap().clone();
                    let is_as = crate::autostart::is_autostart_enabled();
                    #[cfg(target_os = "windows")]
                    let hwnd = self.hwnd;
                    #[cfg(not(target_os = "windows"))]
                    let hwnd = 0;
                    if let Some(action) = native_menu::show_native_context_menu(hwnd, &cfg, is_as) {
                        self.handle_menu_action(action, ctx);
                    }
                }
                _ => {}
            }
        }

        let (layout_mode, is_locked, is_clickthrough) = {
            let cfg = self.config.lock().unwrap();
            (cfg.layout_mode.clone(), cfg.locked, cfg.click_through)
        };

        // ══════════════════════════════════════════════════════════════
        // Real-Time Smooth Drag-to-Resize Engine
        // ══════════════════════════════════════════════════════════════
        let screen_rect = ctx.screen_rect();
        const RESIZE_MARGIN: f32 = 8.0;

        let mut hovered_resize_edge = None;

        #[cfg(target_os = "windows")]
        {
            if let Some(resize) = self.active_resize {
                // Keep the resize cursor active during the entire drag
                set_resize_cursor(ctx, resize.direction);
                hovered_resize_edge = Some(resize.direction);

                if !is_lbutton_down() {
                    // Drag finished: release mouse capture and persist geometry
                    self.active_resize = None;
                    if self.hwnd != 0 {
                        release_mouse_capture();
                        let ppp = ctx.pixels_per_point();
                        sync_window_geometry(self.hwnd, &self.config, ppp);
                    }
                    ctx.request_repaint();
                } else if self.hwnd != 0 {
                    if let Some(cur_pos) = get_cursor_screen_pos() {
                        let dx = (cur_pos.x - resize.start_cursor.x).round() as i32;
                        let dy = (cur_pos.y - resize.start_cursor.y).round() as i32;

                        let x0 = resize.start_rect[0];
                        let y0 = resize.start_rect[1];
                        let w0 = resize.start_rect[2];
                        let h0 = resize.start_rect[3];

                        let ppp = ctx.pixels_per_point();
                        let (min_w, min_h) = if layout_mode == "horizontal" {
                            (
                                (MIN_HORIZONTAL_WIDTH as f32 * ppp).round() as i32,
                                (MIN_HORIZONTAL_HEIGHT as f32 * ppp).round() as i32,
                            )
                        } else {
                            (
                                (MIN_VERTICAL_WIDTH as f32 * ppp).round() as i32,
                                (MIN_VERTICAL_HEIGHT as f32 * ppp).round() as i32,
                            )
                        };

                        let mut new_x = x0;
                        let mut new_y = y0;
                        let mut new_w = w0;
                        let mut new_h = h0;

                        use egui::viewport::ResizeDirection::*;
                        match resize.direction {
                            East => {
                                new_w = (w0 + dx).max(min_w);
                            }
                            West => {
                                new_w = (w0 - dx).max(min_w);
                                new_x = x0 + (w0 - new_w);
                            }
                            South => {
                                new_h = (h0 + dy).max(min_h);
                            }
                            North => {
                                new_h = (h0 - dy).max(min_h);
                                new_y = y0 + (h0 - new_h);
                            }
                            SouthEast => {
                                new_w = (w0 + dx).max(min_w);
                                new_h = (h0 + dy).max(min_h);
                            }
                            SouthWest => {
                                new_w = (w0 - dx).max(min_w);
                                new_x = x0 + (w0 - new_w);
                                new_h = (h0 + dy).max(min_h);
                            }
                            NorthEast => {
                                new_w = (w0 + dx).max(min_w);
                                new_x = x0;
                                new_h = (h0 - dy).max(min_h);
                                new_y = y0 + (h0 - new_h);
                            }
                            NorthWest => {
                                new_w = (w0 - dx).max(min_w);
                                new_x = x0 + (w0 - new_w);
                                new_h = (h0 - dy).max(min_h);
                                new_y = y0 + (h0 - new_h);
                            }
                        }

                        let cur_rect = get_window_rect(self.hwnd);
                        if cur_rect != Some([new_x, new_y, new_w, new_h]) {
                            set_window_rect(self.hwnd, new_x, new_y, new_w, new_h);
                        }
                    }
                    self.request_repaint_coalesced(ctx);
                }
            } else if !is_locked && !is_clickthrough {
                if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                    let left = pos.x <= screen_rect.left() + RESIZE_MARGIN;
                    let right = pos.x >= screen_rect.right() - RESIZE_MARGIN;
                    let top = pos.y <= screen_rect.top() + RESIZE_MARGIN;
                    let bottom = pos.y >= screen_rect.bottom() - RESIZE_MARGIN;

                    hovered_resize_edge = match (top, bottom, left, right) {
                        (true, _, true, _) => Some(egui::viewport::ResizeDirection::NorthWest),
                        (true, _, _, true) => Some(egui::viewport::ResizeDirection::NorthEast),
                        (_, true, true, _) => Some(egui::viewport::ResizeDirection::SouthWest),
                        (_, true, _, true) => Some(egui::viewport::ResizeDirection::SouthEast),
                        (true, _, _, _) => Some(egui::viewport::ResizeDirection::North),
                        (_, true, _, _) => Some(egui::viewport::ResizeDirection::South),
                        (_, _, true, _) => Some(egui::viewport::ResizeDirection::West),
                        (_, _, _, true) => Some(egui::viewport::ResizeDirection::East),
                        _ => None,
                    };

                    if let Some(dir) = hovered_resize_edge {
                        set_resize_cursor(ctx, dir);
                        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                            && self.hwnd != 0
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                        }
                    }
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        if !is_locked && !is_clickthrough {
            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                let left = pos.x <= screen_rect.left() + RESIZE_MARGIN;
                let right = pos.x >= screen_rect.right() - RESIZE_MARGIN;
                let top = pos.y <= screen_rect.top() + RESIZE_MARGIN;
                let bottom = pos.y >= screen_rect.bottom() - RESIZE_MARGIN;

                hovered_resize_edge = match (top, bottom, left, right) {
                    (true, _, true, _) => Some(egui::viewport::ResizeDirection::NorthWest),
                    (true, _, _, true) => Some(egui::viewport::ResizeDirection::NorthEast),
                    (_, true, true, _) => Some(egui::viewport::ResizeDirection::SouthWest),
                    (_, true, _, true) => Some(egui::viewport::ResizeDirection::SouthEast),
                    (true, _, _, _) => Some(egui::viewport::ResizeDirection::North),
                    (_, true, _, _) => Some(egui::viewport::ResizeDirection::South),
                    (_, _, true, _) => Some(egui::viewport::ResizeDirection::West),
                    (_, _, _, true) => Some(egui::viewport::ResizeDirection::East),
                    _ => None,
                };

                if let Some(dir) = hovered_resize_edge {
                    set_resize_cursor(ctx, dir);
                    if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                    }
                }
            }
        }

        // ══════════════════════════════════════════════════════════════
        // Main Central Panel (Frameless HUD Container matching get_hud_stylesheet)
        // ══════════════════════════════════════════════════════════════
        let opacity = self.config.lock().unwrap().opacity.clamp(0.1, 1.0);
        let show_hover_border = ctx
            .input(|input| input.pointer.hover_pos())
            .map(|pos| ctx.screen_rect().contains(pos))
            .unwrap_or(false);

        let hud_frame = egui::Frame::none()
            .fill(BG_DARK)
            .rounding(9.0)
            .stroke(egui::Stroke::new(
                1.0_f32,
                if show_hover_border {
                    BORDER_COLOR
                } else {
                    Color32::TRANSPARENT
                },
            ))
            .multiply_with_opacity(opacity)
            .inner_margin(egui::Margin {
                left: 10.0,
                right: 10.0,
                top: 5.0,
                bottom: 5.0,
            });

        egui::CentralPanel::default()
            .frame(hud_frame)
            .show(ctx, |ui| {
                ui.multiply_opacity(opacity);
                let outer_rect = ui.max_rect();

                let is_resizing = {
                    #[cfg(target_os = "windows")]
                    {
                        self.active_resize.is_some()
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        false
                    }
                };

                // Window dragging, double click to refresh, right-click native menu
                let sense = if is_clickthrough {
                    egui::Sense::hover()
                } else {
                    egui::Sense::click_and_drag()
                };
                let drag_interact = ui.interact(outer_rect, egui::Id::new("hud_main_area"), sense);

                let mouse_over_toggle = if self.toggle_btn_rect.is_positive() {
                    ctx.input(|i| i.pointer.hover_pos())
                        .map(|p| self.toggle_btn_rect.contains(p))
                        .unwrap_or(false)
                } else {
                    false
                };

                let is_hovered = ctx
                    .input(|i| i.pointer.hover_pos())
                    .map(|p| outer_rect.contains(p))
                    .unwrap_or(false);

                // Native smooth DWM drag-to-move immediately upon left mouse button pressed
                if is_hovered
                    && hovered_resize_edge.is_none()
                    && !is_resizing
                    && !mouse_over_toggle
                    && !is_locked
                    && !is_clickthrough
                    && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                {
                    #[cfg(target_os = "windows")]
                    if self.hwnd != 0 {
                        native_drag_window(self.hwnd);
                        let ppp = ctx.pixels_per_point();
                        ensure_window_within_monitor(self.hwnd, &self.config, ppp);
                        sync_window_position(self.hwnd, &self.config, ppp);
                        ctx.request_repaint();
                    }
                    #[cfg(not(target_os = "windows"))]
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                if !is_clickthrough && drag_interact.double_clicked() {
                    self.refresh_ctrl
                        .lock()
                        .unwrap()
                        .refresh(&self.providers_arc);
                }

                // Context Menu on Right Click
                #[cfg(target_os = "windows")]
                if !is_clickthrough && drag_interact.secondary_clicked() {
                    let cfg = self.config.lock().unwrap().clone();
                    let is_as = crate::autostart::is_autostart_enabled();
                    if let Some(action) =
                        native_menu::show_native_context_menu(self.hwnd, &cfg, is_as)
                    {
                        self.handle_menu_action(action, ctx);
                    }
                }

                #[cfg(not(target_os = "windows"))]
                let mut egui_menu_action: Option<native_menu::MenuAction> = None;
                #[cfg(not(target_os = "windows"))]
                if !is_clickthrough {
                    let cfg = self.config.lock().unwrap().clone();
                    let is_as = crate::autostart::is_autostart_enabled();
                    drag_interact.context_menu(|ui| {
                        egui_menu_action = native_menu::render_context_menu_items(ui, &cfg, is_as);
                    });
                }
                #[cfg(not(target_os = "windows"))]
                if let Some(action) = egui_menu_action {
                    self.handle_menu_action(action, ctx);
                }

                ui.spacing_mut().item_spacing = egui::vec2(0.0, HUD_BODY_SPACING as f32);

                // 1. Common Header
                self.render_header(ui, ctx);

                // 2. Body Layout (Horizontal 3-column or Vertical 3-row)
                if layout_mode == "horizontal" {
                    self.render_horizontal_cards(ui);
                } else {
                    self.render_vertical_cards(ui);
                }
            });

        // Request repaint every second for live countdown timer
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

fn set_resize_cursor(ctx: &egui::Context, dir: egui::viewport::ResizeDirection) {
    let cursor =
        match dir {
            egui::viewport::ResizeDirection::North | egui::viewport::ResizeDirection::South => {
                egui::CursorIcon::ResizeVertical
            }
            egui::viewport::ResizeDirection::East | egui::viewport::ResizeDirection::West => {
                egui::CursorIcon::ResizeHorizontal
            }
            egui::viewport::ResizeDirection::NorthWest
            | egui::viewport::ResizeDirection::SouthEast => egui::CursorIcon::ResizeNwSe,
            egui::viewport::ResizeDirection::NorthEast
            | egui::viewport::ResizeDirection::SouthWest => egui::CursorIcon::ResizeNeSw,
        };
    ctx.set_cursor_icon(cursor);
}

#[cfg(target_os = "windows")]
fn get_window_hwnd() -> isize {
    #[link(name = "user32")]
    extern "system" {
        fn EnumThreadWindows(
            dwThreadId: u32,
            lpfn: unsafe extern "system" fn(isize, isize) -> i32,
            lParam: isize,
        ) -> i32;
        fn GetParent(hWnd: isize) -> isize;
        fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
        fn IsWindowVisible(hWnd: isize) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    struct HwndSearch {
        target_hwnd: isize,
        visible_hwnd: isize,
        fallback_hwnd: isize,
    }

    unsafe extern "system" fn enum_proc(hwnd: isize, lparam: isize) -> i32 {
        if GetParent(hwnd) == 0 {
            let mut title = [0u16; 128];
            let len = GetWindowTextW(hwnd, title.as_mut_ptr(), 128);
            let search = &mut *(lparam as *mut HwndSearch);
            if len > 0 {
                let title_str = String::from_utf16_lossy(&title[..len as usize]);
                if title_str.contains("AI Agent HUD")
                    || title_str.contains("Claude")
                    || title_str.contains("HUD")
                {
                    search.target_hwnd = hwnd;
                    return 0; // Target found, stop enumerating
                }
            }
            if search.visible_hwnd == 0 && IsWindowVisible(hwnd) != 0 {
                search.visible_hwnd = hwnd;
            } else if search.fallback_hwnd == 0 {
                search.fallback_hwnd = hwnd;
            }
        }
        1 // Continue
    }

    let mut search = HwndSearch {
        target_hwnd: 0,
        visible_hwnd: 0,
        fallback_hwnd: 0,
    };
    unsafe {
        let tid = GetCurrentThreadId();
        EnumThreadWindows(tid, enum_proc, &mut search as *mut HwndSearch as isize);
    }
    if search.target_hwnd != 0 {
        search.target_hwnd
    } else if search.visible_hwnd != 0 {
        search.visible_hwnd
    } else {
        search.fallback_hwnd
    }
}

#[cfg(target_os = "windows")]
fn strip_native_titlebar_bits(style: i32) -> i32 {
    const WS_CAPTION: i32 = 0x00C00000;
    const WS_SYSMENU: i32 = 0x00080000;
    const WS_MINIMIZEBOX: i32 = 0x00020000;
    const WS_MAXIMIZEBOX: i32 = 0x00010000;
    const WS_THICKFRAME: i32 = 0x00040000;
    const WS_BORDER: i32 = 0x00800000;
    const WS_DLGFRAME: i32 = 0x00400000;

    style
        & !(WS_CAPTION
            | WS_SYSMENU
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX
            | WS_THICKFRAME
            | WS_BORDER
            | WS_DLGFRAME)
}

#[cfg(target_os = "windows")]
fn apply_frameless_window_style(hwnd: isize) {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongW(hWnd: isize, nIndex: i32) -> i32;
        fn SetWindowLongW(hWnd: isize, nIndex: i32, dwNewLong: i32) -> i32;
        fn SetWindowPos(
            hWnd: isize,
            hWndInsertAfter: isize,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
    }
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmExtendFrameIntoClientArea(hWnd: isize, pMarInset: *const std::ffi::c_void) -> i32;
        fn DwmSetWindowAttribute(
            hWnd: isize,
            dwAttribute: u32,
            pvAttribute: *const std::ffi::c_void,
            cbAttribute: u32,
        ) -> i32;
    }

    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const WS_POPUP: i32 = 0x80000000u32 as i32;
    const WS_VISIBLE: i32 = 0x10000000;
    const WS_CAPTION: i32 = 0x00C00000;
    const WS_SYSMENU: i32 = 0x00080000;
    const WS_THICKFRAME: i32 = 0x00040000;
    const WS_MINIMIZEBOX: i32 = 0x00020000;
    const WS_MAXIMIZEBOX: i32 = 0x00010000;
    const WS_BORDER: i32 = 0x00800000;
    const WS_DLGFRAME: i32 = 0x00400000;
    const WS_EX_WINDOWEDGE: i32 = 0x00000100;
    const WS_EX_CLIENTEDGE: i32 = 0x00000200;
    const WS_EX_DLGMODALFRAME: i32 = 0x00000001;
    const WS_EX_STATICEDGE: i32 = 0x00020000;
    const WS_EX_LAYERED: i32 = 0x00080000;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_DONOTROUND: u32 = 1;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct MARGINS {
        cxLeftWidth: i32,
        cxRightWidth: i32,
        cyTopHeight: i32,
        cyBottomHeight: i32,
    }

    if hwnd == 0 {
        return;
    }

    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE);
        let stripped = strip_native_titlebar_bits(style);
        let mut rebuilt_style = (stripped | WS_POPUP | WS_VISIBLE)
            & !(WS_CAPTION
                | WS_SYSMENU
                | WS_THICKFRAME
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_BORDER
                | WS_DLGFRAME);
        rebuilt_style |= WS_POPUP;
        if rebuilt_style != style {
            SetWindowLongW(hwnd, GWL_STYLE, rebuilt_style);
        }

        let exstyle = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let rebuilt_exstyle = (exstyle
            & !(WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME | WS_EX_STATICEDGE))
            | WS_EX_LAYERED;
        if rebuilt_exstyle != exstyle {
            SetWindowLongW(hwnd, GWL_EXSTYLE, rebuilt_exstyle);
        }

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        if rebuilt_style != style || rebuilt_exstyle != exstyle {
            let _ =
                DwmExtendFrameIntoClientArea(hwnd, &margins as *const _ as *const std::ffi::c_void);

            let corner_pref = DWMWCP_DONOTROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner_pref as *const _ as *const std::ffi::c_void,
                4,
            );

            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &DWMWA_COLOR_NONE as *const _ as *const std::ffi::c_void,
                4,
            );

            let _ = SetWindowPos(
                hwnd,
                0,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn apply_no_native_titlebar(hwnd: isize) {
    apply_frameless_window_style(hwnd);
}

#[cfg(target_os = "windows")]
fn init_win32_window_frame(hwnd: isize) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongW(hWnd: isize, nIndex: i32) -> i32;
        fn SetWindowLongW(hWnd: isize, nIndex: i32, dwNewLong: i32) -> i32;
        fn SetClassLongPtrW(hWnd: isize, nIndex: i32, dwNewLong: isize) -> isize;
        fn RegisterWindowMessageW(lpString: *const u16) -> u32;
    }
    #[link(name = "comctl32")]
    extern "system" {
        fn SetWindowSubclass(
            hWnd: isize,
            pfnSubclass: unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize,
            uIdSubclass: usize,
            dwRefData: usize,
        ) -> i32;
        fn DefSubclassProc(hWnd: isize, uMsg: u32, wParam: usize, lParam: isize) -> isize;
    }

    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TOOLWINDOW: i32 = 0x00000080;
    const GCLP_HBRBACKGROUND: i32 = -10;

    unsafe {
        apply_no_native_titlebar(hwnd);

        // Register single-instance wakeup message if not already registered
        let mut msg_id = WAKE_MSG.load(Ordering::Relaxed);
        if msg_id == 0 {
            let wake_name: Vec<u16> = OsStr::new("ClaudeHUD_WakeUp\0").encode_wide().collect();
            msg_id = RegisterWindowMessageW(wake_name.as_ptr());
            if msg_id != 0 {
                WAKE_MSG.store(msg_id, Ordering::Relaxed);
            }
        }
        if msg_id != 0 {
            #[link(name = "user32")]
            extern "system" {
                fn ChangeWindowMessageFilterEx(
                    hWnd: isize,
                    message: u32,
                    action: u32,
                    pChangeFilterStruct: *mut std::ffi::c_void,
                ) -> i32;
            }
            const MSGFLT_ALLOW: u32 = 1;
            ChangeWindowMessageFilterEx(hwnd, msg_id, MSGFLT_ALLOW, std::ptr::null_mut());
        }

        // 1. Prevent GDI from painting standard white window background brush
        SetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND, 0);

        // 2. Subclass window to absorb WM_ERASEBKGND (0x0014) and handle WakeUp
        unsafe extern "system" fn bg_subclass(
            h: isize,
            msg: u32,
            w: usize,
            l: isize,
            _id: usize,
            _data: usize,
        ) -> isize {
            #[link(name = "user32")]
            extern "system" {
                fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
                fn SetForegroundWindow(hWnd: isize) -> i32;
            }
            let wake_msg = WAKE_MSG.load(Ordering::Relaxed);
            if wake_msg != 0 && msg == wake_msg {
                WAKE_REQUESTED.store(true, Ordering::Relaxed);
                ShowWindow(h, 9); // SW_RESTORE
                SetForegroundWindow(h);
                return 0;
            }
            if msg == 0x0014 {
                // WM_ERASEBKGND
                return 1;
            }
            if msg == 0x0005 {
                // WM_SIZE: keep the native transparent window corners rounded after resize.
                apply_window_region(h);
            }
            if msg == 0x0086 {
                // WM_NCACTIVATE: the frameless HUD has no non-client state to repaint.
                log::debug!("[Win32Frame] WM_NCACTIVATE intercepted active={}", w != 0);
                return 0;
            }
            if msg == 0x0085 {
                // WM_NCPAINT: suppress transient DWM non-client frame painting.
                log::debug!("[Win32Frame] WM_NCPAINT intercepted");
                return 0;
            }
            if msg == 0x0082 {
                // WM_NCDESTROY: clean up subclass per Win32 Common Controls best practices
                #[link(name = "comctl32")]
                extern "system" {
                    fn RemoveWindowSubclass(
                        hWnd: isize,
                        pfnSubclass: unsafe extern "system" fn(
                            isize,
                            u32,
                            usize,
                            isize,
                            usize,
                            usize,
                        ) -> isize,
                        uIdSubclass: usize,
                    ) -> i32;
                }
                RemoveWindowSubclass(h, bg_subclass, 1001);
                return DefSubclassProc(h, msg, w, l);
            }
            DefSubclassProc(h, msg, w, l)
        }
        let _ = SetWindowSubclass(hwnd, bg_subclass, 1001, 0);
        apply_window_region(hwnd);

        // Set WS_EX_TOOLWINDOW matching Python Qt.WindowType.Tool (floating overlay).
        let exstyle = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if (exstyle & WS_EX_TOOLWINDOW) == 0 {
            SetWindowLongW(hwnd, GWL_EXSTYLE, exstyle | WS_EX_TOOLWINDOW);
        }
    }
}

#[cfg(target_os = "windows")]
fn native_drag_window(hwnd: isize) {
    #[link(name = "user32")]
    extern "system" {
        fn ReleaseCapture() -> i32;
        fn SendMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> isize;
    }
    const WM_SYSCOMMAND: u32 = 0x0112;
    const SC_MOVE: usize = 0xF010;
    const HTCAPTION: usize = 2;

    unsafe {
        ReleaseCapture();
        SendMessageW(hwnd, WM_SYSCOMMAND, SC_MOVE | HTCAPTION, 0);
    }
}

#[cfg(target_os = "windows")]
fn is_lbutton_down() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(vKey: i32) -> i16;
        fn GetSystemMetrics(nIndex: i32) -> i32;
    }
    const SM_SWAPBUTTON: i32 = 23;
    let vkey = unsafe {
        if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
            0x02 // VK_RBUTTON
        } else {
            0x01 // VK_LBUTTON
        }
    };
    unsafe { (GetAsyncKeyState(vkey) as u16 & 0x8000) != 0 }
}

#[cfg(target_os = "windows")]
fn get_cursor_screen_pos() -> Option<egui::Pos2> {
    #[repr(C)]
    struct POINT {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(lpPoint: *mut POINT) -> i32;
    }
    let mut pt = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut pt) != 0 {
            Some(egui::pos2(pt.x as f32, pt.y as f32))
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
pub fn clamp_rect_to_work_area(
    mut cand_x: i32,
    mut cand_y: i32,
    width: i32,
    height: i32,
    work_area: [i32; 4], // [left, top, right, bottom]
) -> (i32, i32) {
    let [work_left, work_top, work_right, work_bottom] = work_area;
    if cand_x + width > work_right {
        cand_x = (work_right - width).max(work_left);
    }
    if cand_x < work_left {
        cand_x = work_left;
    }
    if cand_y + height > work_bottom {
        cand_y = (work_bottom - height).max(work_top);
    }
    if cand_y < work_top {
        cand_y = work_top;
    }
    (cand_x, cand_y)
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct MONITORINFO {
    cb_size: u32,
    rc_monitor: RECT,
    rc_work: RECT,
    dw_flags: u32,
}

#[cfg(target_os = "windows")]
pub fn validate_saved_position(
    x: Option<i32>,
    y: Option<i32>,
    width: i32,
    height: i32,
) -> (i32, i32) {
    #[link(name = "user32")]
    extern "system" {
        fn MonitorFromRect(lprc: *const RECT, dwFlags: u32) -> isize;
        fn MonitorFromWindow(hWnd: isize, dwFlags: u32) -> isize;
        fn GetMonitorInfoW(hMonitor: isize, lpmi: *mut MONITORINFO) -> i32;
        fn IntersectRect(lprcDst: *mut RECT, lprcSrc1: *const RECT, lprcSrc2: *const RECT) -> i32;
    }

    const MONITOR_DEFAULTTONULL: u32 = 0;
    const MONITOR_DEFAULTTOPRIMARY: u32 = 1;

    if let (Some(cand_x), Some(cand_y)) = (x, y) {
        let cand_rect = RECT {
            left: cand_x,
            top: cand_y,
            right: cand_x + width,
            bottom: cand_y + height,
        };

        unsafe {
            let h_mon = MonitorFromRect(&cand_rect, MONITOR_DEFAULTTONULL);
            if h_mon != 0 {
                let mut mi = MONITORINFO {
                    cb_size: std::mem::size_of::<MONITORINFO>() as u32,
                    rc_monitor: RECT::default(),
                    rc_work: RECT::default(),
                    dw_flags: 0,
                };
                if GetMonitorInfoW(h_mon, &mut mi) != 0 {
                    let mut intersect = RECT::default();
                    if IntersectRect(&mut intersect, &cand_rect, &mi.rc_work) != 0 {
                        let inter_w = intersect.right - intersect.left;
                        let inter_h = intersect.bottom - intersect.top;
                        if inter_w >= 50 && inter_h >= 30 {
                            return clamp_rect_to_work_area(
                                cand_x,
                                cand_y,
                                width,
                                height,
                                [
                                    mi.rc_work.left,
                                    mi.rc_work.top,
                                    mi.rc_work.right,
                                    mi.rc_work.bottom,
                                ],
                            );
                        }
                    }
                }
            }
        }
    }

    // Fallback: place on Primary Monitor work area
    unsafe {
        let h_prim = MonitorFromWindow(0, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO {
            cb_size: std::mem::size_of::<MONITORINFO>() as u32,
            rc_monitor: RECT::default(),
            rc_work: RECT::default(),
            dw_flags: 0,
        };
        if h_prim != 0 && GetMonitorInfoW(h_prim, &mut mi) != 0 {
            let def_x = (mi.rc_work.right - width - 40).max(mi.rc_work.left + 10);
            let def_y = (mi.rc_work.top + 50).max(mi.rc_work.top + 10);
            (def_x, def_y)
        } else {
            (400, 50)
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn validate_saved_position(
    x: Option<i32>,
    y: Option<i32>,
    _width: i32,
    _height: i32,
) -> (i32, i32) {
    (x.unwrap_or(400).max(0), y.unwrap_or(50).max(0))
}

#[cfg(target_os = "windows")]
fn get_primary_monitor_default_pos(logical_w: i32, logical_h: i32) -> (i32, i32) {
    validate_saved_position(None, None, logical_w, logical_h)
}

#[cfg(not(target_os = "windows"))]
fn get_primary_monitor_default_pos(_logical_w: i32, _logical_h: i32) -> (i32, i32) {
    (400, 50)
}

#[cfg(target_os = "windows")]
fn ensure_window_within_monitor(hwnd: isize, config: &Arc<Mutex<Config>>, ppp: f32) {
    #[link(name = "user32")]
    extern "system" {
        fn MonitorFromRect(lprc: *const RECT, dwFlags: u32) -> isize;
        fn MonitorFromWindow(hWnd: isize, dwFlags: u32) -> isize;
        fn GetMonitorInfoW(hMonitor: isize, lpmi: *mut MONITORINFO) -> i32;
        fn IntersectRect(lprcDst: *mut RECT, lprcSrc1: *const RECT, lprcSrc2: *const RECT) -> i32;
    }

    const MONITOR_DEFAULTTONULL: u32 = 0;
    const MONITOR_DEFAULTTOPRIMARY: u32 = 1;

    let Some([cur_x, cur_y, cur_w, cur_h]) = get_window_rect(hwnd) else {
        return;
    };

    let cand_rect = RECT {
        left: cur_x,
        top: cur_y,
        right: cur_x + cur_w,
        bottom: cur_y + cur_h,
    };

    unsafe {
        let mut target_x = cur_x;
        let mut target_y = cur_y;
        let mut adjusted = false;

        let h_mon = MonitorFromRect(&cand_rect, MONITOR_DEFAULTTONULL);
        if h_mon != 0 {
            let mut mi = MONITORINFO {
                cb_size: std::mem::size_of::<MONITORINFO>() as u32,
                rc_monitor: RECT::default(),
                rc_work: RECT::default(),
                dw_flags: 0,
            };
            if GetMonitorInfoW(h_mon, &mut mi) != 0 {
                let mut intersect = RECT::default();
                if IntersectRect(&mut intersect, &cand_rect, &mi.rc_work) != 0 {
                    let inter_w = intersect.right - intersect.left;
                    let inter_h = intersect.bottom - intersect.top;
                    if inter_w >= 50 && inter_h >= 30 {
                        let (cx, cy) = clamp_rect_to_work_area(
                            cur_x,
                            cur_y,
                            cur_w,
                            cur_h,
                            [
                                mi.rc_work.left,
                                mi.rc_work.top,
                                mi.rc_work.right,
                                mi.rc_work.bottom,
                            ],
                        );
                        if cx != cur_x || cy != cur_y {
                            target_x = cx;
                            target_y = cy;
                            adjusted = true;
                        }
                    } else {
                        target_x = (mi.rc_work.right - cur_w - 40).max(mi.rc_work.left + 10);
                        target_y = (mi.rc_work.top + 50).max(mi.rc_work.top + 10);
                        adjusted = true;
                    }
                } else {
                    target_x = (mi.rc_work.right - cur_w - 40).max(mi.rc_work.left + 10);
                    target_y = (mi.rc_work.top + 50).max(mi.rc_work.top + 10);
                    adjusted = true;
                }
            }
        } else {
            let h_prim = MonitorFromWindow(0, MONITOR_DEFAULTTOPRIMARY);
            let mut mi = MONITORINFO {
                cb_size: std::mem::size_of::<MONITORINFO>() as u32,
                rc_monitor: RECT::default(),
                rc_work: RECT::default(),
                dw_flags: 0,
            };
            if h_prim != 0 && GetMonitorInfoW(h_prim, &mut mi) != 0 {
                target_x = (mi.rc_work.right - cur_w - 40).max(mi.rc_work.left + 10);
                target_y = (mi.rc_work.top + 50).max(mi.rc_work.top + 10);
                adjusted = true;
            }
        }

        if adjusted {
            set_window_rect(hwnd, target_x, target_y, cur_w, cur_h);
            sync_window_position(hwnd, config, ppp);
        }
    }
}

#[cfg(target_os = "windows")]
fn get_window_rect(hwnd: isize) -> Option<[i32; 4]> {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowRect(hWnd: isize, lpRect: *mut RECT) -> i32;
    }
    let mut r = RECT::default();
    unsafe {
        if GetWindowRect(hwnd, &mut r) != 0 {
            Some([r.left, r.top, r.right - r.left, r.bottom - r.top])
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn set_window_rect(hwnd: isize, x: i32, y: i32, w: i32, h: i32) {
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowPos(
            hWnd: isize,
            hWndInsertAfter: isize,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
    }
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    unsafe {
        SetWindowPos(hwnd, 0, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    apply_window_region(hwnd);
}

#[cfg(target_os = "windows")]
fn apply_window_region(hwnd: isize) {
    #[link(name = "user32")]
    extern "system" {
        fn GetClientRect(hWnd: isize, lpRect: *mut RECT) -> i32;
        fn GetDpiForWindow(hWnd: isize) -> u32;
        fn CreateRoundRectRgn(
            nLeftRect: i32,
            nTopRect: i32,
            nRightRect: i32,
            nBottomRect: i32,
            nWidthEllipse: i32,
            nHeightEllipse: i32,
        ) -> isize;
        fn SetWindowRgn(hWnd: isize, hRgn: isize, bRedraw: i32) -> i32;
    }

    if hwnd == 0 {
        return;
    }

    unsafe {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetClientRect(hwnd, &mut rect) == 0 {
            return;
        }

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return;
        }

        let dpi = GetDpiForWindow(hwnd).max(96) as f32;
        let diameter = (18.0 * dpi / 96.0).round() as i32;
        let region = CreateRoundRectRgn(-1, -1, width + 2, height + 2, diameter, diameter);
        if region != 0 {
            // SetWindowRgn takes ownership of the region handle on success.
            let _ = SetWindowRgn(hwnd, region, 1);
        }
    }
}

#[cfg(target_os = "windows")]
fn release_mouse_capture() {
    #[link(name = "user32")]
    extern "system" {
        fn ReleaseCapture() -> i32;
    }
    unsafe {
        ReleaseCapture();
    }
}

#[cfg(target_os = "windows")]
fn sync_window_position(hwnd: isize, config: &Arc<Mutex<Config>>, ppp: f32) {
    if let Some([left, top, _, _]) = get_window_rect(hwnd) {
        let mut cfg = config.lock().unwrap();
        cfg.window_x = Some((left as f32 / ppp).round() as i32);
        cfg.window_y = Some((top as f32 / ppp).round() as i32);
        ConfigManager::save(&cfg);
    }
}

#[cfg(target_os = "windows")]
fn sync_window_geometry(hwnd: isize, config: &Arc<Mutex<Config>>, ppp: f32) {
    if let Some([left, top, width, height]) = get_window_rect(hwnd) {
        let logical_w = (width as f32 / ppp).round() as u32;
        let logical_h = (height as f32 / ppp).round() as u32;

        let mut cfg = config.lock().unwrap();
        cfg.window_x = Some((left as f32 / ppp).round() as i32);
        cfg.window_y = Some((top as f32 / ppp).round() as i32);
        if cfg.layout_mode == "horizontal" {
            cfg.horizontal_width = logical_w.max(MIN_HORIZONTAL_WIDTH);
            cfg.horizontal_height = logical_h.max(MIN_HORIZONTAL_HEIGHT);
        } else {
            cfg.vertical_width = logical_w.max(MIN_VERTICAL_WIDTH);
            cfg.vertical_height = logical_h.max(MIN_VERTICAL_HEIGHT);
        }
        ConfigManager::save(&cfg);
    }
}

/// Directly applies or removes WS_EX_TRANSPARENT on the HUD window.
/// More reliable than egui's deferred ViewportCommand::MousePassthrough.
#[cfg(target_os = "windows")]
fn apply_win32_click_through(hwnd: isize, enable: bool) {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongW(hWnd: isize, nIndex: i32) -> i32;
        fn SetWindowLongW(hWnd: isize, nIndex: i32, dwNewLong: i32) -> i32;
        fn SetWindowPos(
            hWnd: isize,
            hWndInsertAfter: isize,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
    }
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TRANSPARENT: i32 = 0x00000020;
    const WS_EX_LAYERED: i32 = 0x00080000;
    const WS_EX_WINDOWEDGE: i32 = 0x00000100;
    const WS_EX_CLIENTEDGE: i32 = 0x00000200;
    const WS_EX_DLGMODALFRAME: i32 = 0x00000001;
    const WS_EX_STATICEDGE: i32 = 0x00020000;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOACTIVATE: u32 = 0x0010;
    if hwnd == 0 {
        return;
    }
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let mut new_style = if enable {
            style | WS_EX_TRANSPARENT | WS_EX_LAYERED
        } else {
            style & !WS_EX_TRANSPARENT
        };
        new_style &=
            !(WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME | WS_EX_STATICEDGE);
        if new_style != style {
            SetWindowLongW(hwnd, GWL_EXSTYLE, new_style);
            SetWindowPos(
                hwnd,
                0,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    #[test]
    fn native_titlebar_bits_are_removed() {
        const WS_CAPTION: i32 = 0x00C00000;
        const WS_SYSMENU: i32 = 0x00080000;
        const WS_MINIMIZEBOX: i32 = 0x00020000;
        const WS_MAXIMIZEBOX: i32 = 0x00010000;

        let style = 0x16CB0000;
        let stripped = super::strip_native_titlebar_bits(style);

        assert_eq!(
            stripped & (WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX),
            0
        );
        assert_ne!(style, stripped);
    }

    #[test]
    fn test_clamp_rect_to_work_area() {
        use super::clamp_rect_to_work_area;

        // 1. Inside bounds: unchanged
        let (x, y) = clamp_rect_to_work_area(100, 100, 690, 152, [0, 0, 1920, 1040]);
        assert_eq!((x, y), (100, 100));

        // 2. Off right edge: clamped to right - width
        let (x, y) = clamp_rect_to_work_area(1800, 100, 690, 152, [0, 0, 1920, 1040]);
        assert_eq!((x, y), (1230, 100));

        // 3. Off bottom edge (e.g. taskbar): clamped to bottom - height
        let (x, y) = clamp_rect_to_work_area(100, 950, 690, 152, [0, 0, 1920, 1040]);
        assert_eq!((x, y), (100, 888));

        // 4. Off left edge
        let (x, y) = clamp_rect_to_work_area(-50, 100, 690, 152, [0, 0, 1920, 1040]);
        assert_eq!((x, y), (0, 100));

        // 5. Off top edge
        let (x, y) = clamp_rect_to_work_area(100, -30, 690, 152, [0, 0, 1920, 1040]);
        assert_eq!((x, y), (100, 0));
    }
}

impl HudApp {
    fn render_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let header_h = 16.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), header_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_height(header_h);
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 0.0);

                // 1. Status Dot (vector drawn circle, perfectly centered with text)
                let busy = self.refresh_ctrl.lock().unwrap().is_busy();
                let has_error = self.metrics.values().any(|m| m.error.is_some());
                let dot_color = if busy {
                    COLOR_BLUE
                } else if has_error {
                    COLOR_AMBER
                } else {
                    COLOR_GREEN
                };
                let dot_radius = 3.5;
                let (dot_rect, _) = ui.allocate_exact_size(
                    egui::vec2(dot_radius * 2.0, header_h),
                    egui::Sense::hover(),
                );

                // 2. Responsive Title Label (QLabel#HeaderTitle: font-size 10.5px, font-weight 800, color #94a3b8)
                let rem_w = ui.available_width();
                let title_text = if rem_w < 140.0 {
                    "HUD"
                } else if rem_w < 185.0 {
                    "AI AGENT HUD"
                } else {
                    "AI AGENT HUD (3-IN-1)"
                };
                let title_resp = ui.label(
                    RichText::new(title_text)
                        .color(TEXT_SECONDARY)
                        .size(10.5)
                        .strong(),
                );
                let dot_center = egui::pos2(dot_rect.center().x, title_resp.rect.center().y - 0.5);
                ui.painter()
                    .circle_filled(dot_center, dot_radius, dot_color);

                // 3. Ghost icon if click-through is active (True color 3D emoji matching Python)
                if self.config.lock().unwrap().click_through {
                    if self.ghost_texture.is_none() {
                        self.ghost_texture = Some(ctx.load_texture(
                            "ghost_icon",
                            load_ghost_image(),
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    if let Some(tex) = &self.ghost_texture {
                        ui.add(egui::Image::new(tex).fit_to_exact_size(egui::vec2(14.0, 14.0)))
                            .on_hover_text("滑鼠穿透中 (Alt+Shift+C 解除)");
                    }
                }

                // 4. Layout Toggle Button "⇄" (QPushButton#LayoutToggleBtn matching Python)
                let toggle_btn = ui
                    .add(
                        egui::Button::new(RichText::new("⇄").size(10.5).color(TEXT_SECONDARY))
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 15))
                            .stroke(egui::Stroke::new(
                                1.0_f32,
                                Color32::from_rgba_unmultiplied(255, 255, 255, 25),
                            ))
                            .rounding(4.0)
                            .min_size(egui::vec2(20.0, 16.0)),
                    )
                    .on_hover_text("切換 橫向三欄並排 / 直式三層堆疊 佈局");

                self.toggle_btn_rect = toggle_btn.rect.expand(2.0);

                if toggle_btn.clicked() {
                    self.toggle_layout(ctx);
                }

                // 5. Live Monospace Clock on the far right (QLabel#HeaderStatus: font-size 9.5px, color #64748b)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_height(header_h);
                    let time_str = chrono::Local::now().format("%H:%M:%S").to_string();
                    ui.label(
                        RichText::new(time_str)
                            .color(TEXT_MUTED)
                            .monospace()
                            .size(9.5),
                    );
                });
            },
        );
    }

    fn render_vertical_cards(&self, ui: &mut egui::Ui) {
        let avail_h = ui.available_height();
        let total_cards = PROVIDER_IDS.len();
        // 2 dividers with 3.0 padding top/bottom + 1.0 line = 7.0 per divider (total 14.0)
        let div_spacing = VERTICAL_DIVIDER_SPACING as f32;
        let total_div_h =
            (total_cards as f32 - 1.0) * (div_spacing * 2.0 + VERTICAL_DIVIDER_LINE_HEIGHT as f32);
        let card_h =
            ((avail_h - total_div_h) / total_cards as f32).max(VERTICAL_CARD_MIN_HEIGHT as f32);

        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

        for (i, &id) in PROVIDER_IDS.iter().enumerate() {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), card_h),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    render_provider_card(ui, id, self.metrics.get(id), card_h);
                },
            );

            if i < total_cards - 1 {
                ui.add_space(div_spacing);
                // h_div: background-color: rgba(255, 255, 255, 0.08); max-height: 1px;
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), VERTICAL_DIVIDER_LINE_HEIGHT as f32),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                );
                ui.add_space(div_spacing);
            }
        }
    }

    fn render_horizontal_cards(&self, ui: &mut egui::Ui) {
        let total_cards = PROVIDER_IDS.len(); // 3
        let spacing = 8.0; // body_layout.setSpacing(8)
        let total_w = ui.available_width();
        let avail_h = ui.available_height();
        let total_divider_spacing = (total_cards as f32 - 1.0) * (spacing * 2.0 + 1.0); // 34.0
        let cards_avail_w = (total_w - total_divider_spacing).max(300.0);

        // Perfectly balanced equal column widths (distributing remainder to middle column)
        let base_w = (cards_avail_w / total_cards as f32).floor();
        let rem = cards_avail_w - (base_w * total_cards as f32);
        let mid = total_cards / 2;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            for (i, &id) in PROVIDER_IDS.iter().enumerate() {
                let card_w = if i == mid { base_w + rem } else { base_w };
                ui.allocate_ui_with_layout(
                    egui::vec2(card_w, avail_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(card_w);
                        ui.set_max_width(card_w);
                        render_provider_card(ui, id, self.metrics.get(id), avail_h);
                    },
                );

                if i < total_cards - 1 {
                    ui.add_space(spacing);
                    // QFrame#Divider: background-color: rgba(255, 255, 255, 0.12); width: 1px;
                    // Pixel-snapped vertical line segment matching card vertical bounds
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(1.0, avail_h), egui::Sense::hover());
                    let x = rect.center().x.round();
                    let y_top = rect.top() + 4.0;
                    let y_bot = rect.bottom() - 4.0;
                    ui.painter().line_segment(
                        [egui::pos2(x, y_top), egui::pos2(x, y_bot)],
                        egui::Stroke::new(
                            1.0_f32,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 30),
                        ),
                    );
                    ui.add_space(spacing);
                }
            }
        });
    }
}

/// Build system tray icon (Windows / macOS)
#[cfg(not(target_os = "linux"))]
fn build_tray_icon() -> Option<tray_icon::TrayIcon> {
    let icon = load_tray_icon_image();
    match tray_icon::TrayIconBuilder::new()
        .with_tooltip("AI HUD Monitor (3-in-1)")
        .with_icon(icon)
        .build()
    {
        Ok(t) => {
            info!("[Tray] System tray icon created");
            Some(t)
        }
        Err(e) => {
            log::warn!("[Tray] Failed to create tray icon: {e}");
            None
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn load_tray_icon_image() -> tray_icon::Icon {
    // 1. Embedded PNG byte buffer resized to 32x32 for Windows tray
    const ICON_PNG_BYTES: &[u8] = include_bytes!("../../assets/app_icon.png");
    if let Ok(img) = image::load_from_memory(ICON_PNG_BYTES) {
        let resized = img.resize_exact(32, 32, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let (w, h) = rgba.dimensions();
        if let Ok(icon) = tray_icon::Icon::from_rgba(rgba.into_raw(), w, h) {
            return icon;
        }
    }

    // 2. Fallback
    tray_icon::Icon::from_rgba(vec![56, 189, 248, 255], 1, 1).unwrap()
}

/// Load embedded color ghost emoji texture (matches Windows 3D Fluent emoji)
fn load_ghost_image() -> egui::ColorImage {
    const GHOST_PNG_BYTES: &[u8] = include_bytes!("../../assets/ghost.png");
    if let Ok(img) = image::load_from_memory(GHOST_PNG_BYTES) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba)
    } else {
        egui::ColorImage::new([1, 1], egui::Color32::WHITE)
    }
}

/// Trims unreferenced physical memory pages back to the Windows OS
#[cfg(target_os = "windows")]
pub fn trim_working_set() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn SetProcessWorkingSetSize(
            hProcess: isize,
            dwMinimumWorkingSetSize: usize,
            dwMaximumWorkingSetSize: usize,
        ) -> i32;
    }
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}
