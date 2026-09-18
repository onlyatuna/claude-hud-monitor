// src/providers/claude.rs — Claude Code usage provider
//
// Reads OAuth token from ~/.claude/.credentials.json
// Calls https://api.anthropic.com/api/oauth/usage
// Mirrors Python core/providers/claude_provider.py

use super::base::{percentage, percent_text, now_str, Provider, UsageMetrics};
use chrono::{DateTime, Utc};
use log::error;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const USER_AGENT: &str = "claude-code/0.2.29";
const BETA_HEADER: &str = "oauth-2025-04-20";

pub struct ClaudeProvider {
    #[allow(dead_code)]
    timeout: Duration,
    client: reqwest::blocking::Client,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let timeout = Duration::from_secs(10);
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
        Self { timeout, client }
    }

    fn credentials_path() -> PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_owned());
        PathBuf::from(home).join(".claude").join(".credentials.json")
    }

    fn get_access_token(&self) -> Option<String> {
        let path = Self::credentials_path();
        if !path.exists() {
            return None;
        }
        let text = fs::read_to_string(&path).ok()?;
        let json: Value = serde_json::from_str(&text).ok()?;
        json["claudeAiOauth"]["accessToken"]
            .as_str()
            .map(|s| s.to_owned())
    }
}

impl Provider for ClaudeProvider {
    fn provider_id(&self) -> &str { "claude" }
    fn display_name(&self) -> &str { "Claude Code" }

    fn fetch_usage(&self) -> UsageMetrics {
        let now = now_str();
        let Some(token) = self.get_access_token() else {
            return UsageMetrics::error_result(
                "claude", "Claude Code",
                "未找到 Claude 登入憑證\n請於終端機執行 claude 登入",
                ""
            );
        };

        let result = self.client
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
                        provider_name: "Claude Code".to_owned(),
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
                        provider_name: "Claude Code".to_owned(),
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
                        "claude", "Claude Code",
                        &format!("API 回應異常: HTTP {}", status.as_u16()),
                        "http"
                    );
                }
                match resp.json::<Value>() {
                    Ok(json) => parse_claude_response(json, &now),
                    Err(_) => UsageMetrics::error_result("claude", "Claude Code", "未取得有效配額資料", "schema"),
                }
            }
            Err(e) => {
                error!("[ClaudeProvider] Request error: {e}");
                UsageMetrics::error_result("claude", "Claude Code", "配額連線失敗，將自動重試", "network")
            }
        }
    }
}

fn parse_claude_response(data: Value, now_str: &str) -> UsageMetrics {
    let five_hour = data.get("five_hour").cloned().unwrap_or(Value::Null);
    let seven_day = data.get("seven_day").cloned().unwrap_or(Value::Null);
    let breakdown = data.get("seven_day_breakdown").cloned().unwrap_or(Value::Null);

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
        error_code: if s_val.is_none() && w_val.is_none() { "schema".to_owned() } else { String::new() },
        ..Default::default()
    }
}

fn parse_iso_datetime(s: Option<&str>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc)))
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<f64> {
    headers.get("Retry-After")?.to_str().ok()?.parse::<f64>().ok()
}
