//! Install the agy status line hook and read its quota-only cache (port of
//! `python/core/agy_statusline.py`).
//!
//! agy runs the hook once per status line refresh and pipes its state to it; the hook keeps only the
//! quota buckets and writes them to `agy-quota.json` in the HUD's config folder. The HUD reads that
//! file to fill the rows the quota endpoints do not report (`GEMINI 7D`, `ARGON 7D`). The hook is the
//! same Python script the Python HUD installs, so both builds share one hook and one cache.

use crate::config::ConfigManager;
use chrono::{DateTime, Local, Utc};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const HOOK_FILENAME: &str = "agy_statusline_hook.py";
pub const QUOTA_CACHE_FILENAME: &str = "agy-quota.json";

/// The hook script, byte for byte the one in `python/core/agy_statusline.py`.
pub const HOOK_SOURCE: &str = r#"import json
import os
import sys
import tempfile
import time
from datetime import datetime, timezone

def main():
    temporary = None
    try:
        directory = os.path.dirname(os.path.abspath(__file__))
        target = os.path.join(directory, "agy-quota.json")
        if os.path.exists(target) and time.time() - os.path.getmtime(target) < 2:
            return
        data = json.loads(sys.stdin.read())
        if not isinstance(data, dict) or not isinstance(data.get("quota"), dict):
            return
        quota = {}
        for key, bucket in data["quota"].items():
            if not isinstance(bucket, dict):
                continue
            fraction = bucket.get("remaining_fraction")
            if isinstance(fraction, bool) or not isinstance(fraction, (int, float)) or not 0 <= fraction <= 1:
                continue
            clean = {"remaining_fraction": fraction}
            if isinstance(bucket.get("reset_time"), str):
                clean["reset_time"] = bucket["reset_time"]
            quota[key] = clean
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=directory, delete=False) as stream:
            temporary = stream.name
            json.dump({"written_at": datetime.now(timezone.utc).isoformat(), "quota": quota}, stream)
        os.replace(temporary, target)
        temporary = None
    except Exception:
        pass
    finally:
        if temporary is not None:
            try:
                os.unlink(temporary)
            except Exception:
                pass

main()
print(" ")
"#;

/// One bucket of the cache: the fraction still available and when it resets (`None` when the hook
/// wrote no usable time).
#[derive(Debug, Clone, PartialEq)]
pub struct CacheBucket {
    pub remaining_fraction: f64,
    pub reset_time: Option<DateTime<Utc>>,
}

/// What `read_quota_cache` returns (Python's `{'written_at', 'buckets'}`).
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaCache {
    pub written_at: DateTime<Utc>,
    pub buckets: HashMap<String, CacheBucket>,
}

/// `~/.gemini/antigravity-cli/settings.json`.
pub fn agy_settings_path() -> PathBuf {
    home_dir()
        .join(".gemini")
        .join("antigravity-cli")
        .join("settings.json")
}

/// The user's home folder (`os.path.expanduser('~')`).
pub fn home_dir() -> PathBuf {
    // Python's `expanduser` reads `USERPROFILE` and ignores `HOME` on Windows.
    let (first, second) = if cfg!(windows) {
        ("USERPROFILE", "HOME")
    } else {
        ("HOME", "USERPROFILE")
    };
    std::env::var_os(first)
        .or_else(|| std::env::var_os(second))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg_attr(test, allow(dead_code))]
pub fn quota_cache_path() -> PathBuf {
    ConfigManager::config_dir().join(QUOTA_CACHE_FILENAME)
}

