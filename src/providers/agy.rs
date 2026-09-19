// src/providers/agy.rs — Antigravity CLI usage provider
//
// Runs `agy --output-format json --print /quota` as a subprocess.
// Parses Gemini 5h / Weekly 7d buckets from the JSON output.
// Mirrors Python core/providers/agy_provider.py

use super::base::{now_str, percent_text, percentage, Provider, UsageMetrics};
use chrono::{DateTime, Utc};
use log::{info, warn};
use serde_json::Value;
use std::process::Command;
use std::time::Instant;

pub struct AgyProvider {
    timeout_secs: u64,
}

impl AgyProvider {
    pub fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    fn find_agy_binary() -> Option<String> {
        // 1. Windows default AppData directly on filesystem (instant, zero process execution)
        #[cfg(target_os = "windows")]
        {
            let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
            for suffix in &[
                "agy\\bin\\agy.exe",
                "agy\\bin\\agy.cmd",
                "agy\\bin\\agy.bat",
            ] {
                let candidate = format!("{}\\{}", local, suffix);
                if std::path::Path::new(&candidate).is_file() {
                    return Some(candidate);
                }
            }
        }

        // 2. Scan PATH directly using filesystem checks (zero process execution, zero console popups)
        if let Some(path_var) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_var) {
                #[cfg(target_os = "windows")]
                for ext in &["exe", "cmd", "bat"] {
                    let cand = dir.join(format!("agy.{}", ext));
                    if cand.is_file() {
                        return Some(cand.to_string_lossy().to_string());
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let cand = dir.join("agy");
                    if cand.is_file() {
                        return Some(cand.to_string_lossy().to_string());
                    }
                }
            }
        }

        // 3. macOS / Linux default paths
        #[cfg(not(target_os = "windows"))]
        {
            let home = std::env::var("HOME").unwrap_or_default();
            for candidate in &[
                format!("{}/.local/bin/agy", home),
                "/usr/local/bin/agy".to_owned(),
                format!("{}/bin/agy", home),
            ] {
                if std::path::Path::new(candidate).is_file() {
                    return Some(candidate.clone());
                }
            }
        }

        None
    }

    fn run_agy(bin: &str, timeout_secs: u64) -> Result<String, String> {
        let started = Instant::now();

        #[cfg(target_os = "windows")]
        let mut cmd = {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            if bin.to_lowercase().ends_with(".cmd") || bin.to_lowercase().ends_with(".bat") {
                let mut c = Command::new("cmd.exe");
                c.creation_flags(CREATE_NO_WINDOW);
                c.args(["/c", bin, "--output-format", "json", "--print", "/quota"]);
                c
            } else {
                let mut c = Command::new(bin);
                c.creation_flags(CREATE_NO_WINDOW);
                c.args(["--output-format", "json", "--print", "/quota"]);
                c
            }
        };

        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = Command::new(bin);
            c.args(["--output-format", "json", "--print", "/quota"]);
            c
        };

        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("無法啟動 agy，請確認安裝與執行權限: {e}"))?;

        // Drain stdout and stderr in background threads to avoid OS pipe buffer deadlock
        let mut stdout_pipe = child.stdout.take().unwrap();
        let mut stderr_pipe = child.stderr.take().unwrap();

        let stdout_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stdout_pipe, &mut buf);
            buf
        });

        let stderr_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stderr_pipe, &mut buf);
            buf
        });

        let timeout = std::time::Duration::from_secs(timeout_secs.max(5));
        let exit_status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = stdout_thread.join();
                        let _ = stderr_thread.join();
                        return Err(format!("agy 執行逾時 (超過 {} 秒)", timeout_secs));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = stdout_thread.join();
                    let _ = stderr_thread.join();
                    return Err(format!("agy 等待錯誤: {e}"));
                }
            }
        };

        let stdout_bytes = stdout_thread.join().unwrap_or_default();
        let stderr_bytes = stderr_thread.join().unwrap_or_default();

        let elapsed = started.elapsed().as_secs_f64();
        info!(
            "quota exit={} elapsed={:.2}s",
            exit_status.code().unwrap_or(-1),
            elapsed
        );

        if !exit_status.success() {
            let stderr_msg = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
            if !stderr_msg.is_empty() {
                return Err(format!(
                    "agy 查詢失敗 (exit {}): {}",
                    exit_status.code().unwrap_or(-1),
                    stderr_msg
                ));
            }
            return Err(format!(
                "agy 查詢失敗 (exit {})",
                exit_status.code().unwrap_or(-1)
            ));
        }

        Ok(String::from_utf8_lossy(&stdout_bytes).to_string())
    }
}

