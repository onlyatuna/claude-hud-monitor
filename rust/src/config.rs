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

use parking_lot::{Condvar, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub window_x: Option<i32>,
    #[serde(default)]
    pub window_y: Option<i32>,
    #[serde(default = "default_ui_mode")]
    pub ui_mode: String,
    #[serde(default = "default_table_width")]
    pub table_width: u32,
    #[serde(default = "default_table_height")]
    pub table_height: u32,
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,
    #[serde(default = "default_appearance")]
    pub appearance: String,
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

// Window size constants, `hud_window.py:31-35`. The minimum is the explicit `setMinimumSize`; it wins
// over the layout's own minimum, so it is not derived from the content (G12.8.f / RC-23).
pub const MIN_HORIZONTAL_WIDTH: u32 = 540;
pub const MIN_HORIZONTAL_HEIGHT: u32 = 125;
pub const MIN_VERTICAL_WIDTH: u32 = 250;
pub const MIN_VERTICAL_HEIGHT: u32 = 320;

pub const DEFAULT_HORIZONTAL_WIDTH: u32 = 690;
pub const DEFAULT_HORIZONTAL_HEIGHT: u32 = 145;
pub const DEFAULT_VERTICAL_WIDTH: u32 = 280;
pub const DEFAULT_VERTICAL_HEIGHT: u32 = 410;
pub const MIN_TABLE_WIDTH: u32 = 380;
pub const MIN_TABLE_HEIGHT: u32 = 280;
pub const DEFAULT_TABLE_WIDTH: u32 = 450;
pub const DEFAULT_TABLE_HEIGHT: u32 = 350;

fn default_ui_mode() -> String {
    "cards".to_owned()
}
fn default_table_width() -> u32 {
    DEFAULT_TABLE_WIDTH
}
fn default_table_height() -> u32 {
    DEFAULT_TABLE_HEIGHT
}
fn default_color_scheme() -> String {
    "scale".to_owned()
}
fn default_appearance() -> String {
    "auto".to_owned()
}

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
            ui_mode: default_ui_mode(),
            table_width: default_table_width(),
            table_height: default_table_height(),
            color_scheme: default_color_scheme(),
            appearance: default_appearance(),
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
    #[cfg_attr(test, allow(dead_code))] // only reached by the non-test `config_path`
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

    /// Path of the config file, resolved as the Python HUD's `get_config_path` does when packaged:
    /// a writable `config.json` next to the executable (portable install) wins, otherwise the
    /// per-user file in [`config_dir`](Self::config_dir). The two builds therefore share settings.
    ///
    /// Unit tests get a throw-away file: they switch layouts and save, and must not rewrite the
    /// settings of the HUD installed on the machine.
    pub fn config_path() -> PathBuf {
        #[cfg(test)]
        {
            std::env::temp_dir()
                .join("ClaudeHUDMonitor-tests")
                .join("config.json")
        }
        #[cfg(not(test))]
        {
            Self::portable_config_path().unwrap_or_else(|| Self::config_dir().join("config.json"))
        }
    }

    #[cfg(not(test))]
    fn portable_config_path() -> Option<PathBuf> {
        let portable = std::env::current_exe().ok()?.parent()?.join("config.json");
        let writable = fs::metadata(&portable)
            .ok()
            .is_some_and(|m| !m.permissions().readonly());
        writable.then_some(portable)
    }

    /// Sanitize and clamp configuration parameters to valid ranges.
    pub fn sanitize(cfg: &mut Config) {
        if cfg.horizontal_height < MIN_HORIZONTAL_HEIGHT || cfg.horizontal_height > 300 {
            cfg.horizontal_height = DEFAULT_HORIZONTAL_HEIGHT;
            if cfg.horizontal_width < 600 {
                cfg.horizontal_width = DEFAULT_HORIZONTAL_WIDTH;
            }
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

    /// Save a Config value atomically. Acquires SAVE_LOCK itself, so it MUST NOT be called
    /// by a path that already holds SAVE_LOCK (use `save_unlocked` there).
    #[allow(dead_code)]
    pub fn save(cfg: &Config) {
        let _save = SAVE_LOCK.lock();
        Self::save_unlocked(cfg);
    }

    /// Serialize, write tmp, rename. Does NOT take SAVE_LOCK: the caller MUST already hold it.
    pub(crate) fn save_unlocked(cfg: &Config) {
        Self::save_internal(&Self::config_path(), cfg);
    }

    /// Mutate the shared Config and persist it immediately.
    ///
    /// Lock order: SAVE_LOCK -> Config lock. The Config lock is released (after cloning a
    /// snapshot) before any serialization or file I/O; SAVE_LOCK alone spans the I/O.
    pub fn update_and_save(config: &Mutex<Config>, mutate: impl FnOnce(&mut Config)) {
        let _save = SAVE_LOCK.lock();
        let snapshot = {
            let mut cfg = config.lock();
            mutate(&mut cfg);
            cfg.clone()
        };
        Self::save_unlocked(&snapshot);
    }

    /// Test helper: serialized save to an explicit path (takes SAVE_LOCK).
    #[cfg(test)]
    pub(crate) fn save_to_path(path: &std::path::Path, cfg: &Config) {
        let _save = SAVE_LOCK.lock();
        Self::save_internal(path, cfg);
    }

    /// Test helper: unserialized save to an explicit path (caller holds SAVE_LOCK).
    #[cfg(test)]
    pub(crate) fn save_unlocked_to_path(path: &std::path::Path, cfg: &Config) {
        Self::save_internal(path, cfg);
    }

    fn save_internal(path: &std::path::Path, cfg: &Config) {
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
                if let Err(e) = fs::rename(&tmp_path, path) {
                    error!("[Config] Rename error: {e}");
                    let _ = fs::remove_file(&tmp_path);
                }
            }
            Err(e) => error!("[Config] Serialize error: {e}"),
        }
    }
}

