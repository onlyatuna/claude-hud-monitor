// src/logger.rs — Logging setup + open log directory
// Mirrors Python core/logger.py

use log::{LevelFilter, Metadata, Record, info};
use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

struct RotatingFileSink {
    file: Option<File>,
    current_size: u64,
    path: PathBuf,
    max_bytes: u64,
    backup_count: usize,
}

impl RotatingFileSink {
    fn open(path: PathBuf, max_bytes: u64, backup_count: usize) -> Self {
        let initial_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if initial_size > max_bytes {
            Self::rotate_files(&path, backup_count);
        }
        let current_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        Self {
            file,
            current_size,
            path,
            max_bytes,
            backup_count,
        }
    }

    fn backup_path(path: &Path, index: usize) -> PathBuf {
        let mut os = path.as_os_str().to_os_string();
        os.push(format!(".{index}"));
        PathBuf::from(os)
    }

    fn rotate_files(path: &Path, backup_count: usize) {
        if backup_count == 0 {
            let _ = std::fs::remove_file(path);
            return;
        }
        let oldest = Self::backup_path(path, backup_count);
        let _ = std::fs::remove_file(&oldest);
        for index in (1..backup_count).rev() {
            let src = Self::backup_path(path, index);
            let dst = Self::backup_path(path, index + 1);
            let _ = std::fs::rename(&src, &dst);
        }
        let _ = std::fs::rename(path, Self::backup_path(path, 1));
    }

    fn write_line(&mut self, bytes: &[u8]) {
        if self.file.is_none() {
            return;
        }
        if self.current_size + bytes.len() as u64 > self.max_bytes {
            // Drop existing open file before renaming (essential on Windows).
            self.file = None;
            Self::rotate_files(&self.path, self.backup_count);
            self.file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .ok();
            self.current_size = 0;
        }
        self.current_size += bytes.len() as u64;
        if let Some(ref mut f) = self.file {
            let _ = f.write_all(bytes);
            let _ = f.flush();
        }
    }

    fn flush(&mut self) {
        if let Some(ref mut f) = self.file {
            let _ = f.flush();
        }
    }
}

static DIAGNOSTICS_SINK: OnceLock<Mutex<RotatingFileSink>> = OnceLock::new();

fn is_core_target(target: &str) -> bool {
    const CORE_PREFIXES: &[&str] = &[
        "claude_hud_monitor::providers",
        "claude_hud_monitor::refresh_controller",
        "claude_hud_monitor::config",
        "claude_hud_monitor::accounts",
        "claude_hud_monitor::agy_statusline",
        "claude_hud_monitor::agy_rows",
        "claude_hud_monitor::autostart",
        "claude_hud_monitor::pace",
        "claude_hud_monitor::tray_quota",
    ];
    CORE_PREFIXES.iter().any(|prefix| {
        target == *prefix
            || target
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with("::"))
    })
}

/// Configure `diagnostics.log` beside `config_path` (256 KB x 2 backups, INFO), matching Python `core/diagnostics.py`.
pub fn configure_diagnostics(config_path: &Path) {
    const MAX_DIAG_SIZE: u64 = 256 * 1024;
    const DIAG_BACKUPS: usize = 2;
    let _ = DIAGNOSTICS_SINK.get_or_init(|| {
        let diag_path = config_path.with_file_name("diagnostics.log");
        Mutex::new(RotatingFileSink::open(
            diag_path,
            MAX_DIAG_SIZE,
            DIAG_BACKUPS,
        ))
    });
}

struct DualLogger {
    sink: Mutex<RotatingFileSink>,
}

impl log::Log for DualLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= LevelFilter::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let line = format!(
                "{} [{}] [{}] {}\n",
                timestamp,
                record.level(),
                record.target(),
                record.args()
            );

            // Stderr output (if console attached); INFO+ only, matching Python console handler.
            if record.level() <= log::Level::Info {
                let _ = std::io::stderr().write_all(line.as_bytes());
            }

            // File output with continuous 2MB rolling check (DEBUG+ for this crate, INFO+ for deps)
            if (record.level() <= log::Level::Info
                || record.target().starts_with("claude_hud_monitor"))
                && let Ok(mut sink) = self.sink.lock()
            {
                sink.write_line(line.as_bytes());
            }

            // Core diagnostics sink beside config.json (256 KB x 2 backups, INFO+)
            if record.level() <= log::Level::Info
                && is_core_target(record.target())
                && let Some(diag) = DIAGNOSTICS_SINK.get()
                && let Ok(mut sink) = diag.lock()
            {
                let diag_line = format!(
                    "{} {} {} {}\n",
                    timestamp,
                    record.target(),
                    record.level(),
                    record.args()
                );
                sink.write_line(diag_line.as_bytes());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut sink) = self.sink.lock() {
            sink.flush();
        }
        if let Some(diag) = DIAGNOSTICS_SINK.get()
            && let Ok(mut sink) = diag.lock()
        {
            sink.flush();
        }
    }
}

/// Initialize logging with physical file persistence in `log_dir()`.
pub fn setup_logging() {
    const MAX_LOG_SIZE: u64 = 2 * 1024 * 1024;
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);
    let log_path = dir.join("hud_monitor.log");

    let logger = DualLogger {
        sink: Mutex::new(RotatingFileSink::open(log_path, MAX_LOG_SIZE, 3)),
    };

    let _ = log::set_boxed_logger(Box::new(logger));
    log::set_max_level(LevelFilter::Debug);
    let _ = MAIN_THREAD.set(std::thread::current().id());
    std::panic::set_hook(Box::new(panic_hook));
}