impl Provider for AgyProvider {
    fn provider_id(&self) -> &str {
        "agy"
    }
    fn display_name(&self) -> &str {
        "Antigravity"
    }

    fn fetch_usage(&self) -> UsageMetrics {
        let now = now_str();

        let Some(bin) = Self::find_agy_binary() else {
            warn!("[AgyProvider] Antigravity CLI binary not found");
            return UsageMetrics::error_result(
                "agy",
                "Antigravity",
                "未找到 agy 指令\n請確認已安裝 Antigravity CLI",
                "cli_not_found",
            );
        };

        let stdout = match Self::run_agy(&bin, self.timeout_secs) {
            Ok(s) => s,
            Err(msg) => {
                if msg.contains("exit") {
                    return UsageMetrics::error_result("agy", "Antigravity", &msg, "cli_exit");
                }
                return UsageMetrics::error_result("agy", "Antigravity", &msg, "cli_start");
            }
        };

        // Resilient JSON extraction (handle CLI banners/prefixes)
        let raw: Option<Value> = try_parse_json(&stdout);
        let Some(raw) = raw else {
            return UsageMetrics::error_result(
                "agy",
                "Antigravity",
                "agy 配額格式不相容，請查看相容性文件",
                "schema",
            );
        };

        parse_agy_json(raw, &now)
    }
}

fn try_parse_json(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    // Try direct parse first
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Some(v);
    }
    // Find outermost {...}
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        serde_json::from_str::<Value>(&text[start..=end]).ok()
    } else {
        None
    }
}

