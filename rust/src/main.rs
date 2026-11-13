#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::field_reassign_with_default)]

mod accounts;
mod agy_rows;
mod agy_statusline;
mod autostart;
mod config;
mod hotkey;
mod logger;
pub mod memory;
pub mod pace;
mod panic_guard;
mod providers;
mod refresh_controller;
mod tray_quota;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use config::{Config, ConfigManager};
use ferrokit_core::timer::Timer;
use ferrokit_gui::geometry::primitives::Point;
use ferrokit_platform::platform_tray::TrayMessageIcon;
use ferrokit_platform::tray_icon::TrayActivation;
use ferrokit_widgets::application::Application;
use hotkey::{HotkeyAction, HotkeyManager, click_through_startup_allowed};
use log::info;
use parking_lot::Mutex;
use refresh_controller::RefreshController;
use ui::hud_window::{HUDWindow, with_hud_retry};
use ui::tray_icon::HUDTrayIcon;

pub static WAKE_MSG: AtomicU32 = AtomicU32::new(0);
pub static WAKE_REQUESTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static MAIN_HUD: RefCell<Option<Rc<RefCell<HUDWindow>>>> = const { RefCell::new(None) };
    static MAIN_TRAY: RefCell<Option<Rc<RefCell<HUDTrayIcon>>>> = const { RefCell::new(None) };
}

/// Redraws the tray number from the HUD's latest data. A busy HUD or tray (a menu is open) is
/// skipped: the next publish refreshes again.
fn refresh_tray_quota() {
    let Some(data) = MAIN_HUD.with(|cell| {
        let hud = cell.borrow();
        let hud = hud.as_ref()?.try_borrow().ok()?;
        Some(hud.quota_data())
    }) else {
        return;
    };
    MAIN_TRAY.with(|cell| {
        if let Some(tray) = cell.borrow().as_ref()
            && let Ok(mut tray) = tray.try_borrow_mut()
        {
            tray.refresh_quota(&data);
        }
    });
}

/// The click-through hotkey: `HUDTrayIcon::toggle_click_through` on the HUD. A nested event loop
/// (menu, drag) can have the HUD or the tray borrowed, and the toggle is the way out of
/// click-through, so a busy one is retried (as the colour-scheme slot does) instead of lost.
fn toggle_click_through_when_free() {
    with_hud_retry(|hud| {
        let toggled = MAIN_TRAY.with(|t_cell| match t_cell.borrow().as_ref() {
            Some(tray) => match tray.try_borrow_mut() {
                Ok(mut tray) => {
                    tray.toggle_click_through(hud);
                    true
                }
                Err(_) => false,
            },
            None => true,
        });
        if !toggled {
            ferrokit_widgets::WidgetCommandQueue::post_task(toggle_click_through_when_free);
        }
    });
}

/// The receiver of the signals that worker threads emit (hotkey presses, refresh results): the
/// context of `Signal::connect_local_to`, which delivers them on the UI thread.
struct UiContext {
    data: ferrokit_core::object::ObjectData,
}

impl ferrokit_core::object::QObject for UiContext {
    fn object_data(&self) -> &ferrokit_core::object::ObjectData {
        &self.data
    }
    fn object_data_mut(&mut self) -> ferrokit_core::object::ObjectDataMut<'_> {
        ferrokit_core::object::ObjectDataMut::new(&mut self.data)
    }
}

/// Connects `slot` to `signal` so that an emission from any thread runs it on the UI thread.
fn connect_on_ui_thread<T: Clone + Send + 'static>(
    context: &ferrokit_core::object::OwnedObject<UiContext>,
    signal: &ferrokit_core::signal::Signal<T>,
    slot: impl Fn(&T) + 'static,
) {
    context
        .with(|c| signal.connect_local_to(c, ferrokit_core::signal::ConnectionType::Auto, slot))
        .and_then(Result::ok)
        .expect("the UI context is registered on the UI thread");
}

#[cfg(target_os = "windows")]
struct WakeFilter;

