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
    #[serde(default = "default_claude_profile")]
    pub claude_profile: String,
}

pub const MIN_HORIZONTAL_WIDTH: u32 = 540;
pub const MIN_HORIZONTAL_HEIGHT: u32 = 130;
pub const MIN_VERTICAL_WIDTH: u32 = 250;

pub const HUD_HEADER_HEIGHT: u32 = 16;
pub const HUD_BODY_SPACING: u32 = 3;
pub const HUD_FRAME_VERTICAL_MARGIN: u32 = 10;
pub const VERTICAL_CARD_MIN_HEIGHT: u32 = 104;
pub const VERTICAL_DIVIDER_SPACING: u32 = 3;
pub const VERTICAL_DIVIDER_LINE_HEIGHT: u32 = 1;

pub const fn vertical_layout_min_height() -> u32 {
    HUD_FRAME_VERTICAL_MARGIN
        + HUD_HEADER_HEIGHT
        + HUD_BODY_SPACING
        + VERTICAL_CARD_MIN_HEIGHT * 3
        + VERTICAL_DIVIDER_SPACING * 4
        + VERTICAL_DIVIDER_LINE_HEIGHT * 2
}

pub const MIN_VERTICAL_HEIGHT: u32 = vertical_layout_min_height();

pub const DEFAULT_HORIZONTAL_WIDTH: u32 = 690;
pub const DEFAULT_HORIZONTAL_HEIGHT: u32 = 152;
pub const DEFAULT_VERTICAL_WIDTH: u32 = 280;
pub const DEFAULT_VERTICAL_HEIGHT: u32 = MIN_VERTICAL_HEIGHT + 40;