/// Python `_utc`: an ISO time with an offset, as UTC; `None` for anything else.
pub fn parse_utc(value: &Value) -> Option<DateTime<Utc>> {
    let text = value.as_str()?;
    let parsed = DateTime::parse_from_rfc3339(text)
        .or_else(|_| DateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f%:z"))
        .ok()?;
    Some(parsed.with_timezone(&Utc))
}

/// `read_quota_cache()` against the default file and the current time.
#[cfg_attr(test, allow(dead_code))]
pub fn read_default_quota_cache() -> Option<QuotaCache> {
    read_quota_cache(&quota_cache_path(), Utc::now())
}

/// Python `read_quota_cache`: the buckets of the cache file that hold a fraction in 0..=1 and have
/// not reset yet. `None` for a missing or unreadable file, a missing `written_at`, or no usable bucket.
pub fn read_quota_cache(path: &Path, now: DateTime<Utc>) -> Option<QuotaCache> {
    let text = fs::read_to_string(path).ok()?;
    let data: Value = serde_json::from_str(&text).ok()?;
    let quota = data.get("quota")?.as_object()?;
    let written_at = parse_utc(data.get("written_at")?)?;
    let mut buckets = HashMap::new();
    for (key, bucket) in quota {
        let Some(bucket) = bucket.as_object() else {
            continue;
        };
        // A JSON number only: `true` and `"0.5"` are not fractions.
        let Some(fraction) = bucket.get("remaining_fraction").and_then(Value::as_f64) else {
            continue;
        };
        if !(0.0..=1.0).contains(&fraction) {
            continue;
        }
        let reset = bucket.get("reset_time").and_then(parse_utc);
        if reset.is_some_and(|reset| reset <= now) {
            continue;
        }
        buckets.insert(
            key.clone(),
            CacheBucket {
                remaining_fraction: fraction,
                reset_time: reset,
            },
        );
    }
    (!buckets.is_empty()).then_some(QuotaCache {
        written_at,
        buckets,
    })
}

fn load_settings(path: &Path) -> Result<Map<String, Value>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    match serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())? {
        Value::Object(map) => Ok(map),
        _ => Err("Settings must be an object".to_owned()),
    }
}

fn owned(statusline: &Value) -> bool {
    statusline
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains(HOOK_FILENAME))
}

/// Copies `path` to `<path>.bak-hud-<stamp>`; an exclusive create keeps the backups made within the
/// same second.
fn backup(path: &Path) -> std::io::Result<()> {
    // Opened first: a missing source must not leave an empty backup behind.
    let mut source = fs::File::open(path)?;
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let base = format!("{}.bak-hud-{stamp}", path.display());
    let mut candidate = base.clone();
    let mut suffix = 0;
    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut target) => {
                std::io::copy(&mut source, &mut target)?;
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                suffix += 1;
                candidate = format!("{base}-{suffix}");
            }
            Err(e) => return Err(e),
        }
    }
}

/// Writes `settings` next to `path` and moves it over `path`, so a reader never sees half a file.
fn write_settings(path: &Path, settings: &Map<String, Value>) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = dir.join(format!(".settings-{}.tmp", std::process::id()));
    let text = serde_json::to_string_pretty(settings)?;
    let result = (|| {
        let mut stream = fs::File::create(&temporary)?;
        stream.write_all(text.as_bytes())?;
        stream.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// When the hook last wrote the quota cache (UTC), or `None` if there is no readable cache yet
/// (Python `cache_written_at`).
pub fn cache_written_at(path: Option<&Path>) -> Option<DateTime<Utc>> {
    let target = path.map_or_else(quota_cache_path, Path::to_path_buf);
    let text = fs::read_to_string(&target).ok()?;
    let data: Value = serde_json::from_str(&text).ok()?;
    parse_utc(data.get("written_at")?)
}

/// Locates `name` on `PATH` (`shutil.which`).
fn which_executable(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    #[cfg(windows)]
    let exts: Vec<String> = std::env::var_os("PATHEXT")
        .map(|v| {
            v.to_string_lossy()
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_owned())
                .collect()
        })
        .unwrap_or_else(|| vec![".COM".into(), ".EXE".into(), ".BAT".into(), ".CMD".into()]);
    #[cfg(windows)]
    let has_pathext = exts.iter().any(|ext| {
        name.len() >= ext.len() && name[name.len() - ext.len()..].eq_ignore_ascii_case(ext)
    });
    for dir in std::env::split_paths(&path_var) {
        #[cfg(windows)]
        {
            if has_pathext {
                let direct = dir.join(name);
                if direct.is_file() {
                    return Some(direct);
                }
            } else {
                for ext in &exts {
                    let candidate = dir.join(format!("{name}{ext}"));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let direct = dir.join(name);
            if let Ok(meta) = fs::metadata(&direct)
                && meta.is_file()
                && (meta.permissions().mode() & 0o111) != 0
            {
                return Some(direct);
            }
        }
    }
    None
}

/// Runs `[executable, "-c", "import sys; sys.exit(0 if sys.version_info >= (3, 6) else 1)"]` with a
/// 10s deadline; `true` iff it exits with 0.
fn verify_python_executable(executable: &Path) -> bool {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let mut cmd = Command::new(executable);
    cmd.args([
        "-c",
        "import sys; sys.exit(0 if sys.version_info >= (3, 6) else 1)",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return false,
        }
    }
}

/// Python `find_python`: the command agy will run the hook with (`"python"` on Windows, `"python3"`
/// elsewhere), if it really runs Python >= 3.6.
pub fn find_python(platform: Option<&str>) -> Option<&'static str> {
    find_python_with(platform, which_executable, verify_python_executable)
}

fn find_python_with(
    platform: Option<&str>,
    which_fn: impl FnOnce(&str) -> Option<PathBuf>,
    verify_fn: impl FnOnce(&Path) -> bool,
) -> Option<&'static str> {
    let default_plat = if cfg!(windows) { "win32" } else { "linux" };
    let name = if platform.unwrap_or(default_plat) == "win32" {
        "python"
    } else {
        "python3"
    };
    let executable = which_fn(name)?;
    verify_fn(&executable).then_some(name)
}

fn needs_quoting(text: &str) -> bool {
    text.chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\'')
}

