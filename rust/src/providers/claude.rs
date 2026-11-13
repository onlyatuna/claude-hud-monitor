// src/providers/claude.rs — Claude Code usage provider
//
// Reads OAuth token from ~/.claude/.credentials.json
// Calls https://api.anthropic.com/api/oauth/usage
// Mirrors Python core/providers/claude_provider.py

use super::base::{
    MetricRow, Provider, UsageMetrics, format_error, now_str, object_field, percent_text,
    percentage, retry_delay,
};
use crate::config::Config;
use chrono::{DateTime, Utc};
use log::{error, info};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use ureq::OrAnyStatus;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const USER_AGENT: &str = "claude-code/0.2.29";
const BETA_HEADER: &str = "oauth-2025-04-20";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeProfile {
    pub id: String,           // "default", "claude01", "claude-01", etc.
    pub display_name: String, // "預設帳號 (~/.claude)", "claude01 (~/.claude01)"
    pub short_name: String,   // "預設", "claude01"
    pub dir_path: PathBuf,
    pub credentials_path: PathBuf,
    pub last_activity: Option<std::time::SystemTime>,
}

pub fn dirs_home() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

fn get_last_activity(dir: &Path) -> Option<std::time::SystemTime> {
    let cred = dir.join(".credentials.json");
    let hist = dir.join("history.jsonl");

    let cred_mtime = fs::metadata(&cred).and_then(|m| m.modified()).ok();
    let hist_mtime = fs::metadata(&hist).and_then(|m| m.modified()).ok();

    match (cred_mtime, hist_mtime) {
        (Some(c), Some(h)) => Some(c.max(h)),
        (Some(c), None) => Some(c),
        (None, Some(h)) => Some(h),
        (None, None) => None,
    }
}

/// Auto-discover valid Claude account directories:
/// - `$HOME/.claude*` (e.g. `.claude`, `.claude01`, `.claude-01`, `.claude_01`)
/// - `$CLAUDE_CONFIG_DIR` if set
///
/// Default `~/.claude` is always included.
pub fn discover_profiles() -> Vec<ClaudeProfile> {
    let home = dirs_home();
    let mut profiles = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. Scan $HOME for entries starting with ".claude"
    if let Ok(entries) = fs::read_dir(&home) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().to_string();
            let lower_name = file_name.to_lowercase();
            if lower_name.starts_with(".claude") {
                let cred_path = path.join(".credentials.json");
                let is_default = file_name == ".claude";
                // For default ~/.claude, include even if credentials don't exist yet
                // For other profiles, require .credentials.json
                if is_default || cred_path.exists() {
                    let norm_path = path.canonicalize().unwrap_or_else(|_| path.clone());
                    if seen_paths.insert(norm_path) {
                        let (id, short_name, display_name) = if is_default {
                            (
                                "default".to_string(),
                                "預設".to_string(),
                                "預設帳號 (~/.claude)".to_string(),
                            )
                        } else {
                            let raw = file_name.strip_prefix('.').unwrap_or(&file_name);
                            (
                                raw.to_string(),
                                raw.to_string(),
                                format!("{} (~/{})", raw, file_name),
                            )
                        };
                        let last_activity = get_last_activity(&path);
                        profiles.push(ClaudeProfile {
                            id,
                            display_name,
                            short_name,
                            dir_path: path,
                            credentials_path: cred_path,
                            last_activity,
                        });
                    }
                }
            }
        }
    }

    // 2. Check CLAUDE_CONFIG_DIR environment variable
    if let Ok(env_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let path = PathBuf::from(env_dir);
        if path.is_dir() {
            let cred_path = path.join(".credentials.json");
            if cred_path.exists() {
                let norm_path = path.canonicalize().unwrap_or_else(|_| path.clone());
                if seen_paths.insert(norm_path) {
                    let folder_name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "env".to_string());
                    let raw = folder_name.strip_prefix('.').unwrap_or(&folder_name);
                    let last_activity = get_last_activity(&path);
                    profiles.push(ClaudeProfile {
                        id: raw.to_string(),
                        display_name: format!("{} ({})", raw, path.display()),
                        short_name: raw.to_string(),
                        dir_path: path,
                        credentials_path: cred_path,
                        last_activity,
                    });
                }
            }
        }
    }

    // 3. Ensure default ~/.claude is included
    let default_dir = home.join(".claude");
    let norm_default = default_dir
        .canonicalize()
        .unwrap_or_else(|_| default_dir.clone());
    if seen_paths.insert(norm_default) {
        let cred_path = default_dir.join(".credentials.json");
        let last_activity = get_last_activity(&default_dir);
        profiles.push(ClaudeProfile {
            id: "default".to_string(),
            display_name: "預設帳號 (~/.claude)".to_string(),
            short_name: "預設".to_string(),
            dir_path: default_dir,
            credentials_path: cred_path,
            last_activity,
        });
    }

    // 4. Sort: "default" first, then other accounts alphabetically by id
    profiles.sort_by(|a, b| {
        if a.id == "default" {
            std::cmp::Ordering::Less
        } else if b.id == "default" {
            std::cmp::Ordering::Greater
        } else {
            a.id.to_lowercase().cmp(&b.id.to_lowercase())
        }
    });

    profiles
}

