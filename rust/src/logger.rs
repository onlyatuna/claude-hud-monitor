// src/logger.rs — Logging setup + open log directory
// Mirrors Python core/logger.py

use log::{info, LevelFilter, Metadata, Record};
use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

struct DualLogger {
    file: Mutex<Option<File>>,
    current_size: Mutex<u64>,
    log_path: PathBuf,
    backup_path: PathBuf,
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

            // Stderr output (if console attached); a failed write must not panic inside the hook.
            let _ = std::io::stderr().write_all(line.as_bytes());

            // File output with continuous 2MB rolling check
            if let Ok(mut lock) = self.file.lock() {
                let bytes = line.as_bytes();
                if let Ok(mut size) = self.current_size.lock() {
                    const MAX_LOG_SIZE: u64 = 2 * 1024 * 1024;
                    if *size + bytes.len() as u64 > MAX_LOG_SIZE {
                        // Drop existing open file before renaming (essential on Windows)
                        *lock = None;
                        let _ = std::fs::remove_file(&self.backup_path);
                        let _ = std::fs::rename(&self.log_path, &self.backup_path);
                        *lock = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&self.log_path)
                            .ok();
                        *size = 0;
                    }
                    *size += bytes.len() as u64;
                }
                if let Some(ref mut f) = *lock {
                    let _ = f.write_all(bytes);
                    let _ = f.flush();
                }
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut lock) = self.file.lock() {
            if let Some(ref mut f) = *lock {
                let _ = f.flush();
            }
        }
    }
}

/// Initialize logging with physical file persistence in `log_dir()`.
pub fn setup_logging() {
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);
    let log_path = dir.join("hud_monitor.log");
    let backup_path = dir.join("hud_monitor.log.1");

    // 2MB log rotation at startup
    let initial_size = std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
    if initial_size > 2 * 1024 * 1024 {
        let _ = std::fs::remove_file(&backup_path);
        let _ = std::fs::rename(&log_path, &backup_path);
    }
    let current_size = std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();

    let logger = DualLogger {
        file: Mutex::new(file),
        current_size: Mutex::new(current_size),
        log_path,
        backup_path,
    };

    let _ = log::set_boxed_logger(Box::new(logger));
    log::set_max_level(LevelFilter::Info);
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

/// Return the log directory path.
pub fn log_dir() -> PathBuf {
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
    use super::format_unhandled;

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
}