/// Windows 8.3 name of an existing directory (`GetShortPathNameW`), or `None`.
#[cfg(windows)]
fn win32_short_path(path: &Path) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetShortPathNameW(
            lpsz_long_path: *const u16,
            lpsz_short_path: *mut u16,
            cch_buffer: u32,
        ) -> u32;
    }

    let abs = absolute(path);
    let wide: Vec<u16> = abs
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut buffer = [0u16; 1024];
    // SAFETY: `wide` is null-terminated UTF-16 and `buffer` has room for `buffer.len()` u16s.
    let len = unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) }
        as usize;
    if len > 0 && len < buffer.len() {
        Some(PathBuf::from(OsString::from_wide(&buffer[..len])))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn win32_short_path(_path: &Path) -> Option<PathBuf> {
    None
}

/// Python `_command_hook_path`: the hook path as it can appear in the command (agy's parsing of
/// quotes is not known, so none is used). A path with spaces on Windows uses its 8.3 directory name
/// when the volume has one; otherwise returns `None`.
fn command_hook_path_with(
    hook_path: &Path,
    is_win32: bool,
    short_path_fn: impl FnOnce(&Path) -> Option<PathBuf>,
) -> Option<String> {
    let text = hook_path.to_string_lossy().into_owned();
    if !needs_quoting(&text) {
        return Some(text);
    }
    if !is_win32 {
        return None;
    }
    let directory = hook_path.parent()?;
    let _ = fs::create_dir_all(directory); // a short name only exists for a directory that exists
    let short_dir = short_path_fn(directory)?;
    let candidate = short_dir.join(hook_path.file_name()?);
    let candidate_text = candidate.to_string_lossy().into_owned();
    (!needs_quoting(&candidate_text)).then_some(candidate_text)
}

fn hook_current(hook_path: &Path) -> bool {
    fs::read_to_string(hook_path).is_ok_and(|content| content == HOOK_SOURCE)
}

fn normcase_abspath(path: &Path) -> String {
    let abs = absolute(path).to_string_lossy().into_owned();
    if cfg!(windows) {
        abs.replace('/', "\\").to_lowercase()
    } else {
        abs
    }
}

fn points_at(statusline: &Value, command_path: Option<&str>) -> bool {
    let Some(command_path) = command_path.filter(|s| !s.is_empty()) else {
        return false;
    };
    if statusline.get("type").and_then(Value::as_str) != Some("command")
        || statusline.get("enabled").and_then(Value::as_bool) != Some(true)
    {
        return false;
    }
    let Some(command) = statusline.get("command").and_then(Value::as_str) else {
        return false;
    };
    let Some(last) = command.split_whitespace().last() else {
        return false;
    };
    normcase_abspath(Path::new(last)) == normcase_abspath(Path::new(command_path))
}

/// Python `statusline_status` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Installed,
    NeedsUpdate,
    NotInstalled,
    Conflict,
    AgyMissing,
    Error,
}

impl Status {
    /// Python `STATUS_LABELS`.
    pub fn label(self) -> &'static str {
        match self {
            Status::Installed => "狀態：已啟用",
            Status::NeedsUpdate => "狀態：需要更新",
            Status::NotInstalled => "狀態：未啟用",
            Status::Conflict => "狀態：agy 已使用其他 statusLine（不會覆蓋）",
            Status::AgyMissing => "狀態：找不到 agy 設定檔",
            Status::Error => "狀態：無法讀取 agy 設定",
        }
    }
}