/// Process-wide serialization of config file writes (tmp write + rename).
///
/// Lock order, everywhere: SAVE_LOCK -> Config lock. The DebounceInner mutex is a leaf:
/// it is only held briefly and never while waiting for SAVE_LOCK or the Config lock.
/// The Config lock is never held across file I/O.
static SAVE_LOCK: Mutex<()> = Mutex::new(());

pub const RESIZE_DEBOUNCE_MS: u64 = 250;

struct DebounceInner {
    pending: bool,
    deadline: Option<Instant>,
    in_flight: bool,
    stopped: bool,
    has_worker: bool,
    save_count: usize,
}

type DebounceState = (Mutex<DebounceInner>, Condvar);

/// Snapshot the latest Config and persist it with `save_fn`.
/// Lock order SAVE_LOCK -> Config; the Config lock is dropped before `save_fn` runs.
fn perform_save(config: &Mutex<Config>, save_fn: &dyn Fn(&Config)) {
    let _save = SAVE_LOCK.lock();
    let snapshot = config.lock().clone();
    save_fn(&snapshot);
}

/// Clears `in_flight` and wakes waiters when a worker save ends, including by panic.
struct InFlightGuard<'a>(&'a DebounceState);

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        let (lock, condvar) = self.0;
        let mut inner = lock.lock();
        inner.in_flight = false;
        inner.save_count += 1;
        if std::thread::panicking() {
            inner.has_worker = false;
        }
        condvar.notify_all();
    }
}

/// Debounces resize-triggered config saves onto a single worker thread.
///
/// At most one debouncer-initiated save exists at any time, and only the worker (or the
/// inline fallback when no worker thread exists) runs it. `flush()` drains by making the
/// pending save due immediately and waiting until nothing is pending or in flight.
pub struct ResizeDebouncer {
    state: Arc<DebounceState>,
    config: Arc<Mutex<Config>>,
    debounce_duration: Duration,
    restoring: Arc<AtomicBool>,
    save_fn: Arc<dyn Fn(&Config) + Send + Sync + 'static>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl ResizeDebouncer {
    pub fn new(config: Arc<Mutex<Config>>) -> Self {
        Self::with_save_fn(
            config,
            Duration::from_millis(RESIZE_DEBOUNCE_MS),
            Arc::new(|cfg| {
                ConfigManager::save_unlocked(cfg);
            }),
        )
    }