/// Resolve the active profile based on preference ("auto" or profile id).
/// Returns (ClaudeProfile, is_auto).
pub fn resolve_active_profile(preference: &str) -> (ClaudeProfile, bool) {
    let profiles = discover_profiles();
    let pref_trimmed = preference.trim();

    if pref_trimmed.is_empty() || pref_trimmed.eq_ignore_ascii_case("auto") {
        // Smart auto: pick the profile with credentials that has the newest activity
        let candidates: Vec<&ClaudeProfile> = profiles
            .iter()
            .filter(|p| p.credentials_path.exists())
            .collect();

        let mut best: Option<&ClaudeProfile> = None;
        for p in candidates.iter().copied() {
            if best.is_none_or(|b| p.last_activity > b.last_activity) {
                best = Some(p);
            }
        }
        if let Some(best) = best {
            return (best.clone(), true);
        }

        if let Some(first) = profiles.first() {
            return (first.clone(), true);
        }

        let home = dirs_home();
        let def_dir = home.join(".claude");
        return (
            ClaudeProfile {
                id: "default".to_string(),
                display_name: "預設帳號 (~/.claude)".to_string(),
                short_name: "預設".to_string(),
                dir_path: def_dir.clone(),
                credentials_path: def_dir.join(".credentials.json"),
                last_activity: None,
            },
            true,
        );
    }

    // Explicit profile requested
    let matched = profiles.iter().find(|p| {
        p.id.eq_ignore_ascii_case(pref_trimmed)
            || p.short_name.eq_ignore_ascii_case(pref_trimmed)
            || (pref_trimmed.starts_with('.') && p.id.eq_ignore_ascii_case(&pref_trimmed[1..]))
            || (pref_trimmed.eq_ignore_ascii_case(".claude") && p.id == "default")
    });

    if let Some(p) = matched {
        return (p.clone(), false);
    }

    // Fallback if configured profile directory is not in discovered list
    let home = dirs_home();
    let fallback_dir = if pref_trimmed.starts_with('.') {
        home.join(pref_trimmed)
    } else {
        home.join(format!(".{}", pref_trimmed))
    };
    let cred_path = fallback_dir.join(".credentials.json");
    let last_activity = get_last_activity(&fallback_dir);
    let fallback_name = fallback_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| pref_trimmed.to_string());
    let raw = pref_trimmed.strip_prefix('.').unwrap_or(pref_trimmed);
    (
        ClaudeProfile {
            id: raw.to_string(),
            display_name: format!("{} (~/{})", raw, fallback_name),
            short_name: raw.to_string(),
            dir_path: fallback_dir,
            credentials_path: cred_path,
            last_activity,
        },
        false,
    )
}

fn extract_token(data: &Value) -> Option<String> {
    data.get("claudeAiOauth")
        .and_then(|oauth| oauth.get("accessToken"))
        .and_then(|tok| tok.as_str())
        .map(|s| s.to_string())
}

#[cfg(target_os = "macos")]
fn token_from_keychain() -> Option<String> {
    use std::process::Command;
    let output = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let json: Value = serde_json::from_str(stdout.trim()).ok()?;
    extract_token(&json)
}