fn parse_agy_json(raw: Value, now_str: &str) -> UsageMetrics {
    let groups = raw
        .pointer("/command/data/groups")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut m1_used_pct: Option<f64> = None;
    let mut m1_reset_dt: Option<DateTime<Utc>> = None;
    let mut m2_used_pct: Option<f64> = None;
    let mut m2_reset_dt: Option<DateTime<Utc>> = None;
    let mut third_party_rem_pct: Option<f64> = None;

    for g in &groups {
        let g_name = g
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();

        if g_name.contains("gemini") {
            let buckets = g
                .get("buckets")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for b in &buckets {
                let b_id = b
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                let b_window = b
                    .get("window")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                let rem_frac = percentage(
                    b.get("remaining_fraction")
                        .or_else(|| b.get("remainingFraction"))
                        .and_then(|v| v.as_f64()),
                    1.0,
                );
                let Some(rem_frac) = rem_frac else {
                    continue;
                };
                let used_pct = (1.0 - rem_frac) * 100.0;
                let used_pct = used_pct.clamp(0.0, 100.0);

                let reset_dt: Option<DateTime<Utc>> = b
                    .get("reset_time")
                    .or_else(|| b.get("resetTime"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(&s.replace('Z', "+00:00")).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                if (b_id.contains("5h") || b_window.contains("5h"))
                    && m1_used_pct.is_none_or(|cur| used_pct > cur)
                {
                    m1_used_pct = Some(used_pct);
                    m1_reset_dt = reset_dt;
                } else if (b_id.contains("week") || b_window.contains("week"))
                    && m2_used_pct.is_none_or(|cur| used_pct > cur)
                {
                    m2_used_pct = Some(used_pct);
                    m2_reset_dt = reset_dt;
                }
            }
        } else if g_name.contains("claude") || g_name.contains("gpt") {
            let buckets = g
                .get("buckets")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for b in &buckets {
                let b_id = b
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                if b_id.contains("week") || b_id.contains("third_party") {
                    let rem_frac = percentage(
                        b.get("remaining_fraction")
                            .or_else(|| b.get("remainingFraction"))
                            .and_then(|v| v.as_f64()),
                        1.0,
                    );
                    if let Some(rem_frac) = rem_frac {
                        let remaining = rem_frac * 100.0;
                        third_party_rem_pct = Some(
                            third_party_rem_pct.map_or(remaining, |cur: f64| cur.min(remaining)),
                        );
                    }
                }
            }
        }
    }

    UsageMetrics {
        provider_id: "agy".to_owned(),
        provider_name: "Antigravity".to_owned(),
        metric1_title: "SESSION 5H".to_owned(),
        metric1_val: m1_used_pct,
        metric1_text: percent_text(m1_used_pct),
        metric1_reset: m1_reset_dt,
        metric2_title: "WEEKLY 7D".to_owned(),
        metric2_val: m2_used_pct,
        metric2_text: percent_text(m2_used_pct),
        metric2_reset: m2_reset_dt,
        badge1_text: format!("C/G 剩餘: {}", percent_text(third_party_rem_pct)),
        badge2_text: "Gemini Models".to_owned(),
        last_updated_time: now_str.to_owned(),
        error: if m1_used_pct.is_none() && m2_used_pct.is_none() {
            Some("未取得有效配額資料".to_owned())
        } else {
            None
        },
        error_code: if m1_used_pct.is_none() && m2_used_pct.is_none() {
            "schema".to_owned()
        } else {
            String::new()
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_parse_json() {
        // Direct parse
        let pure = r#"{"hello": "world"}"#;
        let val = try_parse_json(pure).expect("should parse pure json");
        assert_eq!(val["hello"], "world");

        // With CLI headers/footers
        let banner = "Welcome to AGY CLI v2.0\nInfo: fetching quota\n{\"status\": \"ok\", \"count\": 42}\nDone in 0.2s";
        let val2 = try_parse_json(banner).expect("should parse json inside banner");
        assert_eq!(val2["status"], "ok");
        assert_eq!(val2["count"], 42);

        // Invalid
        assert!(try_parse_json("not json at all").is_none());
        assert!(try_parse_json("").is_none());
    }

    #[test]
    fn test_parse_agy_json() {
        let json_str = r#"{
            "command": {
                "data": {
                    "groups": [
                        {
                            "name": "Gemini Models",
                            "buckets": [
                                {
                                    "id": "session",
                                    "window": "5h",
                                    "remainingFraction": 0.8,
                                    "resetTime": "2030-01-01T00:00:00Z"
                                },
                                {
                                    "id": "weekly",
                                    "window": "7d",
                                    "remainingFraction": 0.45,
                                    "resetTime": "2030-01-07T00:00:00Z"
                                }
                            ]
                        },
                        {
                            "name": "Claude / 3rd Party Models",
                            "buckets": [
                                {
                                    "id": "third_party",
                                    "remainingFraction": 0.92
                                }
                            ]
                        }
                    ]
                }
            }
        }"#;

        let val: Value = serde_json::from_str(json_str).unwrap();
        let m = parse_agy_json(val, "12:00:00");

        assert_eq!(m.provider_id, "agy");
        assert_eq!(m.provider_name, "Antigravity");
        // Used = (1.0 - 0.8) * 100 = 20.0%
        assert!((m.metric1_val.unwrap() - 20.0).abs() < 0.1);
        assert_eq!(m.metric1_text, "20%");
        // Weekly used = (1.0 - 0.45) * 100 = 55.0%
        assert!((m.metric2_val.unwrap() - 55.0).abs() < 0.1);
        assert_eq!(m.metric2_text, "55%");
        // Badge has 92%
        assert!(m.badge1_text.contains("92%"));
        assert!(m.error.is_none());
    }
}
