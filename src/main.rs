#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![allow(clippy::upper_case_acronyms)]

mod autostart;
mod config;
mod hotkey;
mod logger;
mod providers;
mod refresh_controller;
mod ui;

use eframe::egui;
use log::info;
use std::sync::{Arc, Mutex};

use config::ConfigManager;
use providers::{AgyProvider, ClaudeProvider, CodexProvider};
use refresh_controller::RefreshController;
use ui::HudApp;

fn main() -> eframe::Result {
    logger::setup_logging();
    info!("=== Claude HUD Monitor (Rust) starting ===");

    // Set Windows App User Model ID, single-instance mutex and process Dark Mode
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn CreateMutexW(
                lpMutexAttributes: *const std::ffi::c_void,
                bInitialOwner: i32,
                lpName: *const u16,
            ) -> isize;
            fn GetLastError() -> u32;
        }
        #[link(name = "user32")]
        extern "system" {
            fn RegisterWindowMessageW(lpString: *const u16) -> u32;
            fn PostMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> i32;
        }
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let mutex_name: Vec<u16> = OsStr::new("Local\\ClaudeHUDMonitorSingleInstanceMutex\0")
            .encode_wide()
            .collect();
        unsafe {
            let _handle = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
            if GetLastError() == 183 {
                // ERROR_ALREADY_EXISTS: broadcast wake-up message to restore existing instance
                log::warn!("[SingleInstance] Another instance is already running. Waking it up and exiting.");
                let wake_name: Vec<u16> = OsStr::new("ClaudeHUD_WakeUp\0").encode_wide().collect();
                let msg_id = RegisterWindowMessageW(wake_name.as_ptr());
                if msg_id != 0 {
                    const HWND_BROADCAST: isize = 0xFFFF;
                    PostMessageW(HWND_BROADCAST, msg_id, 0, 0);
                }
                return Ok(());
            }
        }

        ui::enable_win32_dark_mode(0);
        let wide: Vec<u16> = OsStr::new("ClaudeHUD.Monitor.App\0")
            .encode_wide()
            .collect();
        unsafe {
            let _ = windows_set_appid(&wide);
        }
    }

    let config = Arc::new(Mutex::new(ConfigManager::load()));
    info!(
        "Config loaded from: {}",
        ConfigManager::config_path().display()
    );

    // Build providers
    let providers: Vec<Box<dyn providers::Provider + Send>> = vec![
        Box::new(ClaudeProvider::new()),
        Box::new(AgyProvider::new()),
        Box::new(CodexProvider::new()),
    ];

    let interval = {
        let cfg = config.lock().unwrap();
        cfg.refresh_interval_sec
    };

    let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(interval)));

    // Load embedded window icon
    let window_icon = {
        const ICON_PNG_BYTES: &[u8] = include_bytes!("../assets/app_icon.png");
        if let Ok(img) = image::load_from_memory(ICON_PNG_BYTES) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            Some(Arc::new(egui::IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            }))
        } else {
            None
        }
    };

    let native_options = {
        let cfg = config.lock().unwrap();
        let (init_w, init_h, min_w, min_h) = if cfg.layout_mode == "horizontal" {
            (
                (cfg.horizontal_width as f32).max(540.0),
                (cfg.horizontal_height as f32).max(150.0),
                540.0,
                130.0,
            )
        } else {
            (
                (cfg.vertical_width as f32).max(250.0),
                (cfg.vertical_height as f32).max(320.0),
                250.0,
                320.0,
            )
        };

        let mut vp = egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(false)
            .with_inner_size([init_w, init_h])
            .with_position([
                cfg.window_x.unwrap_or(400) as f32,
                cfg.window_y.unwrap_or(50) as f32,
            ])
            .with_min_inner_size([min_w, min_h]);

        if let Some(icon) = window_icon {
            vp = vp.with_icon(icon);
        }

        eframe::NativeOptions {
            viewport: vp,
            ..Default::default()
        }
    };

    eframe::run_native(
        "AI Agent HUD Monitor",
        native_options,
        Box::new(move |cc| {
            // Configure CJK, Symbols & Monospace fonts
            let mut fonts = egui::FontDefinitions::default();

            // 1. Latin UI font fallbacks
            let latin_paths = [
                "C:\\Windows\\Fonts\\segoeui.ttf",
                "/System/Library/Fonts/SFPro.ttf",
                "/System/Library/Fonts/Helvetica.ttc",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ];
            for path in &latin_paths {
                if let Ok(bytes) = std::fs::read(path) {
                    fonts
                        .font_data
                        .insert("ui_latin".to_owned(), egui::FontData::from_owned(bytes));
                    fonts
                        .families
                        .get_mut(&egui::FontFamily::Proportional)
                        .unwrap()
                        .insert(0, "ui_latin".to_owned());
                    break;
                }
            }

            // 2. Chinese CJK font fallbacks (Windows, macOS, Linux)
            let cjk_paths = [
                // Windows
                "C:\\Windows\\Fonts\\msjh.ttc",
                "C:\\Windows\\Fonts\\msjhbd.ttc",
                "C:\\Windows\\Fonts\\msyh.ttc",
                // macOS
                "/System/Library/Fonts/PingFang.ttc",
                "/System/Library/Fonts/Hiragino Sans GB.ttc",
                "/System/Library/Fonts/STHeiti Light.ttc",
                "/Library/Fonts/Arial Unicode.ttf",
                // Linux
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            ];
            for path in &cjk_paths {
                if let Ok(bytes) = std::fs::read(path) {
                    fonts
                        .font_data
                        .insert("cjk_fallback".to_owned(), egui::FontData::from_owned(bytes));
                    fonts
                        .families
                        .get_mut(&egui::FontFamily::Proportional)
                        .unwrap()
                        .push("cjk_fallback".to_owned());
                    fonts
                        .families
                        .get_mut(&egui::FontFamily::Monospace)
                        .unwrap()
                        .push("cjk_fallback".to_owned());
                    break;
                }
            }

            // 3. UI Symbols
            let sym_paths = [
                "C:\\Windows\\Fonts\\seguisym.ttf",
                "/System/Library/Fonts/Apple Color Emoji.ttc",
            ];
            for path in &sym_paths {
                if let Ok(bytes) = std::fs::read(path) {
                    fonts
                        .font_data
                        .insert("ui_symbol".to_owned(), egui::FontData::from_owned(bytes));
                    fonts
                        .families
                        .get_mut(&egui::FontFamily::Proportional)
                        .unwrap()
                        .push("ui_symbol".to_owned());
                    fonts
                        .families
                        .get_mut(&egui::FontFamily::Monospace)
                        .unwrap()
                        .push("ui_symbol".to_owned());
                    break;
                }
            }

            // 4. Monospace numbers and metrics
            let mono_paths = [
                "C:\\Windows\\Fonts\\consola.ttf",
                "/System/Library/Fonts/Monaco.ttf",
                "/Library/Fonts/Courier New.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            ];
            for path in &mono_paths {
                if let Ok(bytes) = std::fs::read(path) {
                    fonts
                        .font_data
                        .insert("ui_mono".to_owned(), egui::FontData::from_owned(bytes));
                    fonts
                        .families
                        .get_mut(&egui::FontFamily::Monospace)
                        .unwrap()
                        .insert(0, "ui_mono".to_owned());
                    break;
                }
            }
            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(HudApp::new(
                cc,
                Arc::clone(&config),
                providers,
                Arc::clone(&refresh_ctrl),
            )))
        }),
    )
}

/// Windows-only: set AppUserModelID for proper taskbar grouping.
#[cfg(target_os = "windows")]
unsafe fn windows_set_appid(wide: &[u16]) -> i32 {
    #[link(name = "shell32")]
    extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(appid: *const u16) -> i32;
    }
    SetCurrentProcessExplicitAppUserModelID(wide.as_ptr())
}