/// A credentials file larger than this is not read (a real one is a few hundred bytes).
const MAX_CREDENTIALS_BYTES: u64 = 64 * 1024;

fn get_access_token(credentials_path: &Path) -> Option<String> {
    if credentials_path.exists()
        && fs::metadata(credentials_path).is_ok_and(|m| m.len() <= MAX_CREDENTIALS_BYTES)
        && let Ok(text) = fs::read_to_string(credentials_path)
        && let Ok(json) = serde_json::from_str::<Value>(&text)
        && let Some(token) = extract_token(&json)
    {
        return Some(token);
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(token) = token_from_keychain() {
            return Some(token);
        }
    }

    None
}

pub struct ClaudeProvider {
    client: ureq::Agent,
    config: Option<Arc<Mutex<Config>>>,
    /// An extra account: always this profile, whatever `claude_profile` says.
    pinned_profile: Option<String>,
}

impl ClaudeProvider {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_config(None)
    }

    pub fn with_config(config: Option<Arc<Mutex<Config>>>) -> Self {
        let timeout = Duration::from_secs(10);
        let client = ureq::AgentBuilder::new().timeout(timeout).build();
        Self {
            client,
            config,
            pinned_profile: None,
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    /// The provider of an extra account: reads `profile`'s credentials and nothing else.
    pub fn for_profile(profile: &str) -> Self {
        let mut provider = Self::with_config(None);
        provider.pinned_profile = Some(profile.to_owned());
        provider
    }
}

impl Provider for ClaudeProvider {
    fn provider_id(&self) -> &str {
        "claude"
    }
    fn display_name(&self) -> &str {
        "Claude Code"
    }

    fn fetch_usage(&self) -> UsageMetrics {
        let now = now_str();
        let preference = self.pinned_profile.clone().unwrap_or_else(|| {
            self.config
                .as_ref()
                .map(|c| c.lock().claude_profile.clone())
                .unwrap_or_else(|| "auto".to_string())
        });

        let (active_profile, is_auto) = resolve_active_profile(&preference);
        let display_title = if is_auto {
            if active_profile.id == "default" {
                "CLAUDE CODE".to_string()
            } else {
                format!("CLAUDE [{}]", active_profile.short_name)
            }
        } else if active_profile.id == "default" {
            "CLAUDE CODE".to_string()
        } else {
            format!("CLAUDE ({})", active_profile.short_name)
        };

        let Some(token) = get_access_token(&active_profile.credentials_path) else {
            let error_msg = if active_profile.id == "default" {
                "未找到 Claude 登入憑證\n請於終端機執行 claude 登入".to_string()
            } else {
                format!(
                    "未找到帳號 [{}] 登入憑證\n請使用 {} 登入或檢查目錄",
                    active_profile.short_name, active_profile.short_name
                )
            };
            return UsageMetrics::error_result("claude", &display_title, &error_msg, "auth");
        };

        let result = self
            .client
            .get(USAGE_URL)
            .set("Authorization", &format!("Bearer {}", token))
            .set("User-Agent", USER_AGENT)
            .set("anthropic-beta", BETA_HEADER)
            .set("Accept", "application/json")
            .call()
            .or_any_status();

        match result {
            Ok(resp) => {
                let status = resp.status();
                if status == 401 || status == 403 {
                    let retry = parse_retry_after(&resp);
                    return UsageMetrics {
                        provider_id: "claude".to_owned(),
                        provider_name: display_title,
                        metric1_title: "SESSION 5H".to_owned(),
                        metric1_text: "--".to_owned(),
                        metric2_title: "WEEKLY 7D".to_owned(),
                        metric2_text: "--".to_owned(),
                        last_updated_time: now,
                        error: Some("登入憑證已失效，請使用原 CLI 重新登入".to_owned()),
                        error_code: "auth".to_owned(),
                        retry_after: retry,
                        ..Default::default()
                    };
                }
                if status == 429 {
                    let retry = parse_retry_after(&resp);
                    return UsageMetrics {
                        provider_id: "claude".to_owned(),
                        provider_name: display_title,
                        metric1_title: "SESSION 5H".to_owned(),
                        metric1_text: "--".to_owned(),
                        metric2_title: "WEEKLY 7D".to_owned(),
                        metric2_text: "--".to_owned(),
                        last_updated_time: now,
                        error: Some(format!("配額查詢 HTTP {}", status)),
                        error_code: "rate_limit".to_owned(),
                        retry_after: retry,
                        ..Default::default()
                    };
                }
                if !(200..300).contains(&status) {
                    return UsageMetrics::error_result(
                        "claude",
                        &display_title,
                        &format!("配額查詢 HTTP {}", status),
                        "http",
                    );
                }
                match resp.into_json::<Value>() {
                    Ok(json) => {
                        let mut metrics = parse_claude_response(json, &now);
                        metrics.provider_name = display_title;
                        metrics
                    }
                    Err(_) => UsageMetrics::error_result(
                        "claude",
                        &display_title,
                        "未取得有效配額資料",
                        "schema",
                    ),
                }
            }
            Err(e) => {
                error!("[ClaudeProvider] Request error: {e}");
                UsageMetrics::error_result(
                    "claude",
                    &display_title,
                    "配額連線失敗，將自動重試",
                    "network",
                )
            }
        }
    }
}

fn parse_claude_response(data: Value, now_str: &str) -> UsageMetrics {
    if !data.is_object() {
        return format_error("claude", "Claude", now_str);
    }
    let (Some(five_hour), Some(seven_day), Some(breakdown)) = (
        object_field(&data, "five_hour"),
        object_field(&data, "seven_day"),
        object_field(&data, "seven_day_breakdown"),
    ) else {
        return format_error("claude", "Claude", now_str);
    };

    let mut code_pct: Option<f64> = None;
    let mut chat_pct: Option<f64> = None;

    let empty = Vec::new();
    let rows = match breakdown.get("rows") {
        None => &empty,
        Some(Value::Array(rows)) => rows,
        Some(Value::Object(o)) if o.is_empty() => &empty,
        Some(_) => return format_error("claude", "Claude", now_str),
    };
    for r in rows {
        if !r.is_object() {
            return format_error("claude", "Claude", now_str);
        }
        let key = r.get("key").and_then(|v| v.as_str()).unwrap_or("");
        let pct_val = r.get("percent").and_then(|v| v.as_f64());
        if key == "claude_code" {
            code_pct = percentage(pct_val, 100.0);
        } else if key == "chat" {
            chat_pct = percentage(pct_val, 100.0);
        }
    }

    let limits = parse_limits(&data);

    // A missing (absent / null / empty object) top-level window falls back to the matching
    // `limits[]` entry; a present one always wins.
    let missing = |w: &Value| w.is_null() || w.as_object().is_some_and(|o| o.is_empty());
    let (five_h_dt, s_val) = match (missing(&five_hour), &limits.session) {
        (true, Some(w)) => (w.reset, Some(w.val)),
        _ => (
            parse_iso_datetime(five_hour.get("resets_at").and_then(|v| v.as_str())),
            percentage(five_hour.get("utilization").and_then(|v| v.as_f64()), 100.0),
        ),
    };
    let (seven_d_dt, w_val) = match (missing(&seven_day), &limits.weekly_all) {
        (true, Some(w)) => (w.reset, Some(w.val)),
        _ => (
            parse_iso_datetime(seven_day.get("resets_at").and_then(|v| v.as_str())),
            percentage(seven_day.get("utilization").and_then(|v| v.as_f64()), 100.0),
        ),
    };

    UsageMetrics {
        provider_id: "claude".to_owned(),
        provider_name: "Claude Code".to_owned(),
        metric1_title: "SESSION 5H".to_owned(),
        metric1_val: s_val,
        metric1_text: percent_text(s_val),
        metric1_reset: five_h_dt,
        metric1_key: "claude:5h".to_owned(),
        metric2_title: "WEEKLY 7D".to_owned(),
        metric2_val: w_val,
        metric2_text: percent_text(w_val),
        metric2_reset: seven_d_dt,
        metric2_key: "claude:7d".to_owned(),
        badge1_text: format!("Code: {}", percent_text(code_pct)),
        badge2_text: format!("Chat: {}", percent_text(chat_pct)),
        last_updated_time: now_str.to_owned(),
        error: if s_val.is_none() && w_val.is_none() {
            Some("未取得有效配額資料".to_owned())
        } else {
            None
        },
        error_code: if s_val.is_none() && w_val.is_none() {
            "schema".to_owned()
        } else {
            String::new()
        },
        extra_metrics: limits.extras,
        ..Default::default()
    }
}

/// A main-pair window taken from a `limits[]` entry (fallback only).
struct LimitWindow {
    val: f64,
    reset: Option<DateTime<Utc>>,
}

#[derive(Default)]
struct Limits {
    session: Option<LimitWindow>,
    weekly_all: Option<LimitWindow>,
    extras: Vec<MetricRow>,
}

/// Kinds and groups the parser understands; anything else is still rendered (when it has a valid
/// percent) but logged once per process.
const KNOWN_KINDS: [&str; 3] = ["session", "weekly_all", "weekly_scoped"];
const KNOWN_GROUPS: [&str; 2] = ["session", "weekly"];

/// True the first time `what` is seen in this process (so it is logged once, never persisted).
fn first_sighting(what: &str) -> bool {
    static SEEN: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);
    SEEN.lock().insert(what.to_owned())
}