/// Python `statusline_status`: `'installed'`, `'needs_update'`, `'not_installed'`, `'conflict'`,
/// `'agy_missing'` or `'error'`. Changes nothing.
pub fn statusline_status(settings_path: Option<&Path>, config_dir: Option<&Path>) -> Status {
    statusline_status_with(settings_path, config_dir, cfg!(windows), win32_short_path)
}

fn statusline_status_with(
    settings_path: Option<&Path>,
    config_dir: Option<&Path>,
    is_win32: bool,
    short_path_fn: impl FnOnce(&Path) -> Option<PathBuf>,
) -> Status {
    let settings_path = settings_path.map_or_else(agy_settings_path, Path::to_path_buf);
    if !settings_path.is_file() {
        return Status::AgyMissing;
    }
    let Ok(settings) = load_settings(&settings_path) else {
        return Status::Error;
    };
    let Some(statusline) = settings.get("statusLine") else {
        return Status::NotInstalled;
    };
    if !owned(statusline) {
        return Status::Conflict;
    }
    let config_dir = config_dir.map_or_else(ConfigManager::config_dir, Path::to_path_buf);
    let hook_path = absolute(&config_dir.join(HOOK_FILENAME));
    let cmd_path = command_hook_path_with(&hook_path, is_win32, short_path_fn);
    if !hook_current(&hook_path) || !points_at(statusline, cmd_path.as_deref()) {
        return Status::NeedsUpdate;
    }
    Status::Installed
}

/// What `install_statusline` and `uninstall_statusline` report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Installed,
    Updated,
    Already,
    Conflict,
    UnsupportedPath,
    AgyMissing,
    PythonMissing,
    Error,
    Removed,
    NotInstalled,
}

impl Outcome {
    /// The one line `cli` and the tray notice print (Python `RESULT_MESSAGES`).
    pub fn message(self) -> &'static str {
        match self {
            Outcome::Installed => "已安裝 agy statusLine（請重開 agy 互動模式）",
            Outcome::Updated => "已更新 agy statusLine 掛鉤（請重開 agy 互動模式）",
            Outcome::Already => "已安裝，無需變更",
            Outcome::Conflict => "agy 已設定其他 statusLine，未變更",
            Outcome::UnsupportedPath => "設定資料夾路徑含空白，無法安裝",
            Outcome::AgyMissing => "找不到 agy 設定檔，請先安裝並執行過 agy",
            Outcome::PythonMissing => {
                "找不到可用的 python（需要 PATH 上有 python 或 python3），未變更"
            }
            Outcome::Error => "無法讀寫 agy 設定",
            Outcome::Removed => "已移除 agy statusLine",
            Outcome::NotInstalled => "尚未安裝",
        }
    }

    /// 0 for `SUCCESS_RESULTS` (`installed`, `updated`, `already`, `removed`, `not_installed`), 1 for a failure.
    pub fn exit_code(self) -> i32 {
        match self {
            Outcome::Installed
            | Outcome::Updated
            | Outcome::Already
            | Outcome::Removed
            | Outcome::NotInstalled => 0,
            Outcome::Conflict
            | Outcome::UnsupportedPath
            | Outcome::AgyMissing
            | Outcome::PythonMissing
            | Outcome::Error => 1,
        }
    }
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Python `install_statusline`. `None` arguments take the defaults: agy's settings file, the HUD's
/// config folder, and `find_python(None)` as the interpreter.
pub fn install_statusline(
    settings_path: Option<&Path>,
    config_dir: Option<&Path>,
    python_cmd: Option<&str>,
) -> Outcome {
    install_statusline_with(
        settings_path,
        config_dir,
        python_cmd,
        cfg!(windows),
        win32_short_path,
        || find_python(None),
    )
}