fn default_layout_mode() -> String {
    "vertical".to_owned()
}
fn default_vertical_width() -> u32 {
    DEFAULT_VERTICAL_WIDTH
}
fn default_vertical_height() -> u32 {
    DEFAULT_VERTICAL_HEIGHT
}
fn default_horizontal_width() -> u32 {
    DEFAULT_HORIZONTAL_WIDTH
}
fn default_horizontal_height() -> u32 {
    DEFAULT_HORIZONTAL_HEIGHT
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
fn default_claude_profile() -> String {
    "auto".to_owned()
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
            claude_profile: default_claude_profile(),
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

    /// Sanitize and clamp configuration parameters to valid ranges.
    pub fn sanitize(cfg: &mut Config) {
        if cfg.horizontal_height < MIN_HORIZONTAL_HEIGHT {
            cfg.horizontal_height = DEFAULT_HORIZONTAL_HEIGHT;
        }
        if cfg.horizontal_width < MIN_HORIZONTAL_WIDTH {
            cfg.horizontal_width = DEFAULT_HORIZONTAL_WIDTH;
        }
        if cfg.vertical_height < MIN_VERTICAL_HEIGHT {
            cfg.vertical_height = DEFAULT_VERTICAL_HEIGHT;
        }
        if cfg.vertical_width < MIN_VERTICAL_WIDTH {
            cfg.vertical_width = DEFAULT_VERTICAL_WIDTH;
        }
        if !cfg.opacity.is_finite() || cfg.opacity <= 0.0 {
            cfg.opacity = default_opacity();
        } else {
            cfg.opacity = cfg.opacity.clamp(0.1, 1.0);
        }
        if cfg.refresh_interval_sec < 20 {
            cfg.refresh_interval_sec = default_refresh_interval();
        }
        if cfg.layout_mode != "horizontal" && cfg.layout_mode != "vertical" {
            cfg.layout_mode = default_layout_mode();
        }
        // Multi-monitor disconnect safety check: if coordinates are out of reasonable bounds
        // (e.g. unplugged secondary monitor leaving window at -9999 or 15000), reset to None.
        if let Some(x) = cfg.window_x {
            if !(-5000..=10000).contains(&x) {
                cfg.window_x = None;
            }
        }
        if let Some(y) = cfg.window_y {
            if !(-5000..=10000).contains(&y) {
                cfg.window_y = None;
            }
        }
        if cfg.claude_profile.trim().is_empty() {
            cfg.claude_profile = default_claude_profile();
        }
    }

    /// Load config from disk, falling back to defaults on any error.
    pub fn load() -> Config {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(text) => match serde_json::from_str::<Config>(&text) {
                    Ok(mut cfg) => {
                        info!("[Config] Loaded from {:?}", path);
                        Self::sanitize(&mut cfg);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.layout_mode, "vertical");
        assert_eq!(cfg.vertical_width, 280);
        assert_eq!(cfg.vertical_height, DEFAULT_VERTICAL_HEIGHT);
        assert_eq!(cfg.horizontal_width, 690);
        assert_eq!(cfg.horizontal_height, 152);
        assert!(cfg.always_on_top);
        assert!(!cfg.click_through);
        assert_eq!(cfg.refresh_interval_sec, 60);
        assert_eq!(cfg.hotkey, "Alt+C");
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let cfg = Config {
            layout_mode: "horizontal".to_string(),
            opacity: 0.75,
            click_through: true,
            refresh_interval_sec: 120,
            ..Default::default()
        };

        let json = serde_json::to_string(&cfg).expect("serialization failed");
        let restored: Config = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(restored.layout_mode, "horizontal");
        assert_eq!(restored.opacity, 0.75);
        assert!(restored.click_through);
        assert_eq!(restored.refresh_interval_sec, 120);
    }

    #[test]
    fn test_config_nan_and_bounds_sanitization() {
        let mut cfg = Config {
            opacity: f32::NAN,
            refresh_interval_sec: 5,
            horizontal_height: 50,
            horizontal_width: 100,
            vertical_height: 50,
            vertical_width: 100,
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg);
        assert_eq!(cfg.opacity, 0.88);
        assert_eq!(cfg.refresh_interval_sec, 60);
        assert_eq!(cfg.horizontal_height, DEFAULT_HORIZONTAL_HEIGHT);
        assert_eq!(cfg.horizontal_width, DEFAULT_HORIZONTAL_WIDTH);
        assert_eq!(cfg.vertical_height, DEFAULT_VERTICAL_HEIGHT);
        assert_eq!(cfg.vertical_width, DEFAULT_VERTICAL_WIDTH);

        // Test non-finite and negative opacity
        let mut cfg_neg = Config {
            opacity: -0.5,
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg_neg);
        assert_eq!(cfg_neg.opacity, 0.88);

        // Test clamping of values > 1.0 and < 0.1
        let mut cfg_clamp = Config {
            opacity: 1.5,
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg_clamp);
        assert_eq!(cfg_clamp.opacity, 1.0);

        let mut cfg_clamp2 = Config {
            opacity: 0.05,
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg_clamp2);
        assert_eq!(cfg_clamp2.opacity, 0.1);

        // Test multi-monitor coordinate out-of-bounds reset
        let mut cfg_coords = Config {
            window_x: Some(-99999),
            window_y: Some(50000),
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg_coords);
        assert_eq!(cfg_coords.window_x, None);
        assert_eq!(cfg_coords.window_y, None);

        let mut cfg_valid_coords = Config {
            window_x: Some(-1920),
            window_y: Some(100),
            layout_mode: "invalid_mode_string".to_string(),
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg_valid_coords);
        assert_eq!(cfg_valid_coords.window_x, Some(-1920));
        assert_eq!(cfg_valid_coords.window_y, Some(100));
        assert_eq!(cfg_valid_coords.layout_mode, "vertical");

        // Test claude_profile default and sanitization
        let mut cfg_profile = Config {
            claude_profile: "   ".to_string(),
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg_profile);
        assert_eq!(cfg_profile.claude_profile, "auto");

        // Test backward compatibility deserialization without claude_profile
        let json = r#"{"opacity": 0.5}"#;
        let deserialized: Config = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.claude_profile, "auto");
    }
}
