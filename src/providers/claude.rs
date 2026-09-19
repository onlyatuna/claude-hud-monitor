// src/providers/claude.rs — Claude Code usage provider
//
// Reads OAuth token from ~/.claude/.credentials.json
// Calls https://api.anthropic.com/api/oauth/usage
// Mirrors Python core/providers/claude_provider.py

use super::base::{now_str, percent_text, percentage, Provider, UsageMetrics};
use crate::config::Config;
use chrono::{DateTime, Utc};
use log::error;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

        if let Some(best) = candidates.iter().max_by_key(|p| p.last_activity) {
            return ((*best).clone(), true);
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
            || (pref_trimmed == ".claude" && p.id == "default")
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
    (
        ClaudeProfile {
            id: pref_trimmed.to_string(),
            display_name: format!("{} (~/{})", pref_trimmed, fallback_name),
            short_name: pref_trimmed.trim_start_matches('.').to_string(),
            dir_path: fallback_dir,
            credentials_path: cred_path,
            last_activity,
        },
        false,
    )
}

fn get_access_token(credentials_path: &Path) -> Option<String> {
    if !credentials_path.exists() {
        return None;
    }
    let text = fs::read_to_string(credentials_path).ok()?;
    let json: Value = serde_json::from_str(&text).ok()?;
    json["claudeAiOauth"]["accessToken"]
        .as_str()
        .map(|s| s.to_owned())
}

pub struct ClaudeProvider {
    #[allow(dead_code)]
    timeout: Duration,
    client: reqwest::blocking::Client,
    config: Option<Arc<Mutex<Config>>>,
}

impl ClaudeProvider {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_config(None)
    }

    pub fn with_config(config: Option<Arc<Mutex<Config>>>) -> Self {
        let timeout = Duration::from_secs(10);
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
        Self {
            timeout,
            client,
            config,
        }
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
        let preference = self
            .config
            .as_ref()
            .and_then(|c| c.lock().ok())
            .map(|c| c.claude_profile.clone())
            .unwrap_or_else(|| "auto".to_string());

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
            return UsageMetrics::error_result("claude", &display_title, &error_msg, "");
        };

        let result = self
            .client
            .get(USAGE_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("User-Agent", USER_AGENT)
            .header("anthropic-beta", BETA_HEADER)
            .header("Accept", "application/json")
            .send();

        match result {
            Ok(resp) => {
                let status = resp.status();
                if status == 401 {
                    let retry = parse_retry_after(resp.headers());
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
                    let retry = parse_retry_after(resp.headers());
                    return UsageMetrics {
                        provider_id: "claude".to_owned(),
                        provider_name: display_title,
                        metric1_title: "SESSION 5H".to_owned(),
                        metric1_text: "--".to_owned(),
                        metric2_title: "WEEKLY 7D".to_owned(),
                        metric2_text: "--".to_owned(),
                        last_updated_time: now,
                        error: Some(format!("配額查詢 HTTP {}", status.as_u16())),
                        error_code: "rate_limit".to_owned(),
                        retry_after: retry,
                        ..Default::default()
                    };
                }
                if !status.is_success() {
                    return UsageMetrics::error_result(
                        "claude",
                        &display_title,
                        &format!("API 回應異常: HTTP {}", status.as_u16()),
                        "http",
                    );
                }
                match resp.json::<Value>() {
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
    let five_hour = data.get("five_hour").cloned().unwrap_or(Value::Null);
    let seven_day = data.get("seven_day").cloned().unwrap_or(Value::Null);
    let breakdown = data
        .get("seven_day_breakdown")
        .cloned()
        .unwrap_or(Value::Null);

    let mut code_pct: Option<f64> = None;
    let mut chat_pct: Option<f64> = None;

    if let Some(rows) = breakdown.get("rows").and_then(|v| v.as_array()) {
        for r in rows {
            let key = r.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let pct_val = r.get("percent").and_then(|v| v.as_f64());
            if key == "claude_code" {
                code_pct = percentage(pct_val, 100.0);
            } else if key == "chat" {
                chat_pct = percentage(pct_val, 100.0);
            }
        }
    }

    let five_h_dt = parse_iso_datetime(five_hour.get("resets_at").and_then(|v| v.as_str()));
    let seven_d_dt = parse_iso_datetime(seven_day.get("resets_at").and_then(|v| v.as_str()));

    let s_val = percentage(five_hour.get("utilization").and_then(|v| v.as_f64()), 100.0);
    let w_val = percentage(seven_day.get("utilization").and_then(|v| v.as_f64()), 100.0);

    UsageMetrics {
        provider_id: "claude".to_owned(),
        provider_name: "Claude Code".to_owned(),
        metric1_title: "SESSION 5H".to_owned(),
        metric1_val: s_val,
        metric1_text: percent_text(s_val),
        metric1_reset: five_h_dt,
        metric2_title: "WEEKLY 7D".to_owned(),
        metric2_val: w_val,
        metric2_text: percent_text(w_val),
        metric2_reset: seven_d_dt,
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
        ..Default::default()
    }
}

fn parse_iso_datetime(s: Option<&str>) -> Option<DateTime<Utc>> {
    s.and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    })
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<f64> {
    headers
        .get("Retry-After")?
        .to_str()
        .ok()?
        .parse::<f64>()
        .ok()
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
        assert_eq!(def_prof.id, "default");

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
}