    /// `save_fn` is always invoked with SAVE_LOCK held and the Config lock released, so it
    /// must not take SAVE_LOCK itself (use `ConfigManager::save_unlocked`).
    pub fn with_save_fn(
        config: Arc<Mutex<Config>>,
        debounce_duration: Duration,
        save_fn: Arc<dyn Fn(&Config) + Send + Sync + 'static>,
    ) -> Self {
        let state: Arc<DebounceState> = Arc::new((
            Mutex::new(DebounceInner {
                pending: false,
                deadline: None,
                in_flight: false,
                stopped: false,
                has_worker: false,
                save_count: 0,
            }),
            Condvar::new(),
        ));
        let state_clone = Arc::clone(&state);
        let config_clone = Arc::clone(&config);
        let save_fn_clone = Arc::clone(&save_fn);

        let worker = std::thread::Builder::new()
            .name("resize-debouncer".to_string())
            .spawn(move || {
                let (lock, condvar) = &*state_clone;
                let mut inner = lock.lock();
                loop {
                    if inner.stopped {
                        return;
                    }
                    if !inner.pending {
                        condvar.wait(&mut inner);
                        continue;
                    }
                    if let Some(deadline) = inner.deadline {
                        let now = Instant::now();
                        if now < deadline {
                            condvar.wait_for(&mut inner, deadline - now);
                            continue;
                        }
                    }

                    // Deadline reached: claim the save, then snapshot the *current* Config.
                    inner.pending = false;
                    inner.deadline = None;
                    inner.in_flight = true;
                    drop(inner);
                    {
                        let _flight = InFlightGuard(&state_clone);
                        perform_save(&config_clone, &*save_fn_clone);
                    }
                    inner = lock.lock();
                }
            })
            .ok();

        state.0.lock().has_worker = worker.is_some();

        Self {
            state,
            config,
            debounce_duration,
            restoring: Arc::new(AtomicBool::new(false)),
            save_fn,
            worker: Mutex::new(worker),
        }
    }

    #[inline]
    pub fn is_restoring(&self) -> bool {
        self.restoring.load(Ordering::SeqCst)
    }

    #[inline]
    pub fn set_restoring(&self, restoring: bool) {
        self.restoring.store(restoring, Ordering::SeqCst);
    }

    /// Schedule a save `debounce_duration` from now, replacing any earlier deadline.
    pub fn request_save(&self) {
        if self.is_restoring() {
            return;
        }
        let (lock, condvar) = &*self.state;
        {
            let mut inner = lock.lock();
            if inner.stopped {
                return;
            }
            inner.pending = true;
            inner.deadline = Some(Instant::now() + self.debounce_duration);
        }
        condvar.notify_all();
    }

    /// Drain all debouncer persistence work.
    ///
    /// - pending, idle:       the save becomes due now; waits for the worker to run it.
    /// - idle, in flight:     waits for the running save.
    /// - pending, in flight:  waits for the running save, then the worker runs exactly one
    ///   more save with the latest Config. No second concurrent save is ever started.
    /// - idle, idle:          returns immediately.
    ///
    /// Must not be called while holding the Config lock or SAVE_LOCK.
    pub fn flush(&self) {
        let (lock, condvar) = &*self.state;
        let mut inner = lock.lock();

        if inner.pending && !inner.has_worker {
            // No worker thread exists (spawn failed or it panicked): run inline.
            inner.pending = false;
            inner.deadline = None;
            drop(inner);
            perform_save(&self.config, &*self.save_fn);
            lock.lock().save_count += 1;
            return;
        }

        if inner.pending {
            inner.deadline = Some(Instant::now());
            condvar.notify_all();
        }
        while inner.has_worker && (inner.in_flight || (inner.pending && !inner.stopped)) {
            condvar.wait(&mut inner);
        }
    }

    /// Apply `mutate` to the shared Config and persist immediately, superseding any pending
    /// debounced save. Lock order SAVE_LOCK -> Config; Config is released before I/O.
    /// Any in-flight worker save finishes first because it holds SAVE_LOCK.
    pub fn update_and_save_now(&self, mutate: impl FnOnce(&mut Config)) {
        if self.is_restoring() {
            return;
        }
        let _save = SAVE_LOCK.lock();
        let (lock, condvar) = &*self.state;
        {
            let mut inner = lock.lock();
            inner.pending = false;
            inner.deadline = None;
            inner.save_count += 1;
            condvar.notify_all();
        }
        let snapshot = {
            let mut cfg = self.config.lock();
            mutate(&mut cfg);
            cfg.clone()
        };
        (self.save_fn)(&snapshot);
    }