fn note_unknown(label: &str, value: &str) {
    let value: String = value.chars().take(40).collect();
    if first_sighting(&format!("{label}={value}")) {
        info!("[ClaudeProvider] unknown limits[] {label}: {value:?}");
    }
}

/// Lower-case slug: every run of non-alphanumeric characters becomes one '-', edges trimmed.
pub(super) fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn non_empty_str(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Read the optional top-level `limits[]` array. Anything that is not a list is ignored; entries
/// that are not objects, have no finite 0..=100 `percent` or repeat an earlier row are skipped.
/// `is_active` is deliberately not consulted: it only marks the currently binding limit.
fn parse_limits(data: &Value) -> Limits {
    let mut out = Limits::default();
    let Some(Value::Array(entries)) = data.get("limits") else {
        return out;
    };
    for entry in entries {
        if !entry.is_object() {
            continue;
        }
        let kind = non_empty_str(entry.get("kind")).map(str::to_lowercase);
        let Some(kind) = kind.as_deref() else {
            continue;
        };
        let group = non_empty_str(entry.get("group")).map_or_else(String::new, str::to_lowercase);
        let group = group.as_str();
        if !KNOWN_KINDS.contains(&kind) {
            note_unknown("kind", kind);
        }
        if !group.is_empty() && !KNOWN_GROUPS.contains(&group) {
            note_unknown("group", group);
        }
        let Some(val) = percentage(entry.get("percent").and_then(Value::as_f64), 100.0) else {
            continue;
        };
        let reset = parse_iso_datetime(entry.get("resets_at").and_then(Value::as_str));
        match kind {
            // These two duplicate five_hour / seven_day; kept only as a main-pair fallback.
            "session" => {
                out.session.get_or_insert(LimitWindow { val, reset });
            }
            "weekly_all" => {
                out.weekly_all.get_or_insert(LimitWindow { val, reset });
            }
            _ => {
                let mut row = scoped_row(entry, kind, group, val, reset);
                if out.extras.iter().all(|r| r.key != row.key) {
                    row.subtext = String::new();
                    out.extras.push(row);
                }
            }
        }
    }
    out
}

/// A usable scope name: non-empty after trimming and with a non-empty slug.
fn scope_name(v: Option<&Value>) -> Option<(&str, String)> {
    let name = non_empty_str(v)?;
    let s = slug(name);
    (!s.is_empty()).then_some((name, s))
}

/// Row for a scoped limit: title `<NAME> <WINDOW>` (`FABLE 7D`), key
/// `claude:<kind>[:<slug(model or surface)>[:<slug(surface)>]]`. NAME is the scope's model display
/// name, else its surface, else the kind.
fn scoped_row(
    entry: &Value,
    kind: &str,
    group: &str,
    val: f64,
    reset: Option<DateTime<Utc>>,
) -> MetricRow {
    let scope = entry.get("scope").filter(|s| s.is_object());
    let model = scope
        .and_then(|s| s.get("model"))
        .filter(|m| m.is_object())
        .and_then(|m| scope_name(m.get("display_name")));
    let surface = scope_name(scope.and_then(|s| s.get("surface")));
    let mut title = match (&model, &surface) {
        (Some((n, _)), _) | (None, Some((n, _))) => n.to_uppercase(),
        (None, None) => kind.to_uppercase(),
    };
    match group {
        "weekly" => title.push_str(" 7D"),
        "session" => title.push_str(" 5H"),
        _ => {}
    }
    let mut key = format!("claude:{kind}");
    let parts = match (&model, &surface) {
        (Some((_, m)), Some((_, s))) => [Some(m), Some(s)],
        (Some((_, m)), None) | (None, Some((_, m))) => [Some(m), None],
        (None, None) => [None, None],
    };
    for part in parts.into_iter().flatten() {
        key.push(':');
        key.push_str(part);
    }
    let mut row = MetricRow::new(&title, Some(val), &percent_text(Some(val)));
    row.reset = reset;
    row.key = key;
    row
}

fn parse_iso_datetime(s: Option<&str>) -> Option<DateTime<Utc>> {
    s.and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    })
}

