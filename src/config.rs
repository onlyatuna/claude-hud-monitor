// src/config.rs — Configuration manager (mirrors Python ConfigManager)
//
// Config is stored as JSON in:
//   Windows: %APPDATA%\ClaudeHUDMonitor\config.json
//   macOS:   ~/Library/Application Support/ClaudeHUDMonitor/config.json
//   Linux:   ~/.config/ClaudeHUDMonitor/config.json
//
// Atomic save: write temp file → fsync → rename (same as Python version).

use log::{error, info};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub window_x: Option<i32>,
    #[serde(default)]
    pub window_y: Option<i32>,
    #[serde(default = "default_layout_mode")]
    pub layout_mode: String,
    #[serde(default = "default_vertical_width")]
    pub vertical_width: u32,
    #[serde(default = "default_vertical_height")]
    pub vertical_height: u32,
    #[serde(default = "default_horizontal_width")]
    pub horizontal_width: u32,
    #[serde(default = "default_horizontal_height")]
    pub horizontal_height: u32,
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub click_through: bool,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_sec: u64,
    #[serde(default = "default_true")]
    pub hotkey_enabled: bool,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub autostart: bool,
}

fn default_layout_mode() -> String {
    "vertical".to_owned()
}
fn default_vertical_width() -> u32 {
    280
}
fn default_vertical_height() -> u32 {
    410
}
fn default_horizontal_width() -> u32 {
    690
}
fn default_horizontal_height() -> u32 {
    152
}
fn default_true() -> bool {
    true
}
fn default_opacity() -> f32 {
    0.88
}
fn default_refresh_interval() -> u64 {
    60
}
fn default_hotkey() -> String {
    "Alt+C".to_owned()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window_x: None,
            window_y: None,
            layout_mode: default_layout_mode(),
            vertical_width: default_vertical_width(),
            vertical_height: default_vertical_height(),
            horizontal_width: default_horizontal_width(),
            horizontal_height: default_horizontal_height(),
            always_on_top: true,
            opacity: default_opacity(),
            click_through: false,
            refresh_interval_sec: default_refresh_interval(),
            hotkey_enabled: true,
            hotkey: default_hotkey(),
            locked: false,
            autostart: false,
        }
    }
}

pub struct ConfigManager;

impl ConfigManager {
    /// Return the platform-appropriate config directory path.
    pub fn config_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            let appdata = std::env::var("APPDATA")
                .unwrap_or_else(|_| dirs_home().to_string_lossy().to_string());
            PathBuf::from(appdata).join("ClaudeHUDMonitor")
        }
        #[cfg(target_os = "macos")]
        {
            dirs_home()
                .join("Library")
                .join("Application Support")
                .join("ClaudeHUDMonitor")
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            dirs_home().join(".config").join("ClaudeHUDMonitor")
        }
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    /// Load config from disk, falling back to defaults on any error.
    pub fn load() -> Config {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(text) => match serde_json::from_str::<Config>(&text) {
                    Ok(mut cfg) => {
                        info!("[Config] Loaded from {:?}", path);
                        if cfg.horizontal_height < 125 {
                            cfg.horizontal_height = 145;
                        }
                        if cfg.horizontal_width < 540 {
                            cfg.horizontal_width = 690;
                        }
                        if cfg.vertical_height < 320 {
                            cfg.vertical_height = 410;
                        }
                        if cfg.vertical_width < 250 {
                            cfg.vertical_width = 280;
                        }
                        return cfg;
                    }
                    Err(e) => error!("[Config] Parse error: {e}"),
                },
                Err(e) => error!("[Config] Read error: {e}"),
            }
        }
        Config::default()
    }

    /// Save config atomically: write temp → rename.
    pub fn save(cfg: &Config) {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            if let Err(e) = fs::create_dir_all(dir) {
                error!("[Config] Cannot create config dir: {e}");
                return;
            }
        }
        let tmp_path = path.with_extension("tmp");
        match serde_json::to_string_pretty(cfg) {
            Ok(text) => {
                if let Err(e) = fs::write(&tmp_path, &text) {
                    error!("[Config] Write temp error: {e}");
                    return;
                }
                if let Err(e) = fs::rename(&tmp_path, &path) {
                    error!("[Config] Rename error: {e}");
                    let _ = fs::remove_file(&tmp_path);
                }
            }
            Err(e) => error!("[Config] Serialize error: {e}"),
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}
