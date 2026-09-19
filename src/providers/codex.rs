// src/providers/codex.rs — OpenAI Codex usage provider
//
// Reads auth from ~/.codex/auth.json
// Calls https://chatgpt.com/backend-api/wham/usage
// Mirrors Python core/providers/codex_provider.py

use super::base::{now_str, percent_text, percentage, Provider, UsageMetrics};
use chrono::{DateTime, TimeZone, Utc};
use log::error;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const USER_AGENT: &str = "codex-cli/0.154.0";

pub struct CodexProvider {
    #[allow(dead_code)]
    timeout: Duration,
    client: reqwest::blocking::Client,
}

impl CodexProvider {
    pub fn new() -> Self {
        let timeout = Duration::from_secs(8);
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
        Self { timeout, client }
    }

    fn auth_path() -> PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_owned());
        PathBuf::from(home).join(".codex").join("auth.json")
    }

    fn get_auth_data(&self) -> Option<Value> {
        let path = Self::auth_path();
        if !path.exists() {
            return None;
        }
        let text = fs::read_to_string(&path).ok()?;
        serde_json::from_str::<Value>(&text).ok()
    }
}

fn parse_timestamp(ts: Option<&Value>) -> Option<DateTime<Utc>> {
    let ts = ts?;
    let val = ts.as_f64()?;
    let val = if val > 1e11 { val / 1000.0 } else { val };
    let secs = val as i64;
    let nanos = ((val - secs as f64) * 1e9) as u32;
    Utc.timestamp_opt(secs, nanos).single()
}

fn window_title(window: &Value, fallback: &str) -> String {
    if let Some(secs) = window.get("limit_window_seconds").and_then(|v| v.as_f64()) {
        if secs > 0.0 {
            if secs % 86400.0 == 0.0 {
                return format!("WINDOW {}D", secs / 86400.0);
            }
            if secs % 3600.0 == 0.0 {
                return format!("WINDOW {}H", secs / 3600.0);
            }
            return format!("WINDOW {}M", secs / 60.0);
        }
    }
    fallback.to_owned()
}

impl Provider for CodexProvider {
    fn provider_id(&self) -> &str {
        "codex"
    }
    fn display_name(&self) -> &str {
        "OpenAI Codex"
    }

    fn fetch_usage(&self) -> UsageMetrics {
        let now = now_str();
        let Some(auth_data) = self.get_auth_data() else {
            return UsageMetrics::error_result(
                "codex",
                "OpenAI Codex",
                "未找到 Codex 授權檔 (~/.codex/auth.json)\n請執行 codex 登入",
                "",
            );
        };

        let tokens = auth_data.get("tokens").cloned().unwrap_or(Value::Null);
        let access_token = match tokens.get("access_token").and_then(|v| v.as_str()) {
            Some(t) => t.to_owned(),
            None => {
                return UsageMetrics::error_result(
                    "codex",
                    "OpenAI Codex",
                    "未找到 access_token\n請於終端機執行 codex 登入",
                    "",
                );
            }
        };
        let account_id = tokens
            .get("account_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());

        let mut req = self
            .client
            .get(USAGE_URL)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json");

        if let Some(acct) = &account_id {
            req = req.header("ChatGPT-Account-Id", acct);
        }

        match req.send() {
            Ok(resp) => {
                let status = resp.status();
                if status == 401 {
                    let retry = parse_retry_after(resp.headers());
                    return UsageMetrics {
                        provider_id: "codex".to_owned(),
                        provider_name: "OpenAI Codex".to_owned(),
                        metric1_title: "PRIMARY".to_owned(),
                        metric1_text: "--".to_owned(),
                        metric2_title: "SECONDARY".to_owned(),
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
                        provider_id: "codex".to_owned(),
                        provider_name: "OpenAI Codex".to_owned(),
                        metric1_title: "PRIMARY".to_owned(),
                        metric1_text: "--".to_owned(),
                        metric2_title: "SECONDARY".to_owned(),
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
                        "codex",
                        "OpenAI Codex",
                        &format!("API 回應異常: HTTP {}", status.as_u16()),
                        "http",
                    );
                }
                match resp.json::<Value>() {
                    Ok(json) => parse_codex_response(json, &now),
                    Err(_) => UsageMetrics::error_result(
                        "codex",
                        "OpenAI Codex",
                        "未取得有效配額資料",
                        "schema",
                    ),
                }
            }
            Err(e) => {
                error!("[CodexProvider] Request error: {e}");
                UsageMetrics::error_result(
                    "codex",
                    "OpenAI Codex",
                    "配額連線失敗，將自動重試",
                    "network",
                )
            }
        }
    }
}

fn parse_codex_response(data: Value, now_str: &str) -> UsageMetrics {
    let rl = data.get("rate_limit").cloned().unwrap_or(Value::Null);
    let primary = rl.get("primary_window").cloned().unwrap_or(Value::Null);
    let secondary = rl.get("secondary_window").cloned().unwrap_or(Value::Null);

    let s_used_pct = percentage(primary.get("used_percent").and_then(|v| v.as_f64()), 100.0);
    let s_reset_dt = parse_timestamp(primary.get("reset_at"));

    let w_used_pct = percentage(
        secondary.get("used_percent").and_then(|v| v.as_f64()),
        100.0,
    );
    let w_reset_dt = parse_timestamp(secondary.get("reset_at"));

    let plan = data.get("plan_type").and_then(|v| v.as_str()).unwrap_or("");
    let plan_badge = if !plan.is_empty() {
        let mut c = plan.chars();
        format!(
            "Plan: {}",
            c.next()
                .map(|ch| ch.to_uppercase().to_string())
                .unwrap_or_default()
                + c.as_str()
        )
    } else {
        String::new()
    };

    UsageMetrics {
        provider_id: "codex".to_owned(),
        provider_name: "OpenAI Codex".to_owned(),
        metric1_title: window_title(&primary, "PRIMARY"),
        metric1_val: s_used_pct,
        metric1_text: percent_text(s_used_pct),
        metric1_reset: s_reset_dt,
        metric2_title: window_title(&secondary, "SECONDARY"),
        metric2_val: w_used_pct,
        metric2_text: percent_text(w_used_pct),
        metric2_reset: w_reset_dt,
        badge1_text: plan_badge,
        last_updated_time: now_str.to_owned(),
        error: if s_used_pct.is_none() && w_used_pct.is_none() {
            Some("未取得有效配額資料".to_owned())
        } else {
            None
        },
        error_code: if s_used_pct.is_none() && w_used_pct.is_none() {
            "schema".to_owned()
        } else {
            String::new()
        },
        ..Default::default()
    }
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
    fn test_parse_codex_response() {
        let json_data = serde_json::json!({
            "rate_limit": {
                "primary_window": {
                    "used_percent": 15.0,
                    "reset_at": 1893456000
                },
                "secondary_window": {
                    "used_percent": 82.5,
                    "reset_at": 1893542400
                }
            },
            "plan_type": "pro"
        });

        let metrics = parse_codex_response(json_data, "10:00:00");
        assert_eq!(metrics.provider_id, "codex");
        assert_eq!(metrics.metric1_val, Some(15.0));
        assert_eq!(metrics.metric1_text, "15%");
        assert_eq!(metrics.metric2_val, Some(82.5));
        assert_eq!(metrics.metric2_text, "82%");
        assert_eq!(metrics.badge1_text, "Plan: Pro");
        assert!(metrics.error.is_none());
    }
}