    /// Graceful shutdown: drain via `flush()`, stop the worker, join it.
    /// On return no debouncer save is running and the worker thread has exited.
    /// Idempotent. Must not be called while holding the Config lock or SAVE_LOCK.
    pub fn shutdown(&self) {
        self.flush();
        {
            let (lock, condvar) = &*self.state;
            let mut inner = lock.lock();
            inner.stopped = true;
            condvar.notify_all();
        }
        if let Some(handle) = self.worker.lock().take() {
            let _ = handle.join();
        }
        // A request_save() racing between flush() and `stopped` would leave a pending save
        // with no worker; persist it here.
        let leftover = {
            let mut inner = self.state.0.lock();
            std::mem::take(&mut inner.pending)
        };
        if leftover {
            perform_save(&self.config, &*self.save_fn);
            self.state.0.lock().save_count += 1;
        }
    }

    #[allow(dead_code)]
    pub fn is_worker_joined(&self) -> bool {
        self.worker.lock().is_none()
    }

    #[allow(dead_code)]
    pub fn save_count(&self) -> usize {
        let (lock, _) = &*self.state;
        lock.lock().save_count
    }

    #[allow(dead_code)]
    pub fn is_pending(&self) -> bool {
        let (lock, _) = &*self.state;
        lock.lock().pending
    }

    #[allow(dead_code)]
    pub fn is_in_flight(&self) -> bool {
        let (lock, _) = &*self.state;
        lock.lock().in_flight
    }
}

