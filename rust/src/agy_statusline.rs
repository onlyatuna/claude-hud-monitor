//! Install the agy status line hook and read its quota-only cache (port of
//! `python/core/agy_statusline.py`).
//!
//! agy runs the hook once per status line refresh and pipes its state to it; the hook keeps only the
//! quota buckets and writes them to `agy-quota.json` in the HUD's config folder. The HUD reads that
//! file to fill the rows the quota endpoints do not report (`GEMINI 7D`, `ARGON 7D`). The hook is the
//! same Python script the Python HUD installs, so both builds share one hook and one cache.

use crate::config::ConfigManager;
use chrono::{DateTime, Local, Utc};
use serde_json::{json, Map, Value};
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

/// What `install_statusline` and `uninstall_statusline` report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Installed,
    Already,
    Conflict,
    UnsupportedPath,
    Error,
    Removed,
    NotInstalled,
}

impl Outcome {
    /// The one line `cli` prints (Python `messages`).
    pub fn message(self) -> &'static str {
        match self {
            Outcome::Installed => "已安裝 agy statusLine（請重開 agy 互動模式）",
            Outcome::Already => "已安裝，無需變更",
            Outcome::Conflict => "agy 已設定其他 statusLine，未變更",
            Outcome::UnsupportedPath => "設定資料夾路徑含空白，無法安裝",
            Outcome::Error => "無法讀寫 agy 設定",
            Outcome::Removed => "已移除 agy statusLine",
            Outcome::NotInstalled => "尚未安裝",
        }
    }

    /// 0 for the outcomes that leave things as asked, 1 for a failure.
    pub fn exit_code(self) -> i32 {
        match self {
            Outcome::Installed | Outcome::Already | Outcome::Removed | Outcome::NotInstalled => 0,
            Outcome::Conflict | Outcome::UnsupportedPath | Outcome::Error => 1,
        }
    }
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Python `install_statusline`. `None` arguments take the defaults: agy's settings file, the HUD's
/// config folder, and `python` (`python3` outside Windows) as the interpreter.
pub fn install_statusline(
    settings_path: Option<&Path>,
    config_dir: Option<&Path>,
    python_cmd: Option<&str>,
) -> Outcome {
    let config_dir = config_dir.map_or_else(ConfigManager::config_dir, Path::to_path_buf);
    let hook_path = absolute(&config_dir.join(HOOK_FILENAME));
    let python_cmd = python_cmd.unwrap_or(if cfg!(windows) { "python" } else { "python3" });
    let quoted = |text: &str| text.chars().any(|c| c == '"' || c == '\'');
    let hook_text = hook_path.to_string_lossy().into_owned();
    if hook_text.chars().any(|c| c.is_whitespace()) || quoted(&hook_text) || quoted(python_cmd) {
        return Outcome::UnsupportedPath;
    }
    let settings_path = settings_path.map_or_else(agy_settings_path, Path::to_path_buf);
    let Ok(mut settings) = load_settings(&settings_path) else {
        return Outcome::Error;
    };
    let desired = json!({
        "type": "command",
        "command": format!("{python_cmd} {hook_text}"),
        "enabled": true,
    });
    if let Some(current) = settings.get("statusLine") {
        if !owned(current) {
            return Outcome::Conflict;
        }
        if *current == desired {
            return Outcome::Already;
        }
    }
    let written = (|| -> std::io::Result<()> {
        backup(&settings_path)?;
        fs::create_dir_all(&config_dir)?;
        fs::write(&hook_path, HOOK_SOURCE)?;
        settings.insert("statusLine".to_owned(), desired);
        write_settings(&settings_path, &settings)
    })();
    if written.is_ok() {
        Outcome::Installed
    } else {
        Outcome::Error
    }
}

/// Python `uninstall_statusline`: removes our `statusLine` entry; the hook file stays.
pub fn uninstall_statusline(settings_path: Option<&Path>) -> Outcome {
    let settings_path = settings_path.map_or_else(agy_settings_path, Path::to_path_buf);
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
        assert!(!data["statusLine"]["command"]
            .as_str()
            .unwrap()
            .contains('"'));
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
                install_statusline(Some(&settings), Some(&folder), Some("python")),
                Outcome::UnsupportedPath
            );
            assert!(!folder.exists());
        }
        assert_eq!(install(&dir, &settings), Outcome::Error, "no settings file");
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
            (Outcome::Already, 0),
            (Outcome::Removed, 0),
            (Outcome::NotInstalled, 0),
            (Outcome::Conflict, 1),
            (Outcome::UnsupportedPath, 1),
            (Outcome::Error, 1),
        ] {
            assert_eq!(outcome.exit_code(), code);
            assert!(!outcome.message().contains('\n'));
        }
    }
}