fn install_statusline_with(
    settings_path: Option<&Path>,
    config_dir: Option<&Path>,
    python_cmd: Option<&str>,
    is_win32: bool,
    short_path_fn: impl FnOnce(&Path) -> Option<PathBuf>,
    find_python_fn: impl FnOnce() -> Option<&'static str>,
) -> Outcome {
    let config_dir = config_dir.map_or_else(ConfigManager::config_dir, Path::to_path_buf);
    let hook_path = absolute(&config_dir.join(HOOK_FILENAME));
    if python_cmd.is_some_and(|cmd| cmd.chars().any(|c| c == '"' || c == '\'')) {
        return Outcome::UnsupportedPath;
    }
    let Some(command_path) = command_hook_path_with(&hook_path, is_win32, short_path_fn) else {
        return Outcome::UnsupportedPath;
    };
    let settings_path = settings_path.map_or_else(agy_settings_path, Path::to_path_buf);
    if !settings_path.is_file() {
        return Outcome::AgyMissing;
    }
    let Ok(mut settings) = load_settings(&settings_path) else {
        return Outcome::Error;
    };
    let statusline = settings.get("statusLine");
    if let Some(current) = statusline
        && !owned(current)
    {
        return Outcome::Conflict;
    }
    let ours_before = statusline.is_some();
    let settings_ok = statusline.is_some_and(|s| points_at(s, Some(&command_path)));
    let hook_ok = hook_current(&hook_path);
    if settings_ok && hook_ok {
        return Outcome::Already;
    }
    let resolved_python = if !settings_ok {
        match python_cmd {
            Some(cmd) => Some(cmd.to_owned()),
            None => find_python_fn().map(str::to_owned),
        }
    } else {
        None
    };
    if !settings_ok && resolved_python.is_none() {
        return Outcome::PythonMissing;
    }
    let written = (|| -> std::io::Result<()> {
        if !settings_ok {
            backup(&settings_path)?;
        }
        fs::create_dir_all(&config_dir)?;
        fs::write(&hook_path, HOOK_SOURCE)?;
        if let Some(py) = resolved_python {
            // A stale hook file alone leaves the settings untouched.
            settings.insert(
                "statusLine".to_owned(),
                json!({
                    "type": "command",
                    "command": format!("{py} {command_path}"),
                    "enabled": true,
                }),
            );
            write_settings(&settings_path, &settings)?;
        }
        Ok(())
    })();
    match written {
        Ok(()) => {
            if ours_before {
                Outcome::Updated
            } else {
                Outcome::Installed
            }
        }
        Err(_) => Outcome::Error,
    }
}

/// Python `uninstall_statusline`: removes our `statusLine` entry; the hook file stays.
pub fn uninstall_statusline(settings_path: Option<&Path>) -> Outcome {
    let settings_path = settings_path.map_or_else(agy_settings_path, Path::to_path_buf);
    if !settings_path.is_file() {
        return Outcome::NotInstalled;
    }
    let Ok(mut settings) = load_settings(&settings_path) else {
        return Outcome::Error;
    };
    match settings.get("statusLine") {
        None => return Outcome::NotInstalled,
        Some(current) if !owned(current) => return Outcome::Conflict,
        Some(_) => {}
    }
    let written = (|| -> std::io::Result<()> {
        backup(&settings_path)?;
        // `shift_remove` keeps the order of the user's other keys (`remove` swaps the last one in).
        settings.shift_remove("statusLine");
        write_settings(&settings_path, &settings)
    })();
    if written.is_ok() {
        Outcome::Removed
    } else {
        Outcome::Error
    }
}