fn parse_retry_after(resp: &ureq::Response) -> Option<f64> {
    retry_delay(resp.header("Retry-After")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_claude_response() {
        let json_data = serde_json::json!({
            "five_hour": {
                "utilization": 28.5,
                "resets_at": "2030-01-01T05:00:00Z"
            },
            "seven_day": {
                "utilization": 64.0,
                "resets_at": "2030-01-07T00:00:00Z"
            },
            "seven_day_breakdown": {
                "rows": [
                    {"key": "claude_code", "percent": 50.0},
                    {"key": "chat", "percent": 14.0}
                ]
            }
        });

        let metrics = parse_claude_response(json_data, "10:00:00");
        assert_eq!(metrics.provider_id, "claude");
        assert_eq!(metrics.metric1_val, Some(28.5));
        assert_eq!(metrics.metric1_text, "28%");
        assert_eq!(metrics.metric2_val, Some(64.0));
        assert_eq!(metrics.metric2_text, "64%");
        assert_eq!(metrics.badge1_text, "Code: 50%");
        assert_eq!(metrics.badge2_text, "Chat: 14%");
        assert!(metrics.error.is_none());
    }

    #[test]
    fn test_resolve_active_profile() {
        let (def_prof, is_auto) = resolve_active_profile("auto");
        assert!(is_auto);
        assert!(!def_prof.id.is_empty());

        let (explicit_prof, is_auto2) = resolve_active_profile("claude01");
        assert!(!is_auto2);
        assert_eq!(explicit_prof.id, "claude01");
        assert_eq!(explicit_prof.short_name, "claude01");

        let (dot_prof, is_auto3) = resolve_active_profile(".claude-02");
        assert!(!is_auto3);
        assert_eq!(dot_prof.short_name, "claude-02");

        let (default_alias, is_auto4) = resolve_active_profile(".claude");
        assert!(!is_auto4);
        assert_eq!(default_alias.id, "default");
    }

    #[test]
    fn test_malformed_schema_is_reported() {
        for bad in [
            serde_json::json!([]),
            serde_json::json!({"five_hour": "x"}),
            serde_json::json!({"seven_day_breakdown": {"rows": [1]}}),
        ] {
            let m = parse_claude_response(bad, "10:00:00");
            assert_eq!(m.error.as_deref(), Some("配額資料格式不相容"));
            assert_eq!(m.error_code, "schema");
        }
    }

    #[test]
    fn test_extract_token() {
        let valid_json = serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "sk-ant-test-token"
            }
        });
        assert_eq!(
            extract_token(&valid_json),
            Some("sk-ant-test-token".to_string())
        );

        let invalid_json = serde_json::json!({
            "claudeAiOauth": "not an object"
        });
        assert_eq!(extract_token(&invalid_json), None);

        let empty_json = serde_json::json!({});
        assert_eq!(extract_token(&empty_json), None);
    }

    fn limits_payload() -> Value {
        serde_json::json!({
            "five_hour": {"utilization": 28.5, "resets_at": "2030-01-01T05:00:00Z"},
            "seven_day": {"utilization": 64.0, "resets_at": "2030-01-07T00:00:00Z"},
            "seven_day_sonnet": null,
            "seven_day_opus": null,
            "seven_day_omelette": null,
            "iguana_necktie": null,
            "limits": [
                {"kind": "session", "group": "session", "percent": 28, "severity": "normal",
                 "resets_at": "2030-01-01T05:00:00Z", "scope": null, "is_active": false},
                {"kind": "weekly_all", "group": "weekly", "percent": 64, "severity": "normal",
                 "resets_at": "2030-01-07T00:00:00Z", "scope": null, "is_active": false},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 84, "severity": "warning",
                 "resets_at": "2030-01-06T00:00:00Z",
                 "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null},
                 "is_active": true},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 0, "severity": "normal",
                 "resets_at": null,
                 "scope": {"model": {"id": "m-1", "display_name": "Quiet Model"}, "surface": null},
                 "is_active": false},
                "not an object",
                {"kind": "weekly_scoped", "group": "weekly", "percent": "lots",
                 "scope": {"model": {"display_name": "Broken"}}},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 150,
                 "scope": {"model": {"display_name": "Too High"}}},
                {"kind": "brand_new_kind", "group": "brand_new_group", "percent": 12,
                 "scope": {"model": {"display_name": "Mystery"}}},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 99,
                 "scope": {"model": {"display_name": "FABLE"}}}
            ]
        })
    }

    #[test]
    fn scoped_limits_become_extra_rows_and_the_main_pair_is_unchanged() {
        let m = parse_claude_response(limits_payload(), "10:00:00");
        assert!(m.error.is_none());
        assert_eq!(m.metric1_val, Some(28.5));
        assert_eq!(m.metric2_val, Some(64.0));
        assert_eq!(m.metric1_key, "claude:5h");
        assert_eq!(m.metric2_key, "claude:7d");

        let titles: Vec<&str> = m.extra_metrics.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["FABLE 7D", "QUIET MODEL 7D", "MYSTERY"]);

        let fable = &m.extra_metrics[0];
        assert_eq!(fable.key, "claude:weekly_scoped:fable");
        assert_eq!(fable.val, Some(84.0));
        assert_eq!(fable.text, "84%");
        assert!(fable.reset.is_some());

        // A quiet entry (0 %, no reset, not active) is still rendered; `is_active` is not a filter.
        let quiet = &m.extra_metrics[1];
        assert_eq!(quiet.key, "claude:weekly_scoped:quiet-model");
        assert_eq!(quiet.val, Some(0.0));
        assert_eq!(quiet.text, "0%");
        assert_eq!(quiet.reset, None);

        // Unknown kind/group with a valid percent is kept without panicking.
        assert_eq!(m.extra_metrics[2].key, "claude:brand_new_kind:mystery");
    }

    #[test]
    fn duplicate_scoped_entries_keep_the_first() {
        let m = parse_claude_response(limits_payload(), "10:00:00");
        let fables: Vec<_> = m
            .extra_metrics
            .iter()
            .filter(|r| r.key == "claude:weekly_scoped:fable")
            .collect();
        assert_eq!(fables.len(), 1);
        assert_eq!(fables[0].val, Some(84.0));
    }

    #[test]
    fn missing_top_level_windows_fall_back_to_limits() {
        let mut data = limits_payload();
        let obj = data.as_object_mut().unwrap();
        obj.remove("five_hour");
        obj.insert("seven_day".to_owned(), Value::Null);
        let m = parse_claude_response(data, "10:00:00");
        assert!(m.error.is_none());
        assert_eq!(m.metric1_val, Some(28.0));
        assert_eq!(
            m.metric1_reset,
            parse_iso_datetime(Some("2030-01-01T05:00:00Z"))
        );
        assert_eq!(m.metric2_val, Some(64.0));
        assert_eq!(
            m.metric2_reset,
            parse_iso_datetime(Some("2030-01-07T00:00:00Z"))
        );
        // session / weekly_all never become extra rows.
        assert_eq!(m.extra_metrics.len(), 3);
    }

    #[test]
    fn present_top_level_windows_win_over_limits() {
        let mut data = limits_payload();
        data["five_hour"]["utilization"] = serde_json::json!(40.0);
        let m = parse_claude_response(data, "10:00:00");
        assert_eq!(m.metric1_val, Some(40.0));
    }

    #[test]
    fn wrong_typed_limits_never_break_the_main_pair() {
        for bad in [
            serde_json::json!("x"),
            serde_json::json!({"a": 1}),
            serde_json::json!(7),
            Value::Null,
            serde_json::json!([1, null, "x", [], {"percent": null}]),
        ] {
            let mut data = limits_payload();
            data["limits"] = bad;
            let m = parse_claude_response(data, "10:00:00");
            assert!(m.error.is_none());
            assert_eq!(m.metric1_val, Some(28.5));
            assert_eq!(m.metric2_val, Some(64.0));
            assert!(m.extra_metrics.is_empty());
        }
    }

    #[test]
    fn surface_and_kind_fallback_titles_and_slugs() {
        let data = serde_json::json!({
            "five_hour": {"utilization": 1.0},
            "seven_day": {"utilization": 2.0},
            "limits": [
                {"kind": "weekly_scoped", "group": "weekly", "percent": 10,
                 "scope": {"model": null, "surface": "Claude Design"}},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 11, "scope": null},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 12,
                 "scope": {"model": {"display_name": "  Opus 4.1 (Beta)! "}}}
            ]
        });
        let m = parse_claude_response(data, "10:00:00");
        let rows: Vec<(&str, &str)> = m
            .extra_metrics
            .iter()
            .map(|r| (r.title.as_str(), r.key.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("CLAUDE DESIGN 7D", "claude:weekly_scoped:claude-design"),
                ("WEEKLY_SCOPED 7D", "claude:weekly_scoped"),
                ("OPUS 4.1 (BETA)! 7D", "claude:weekly_scoped:opus-4-1-beta"),
            ]
        );
    }

    #[test]
    fn unknown_kinds_are_logged_once_per_process() {
        assert!(first_sighting("kind=test-only-kind"));
        assert!(!first_sighting("kind=test-only-kind"));
    }

    #[test]
    fn model_and_surface_both_in_the_key_and_empty_kind_skipped() {
        let data = serde_json::json!({
            "five_hour": {},
            "seven_day": {"utilization": 2.0},
            "limits": [
                {"kind": "weekly_scoped", "group": "weekly", "percent": 10,
                 "scope": {"model": {"display_name": "Fable"}, "surface": "Daily Routines"}},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 11,
                 "scope": {"model": {"display_name": "Fable"}, "surface": "Other"}},
                {"kind": "", "group": "weekly", "percent": 12},
                {"kind": "Session", "group": "session", "percent": 33, "resets_at": null}
            ]
        });
        let m = parse_claude_response(data, "10:00:00");
        let rows: Vec<(&str, &str)> = m
            .extra_metrics
            .iter()
            .map(|r| (r.title.as_str(), r.key.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("FABLE 7D", "claude:weekly_scoped:fable:daily-routines"),
                ("FABLE 7D", "claude:weekly_scoped:fable:other"),
            ]
        );
        // An empty top-level `five_hour` object counts as missing: the session entry fills it.
        assert_eq!(m.metric1_val, Some(33.0));
    }

    /// Manual integration test against the live Claude usage endpoint; parsing is covered offline
    /// by `test_parse_claude_response`. Usage numbers depend on the account, so only success is
    /// asserted. Run with `cargo test test_live_fetch_usage -- --ignored`.
    #[test]
    #[ignore = "needs network and a valid Claude login"]
    fn test_live_fetch_usage() {
        let p = ClaudeProvider::new();
        let m = p.fetch_usage();
        eprintln!("[test_live_fetch_usage] provider_name: {}", m.provider_name);
        eprintln!(
            "[test_live_fetch_usage] metric1: {} = {}",
            m.metric1_title, m.metric1_text
        );
        eprintln!(
            "[test_live_fetch_usage] metric2: {} = {}",
            m.metric2_title, m.metric2_text
        );
        eprintln!("[test_live_fetch_usage] error: {:?}", m.error);
        eprintln!("[test_live_fetch_usage] error_code: {}", m.error_code);
        assert!(
            m.error.is_none(),
            "live fetch failed: {:?} ({})",
            m.error,
            m.error_code
        );
    }
}
