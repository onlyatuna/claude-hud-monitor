// src/ui/mod.rs — egui HUD application

pub mod hud_app;
pub mod native_menu;
pub mod provider_card;
pub mod styles;

pub use hud_app::HudApp;
#[cfg(target_os = "windows")]
pub use hud_app::WAKE_MSG;
#[cfg(target_os = "windows")]
pub use native_menu::enable_win32_dark_mode;
