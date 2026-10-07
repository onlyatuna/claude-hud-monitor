#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![allow(clippy::upper_case_acronyms)]

mod autostart;
mod config;
mod hotkey;
mod logger;
pub mod memory;
pub mod pace;
mod providers;
mod refresh_controller;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use config::{Config, ConfigManager};
use hotkey::{click_through_startup_allowed, HotkeyManager};
use log::info;
use parking_lot::Mutex;
use qtrs_core::timer::Timer;
use qtrs_gui::geometry::primitives::Point;
use qtrs_platform::platform_tray::TrayMessageIcon;
use qtrs_platform::tray_icon::TrayActivation;
use qtrs_widgets::application::Application;
use refresh_controller::RefreshController;
use ui::hud_window::HUDWindow;
use ui::tray_icon::HUDTrayIcon;

pub static WAKE_MSG: AtomicU32 = AtomicU32::new(0);
pub static WAKE_REQUESTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static MAIN_HUD: RefCell<Option<Rc<RefCell<HUDWindow>>>> = const { RefCell::new(None) };
    static MAIN_TRAY: RefCell<Option<Rc<RefCell<HUDTrayIcon>>>> = const { RefCell::new(None) };
}

fn run_on_main_thread<F: FnOnce(&Rc<RefCell<HUDWindow>>) + Send + 'static>(
    main_thread_id: qtrs_core::object::ThreadId,
    f: F,
) {
    qtrs_core::event_loop::post_event_to_thread(
        main_thread_id,
        qtrs_core::object::ObjectId(0),
        qtrs_core::event::Event::new(qtrs_core::event::EventKind::MetaCall(Box::new(move |_| {
            MAIN_HUD.with(|cell| {
                if let Some(hud) = cell.borrow().as_ref() {
                    f(hud);
                }
            });
        }))),
    );
}

#[cfg(target_os = "windows")]
struct WakeFilter;

#[cfg(target_os = "windows")]
impl qtrs_core::event::NativeEventFilter for WakeFilter {
    fn native_event_filter(
        &mut self,
        _event_type: &str,
        msg: &qtrs_core::event::NativeMessage,
        _result: &mut isize,
    ) -> bool {
        let wake_msg = WAKE_MSG.load(Ordering::Relaxed);
        if wake_msg != 0 {
            if let qtrs_core::event::NativeMessage::Windows(wmsg) = msg {
                if wmsg.message == wake_msg {
                    MAIN_HUD.with(|cell| {
                        if let Some(hud) = cell.borrow().as_ref() {
                            hud.borrow_mut().show();
                        }
                    });
                    return true;
                }
            }
        }
        false
    }
}