/// Drop runs `shutdown()` (drain + join) during normal object destruction only.
/// It does not run on process::abort, SIGKILL, TerminateProcess, access violations,
/// OS crash or power loss.
impl Drop for ResizeDebouncer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg_attr(test, allow(dead_code))] // only reached by the non-test config paths
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
        // PySide6 `DEFAULT_CONFIG` (`config_manager.py:15-17`).
        assert_eq!(cfg.vertical_width, 280);
        assert_eq!(cfg.vertical_height, 410);
        assert_eq!(cfg.horizontal_width, 690);
        assert_eq!(cfg.horizontal_height, 145);
        assert!(cfg.always_on_top);
        assert!(!cfg.click_through);
        assert_eq!(cfg.refresh_interval_sec, 60);
        assert_eq!(cfg.hotkey, "Alt+C");
    }

    /// G12.8.f / RC-23. Python (`hud_window.py:31-35`, `config_manager.py:15-17`) keeps any stored
    /// size at or above its minimum (125 high horizontal, 320 high vertical) and only below that
    /// falls back to the default (145 / 410). Rust's higher minimums used to reset stored sizes
    /// Python keeps.
    #[test]
    fn test_sanitize_keeps_stored_sizes_pyside6_keeps() {
        let mut cfg = Config {
            horizontal_height: 125,
            vertical_height: 320,
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg);
        assert_eq!((cfg.horizontal_height, cfg.vertical_height), (125, 320));

        let mut cfg = Config {
            horizontal_height: 124,
            vertical_height: 319,
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg);
        assert_eq!((cfg.horizontal_height, cfg.vertical_height), (145, 410));

        // Stored by an earlier Rust build (its old defaults / minimum): still valid in Python.
        let mut cfg = Config {
            horizontal_height: 152,
            vertical_height: 490,
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg);
        assert_eq!((cfg.horizontal_height, cfg.vertical_height), (152, 490));
        let mut cfg = Config {
            vertical_height: 463,
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg);
        assert_eq!(cfg.vertical_height, 463);
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

        // Test corrupted horizontal dimensions (e.g. from vertical leak)
        let mut cfg_corrupt = Config {
            horizontal_height: 463,
            horizontal_width: 540,
            ..Default::default()
        };
        ConfigManager::sanitize(&mut cfg_corrupt);
        assert_eq!(cfg_corrupt.horizontal_height, DEFAULT_HORIZONTAL_HEIGHT);
        assert_eq!(cfg_corrupt.horizontal_width, DEFAULT_HORIZONTAL_WIDTH);
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

    /// Waits until the debouncer has finished `expected` saves (5 s cap). The worker counts a
    /// save after `save_fn` returns, so waiting on the debouncer's own count also covers the
    /// test's counter; a fixed sleep right after the debounce expires fails on a loaded runner.
    fn wait_for_saves(debouncer: &ResizeDebouncer, expected: usize) {
        let limit = Instant::now() + Duration::from_secs(5);
        while debouncer.save_count() < expected && Instant::now() < limit {
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Asserts no save has run yet, when that is decidable: the counter is read first, then the
    /// time since `requested` (taken before the last `request_save`). The save cannot run
    /// before `requested + debounce`, and no earlier deadline can have expired unless the test
    /// thread stalled for a debounce between two events (`split`). A stalled thread observes
    /// nothing instead of failing on a save the debouncer was right to make.
    fn assert_no_save_yet(
        counter: &std::sync::atomic::AtomicUsize,
        requested: Instant,
        debounce: Duration,
        split: bool,
    ) {
        let saves = counter.load(Ordering::SeqCst);
        if !split && requested.elapsed() < debounce {
            assert_eq!(saves, 0, "a save ran before the debounce elapsed");
        }
    }

    /// Whether the previous request (`previous`, taken before its `request_save`) is at least
    /// one debounce before now (taken after the current `request_save` returned). Only then can
    /// the debouncer have saved between the two, splitting the burst.
    fn gap_reached_debounce(previous: Option<Instant>, debounce: Duration) -> bool {
        previous.is_some_and(|t| t.elapsed() >= debounce)
    }

    /// A burst whose events were all closer than the debounce coalesces into exactly one save;
    /// a burst the test thread split by stalling may legitimately save more than once.
    fn assert_one_save(
        counter: &std::sync::atomic::AtomicUsize,
        debouncer: &ResizeDebouncer,
        split: bool,
    ) {
        let saves = counter.load(Ordering::SeqCst);
        if split {
            assert!(saves >= 1, "the burst was never saved");
        } else {
            assert_eq!(saves, 1, "the burst was not coalesced into one save");
            assert_eq!(debouncer.save_count(), 1);
        }
    }

    #[test]
    fn test_resize_debounce_coalesces_multiple_events() {
        let save_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = Arc::clone(&save_counter);

        let cfg = Arc::new(Mutex::new(Config::default()));
        let debounce = Duration::from_millis(60);
        let debouncer = ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            debounce,
            Arc::new(move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );

        // Multiple rapid resize events
        let mut last_request: Option<Instant> = None;
        let mut split = false;
        for i in 0..5 {
            cfg.lock().table_width = 500 + i * 10;
            let requested = Instant::now();
            debouncer.request_save();
            split |= gap_reached_debounce(last_request, debounce);
            last_request = Some(requested);
            std::thread::sleep(Duration::from_millis(10));
        }

        // Still within the debounce window of the last event: nothing saved yet
        assert_no_save_yet(&save_counter, last_request.unwrap(), debounce, split);

        wait_for_saves(&debouncer, 1);
        assert_one_save(&save_counter, &debouncer, split);
    }

    #[test]
    fn test_resize_debounce_resets_timer_on_continued_resizing() {
        let save_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = Arc::clone(&save_counter);

        let cfg = Arc::new(Mutex::new(Config::default()));
        let debounce = Duration::from_millis(120);
        let debouncer = ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            debounce,
            Arc::new(move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );

        // Events A, B, C 60 ms apart: each one resets the 120 ms deadline
        let mut last_request: Option<Instant> = None;
        let mut split = false;
        for width in [510, 520, 530] {
            cfg.lock().table_width = width;
            let requested = Instant::now();
            debouncer.request_save();
            split |= gap_reached_debounce(last_request, debounce);
            last_request = Some(requested);
            std::thread::sleep(Duration::from_millis(60));
            assert_no_save_yet(&save_counter, requested, debounce, split);
        }

        wait_for_saves(&debouncer, 1);
        assert_one_save(&save_counter, &debouncer, split);
    }

    #[test]
    fn test_resize_debounce_close_flushes_immediately() {
        let save_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = Arc::clone(&save_counter);

        let cfg = Arc::new(Mutex::new(Config::default()));
        let debouncer = ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(300),
            Arc::new(move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );

        // Resize happens
        cfg.lock().table_width = 550;
        debouncer.request_save();

        // Immediately before debounce fires:
        assert_eq!(save_counter.load(Ordering::SeqCst), 0);
        assert!(debouncer.is_pending());

        // Close/flush triggered
        debouncer.flush();

        // Saved immediately!
        assert_eq!(save_counter.load(Ordering::SeqCst), 1);
        assert!(!debouncer.is_pending());

        // Wait past original 300ms debounce duration
        std::thread::sleep(Duration::from_millis(350));

        // Must NOT double save!
        assert_eq!(save_counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_resize_debounce_restore_geometry_guard_prevents_spurious_save() {
        let save_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = Arc::clone(&save_counter);

        let cfg = Arc::new(Mutex::new(Config::default()));
        let debouncer = ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(60),
            Arc::new(move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );

        // Simulating restore geometry phase during startup
        debouncer.set_restoring(true);
        assert!(debouncer.is_restoring());

        // During restore, resize callback occurs
        cfg.lock().table_width = 580;
        debouncer.request_save();
        debouncer.flush();

        std::thread::sleep(Duration::from_millis(100));

        // Guard must have prevented any save!
        assert_eq!(save_counter.load(Ordering::SeqCst), 0);
        assert_eq!(debouncer.save_count(), 0);

        // Restore completes
        debouncer.set_restoring(false);

        // Normal resize after restore
        debouncer.request_save();
        wait_for_saves(&debouncer, 1);

        assert_eq!(save_counter.load(Ordering::SeqCst), 1);
        assert_eq!(debouncer.save_count(), 1);
    }

    #[test]
    fn test_concurrent_save_serialization_no_corruption() {
        let temp_dir = std::env::temp_dir().join(format!("claude_hud_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let test_path = temp_dir.join("test_concurrent_config.json");

        let mut handles = Vec::new();
        for i in 0..10 {
            let path_clone = test_path.clone();
            let handle = std::thread::spawn(move || {
                for j in 0..15 {
                    let mut cfg = Config::default();
                    cfg.table_width = 500 + i * 20 + j;
                    cfg.opacity = 0.5 + (j as f32 * 0.01);
                    ConfigManager::save_to_path(&path_clone, &cfg);
                }
            });
            handles.push(handle);
        }

        for h in handles {
            h.join().expect("thread join failed");
        }

        // Must exist, be valid JSON, and parseable into Config
        assert!(test_path.exists());
        let text = fs::read_to_string(&test_path).expect("failed to read written config");
        let parsed: Result<Config, _> = serde_json::from_str(&text);
        assert!(parsed.is_ok(), "Written JSON was corrupted by race: {text}");

        // Cleanup
        let _ = fs::remove_file(&test_path);
        let _ = fs::remove_file(test_path.with_extension("tmp"));
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_flush_waits_for_in_flight_save() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let save_started = Arc::new(AtomicBool::new(false));
        let save_finished = Arc::new(AtomicBool::new(false));
        let can_finish_save = Arc::new(AtomicBool::new(false));

        let s_start = Arc::clone(&save_started);
        let s_finish = Arc::clone(&save_finished);
        let c_finish = Arc::clone(&can_finish_save);

        let debouncer = Arc::new(ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(20),
            Arc::new(move |_| {
                s_start.store(true, Ordering::SeqCst);
                // Simulate slow I/O by waiting until signaled
                while !c_finish.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                s_finish.store(true, Ordering::SeqCst);
            }),
        ));

        // Request save
        debouncer.request_save();

        // Wait until worker enters save_fn (in_flight is true)
        while !save_started.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }

        // Worker is now inside save_fn with in_flight = true
        let debouncer_flush = Arc::clone(&debouncer);
        let flush_returned = Arc::new(AtomicBool::new(false));
        let f_ret = Arc::clone(&flush_returned);

        let flush_handle = std::thread::spawn(move || {
            debouncer_flush.flush();
            f_ret.store(true, Ordering::SeqCst);
        });

        // Give flush thread time to enter wait
        std::thread::sleep(Duration::from_millis(40));

        // Because save is not allowed to finish yet, flush MUST still be waiting!
        assert!(
            !flush_returned.load(Ordering::SeqCst),
            "flush returned before in-flight save finished!"
        );
        assert!(!save_finished.load(Ordering::SeqCst));

        // Now allow save to finish
        can_finish_save.store(true, Ordering::SeqCst);

        flush_handle.join().expect("flush thread join failed");

        // After flush returned, save MUST be finished!
        assert!(flush_returned.load(Ordering::SeqCst));
        assert!(save_finished.load(Ordering::SeqCst));
    }

    #[test]
    fn test_stale_snapshot_cannot_overwrite_newer_config() {
        let temp_dir =
            std::env::temp_dir().join(format!("claude_hud_stale_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let test_path = temp_dir.join("test_stale_config.json");

        let cfg = Arc::new(Mutex::new(Config::default()));
        let path_clone = test_path.clone();

        let debouncer = ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(40),
            Arc::new(move |c| {
                ConfigManager::save_unlocked_to_path(&path_clone, c);
            }),
        );

        // Resize sets S1: table_width = 800
        cfg.lock().table_width = 800;
        debouncer.request_save();

        // Concurrent immediate save of S2 (opacity 0.5, table_width 950).
        std::thread::sleep(Duration::from_millis(10));
        debouncer.update_and_save_now(|c| {
            c.opacity = 0.5;
            c.table_width = 950;
        });
        debouncer.flush();

        let text = fs::read_to_string(&test_path).expect("failed to read config");
        let on_disk: Config = serde_json::from_str(&text).expect("invalid json");
        assert_eq!(on_disk.opacity, 0.5);
        assert_eq!(on_disk.table_width, 950);

        let _ = fs::remove_file(&test_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_shutdown_joins_worker_and_completes_save() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let saved = Arc::new(AtomicBool::new(false));
        let saved_clone = Arc::clone(&saved);

        let debouncer = ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(500),
            Arc::new(move |_| {
                saved_clone.store(true, Ordering::SeqCst);
            }),
        );

        debouncer.request_save();
        assert!(debouncer.is_pending());

        // Shutdown immediately
        debouncer.shutdown();

        // 1. Pending must be cleared
        assert!(!debouncer.is_pending());
        // 2. Pending save must have completed
        assert!(saved.load(Ordering::SeqCst));
        // 3. Worker thread must be joined
        assert!(debouncer.is_worker_joined());
    }

    use std::sync::atomic::AtomicUsize;

    /// Mock save target. The first call blocks until `release` is set; all calls record the
    /// table_width they were given and how many saves overlapped.
    struct Gate {
        started: AtomicBool,
        release: AtomicBool,
        calls: AtomicUsize,
        active: AtomicUsize,
        max_active: AtomicUsize,
        widths: Mutex<Vec<u32>>,
    }

    impl Gate {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                started: AtomicBool::new(false),
                release: AtomicBool::new(false),
                calls: AtomicUsize::new(0),
                active: AtomicUsize::new(0),
                max_active: AtomicUsize::new(0),
                widths: Mutex::new(Vec::new()),
            })
        }

        fn save_fn(self: &Arc<Self>) -> Arc<dyn Fn(&Config) + Send + Sync + 'static> {
            let g = Arc::clone(self);
            Arc::new(move |c: &Config| {
                let n = g.calls.fetch_add(1, Ordering::SeqCst);
                let a = g.active.fetch_add(1, Ordering::SeqCst) + 1;
                g.max_active.fetch_max(a, Ordering::SeqCst);
                g.widths.lock().push(c.table_width);
                g.started.store(true, Ordering::SeqCst);
                if n == 0 {
                    while !g.release.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                g.active.fetch_sub(1, Ordering::SeqCst);
            })
        }

        fn wait_started(&self) {
            let start = Instant::now();
            while !self.started.load(Ordering::SeqCst) {
                assert!(
                    start.elapsed() < Duration::from_secs(10),
                    "save never started"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn test_config_lock_not_held_during_disk_io() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let gate = Gate::new();
        let debouncer = ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(5),
            gate.save_fn(),
        );

        debouncer.request_save();
        gate.wait_started();
        assert!(debouncer.is_in_flight());

        // The save is blocked "in disk I/O"; the Config lock must be free.
        let acquired = cfg.try_lock_for(Duration::from_secs(2)).is_some();
        gate.release.store(true, Ordering::SeqCst);
        debouncer.flush();
        assert!(acquired, "Config lock was held across the save");
    }

    #[test]
    fn test_immediate_save_during_in_flight_worker_save_does_not_deadlock() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let gate = Gate::new();
        let debouncer = Arc::new(ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(5),
            gate.save_fn(),
        ));

        debouncer.request_save();
        gate.wait_started();

        // Same shape as persist_geometry while the worker has in_flight == true.
        let d = Arc::clone(&debouncer);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            d.update_and_save_now(|c| c.table_width = 777);
            let _ = tx.send(());
        });

        let config_free = cfg.try_lock_for(Duration::from_secs(2)).is_some();
        gate.release.store(true, Ordering::SeqCst);
        let finished = rx.recv_timeout(Duration::from_secs(10)).is_ok();
        debouncer.flush();

        assert!(config_free);
        assert!(finished, "update_and_save_now deadlocked against worker");
        assert_eq!(cfg.lock().table_width, 777);
        assert_eq!(gate.widths.lock().last().copied(), Some(777));
        assert_eq!(gate.max_active.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_flush_pending_and_in_flight_runs_one_save_at_a_time() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let gate = Gate::new();
        let debouncer = Arc::new(ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(20),
            gate.save_fn(),
        ));

        debouncer.request_save();
        gate.wait_started();

        // New resize arrives while the first save is in flight: pending && in_flight.
        cfg.lock().table_width = 999;
        debouncer.request_save();
        assert!(debouncer.is_pending());
        assert!(debouncer.is_in_flight());

        let d = Arc::clone(&debouncer);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            d.flush();
            let _ = tx.send(());
        });
        std::thread::sleep(Duration::from_millis(40));
        let returned_early = rx.try_recv().is_ok();

        gate.release.store(true, Ordering::SeqCst);
        let finished = rx.recv_timeout(Duration::from_secs(10)).is_ok();

        assert!(!returned_early, "flush returned while a save was in flight");
        assert!(finished);
        assert_eq!(gate.calls.load(Ordering::SeqCst), 2);
        assert_eq!(gate.max_active.load(Ordering::SeqCst), 1);
        assert_eq!(gate.widths.lock().last().copied(), Some(999));
        assert!(!debouncer.is_pending());
        assert!(!debouncer.is_in_flight());
    }

    #[test]
    fn test_worker_and_immediate_save_race_leaves_latest_config() {
        let temp_dir = std::env::temp_dir().join(format!("claude_hud_race_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let test_path = temp_dir.join("test_race_config.json");

        let cfg = Arc::new(Mutex::new(Config::default()));
        let path_clone = test_path.clone();
        let debouncer = ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(1),
            Arc::new(move |c| ConfigManager::save_unlocked_to_path(&path_clone, c)),
        );

        for i in 0..40u32 {
            cfg.lock().table_width = 100 + i;
            debouncer.request_save();
            if i % 2 == 0 {
                std::thread::yield_now();
            }
            let opacity = 0.2 + i as f32 * 0.01;
            debouncer.update_and_save_now(|c| c.opacity = opacity);
            debouncer.flush();

            let text = fs::read_to_string(&test_path).expect("read config");
            let on_disk: Config = serde_json::from_str(&text).expect("valid json");
            assert_eq!(on_disk.table_width, 100 + i, "iteration {i}");
            assert_eq!(on_disk.opacity, opacity, "iteration {i}");
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_lock_is_not_reentrant() {
        let guard = SAVE_LOCK.lock();
        assert!(SAVE_LOCK.try_lock().is_none());
        drop(guard);
    }

    #[test]
    fn test_shutdown_waits_for_in_flight_save_then_joins() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let gate = Gate::new();
        let debouncer = Arc::new(ResizeDebouncer::with_save_fn(
            Arc::clone(&cfg),
            Duration::from_millis(5),
            gate.save_fn(),
        ));

        debouncer.request_save();
        gate.wait_started();

        let d = Arc::clone(&debouncer);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            d.shutdown();
            let _ = tx.send(());
        });
        std::thread::sleep(Duration::from_millis(30));
        let returned_early = rx.try_recv().is_ok();
        let joined_early = debouncer.is_worker_joined();

        gate.release.store(true, Ordering::SeqCst);
        let finished = rx.recv_timeout(Duration::from_secs(10)).is_ok();

        assert!(!returned_early && !joined_early);
        assert!(finished);
        assert!(debouncer.is_worker_joined());
        assert!(!debouncer.is_in_flight());
        assert!(!debouncer.is_pending());
    }
}
