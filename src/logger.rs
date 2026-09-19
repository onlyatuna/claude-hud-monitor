// src/logger.rs — Logging setup + open log directory
// Mirrors Python core/logger.py

use log::{info, LevelFilter, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

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

            // Stderr output (if console attached)
            eprint!("{}", line);

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
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
}