fn main() -> std::process::ExitCode {
    logger::setup_logging();
    info!("=== Claude HUD Monitor (Rust Qt) starting ===");

    if std::env::args().any(|a| a == "--smoke-test") {
        info!("Running offline smoke-test...");
        let config = Arc::new(Mutex::new(config::Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let _app = Application::new(std::env::args().collect());

        let mut hud = HUDWindow::new(config, refresh_ctrl).expect("Failed to initialize HUDWindow");
        hud.on_data_fetched(&crate::providers::base::UsageMetrics {
            provider_id: "agy".to_string(),
            error: Some("test error".to_string()),
            ..Default::default()
        });
        hud.on_data_fetched(&crate::providers::base::UsageMetrics {
            provider_id: "agy".to_string(),
            metric1_val: Some(20.0),
            metric1_text: "20%".to_string(),
            ..Default::default()
        });
        hud.apply_ui_mode("table");
        hud.apply_ui_mode("cards");
        hud.set_theme(true);
        hud.update_clock();
        let _ = qtrs_core::application::CoreApplication::process_events(false);
        info!("=== Smoke-test passed successfully! ===");
        return std::process::ExitCode::SUCCESS;
    }

    if std::env::args().any(|a| a == "--snapshot") {
        info!("Generating visual snapshot previews of HUD modes...");
        let config = Arc::new(Mutex::new(config::Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let _app = Application::new(std::env::args().collect());

        let mut hud = HUDWindow::new(Arc::clone(&config), refresh_ctrl)
            .expect("Failed to initialize HUDWindow");

        // Realistic Mock metrics for Claude
        let claude_metrics = crate::providers::base::UsageMetrics {
            provider_id: "claude".to_string(),
            provider_name: "Claude".to_string(),
            metric1_title: "SESSION 5H".to_string(),
            metric1_val: Some(42.5),
            metric1_text: "43%".to_string(),
            metric1_reset: Some(
                chrono::Utc::now() + chrono::Duration::hours(2) + chrono::Duration::minutes(15),
            ),
            metric2_title: "WEEKLY 7D".to_string(),
            metric2_val: Some(68.0),
            metric2_text: "68%".to_string(),
            metric2_reset: Some(chrono::Utc::now() + chrono::Duration::days(3)),
            badge1_text: "1.2x pace".to_string(),
            badge2_text: "Pro Tier".to_string(),
            last_updated_time: "21:50:00".to_string(),
            error: None,
            error_code: String::new(),
            retry_after: None,
            stale: false,
            last_success: Some(chrono::Utc::now()),
        };
        hud.on_data_fetched(&claude_metrics);

        // Realistic Mock metrics for Antigravity
        let agy_metrics = crate::providers::base::UsageMetrics {
            provider_id: "agy".to_string(),
            provider_name: "Antigravity".to_string(),
            metric1_title: "DAILY QUOTA".to_string(),
            metric1_val: Some(18.0),
            metric1_text: "18%".to_string(),
            metric1_reset: Some(
                chrono::Utc::now() + chrono::Duration::hours(18) + chrono::Duration::minutes(40),
            ),
            metric2_title: "BURST LIMIT".to_string(),
            metric2_val: Some(5.0),
            metric2_text: "5%".to_string(),
            metric2_reset: None,
            badge1_text: "0.4x pace".to_string(),
            badge2_text: "Enterprise".to_string(),
            last_updated_time: "21:50:02".to_string(),
            error: None,
            error_code: String::new(),
            retry_after: None,
            stale: false,
            last_success: Some(chrono::Utc::now()),
        };
        hud.on_data_fetched(&agy_metrics);

        // Realistic Mock metrics for Codex
        let codex_metrics = crate::providers::base::UsageMetrics {
            provider_id: "codex".to_string(),
            provider_name: "OpenAI Codex".to_string(),
            metric1_title: "SESSION 5H".to_string(),
            metric1_val: Some(85.0),
            metric1_text: "85%".to_string(),
            metric1_reset: Some(chrono::Utc::now() + chrono::Duration::minutes(45)),
            metric2_title: "MONTHLY".to_string(),
            metric2_val: Some(92.0),
            metric2_text: "92%".to_string(),
            metric2_reset: Some(chrono::Utc::now() + chrono::Duration::days(12)),
            badge1_text: "1.8x pace".to_string(),
            badge2_text: "Plus Tier".to_string(),
            last_updated_time: "21:49:58".to_string(),
            error: None,
            error_code: String::new(),
            retry_after: None,
            stale: false,
            last_success: Some(chrono::Utc::now()),
        };
        hud.on_data_fetched(&codex_metrics);

        let out_dir = std::path::Path::new("target/snapshots");
        let _ = std::fs::create_dir_all(out_dir);

        // 1. Table mode snapshot
        hud.apply_ui_mode("table");
        hud.update_clock();
        let _ = qtrs_core::application::CoreApplication::process_events(false);
        hud.window.render_and_present();
        let table_path = out_dir.join("hud_table_mode.png");
        hud.window
            .save_png(&table_path)
            .expect("failed to save table png");
        info!("Saved table mode snapshot: {}", table_path.display());

        // 2. Cards mode horizontal snapshot
        hud.apply_cards_layout_mode("horizontal");
        hud.update_clock();
        let _ = qtrs_core::application::CoreApplication::process_events(false);
        hud.window.render_and_present();
        let cards_h_path = out_dir.join("hud_cards_horizontal.png");
        hud.window
            .save_png(&cards_h_path)
            .expect("failed to save cards h png");
        info!(
            "Saved cards horizontal snapshot: {}",
            cards_h_path.display()
        );

        // 3. Cards mode vertical snapshot
        hud.apply_cards_layout_mode("vertical");
        hud.update_clock();
        let _ = qtrs_core::application::CoreApplication::process_events(false);
        hud.window.render_and_present();
        let cards_v_path = out_dir.join("hud_cards_vertical.png");
        hud.window
            .save_png(&cards_v_path)
            .expect("failed to save cards v png");
        info!("Saved cards vertical snapshot: {}", cards_v_path.display());
        // 4. Context Menu snapshot
        use qtrs_gui::paint::{Painter, Pixmap};
        use qtrs_gui::tiny_skia::Color;
        use qtrs_widgets::Widget;
        let cfg = config.lock();
        let mut context_menu = ui::tray_icon::build_hud_context_menu(&cfg);
        context_menu.set_visible(true);
        let menu_size = context_menu.size_hint();
        context_menu.set_geometry(qtrs_gui::geometry::primitives::Rect::new(
            0,
            0,
            menu_size.width,
            menu_size.height,
        ));
        let mut menu_pixmap = Pixmap::new(menu_size.width as u32, menu_size.height as u32).unwrap();
        menu_pixmap.fill(Color::TRANSPARENT);
        {
            let mut painter = Painter::begin(&mut menu_pixmap);
            context_menu.paint_event(&mut painter);
        }
        let menu_path = out_dir.join("hud_context_menu.png");
        menu_pixmap
            .save_png(&menu_path)
            .expect("failed to save menu png");
        info!("Saved context menu snapshot: {}", menu_path.display());

        info!("=== Snapshots generated successfully! ===");
        return std::process::ExitCode::SUCCESS;
    }

    // Single instance protection and IPC wake-up broadcast across all platforms
    use qtrs_platform::{SingleInstance, SingleInstanceCommand, SingleInstanceResult};

    let single_instance = SingleInstance::acquire(
        "ClaudeHUDMonitorSingleInstance",
        SingleInstanceCommand::WakeUp,
    );
    let _instance_guard = match single_instance {
        SingleInstanceResult::Primary(guard) => {
            #[cfg(target_os = "windows")]
            {
                WAKE_MSG.store(guard.wake_message_id(), Ordering::Relaxed);
            }
            guard.on_command(|_cmd| {
                log::info!(
                    "[SingleInstance] Received remote wake-up command from secondary instance"
                );
                WAKE_REQUESTED.store(true, Ordering::Relaxed);
            });
            guard
        }
        SingleInstanceResult::Secondary { command_sent } => {
            log::warn!("[SingleInstance] Another instance is already running (wake command sent: {command_sent}). Exiting.");
            return std::process::ExitCode::SUCCESS;
        }
    };

    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
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

    let interval = {
        let cfg = config.lock();
        cfg.refresh_interval_sec
    };

    let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(interval)));

    let mut _app = Application::new(std::env::args().collect());

    #[cfg(target_os = "windows")]
    qtrs_core::application::CoreApplication::install_native_event_filter(Box::new(WakeFilter));

    let main_thread_id = qtrs_core::object::ThreadId::current();

    // Create HUD window
    let hud_res = HUDWindow::new(Arc::clone(&config), Arc::clone(&refresh_ctrl));
    let mut hud = match hud_res {
        Ok(h) => h,
        Err(e) => {
            log::error!("Failed to create HUD window: {}", e);
            return std::process::ExitCode::FAILURE;
        }
    };
    hud.show();
    info!("HUD Window created and shown");

    let hud = Rc::new(RefCell::new(hud));
    MAIN_HUD.with(|cell| {
        *cell.borrow_mut() = Some(Rc::clone(&hud));
    });

    // Create System Tray Icon
    let mut tray = HUDTrayIcon::new(Arc::clone(&config));
    tray.show();
    info!("Tray icon created and shown");
    let tray = Rc::new(RefCell::new(tray));
    MAIN_TRAY.with(|cell| {
        *cell.borrow_mut() = Some(Rc::clone(&tray));
    });

    // Connect tray activations
    tray.borrow()
        .tray
        .on_activated
        .connect(move |act: &TrayActivation| {
            if *act == TrayActivation::Trigger {
                MAIN_HUD.with(|cell| {
                    if let Some(hud) = cell.borrow().as_ref() {
                        hud.borrow_mut().toggle_visibility();
                    }
                });
            }
        });

    // Connect title bar layout toggle button ("⇄")
    if let Some(btn) = hud
        .borrow()
        .layout_toggle_btn
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<qtrs_widgets::button::Button>()
    {
        btn.clicked.connect(move |()| {
            qtrs_widgets::command::WidgetCommandQueue::post_task(move || {
                MAIN_HUD.with(|h_cell| {
                    if let Some(hud) = h_cell.borrow().as_ref() {
                        hud.borrow_mut().toggle_cards_layout();
                    }
                });
                MAIN_TRAY.with(|t_cell| {
                    if let Some(tray) = t_cell.borrow().as_ref() {
                        tray.borrow_mut().update_menu_state();
                    }
                });
            });
        });
    }

    // Helper to pop up the modern, styled HUD context menu (used by both HUD body and system tray)
    fn show_hud_popup_menu(global_pos: Point, config: &Arc<Mutex<Config>>) {
        let cfg = config.lock().clone();
        let mut menu = ui::tray_icon::build_hud_context_menu(&cfg);
        if let Some(action) = menu.exec_popup(global_pos) {
            if let Some(id) = action.borrow().data().to_u64() {
                let action_id = id as u32;
                MAIN_TRAY.with(|t_cell| {
                    if let Some(tray) = t_cell.borrow().as_ref() {
                        MAIN_HUD.with(|h_cell| {
                            if let Some(hud) = h_cell.borrow().as_ref() {
                                let mut tray_ref = tray.borrow_mut();
                                let mut hud_ref = hud.borrow_mut();
                                if tray_ref.handle_action(action_id, &mut hud_ref) {
                                    hud_ref.persist_geometry();
                                    hud_ref.window.hide();
                                    info!("Exit requested from context menu. Exiting cleanly.");
                                    Application::exit(0);
                                }
                            }
                        });
                    }
                });
            }
        }
    }

    // Connect system tray context menu request
    let config_clone = Arc::clone(&config);
    tray.borrow()
        .tray
        .on_context_menu_requested
        .connect(move |pos: &Point| {
            let global_pos = *pos;
            let cfg_clone = Arc::clone(&config_clone);
            Timer::single_shot(0, move || {
                show_hud_popup_menu(global_pos, &cfg_clone);
            });
        });

    // Fallback if platform menu action is emitted directly
    tray.borrow().tray.on_menu_action.connect(move |id: &u32| {
        let action_id = *id;
        MAIN_TRAY.with(|t_cell| {
            if let Some(tray) = t_cell.borrow().as_ref() {
                MAIN_HUD.with(|h_cell| {
                    if let Some(hud) = h_cell.borrow().as_ref() {
                        let mut tray_ref = tray.borrow_mut();
                        let mut hud_ref = hud.borrow_mut();
                        if tray_ref.handle_action(action_id, &mut hud_ref) {
                            hud_ref.persist_geometry();
                            hud_ref.window.hide();
                            info!("Exit requested from tray menu. Exiting cleanly.");
                            Application::exit(0);
                        }
                    }
                });
            }
        });
    });

    // Connect window body right-click context menu (mirrors Python contextMenuEvent via QPainter Menu)
    let config_clone2 = Arc::clone(&config);
    hud.borrow_mut()
        .window
        .set_context_menu_handler(move |global_pos| {
            let cfg_clone = Arc::clone(&config_clone2);
            Timer::single_shot(0, move || {
                show_hud_popup_menu(global_pos, &cfg_clone);
            });
        });

    // Global hotkey manager
    let hotkey_enabled = { config.lock().hotkey_enabled };
    let hotkey_str = { config.lock().hotkey.clone() };
    let hotkey = if hotkey_enabled {
        match HotkeyManager::start(&hotkey_str) {
            Ok(hk) => {
                info!("Global hotkey manager registered: {}", hotkey_str);
                let hk = Arc::new(hk);
                let hk_weak = Arc::downgrade(&hk);
                hk.set_notify_callback(move || {
                    let hk_weak = hk_weak.clone();
                    run_on_main_thread(main_thread_id, move |hud| {
                        if let Some(hk) = hk_weak.upgrade() {
                            if hk.poll_toggle() {
                                hud.borrow_mut().toggle_visibility();
                            }
                            if hk.poll_clickthrough() {
                                hud.borrow_mut().toggle_click_through();
                            }
                        }
                    });
                });
                Some(hk)
            }
            Err(e) => {
                log::warn!("Global hotkey registration failed: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Registration failures are reported to the user (Python `hotkey_failed` -> `tray.showMessage`,
    // main.py:77-88), one balloon per message, in Python's order.
    let hotkey_registration = hotkey.as_ref().map(|hk| hk.registration().clone());
    if let Some(registration) = &hotkey_registration {
        for notice in registration.failure_notices() {
            log::warn!("Hotkey registration issue: {}", notice);
            let _ = tray.borrow_mut().tray.show_message(
                "⚠️ 全域快捷鍵通知",
                &format!("{notice}\n您仍可透過系統匣圖示完整操作所有功能。"),
                TrayMessageIcon::Warning,
                5000,
            );
        }
    }

    // Safe click-through initialization: prevent lockout unless the click-through hotkey is
    // registered (mirrors Python _safe_init_click_through, which checks `clickthrough_registered`)
    if !click_through_startup_allowed(hotkey_registration.as_ref()) && config.lock().click_through {
        log::warn!(
            "[HUD] Click-through mode disabled on startup: hotkeys unavailable to prevent lockout"
        );
        config.lock().click_through = false;
        hud.borrow_mut().set_click_through(false);
        tray.borrow_mut().update_menu_state();
        let _ = tray.borrow_mut().tray.show_message(
            "⚠️ 穿透模式已暫停",
            "全域快捷鍵未註冊成功，已自動停用啟動時穿透模式以防視窗鎖死。\n您仍可由系統匣右鍵選單手動開啟。",
            TrayMessageIcon::Warning,
            5000,
        );
    }

    // Follow the operating system's light/dark setting while `appearance` is "auto".
    // `WM_SETTINGCHANGE` refreshes the platform theme inside the window procedure, so the
    // change is queued to the UI event loop instead of re-entering the HUD from there.
    qtrs_platform::platform()
        .theme()
        .theme_changed()
        .connect(move |_| {
            run_on_main_thread(main_thread_id, |hud| {
                hud.borrow_mut().follow_system_theme();
            });
        });

    // Connect worker thread results callback
    let refresh_ctrl_clone = Arc::clone(&refresh_ctrl);
    refresh_ctrl.lock().set_notify_callback(move || {
        let ctrl_clone = Arc::clone(&refresh_ctrl_clone);
        run_on_main_thread(main_thread_id, move |hud| {
            let updates = {
                let mut ctrl = ctrl_clone.lock();
                let providers = hud.borrow().providers.clone();
                ctrl.drain_results(&providers)
            };
            for update in updates {
                hud.borrow_mut().on_data_fetched(&update);
            }
        });
    });

    // Timers matching Qt C++ QTimer architecture
    let mut clock_timer = Timer::new();
    clock_timer.set_interval(1000);
    clock_timer.timeout.connect(move |()| {
        MAIN_HUD.with(|cell| {
            if let Some(hud) = cell.borrow().as_ref() {
                hud.borrow_mut().update_clock();
                if WAKE_REQUESTED.swap(false, Ordering::Relaxed) {
                    hud.borrow_mut().show();
                }
            }
        });
    });
    unsafe { clock_timer.start() };

    let mut poll_timer = Timer::new();
    poll_timer.set_interval(100);
    let refresh_ctrl_poll = Arc::clone(&refresh_ctrl);
    poll_timer.timeout.connect(move |()| {
        MAIN_HUD.with(|cell| {
            if let Some(hud) = cell.borrow().as_ref() {
                let providers = hud.borrow().providers.clone();
                let updates = {
                    let mut ctrl = refresh_ctrl_poll.lock();
                    ctrl.poll(&providers)
                };
                for update in updates {
                    hud.borrow_mut().on_data_fetched(&update);
                }
                let busy = refresh_ctrl_poll.lock().is_busy();
                hud.borrow_mut().set_busy(busy);
            }
        });
    });
    unsafe { poll_timer.start() };

    // Initial memory trim after 2.5s (mirrors Python QTimer.singleShot(2500, trim_memory))
    Timer::single_shot(2500, || {
        crate::memory::trim_memory();
    });

    // Run Qt event loop
    let exit_code = _app.exec();
    clock_timer.stop();
    poll_timer.stop();
    let _ = MAIN_HUD.try_with(|cell| {
        if let Ok(mut borrow) = cell.try_borrow_mut() {
            *borrow = None;
        }
    });
    let _ = MAIN_TRAY.try_with(|cell| {
        if let Ok(mut borrow) = cell.try_borrow_mut() {
            *borrow = None;
        }
    });

    // Explicitly drop HUD and Tray to ensure HUDWindow::drop, ResizeDebouncer::shutdown,
    // and worker thread join complete cleanly via standard Rust destructor execution.
    drop(tray);
    drop(hud);

    info!("Claude HUD Monitor exited cleanly with code: {}", exit_code);
    std::process::ExitCode::from(exit_code as u8)
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