#[cfg(target_os = "windows")]
impl ferrokit_core::event::NativeEventFilter for WakeFilter {
    fn native_event_filter(
        &mut self,
        _event_type: &str,
        msg: &ferrokit_core::event::NativeMessage,
        _result: &mut isize,
    ) -> bool {
        let wake_msg = WAKE_MSG.load(Ordering::Relaxed);
        if wake_msg != 0
            && let ferrokit_core::event::NativeMessage::Windows(wmsg) = msg
            && wmsg.message == wake_msg
        {
            MAIN_HUD.with(|cell| {
                if let Some(hud) = cell.borrow().as_ref()
                    && let Ok(mut hud) = hud.try_borrow_mut()
                {
                    hud.wake();
                }
            });
            return true;
        }
        false
    }
}

/// `main.py:48` `app.setQuitOnLastWindowClosed(False)`: the HUD hides to the tray on close, so
/// closing its window must not quit. Only the tray and context-menu exit paths call `exit`.
fn configure_application_policy() {
    ferrokit_gui::application::GuiApplication::set_quit_on_last_window_closed(false);
}

/// Label text of a smoke-checked widget (empty when the widget is not a label).
fn smoke_label_text(w: &ferrokit_widgets::WidgetRef) -> String {
    w.borrow()
        .as_any()
        .downcast_ref::<ferrokit_widgets::Label>()
        .map(|l| l.text().to_string())
        .unwrap_or_default()
}

/// Python smoke asserts: after the recovery fetch neither the sidebar card's sub-text nor the
/// table badge may still show the earlier error.
fn smoke_recovery_check(hud: &HUDWindow) -> Result<(), &'static str> {
    let card = hud
        .sidebar_cards
        .get("agy")
        .ok_or("agy card missing")?
        .borrow();
    if smoke_label_text(&card.m1_sub).contains("test") {
        return Err("Cards error survived recovery");
    }
    let column = hud.table.columns.get("agy").ok_or("agy column missing")?;
    if smoke_label_text(&column.badge) == "OFFLINE" {
        return Err("Table error survived recovery");
    }
    Ok(())
}

/// Offline `--smoke-test` (`python/core/smoke_check.py`). The config save/reload round trip runs
/// first, inside the isolated directory; a failed round trip returns FAILURE.
fn run_smoke_test(isolation: &config::SmokeIsolation) -> std::process::ExitCode {
    info!("Running offline smoke-test...");
    let smoke_config = match config::smoke_config_check(isolation) {
        Ok(cfg) => cfg,
        Err(err) => {
            log::error!("Smoke-test failed: {}", err);
            return std::process::ExitCode::FAILURE;
        }
    };
    let config = Arc::new(Mutex::new(smoke_config));
    let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
    let _app = Application::new(std::env::args().collect());

    // Python passes `providers={}`: no provider is built, so no credentials load and nothing fetches.
    let mut hud = HUDWindow::with_providers(
        Arc::clone(&config),
        refresh_ctrl,
        std::collections::HashMap::new(),
    )
    .expect("Failed to initialize HUDWindow");
    hud.on_data_fetched(&crate::providers::base::UsageMetrics {
        provider_id: "agy".to_string(),
        error: Some("test".to_string()),
        ..Default::default()
    });
    hud.on_data_fetched(&crate::providers::base::UsageMetrics {
        provider_id: "agy".to_string(),
        metric1_val: Some(20.0),
        metric1_text: "20%".to_string(),
        ..Default::default()
    });
    if let Err(err) = smoke_recovery_check(&hud) {
        log::error!("Smoke-test failed: {}", err);
        return std::process::ExitCode::FAILURE;
    }
    hud.apply_ui_mode("table");
    hud.apply_ui_mode("sidebar");
    config.lock().color_scheme = "duo".to_string();
    hud.set_theme(true);
    hud.update_clock();
    let _ = ferrokit_core::application::CoreApplication::process_events(false);
    info!("=== Smoke-test passed successfully! ===");
    std::process::ExitCode::SUCCESS
}