/// Python `cli`: runs `--install-agy-statusline` or `--uninstall-agy-statusline`, prints one line
/// and returns the exit code; `None` when `args` ask for neither.
pub fn cli(args: &[String]) -> Option<i32> {
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let outcome = if has("--install-agy-statusline") {
        install_statusline(None, None, None)
    } else if has("--uninstall-agy-statusline") {
        uninstall_statusline(None)
    } else {
        return None;
    };
    println!("{}", outcome.message());
    Some(outcome.exit_code())
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!("hud_agy_{tag}_{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Dir(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 16, 0, 0, 0).unwrap()
    }

    fn write_cache(dir: &Dir, quota: Value) -> PathBuf {
        let path = dir.path().join(QUOTA_CACHE_FILENAME);
        let body = json!({"written_at": "2026-10-16T00:00:00Z", "quota": quota});
        fs::write(&path, body.to_string()).unwrap();
        path
    }

    #[test]
    fn reader_returns_the_five_buckets_as_floats() {
        let dir = Dir::new("five");
        let names = [
            "3p-5h",
            "3p-weekly",
            "gemini-5h",
            "gemini-argon-wtu-weekly",
            "gemini-weekly",
        ];
        let values = [
            json!(1),
            json!(0.8469924),
            json!(1),
            json!(0.78630334),
            json!(0.9914498),
        ];
        let mut quota = Map::new();
        for (name, value) in names.iter().zip(&values) {
            quota.insert(
                (*name).to_owned(),
                json!({"remaining_fraction": value, "reset_time": "2026-10-23T08:42:04Z", "reset_in_seconds": 583948}),
            );
        }
        let path = write_cache(&dir, Value::Object(quota));
        let cache = read_quota_cache(&path, now()).expect("cache");
        assert_eq!(cache.written_at, now());
        assert_eq!(cache.buckets.len(), 5);
        assert_eq!(cache.buckets["3p-5h"].remaining_fraction, 1.0);
        assert_eq!(cache.buckets["gemini-weekly"].remaining_fraction, 0.9914498);
    }

    #[test]
    fn reader_drops_bad_fractions_and_expired_buckets_and_ignores_a_broken_reset() {
        let dir = Dir::new("filter");
        let quota = json!({
            "1.5": {"remaining_fraction": 1.5},
            "-0.1": {"remaining_fraction": -0.1},
            "true": {"remaining_fraction": true},
            "0.5": {"remaining_fraction": "0.5"},
            "expired": {"remaining_fraction": 1, "reset_time": "2026-10-16T00:00:00+00:00"},
            "valid": {"remaining_fraction": 0.5, "reset_time": "broken"},
        });
        let path = write_cache(&dir, quota);
        let cache = read_quota_cache(&path, now()).expect("cache");
        assert_eq!(
            cache.buckets,
            HashMap::from([(
                "valid".to_owned(),
                CacheBucket {
                    remaining_fraction: 0.5,
                    reset_time: None
                }
            )])
        );
    }

    #[test]
    fn reader_returns_none_for_a_missing_or_unusable_file() {
        let dir = Dir::new("bad");
        let path = dir.path().join(QUOTA_CACHE_FILENAME);
        assert_eq!(read_quota_cache(&path, now()), None);
        for content in [
            "{",
            "[]",
            "{}",
            r#"{"quota":{}}"#,
            r#"{"quota":{"x":null}}"#,
        ] {
            fs::write(&path, content).unwrap();
            assert_eq!(read_quota_cache(&path, now()), None, "{content}");
        }
    }

    fn install(dir: &Dir, settings: &Path) -> Outcome {
        install_statusline(
            Some(settings),
            Some(&dir.path().join("config")),
            Some("python"),
        )
    }

    #[test]
    fn install_then_uninstall_round_trips_and_keeps_a_backup() {
        let dir = Dir::new("install");
        let settings = dir.path().join("settings.json");
        let original = r#"{"model":"x"}"#;
        fs::write(&settings, original).unwrap();
        assert_eq!(install(&dir, &settings), Outcome::Installed);
        let data: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        let hook = absolute(&dir.path().join("config").join(HOOK_FILENAME));
        assert_eq!(
            data,
            json!({"model": "x", "statusLine": {
                "type": "command", "command": format!("python {}", hook.display()), "enabled": true}})
        );
        assert!(
            !data["statusLine"]["command"]
                .as_str()
                .unwrap()
                .contains('"')
        );
        assert_eq!(fs::read_to_string(&hook).unwrap(), HOOK_SOURCE);
        let backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("settings.json.bak-hud-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), original);
        assert_eq!(install(&dir, &settings), Outcome::Already);
        assert_eq!(uninstall_statusline(Some(&settings)), Outcome::Removed);
        let data: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(data, json!({"model": "x"}));
        assert!(hook.exists(), "the hook file stays");
        assert_eq!(uninstall_statusline(Some(&settings)), Outcome::NotInstalled);
    }

    #[test]
    fn a_status_line_that_is_not_ours_is_left_alone() {
        let dir = Dir::new("conflict");
        let settings = dir.path().join("settings.json");
        for value in [json!({"command": "other"}), Value::Null, json!("other")] {
            let raw = json!({"statusLine": value}).to_string();
            fs::write(&settings, &raw).unwrap();
            assert_eq!(install(&dir, &settings), Outcome::Conflict);
            assert_eq!(uninstall_statusline(Some(&settings)), Outcome::Conflict);
            assert_eq!(fs::read_to_string(&settings).unwrap(), raw);
        }
        assert!(!dir.path().join("config").exists());
    }

    #[test]
    fn unsupported_paths_and_unreadable_settings_change_nothing() {
        let dir = Dir::new("unsupported");
        let settings = dir.path().join("settings.json");
        for name in ["has space", "has\"quote", "has'quote"] {
            let folder = dir.path().join(name);
            assert_eq!(
                install_statusline_with(
                    Some(&settings),
                    Some(&folder),
                    Some("python"),
                    false,
                    |_| None,
                    || Some("python"),
                ),
                Outcome::UnsupportedPath
            );
            assert!(!folder.exists());
        }
        assert_eq!(
            install(&dir, &settings),
            Outcome::AgyMissing,
            "no settings file"
        );
        for raw in ["[]", "{"] {
            fs::write(&settings, raw).unwrap();
            assert_eq!(install(&dir, &settings), Outcome::Error);
            assert_eq!(fs::read_to_string(&settings).unwrap(), raw);
        }
    }

    #[test]
    fn install_keeps_the_order_of_the_users_keys() {
        let dir = Dir::new("order");
        let settings = dir.path().join("settings.json");
        fs::write(&settings, r#"{"zeta":1,"alpha":2}"#).unwrap();
        assert_eq!(install(&dir, &settings), Outcome::Installed);
        let text = fs::read_to_string(&settings).unwrap();
        let (zeta, alpha, status) = (
            text.find("zeta").unwrap(),
            text.find("alpha").unwrap(),
            text.find("statusLine").unwrap(),
        );
        assert!(zeta < alpha && alpha < status, "{text}");
    }

    #[test]
    fn uninstall_keeps_the_order_of_the_users_keys() {
        let dir = Dir::new("uninstall_order");
        let settings = dir.path().join("settings.json");
        fs::write(&settings, r#"{"zeta":1,"alpha":2,"mid":3}"#).unwrap();
        assert_eq!(install(&dir, &settings), Outcome::Installed);
        // Put our entry first so a swap-style removal would move `mid` into its place.
        let mut data: Map<String, Value> =
            serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        let status = data.shift_remove("statusLine").unwrap();
        data.shift_insert(0, "statusLine".to_owned(), status);
        fs::write(&settings, serde_json::to_string(&data).unwrap()).unwrap();
        assert_eq!(uninstall_statusline(Some(&settings)), Outcome::Removed);
        let text = fs::read_to_string(&settings).unwrap();
        let (zeta, alpha, mid) = (
            text.find("zeta").unwrap(),
            text.find("alpha").unwrap(),
            text.find("mid").unwrap(),
        );
        assert!(zeta < alpha && alpha < mid, "{text}");
    }

    #[test]
    fn cli_exit_codes_follow_the_outcomes() {
        assert_eq!(cli(&[]), None);
        assert_eq!(cli(&["--other".to_owned()]), None);
        for (outcome, code) in [
            (Outcome::Installed, 0),
            (Outcome::Updated, 0),
            (Outcome::Already, 0),
            (Outcome::Removed, 0),
            (Outcome::NotInstalled, 0),
            (Outcome::Conflict, 1),
            (Outcome::UnsupportedPath, 1),
            (Outcome::AgyMissing, 1),
            (Outcome::PythonMissing, 1),
            (Outcome::Error, 1),
        ] {
            assert_eq!(outcome.exit_code(), code);
            assert!(!outcome.message().is_empty());
            assert!(!outcome.message().contains('\n'));
        }
        for status in [
            Status::Installed,
            Status::NeedsUpdate,
            Status::NotInstalled,
            Status::Conflict,
            Status::AgyMissing,
            Status::Error,
        ] {
            assert!(!status.label().is_empty());
        }
    }

    #[test]
    fn statusline_status_covers_all_states() {
        let dir = Dir::new("status_states");
        let settings = dir.path().join("settings.json");
        let config = dir.path().join("config");
        let hook = config.join(HOOK_FILENAME);

        assert_eq!(
            statusline_status(Some(&settings), Some(&config)),
            Status::AgyMissing
        );
        fs::write(&settings, r#"{"model":"x"}"#).unwrap();
        assert_eq!(
            statusline_status(Some(&settings), Some(&config)),
            Status::NotInstalled
        );
        assert_eq!(install(&dir, &settings), Outcome::Installed);
        assert_eq!(
            statusline_status(Some(&settings), Some(&config)),
            Status::Installed
        );
        fs::write(&hook, "# stale").unwrap();
        assert_eq!(
            statusline_status(Some(&settings), Some(&config)),
            Status::NeedsUpdate
        );
        fs::remove_file(&hook).unwrap();
        assert_eq!(
            statusline_status(Some(&settings), Some(&config)),
            Status::NeedsUpdate
        );
        fs::write(
            &settings,
            r#"{"statusLine":{"type":"command","command":"other"}}"#,
        )
        .unwrap();
        assert_eq!(
            statusline_status(Some(&settings), Some(&config)),
            Status::Conflict
        );
        for raw in ["{", "[]"] {
            fs::write(&settings, raw).unwrap();
            assert_eq!(
                statusline_status(Some(&settings), Some(&config)),
                Status::Error
            );
        }
    }

    #[test]
    fn stale_or_missing_hook_is_updated_without_touching_settings_or_making_a_backup() {
        let dir = Dir::new("stale_hook");
        let settings = dir.path().join("settings.json");
        let config = dir.path().join("config");
        let hook = config.join(HOOK_FILENAME);
        fs::write(&settings, "{}").unwrap();
        assert_eq!(install(&dir, &settings), Outcome::Installed);
        assert_eq!(install(&dir, &settings), Outcome::Already);

        fs::write(&hook, "# stale").unwrap();
        let before = fs::read_to_string(&settings).unwrap();
        let count_backups = || {
            fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("settings.json.bak-hud-")
                })
                .count()
        };
        let backups = count_backups();
        assert_eq!(install(&dir, &settings), Outcome::Updated);
        assert_eq!(fs::read_to_string(&hook).unwrap(), HOOK_SOURCE);
        assert_eq!(fs::read_to_string(&settings).unwrap(), before);
        assert_eq!(count_backups(), backups);
        assert_eq!(install(&dir, &settings), Outcome::Already);

        fs::remove_file(&hook).unwrap();
        assert_eq!(install(&dir, &settings), Outcome::Updated);
        assert!(hook.exists());
    }

    #[test]
    fn python_detection_and_missing_python_handling() {
        let dir = Dir::new("py_detect");
        let settings = dir.path().join("settings.json");
        let config = dir.path().join("config");
        fs::write(&settings, r#"{"model":"x"}"#).unwrap();
        let raw = fs::read_to_string(&settings).unwrap();
        assert_eq!(
            install_statusline_with(
                Some(&settings),
                Some(&config),
                None,
                false,
                |_| None,
                || None,
            ),
            Outcome::PythonMissing
        );
        assert_eq!(fs::read_to_string(&settings).unwrap(), raw);
        assert!(!config.exists());

        assert_eq!(
            find_python_with(Some("win32"), |n| Some(PathBuf::from(n)), |_| true),
            Some("python")
        );
        assert_eq!(
            find_python_with(Some("darwin"), |n| Some(PathBuf::from(n)), |_| true),
            Some("python3")
        );
        assert_eq!(
            find_python_with(Some("linux"), |n| Some(PathBuf::from(n)), |_| true),
            Some("python3")
        );
        assert_eq!(find_python_with(Some("win32"), |_| None, |_| true), None);
        assert_eq!(
            find_python_with(Some("win32"), |n| Some(PathBuf::from(n)), |_| false),
            None
        );
    }

    #[test]
    fn windows_short_path_fallback_and_cache_written_at() {
        let dir = Dir::new("short_path");
        let settings = dir.path().join("settings.json");
        fs::write(&settings, "{}").unwrap();
        let spaced = dir.path().join("John Smith").join("config");
        let short = dir.path().join("JOHNSM~1").join("config");

        assert_eq!(
            install_statusline_with(
                Some(&settings),
                Some(&spaced),
                Some("python"),
                true,
                |_| Some(short.clone()),
                || Some("python"),
            ),
            Outcome::Installed
        );
        assert_eq!(
            statusline_status_with(Some(&settings), Some(&spaced), true, |_| Some(
                short.clone()
            )),
            Status::Installed
        );
        assert!(spaced.join(HOOK_FILENAME).exists());

        let cache = dir.path().join(QUOTA_CACHE_FILENAME);
        assert_eq!(cache_written_at(Some(&cache)), None);
        fs::write(
            &cache,
            r#"{"written_at":"2026-11-02T08:30:00+00:00","quota":{}}"#,
        )
        .unwrap();
        assert_eq!(
            cache_written_at(Some(&cache)),
            Some(Utc.with_ymd_and_hms(2026, 11, 2, 8, 30, 0).unwrap())
        );
    }
}