static MAIN_THREAD: OnceLock<std::thread::ThreadId> = OnceLock::new();

thread_local! {
    /// Set while a caller that contains a panic itself logs its own record instead.
    static PANIC_LOG_SUPPRESSED: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` with the unhandled-panic record suppressed on this thread; the caller logs instead.
pub fn with_panic_log_suppressed<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            PANIC_LOG_SUPPRESSED.with(|flag| flag.set(self.0));
        }
    }
    let _restore = Restore(PANIC_LOG_SUPPRESSED.with(|flag| flag.replace(true)));
    f()
}

/// Python `core/logger.py` header: main thread vs. named background thread.
pub fn format_unhandled(
    is_main: bool,
    thread_name: Option<&str>,
    payload: &str,
    location: &str,
) -> String {
    let header = if is_main {
        "Unhandled exception in main thread:".to_string()
    } else {
        format!(
            "Unhandled exception in background thread '{}':",
            thread_name.unwrap_or("unknown")
        )
    };
    format!("{header} {payload} ({location})")
}

/// Logs an unhandled panic and returns, so the process keeps running (Python `sys.excepthook`
/// / `threading.excepthook` behaviour). Must not panic itself: a panic here would abort.
fn panic_hook(info: &std::panic::PanicHookInfo<'_>) {
    // Contained panics count too: a recurring one is what the restart policy is for.
    let restart = crate::panic_guard::note_panic();
    if !PANIC_LOG_SUPPRESSED.with(|flag| flag.get()) {
        log_panic(info);
    }
    if restart {
        log::error!(
            "[PanicGuard] {} panics within {} min; restarting the process",
            crate::panic_guard::PANIC_LIMIT,
            crate::panic_guard::PANIC_WINDOW.as_secs() / 60
        );
        log::logger().flush();
        crate::panic_guard::restart_process();
    }
}

fn log_panic(info: &std::panic::PanicHookInfo<'_>) {
    let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    };
    let location = info
        .location()
        .map(|l| format!("{}:{}", l.file(), l.line()))
        .unwrap_or_else(|| "unknown location".to_string());
    let current = std::thread::current();
    let is_main = MAIN_THREAD.get() == Some(&current.id());
    log::error!(
        "{}",
        format_unhandled(is_main, current.name(), &payload, &location)
    );
}

/// Return the log directory path (`exe_dir/logs` when a writable portable `config.json` sits
/// beside the executable, matching Python `logger.py`).
pub fn log_dir() -> PathBuf {
    #[cfg(not(test))]
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        let portable_cfg = exe_dir.join("config.json");
        if std::fs::metadata(&portable_cfg)
            .ok()
            .is_some_and(|m| !m.permissions().readonly())
        {
            return exe_dir.join("logs");
        }
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_owned());
        PathBuf::from(appdata).join("ClaudeHUDMonitor").join("logs")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
        PathBuf::from(home)
            .join("Library")
            .join("Logs")
            .join("ClaudeHUDMonitor")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("ClaudeHUDMonitor")
            .join("logs")
    }
}

/// Open the log directory in the OS file manager.
pub fn open_log_dir() {
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);
    info!("[Logger] Opening log dir: {:?}", dir);
    let _ = ferrokit_platform::desktop_services::open_file(&dir);
}

#[cfg(test)]
mod tests {
    use super::{RotatingFileSink, format_unhandled, is_core_target};

    #[test]
    fn main_thread_header_matches_python_record() {
        assert_eq!(
            format_unhandled(true, Some("main"), "boom", "a.rs:1"),
            "Unhandled exception in main thread: boom (a.rs:1)"
        );
    }

    #[test]
    fn background_header_uses_thread_name_or_unknown() {
        assert_eq!(
            format_unhandled(false, Some("quota-claude"), "boom", "a.rs:1"),
            "Unhandled exception in background thread 'quota-claude': boom (a.rs:1)"
        );
        assert_eq!(
            format_unhandled(false, None, "boom", "a.rs:1"),
            "Unhandled exception in background thread 'unknown': boom (a.rs:1)"
        );
    }

    #[test]
    fn core_targets_route_to_diagnostics_and_ui_targets_do_not() {
        assert!(is_core_target("claude_hud_monitor::providers::claude"));
        assert!(is_core_target("claude_hud_monitor::refresh_controller"));
        assert!(is_core_target("claude_hud_monitor::config"));
        assert!(!is_core_target("claude_hud_monitor::ui::hud_window"));
        assert!(!is_core_target("claude_hud_monitor"));
    }

    #[test]
    fn rotating_sink_keeps_requested_backup_count() {
        let dir = std::env::temp_dir().join(format!(
            "hud-logger-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("diagnostics.log");
        let mut sink = RotatingFileSink::open(base.clone(), 16, 2);
        sink.write_line(b"0123456789\n");
        sink.write_line(b"abcdefghij\n");
        sink.write_line(b"klmnopqrst\n");
        drop(sink);

        assert!(base.exists());
        assert!(dir.join("diagnostics.log.1").exists());
        assert!(dir.join("diagnostics.log.2").exists());
        assert!(!dir.join("diagnostics.log.3").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