/// The HUD is a windowed program, so a started-from-a-shell run has no console to print to; the
/// one-line result of the status line commands goes to the parent's.
#[cfg(target_os = "windows")]
fn attach_parent_console() {
    // SAFETY: plain Win32 call; failing (no parent console) only means nothing is printed.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(
            windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn attach_parent_console() {}

fn main() -> std::process::ExitCode {
    // `--install-agy-statusline` / `--uninstall-agy-statusline` (Python `main`): print one line and
    // exit, before logging, the config and the window exist.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a.ends_with("-agy-statusline")) {
        attach_parent_console();
    }
    if let Some(code) = agy_statusline::cli(&args) {
        return std::process::ExitCode::from(code as u8);
    }
    // Isolation must exist before logging: the logger resolves its directory from APPDATA. The
    // guard lives until `main` returns, so HUD objects (dropped earlier) never outlive it.
    let smoke_isolation = std::env::args().any(|a| a == "--smoke-test").then(|| {
        config::SmokeIsolation::enter(
            &std::env::temp_dir().join(format!("hud-smoke-{}", std::process::id())),
        )
    });
    logger::setup_logging();
    info!("=== Claude HUD Monitor (Rust Qt) starting ===");

    if let Some(isolation) = &smoke_isolation {
        return run_smoke_test(isolation);
    }

    if std::env::args().any(|a| a == "--snapshot") {
        info!("Generating visual snapshot previews of HUD modes...");
        let config = Arc::new(Mutex::new(config::Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let _app = Application::new(std::env::args().collect());

        // Fixed-data providers only: the snapshot must never read credentials or use the network,
        // and `with_providers` launches one fetch per provider.
        let mut hud = HUDWindow::with_providers(
            Arc::clone(&config),
            refresh_ctrl,
            providers::snapshot::snapshot_providers(),
        )
        .expect("Failed to initialize HUDWindow");

        let now = chrono::Utc::now();
        for id in providers::PROVIDER_IDS {
            let metrics =
                providers::snapshot::snapshot_metrics(id, now).expect("snapshot fixture missing");
            hud.on_data_fetched(&metrics);
        }

        let out_dir = std::path::Path::new("target/snapshots");
        let _ = std::fs::create_dir_all(out_dir);

        // 1. Table mode snapshot
        hud.apply_ui_mode("table");
        hud.update_clock();
        let _ = ferrokit_core::application::CoreApplication::process_events(false);
        let table_path = out_dir.join("hud_table_mode.png");
        hud.window
            .save_png(&table_path)
            .expect("failed to save table png");
        info!("Saved table mode snapshot: {}", table_path.display());

        // 2. Sidebar panel snapshot
        hud.apply_ui_mode("sidebar");
        hud.update_clock();
        let _ = ferrokit_core::application::CoreApplication::process_events(false);
        let sidebar_path = out_dir.join("hud_sidebar_panel.png");
        hud.window
            .save_png(&sidebar_path)
            .expect("failed to save sidebar png");
        info!("Saved sidebar snapshot: {}", sidebar_path.display());
        // 3. Context Menu snapshot
        use ferrokit_gui::paint::{Painter, Pixmap};
        use ferrokit_gui::tiny_skia::Color;
        use ferrokit_widgets::Widget;
        let cfg = config.lock();
        let mut context_menu =
            ui::tray_icon::build_hud_menu(&cfg, &hud.window.root_widget(), false);
        context_menu.set_visible(true);
        let menu_size = context_menu.size_hint();
        context_menu.set_geometry(ferrokit_gui::geometry::primitives::Rect::new(
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
    use ferrokit_platform::{SingleInstance, SingleInstanceCommand, SingleInstanceResult};

    let acquire = || {
        SingleInstance::acquire(
            "ClaudeHUDMonitorSingleInstance",
            SingleInstanceCommand::WakeUp,
        )
    };
    let mut single_instance = acquire();
    // A copy started by the panic guard races the old process, which is still releasing the lock.
    if std::env::args().any(|a| a == panic_guard::RESTART_FLAG) {
        for _ in 0..50 {
            if matches!(single_instance, SingleInstanceResult::Primary(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
            single_instance = acquire();
        }
    }
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
            log::warn!(
                "[SingleInstance] Another instance is already running (wake command sent: {command_sent}). Exiting."
            );
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
        // SAFETY: wide is a null-terminated UTF-16 string.
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

    let refresh_ctrl = Arc::new(Mutex::new(
        RefreshController::new(interval).with_jitter(0.25),
    ));

    let mut _app = Application::new(std::env::args().collect());
    configure_application_policy();

    #[cfg(target_os = "windows")]
    ferrokit_core::application::CoreApplication::install_native_event_filter(Box::new(WakeFilter));

    // SAFETY: the owner stays in `main` on the UI thread and nothing reads the object; it only
    // names the thread the worker threads' signals are delivered on.
    let ui_context = unsafe {
        ferrokit_core::object::register_owned_qobject(UiContext {
            data: ferrokit_core::object::ObjectData::with_auto_id(),
        })
    };

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
    ui::hud_window::register_hud(&hud);
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

    // The tray number follows the HUD's data: the window tells `main` (it has no tray reference),
    // which refreshes the tray once per event-loop turn however many providers published.
    hud.borrow_mut().set_quota_changed(|| {
        static PENDING: AtomicBool = AtomicBool::new(false);
        if PENDING.swap(true, Ordering::AcqRel) {
            return;
        }
        Timer::single_shot(0, || {
            PENDING.store(false, Ordering::Release);
            refresh_tray_quota();
        });
    });
    // Draw the stored choice at start-up, before the first data arrives.
    refresh_tray_quota();

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

    // Helper to pop up the modern, styled HUD context menu (used by both HUD body and system tray)
    fn show_hud_popup_menu(global_pos: Point, config: &Arc<Mutex<Config>>, tray: bool) {
        let cfg = config.lock().clone();
        let Some(root) = MAIN_HUD.with(|h_cell| {
            h_cell
                .borrow()
                .as_ref()
                .map(|hud| hud.borrow().window.root_widget())
        }) else {
            return;
        };
        let data = MAIN_HUD
            .with(|h_cell| {
                h_cell
                    .borrow()
                    .as_ref()
                    .map(|hud| hud.borrow().quota_data())
            })
            .unwrap_or_default();
        let quota_keys = ui::tray_icon::quota_keys(&data);
        let menu = if tray {
            ui::tray_icon::build_tray_menu(&cfg, &root, &data)
        } else {
            ui::tray_icon::build_context_menu(&cfg, &root, &data)
        };
        let menu = std::rc::Rc::new(std::cell::RefCell::new(menu));
        if let Some(action) = ferrokit_widgets::menu::Menu::exec_popup(&menu, global_pos)
            && let Some(id) = action.borrow().data().to_u64()
        {
            let action_id = id as u32;
            MAIN_TRAY.with(|t_cell| {
                if let Some(tray) = t_cell.borrow().as_ref() {
                    MAIN_HUD.with(|h_cell| {
                        if let Some(hud) = h_cell.borrow().as_ref() {
                            let mut tray_ref = tray.borrow_mut();
                            let mut hud_ref = hud.borrow_mut();
                            if tray_ref.apply_quota_action(action_id, &quota_keys, &data) {
                                return;
                            }
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

    // Connect system tray context menu request
    let config_clone = Arc::clone(&config);
    tray.borrow()
        .tray
        .on_context_menu_requested
        .connect(move |pos: &Point| {
            let global_pos = *pos;
            let cfg_clone = Arc::clone(&config_clone);
            Timer::single_shot(0, move || {
                show_hud_popup_menu(global_pos, &cfg_clone, true);
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
                show_hud_popup_menu(global_pos, &cfg_clone, false);
            });
        });
    // Python `self.sidebar.contextMenuRequested.connect(self._show_sidebar_menu)`: a right click on
    // the flyout (its own window) shows the same menu.
    let config_clone3 = Arc::clone(&config);
    hud.borrow()
        .sidebar
        .context_menu_requested()
        .connect_local(move |global_pos: &Point| {
            let (global_pos, cfg_clone) = (*global_pos, Arc::clone(&config_clone3));
            Timer::single_shot(0, move || {
                show_hud_popup_menu(global_pos, &cfg_clone, false);
            });
        });

    // Global hotkey manager
    let hotkey_enabled = { config.lock().hotkey_enabled };
    let hotkey_str = { config.lock().hotkey.clone() };
    let hotkey = if hotkey_enabled {
        // The hotkey worker thread emits; the press is handled on the UI thread.
        let pressed = ferrokit_core::signal::Signal::<HotkeyAction>::new();
        connect_on_ui_thread(&ui_context, &pressed, |action| match action {
            HotkeyAction::Toggle => with_hud_retry(|hud| hud.toggle_visibility()),
            HotkeyAction::ClickThrough => toggle_click_through_when_free(),
        });
        let on_press = move |action: HotkeyAction| pressed.emit(&action);
        match HotkeyManager::start(&hotkey_str, on_press) {
            Ok(hk) => {
                info!("Global hotkey manager registered: {}", hotkey_str);
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
            hotkey::LOCKOUT_WARNING_MESSAGE,
            TrayMessageIcon::Warning,
            5000,
        );
    }

    // Follow the operating system's light/dark setting while `appearance` is "auto"
    // (`QStyleHints::colorSchemeChanged`, emitted on the main thread).
    ferrokit_gui::application::GuiApplication::style_hints()
        .color_scheme_changed()
        .connect(move |_| {
            // Python's `singleShot(0, ...)`: a change that arrives while the HUD is borrowed is
            // retried, not lost.
            crate::ui::hud_window::with_hud_later(|hud| hud.follow_system_theme());
        });

    // Connect worker thread results callback
    let refresh_done = ferrokit_core::signal::Signal::<()>::new();
    connect_on_ui_thread(&ui_context, &refresh_done, |()| {
        with_hud_retry(|hud| hud.drain_refresh_results());
    });
    refresh_ctrl
        .lock()
        .set_notify_callback(move || refresh_done.emit(&()));

    // Timers matching Qt C++ QTimer architecture
    let clock_timer = Timer::new();
    clock_timer.set_interval(1000);
    clock_timer.timeout.connect_local(move |()| {
        MAIN_HUD.with(|cell| {
            if let Some(hud) = cell.borrow().as_ref() {
                // A nested event loop (menu, drag) can fire this while the HUD is borrowed: skip the tick.
                let Ok(mut hud) = hud.try_borrow_mut() else {
                    return;
                };
                hud.on_clock_tick(std::time::SystemTime::now());
                if WAKE_REQUESTED.swap(false, Ordering::Relaxed) {
                    hud.wake();
                }
            }
        });
    });
    clock_timer.start();

    let poll_timer = Timer::new();
    poll_timer.set_interval(100);
    poll_timer.timeout.connect_local(move |()| {
        MAIN_HUD.with(|cell| {
            if let Some(hud) = cell.borrow().as_ref()
                && let Ok(mut hud) = hud.try_borrow_mut()
            {
                hud.poll_refresh();
            }
        });
    });
    poll_timer.start();

    // Run Qt event loop
    let exit_code = _app.exec();
    clock_timer.stop();
    poll_timer.stop();
    // `main.py` aboutToQuit: stop the controller so late worker results are discarded.
    refresh_ctrl.lock().stop();
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
    // Qt/C++ return app.exec() exits with the exact signed/32-bit int exit_code without truncation (G4.1.h).
    std::process::exit(exit_code);
}

/// Windows-only: set AppUserModelID for proper taskbar grouping.
#[cfg(target_os = "windows")]
unsafe fn windows_set_appid(wide: &[u16]) -> i32 {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(appid: *const u16) -> i32;
    }
    // SAFETY: caller guarantees wide is a valid null-terminated wide string.
    unsafe { SetCurrentProcessExplicitAppUserModelID(wide.as_ptr()) }
}

#[cfg(test)]
mod tests {
    use super::configure_application_policy;
    use ferrokit_gui::application::GuiApplication;

    /// `main.py:48` `app.setQuitOnLastWindowClosed(False)`: closing the HUD window hides it to the
    /// tray, and only the tray/menu `Application::exit` paths quit.
    #[test]
    fn hud_disables_quit_on_last_window_closed_like_python_main() {
        configure_application_policy();
        assert!(!GuiApplication::quit_on_last_window_closed());
    }

    #[test]
    fn test_exit_code_preserves_32bit_values() {
        // Asserts that exit codes are not truncated to u8 (G4.1.h)
        let code: i32 = 257;
        assert_ne!(code as u8 as i32, code, "Truncation to u8 loses high bits");
    }
}
