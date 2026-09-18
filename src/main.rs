#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![allow(clippy::upper_case_acronyms)]

mod config;
mod providers;
mod refresh_controller;
mod ui;
mod hotkey;
mod autostart;
mod logger;

use std::sync::{Arc, Mutex};
use eframe::egui;
use log::info;

use config::ConfigManager;
use providers::{ClaudeProvider, AgyProvider, CodexProvider};
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
            fn CreateMutexW(lpMutexAttributes: *const std::ffi::c_void, bInitialOwner: i32, lpName: *const u16) -> isize;
            fn GetLastError() -> u32;
        }
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let mutex_name: Vec<u16> = OsStr::new("Local\\ClaudeHUDMonitorSingleInstanceMutex\0").encode_wide().collect();
        unsafe {
            let _handle = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
            if GetLastError() == 183 { // ERROR_ALREADY_EXISTS
                log::warn!("[SingleInstance] Another instance is already running. Exiting.");
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
    info!("Config loaded from: {}", ConfigManager::config_path().display());

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
            .with_resizable(true)
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
            #[allow(unused_mut)]
            let mut fonts = egui::FontDefinitions::default();
            #[cfg(target_os = "windows")]
            {
                // 1. Segoe UI for sleek Latin UI typography
                let segoe_path = "C:\\Windows\\Fonts\\segoeui.ttf";
                if let Ok(bytes) = std::fs::read(segoe_path) {
                    fonts.font_data.insert(
                        "segoe_ui".to_owned(),
                        egui::FontData::from_owned(bytes),
                    );
                    fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "segoe_ui".to_owned());
                }

                // 2. Microsoft JhengHei for crisp Chinese rendering
                let font_path = "C:\\Windows\\Fonts\\msjh.ttc";
                if let Ok(bytes) = std::fs::read(font_path) {
                    fonts.font_data.insert(
                        "microsoft_jhenghei".to_owned(),
                        egui::FontData::from_owned(bytes),
                    );
                    fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().push("microsoft_jhenghei".to_owned());
                    fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().push("microsoft_jhenghei".to_owned());
                }

                // 3. Segoe UI Symbol for UI icons: ⇄, 👻, ●, etc.
                let segui_sym = "C:\\Windows\\Fonts\\seguisym.ttf";
                if let Ok(bytes) = std::fs::read(segui_sym) {
                    fonts.font_data.insert(
                        "segoe_ui_symbol".to_owned(),
                        egui::FontData::from_owned(bytes),
                    );
                    fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().push("segoe_ui_symbol".to_owned());
                    fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().push("segoe_ui_symbol".to_owned());
                }

                // 4. Consolas for monospace numbers
                let consolas_path = "C:\\Windows\\Fonts\\consola.ttf";
                if let Ok(bytes) = std::fs::read(consolas_path) {
                    fonts.font_data.insert(
                        "consolas".to_owned(),
                        egui::FontData::from_owned(bytes),
                    );
                    fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "consolas".to_owned());
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
