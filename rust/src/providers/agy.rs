// src/providers/agy.rs — Antigravity usage provider
//
// Mirrors Python core/providers/agy_provider.py: two CloudCode quota endpoints (the summary of the
// Claude/GPT buckets and the per-model Gemini buckets), completed by the quota cache that agy's
// status line hook writes (`agy_statusline`). There is no CLI fallback.

use super::base::{now_str, percent_text, percentage, MetricRow, Provider, UsageMetrics};
use crate::agy_statusline::{self, QuotaCache};
use chrono::{DateTime, Utc};
use log::{debug, info};
use serde_json::Value;
use std::time::Duration;
use ureq::OrAnyStatus;

const SUMMARY_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";
const QUOTA_URL: &str = "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";
const USER_AGENT: &str = "antigravity/1.2.4";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);

const AUTH_MESSAGE: &str = "登入憑證已失效，請使用原 CLI 重新登入";
const NO_DATA_MESSAGE: &str = "未取得有效配額資料";
const STATUSLINE_HINT: &str = "需啟用 agy statusLine";
const FORMAT_MESSAGE: &str = "配額資料格式不相容";

#[cfg(target_os = "windows")]
mod os_cred {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    struct CREDENTIALW {
        flags: u32,
        r#type: u32,
        target_name: *mut u16,
        comment: *mut u16,
        last_written: [u32; 2],
        credential_blob_size: u32,
        credential_blob: *mut u8,
        persist: u32,
        attribute_count: u32,
        attributes: *mut std::ffi::c_void,
        target_alias: *mut u16,
        user_name: *mut u16,
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn CredReadW(
            target_name: *const u16,
            r#type: u32,
            flags: u32,
            credential: *mut *mut CREDENTIALW,
        ) -> i32;
        fn CredFree(buffer: *mut std::ffi::c_void);
    }

    /// The access token in the Credential Manager entry `gemini:antigravity`.
    pub fn get_gemini_token() -> Option<String> {
        let target: Vec<u16> = OsStr::new("gemini:antigravity\0").encode_wide().collect();
        let mut cred_ptr: *mut CREDENTIALW = std::ptr::null_mut();
        // CRED_TYPE_GENERIC = 1
        let res = unsafe { CredReadW(target.as_ptr(), 1, 0, &mut cred_ptr) };
        if res == 0 || cred_ptr.is_null() {
            return None;
        }

        let slice = unsafe {
            std::slice::from_raw_parts(
                (*cred_ptr).credential_blob,
                (*cred_ptr).credential_blob_size as usize,
            )
        };
        let text = String::from_utf8_lossy(slice).to_string();
        unsafe {
            CredFree(cred_ptr as *mut std::ffi::c_void);
        }
        super::token_of(&serde_json::from_str(&text).ok()?)
    }
}

#[cfg(target_os = "macos")]
mod os_cred {
    use std::process::Command;

    /// The access token in the Keychain item `gemini:antigravity`.
    pub fn get_gemini_token() -> Option<String> {
        let output = Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "gemini",
                "-a",
                "antigravity",
                "-w",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        super::token_of(&serde_json::from_str(&text).ok()?)
    }
}

/// Linux has no credential store lookup in the Python HUD; only the token file is read.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod os_cred {
    pub fn get_gemini_token() -> Option<String> {
        None
    }
}

/// `data["token"]["access_token"]`.
fn token_of(data: &Value) -> Option<String> {
    data.get("token")?
        .get("access_token")?
        .as_str()
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
}

/// Python `_get_access_token`: the OS credential store first (Windows, macOS), then
/// `~/.gemini/antigravity-cli/antigravity-oauth-token`.
fn get_access_token() -> Option<String> {
    if let Some(token) = os_cred::get_gemini_token() {
        return Some(token);
    }
    let path = agy_statusline::home_dir()
        .join(".gemini")
        .join("antigravity-cli")
        .join("antigravity-oauth-token");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return None,
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(data) => token_of(&data),
        Err(e) => {
            debug!("[AgyProvider] Token file read failed: {e}");
            None
        }
    }
}

/// One POST of an empty JSON object: the HTTP status and the body, or why no answer came.
type PostFn = Box<dyn Fn(&str, &str) -> Result<(u16, String), String> + Send + Sync>;
type TokenFn = Box<dyn Fn() -> Option<String> + Send + Sync>;
type CacheFn = Box<dyn Fn() -> Option<QuotaCache> + Send + Sync>;

pub struct AgyProvider {
    post: PostFn,
    token: TokenFn,
    cache: CacheFn,
}

impl AgyProvider {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new().timeout(HTTP_TIMEOUT).build();
        Self::from_parts(
            Box::new(move |url, token| {
                let reply = agent
                    .post(url)
                    .set("Authorization", &format!("Bearer {token}"))
                    .set("Content-Type", "application/json")
                    .set("User-Agent", USER_AGENT)
                    .send_string("{}")
                    .or_any_status()
                    .map_err(|e| e.to_string())?;
                let status = reply.status();
                // An error status carries a body the caller does not read.
                let body = if status == 200 {
                    reply.into_string().map_err(|e| e.to_string())?
                } else {
                    String::new()
                };
                Ok((status, body))
            }),
            Box::new(get_access_token),
            Box::new(agy_statusline::read_default_quota_cache),
        )
    }

    /// The provider with its three outside sources given: the POST, the token and the cache.
    fn from_parts(post: PostFn, token: TokenFn, cache: CacheFn) -> Self {
        Self { post, token, cache }
    }

    /// Python `_fetch_via_http`: the summary endpoint, then the quota endpoint. A 401 from either
    /// is an auth failure; any other failure of one endpoint leaves its part of the result empty.
    fn fetch_via_http(&self, token: &str, now: &str) -> UsageMetrics {
        let mut summary: Option<UsageMetrics> = None;
        match self.request(SUMMARY_URL, token) {
            Reply::Auth => return failure(now, "auth", AUTH_MESSAGE),
            Reply::Body(body) => {
                let parsed = parse_agy_json(&body, now);
                if parsed.error.is_none() {
                    summary = Some(parsed);
                } else {
                    debug!("[AgyProvider] {SUMMARY_URL} returned no usable quota data");
                }
            }
            Reply::Failed => {}
        }

        let mut quota: Option<UsageMetrics> = None;
        match self.request(QUOTA_URL, token) {
            Reply::Auth => return failure(now, "auth", AUTH_MESSAGE),
            Reply::Body(body) => {
                let parsed = parse_model_buckets(&body, now);
                if parsed.error.is_none() {
                    quota = Some(parsed);
                } else {
                    debug!("[AgyProvider] {QUOTA_URL} returned no usable quota data");
                }
            }
            Reply::Failed => {}
        }

        let m1_val = quota.as_ref().and_then(|q| q.metric1_val);
        let m1_reset = quota.as_ref().and_then(|q| q.metric1_reset);
        let badge1 = summary
            .as_ref()
            .filter(|s| !s.badge1_text.is_empty())
            .map_or_else(
                || format!("C/G 剩餘: {}", percent_text(None)),
                |s| s.badge1_text.clone(),
            );
        let rows = summary.map_or_else(empty_rows, |s| s.extra_metrics);
        let has_cg = rows.iter().take(2).any(|row| row.val.is_some());
        let has_error = m1_val.is_none() && !has_cg;

        UsageMetrics {
            provider_name: "Antigravity".to_owned(),
            provider_id: "agy".to_owned(),
            metric1_title: "GEMINI 5H".to_owned(),
            metric1_val: m1_val,
            metric1_text: percent_text(m1_val),
            metric1_reset: m1_reset,
            metric2_title: "GEMINI 7D".to_owned(),
            metric2_val: None,
            metric2_text: "--".to_owned(),
            metric2_reset: None,
            metric2_subtext: "Gemini 週額度來源未知".to_owned(),
            badge1_text: badge1,
            badge2_text: "Gemini Models".to_owned(),
            last_updated_time: now.to_owned(),
            error: has_error.then(|| NO_DATA_MESSAGE.to_owned()),
            error_code: if has_error { "schema" } else { "" }.to_owned(),
            extra_metrics: if has_error { Vec::new() } else { rows },
            ..Default::default()
        }
    }

    /// One endpoint: its parsed JSON body on HTTP 200, `Auth` on 401, `Failed` on anything else
    /// (another status, no connection, a body that is not JSON).
    fn request(&self, url: &str, token: &str) -> Reply {
        match (self.post)(url, token) {
            Ok((401, _)) => Reply::Auth,
            Ok((200, body)) => match serde_json::from_str::<Value>(&body) {
                Ok(value) => Reply::Body(value),
                Err(e) => {
                    debug!("[AgyProvider] {url} HTTP fetch error: {e}");
                    Reply::Failed
                }
            },
            Ok((status, _)) => {
                debug!("[AgyProvider] {url} returned HTTP {status}");
                Reply::Failed
            }
            Err(e) => {
                debug!("[AgyProvider] {url} HTTP fetch error: {e}");
                Reply::Failed
            }
        }
    }

    /// Python `_merge_cache`: fills what the endpoints did not report from the status line cache.
    fn merge_cache(
        &self,
        res: Option<UsageMetrics>,
        cache: &QuotaCache,
        now: &str,
    ) -> UsageMetrics {
        let had_response = res.is_some();
        let mut result = res.unwrap_or_default();
        let buckets = &cache.buckets;
        let used = |fraction: f64| ((1.0 - fraction) * 100.0).clamp(0.0, 100.0);

        let mut rows = std::mem::take(&mut result.extra_metrics);
        if rows.is_empty() {
            rows = empty_rows();
        }
        for (row, key) in rows
            .iter_mut()
            .zip(["3p-5h", "3p-weekly", "gemini-argon-wtu-weekly"])
        {
            if let (None, Some(bucket)) = (row.val, buckets.get(key)) {
                let val = used(bucket.remaining_fraction);
                row.val = Some(val);
                row.text = percent_text(Some(val));
                row.reset = bucket.reset_time;
                row.subtext = String::new();
            }
        }

        if result.metric1_val.is_none() {
            if let Some(bucket) = buckets.get("gemini-5h") {
                result.metric1_val = Some(used(bucket.remaining_fraction));
                result.metric1_reset = bucket.reset_time;
            }
        }
        let weekly = buckets.get("gemini-weekly");
        result.metric2_val = weekly.map(|b| used(b.remaining_fraction));
        result.metric2_reset = weekly.and_then(|b| b.reset_time);
        result.metric2_subtext = if weekly.is_some() {
            String::new()
        } else {
            STATUSLINE_HINT.to_owned()
        };
        if !had_response
            || result.badge1_text.is_empty()
            || result.badge1_text == format!("C/G 剩餘: {}", percent_text(None))
        {
            let remaining = buckets
                .get("3p-weekly")
                .map(|b| b.remaining_fraction * 100.0);
            result.badge1_text = format!("C/G 剩餘: {}", percent_text(remaining));
        }
        let argon = buckets
            .get("gemini-argon-wtu-weekly")
            .map(|b| b.remaining_fraction * 100.0);
        result.badge2_text = format!("Argon 剩餘: {}", percent_text(argon));
        result.metric1_title = "GEMINI 5H".to_owned();
        result.metric2_title = "GEMINI 7D".to_owned();
        result.metric1_text = percent_text(result.metric1_val);
        result.metric2_text = percent_text(result.metric2_val);
        result.provider_name = "Antigravity".to_owned();
        result.provider_id = "agy".to_owned();
        result.last_updated_time = now.to_owned();
        let has_value = result.metric1_val.is_some()
            || result.metric2_val.is_some()
            || rows.iter().any(|row| row.val.is_some());
        result.error = (!has_value).then(|| NO_DATA_MESSAGE.to_owned());
        result.error_code = if has_value { "" } else { "schema" }.to_owned();
        result.extra_metrics = if has_value { rows } else { Vec::new() };
        result
    }
}

enum Reply {
    Body(Value),
    Auth,
    Failed,
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
        let cache = (self.cache)();
        let token = (self.token)();
        let res = token.as_deref().map(|token| self.fetch_via_http(token, &now));
        let Some(cache) = cache else {
            let mut res = res.unwrap_or_else(|| {
                failure(&now, "no_token", "未找到 Antigravity 登入憑證\n請先登入 Antigravity")
            });
            res.badge2_text = "Argon: --".to_owned();
            return res;
        };
        debug!("[AgyProvider] completing the quota with the statusLine cache");
        self.merge_cache(res, &cache, &now)
    }
}

static NO_ITEMS: Vec<Value> = Vec::new();

/// Python `_empty_rows`.
fn empty_rows() -> Vec<MetricRow> {
    let mut argon = MetricRow::new("ARGON 7D", None, "--");
    argon.subtext = STATUSLINE_HINT.to_owned();
    vec![
        MetricRow::new("C/G 5H", None, "--"),
        MetricRow::new("C/G 7D", None, "--"),
        argon,
    ]
}

/// Python `_failure`.
fn failure(now: &str, code: &str, message: &str) -> UsageMetrics {
    UsageMetrics {
        provider_name: "Antigravity".to_owned(),
        provider_id: "agy".to_owned(),
        last_updated_time: now.to_owned(),
        error: Some(message.to_owned()),
        error_code: code.to_owned(),
        ..Default::default()
    }
}

/// The result `safe_parse` gives for a body that does not have the expected shape.
fn format_error(now: &str) -> UsageMetrics {
    failure(now, "schema", FORMAT_MESSAGE)
}

/// The text of the first non-empty field among `keys` (Python `a.get(x) or a.get(y) or ""`);
/// `Err` when that value is not a string.
fn text_of(object: &Value, keys: &[&str]) -> Result<String, ()> {
    for key in keys {
        match object.get(*key) {
            None | Some(Value::Null) => {}
            Some(Value::String(s)) if s.is_empty() => {}
            Some(Value::String(s)) => return Ok(s.clone()),
            Some(Value::Bool(false)) => {}
            Some(Value::Number(n)) if n.as_f64() == Some(0.0) => {}
            Some(Value::Array(a)) if a.is_empty() => {}
            Some(Value::Object(o)) if o.is_empty() => {}
            Some(_) => return Err(()),
        }
    }
    Ok(String::new())
}

/// The fraction left in a bucket (`remaining_fraction` or `remainingFraction`), in 0..=1.
fn remaining_fraction(bucket: &Value) -> Option<f64> {
    let value = if bucket.get("remaining_fraction").is_some() {
        bucket.get("remaining_fraction")
    } else {
        bucket.get("remainingFraction")
    };
    percentage(value.and_then(Value::as_f64), 1.0)
}

/// The reset time of a bucket (`reset_time` or `resetTime`).
fn reset_time(bucket: &Value) -> Option<DateTime<Utc>> {
    let value = bucket
        .get("reset_time")
        .filter(|v| !is_falsy(v))
        .or_else(|| bucket.get("resetTime"))?;
    agy_statusline::parse_utc(&Value::String(value.as_str()?.replace('Z', "+00:00")))
}

fn is_falsy(value: &Value) -> bool {
    matches!(value, Value::Null | Value::Bool(false))
        || value.as_str().is_some_and(str::is_empty)
}

/// Python `_parse_model_buckets`: the most used Gemini bucket of `retrieveUserQuota` is `GEMINI 5H`.
fn parse_model_buckets(raw: &Value, now: &str) -> UsageMetrics {
    if !raw.is_object() {
        return format_error(now);
    }
    let buckets = match raw.get("buckets") {
        None => &NO_ITEMS,
        Some(Value::Array(list)) => list,
        Some(_) => return format_error(now),
    };

    let mut m1_used: Option<f64> = None;
    let mut m1_reset: Option<DateTime<Utc>> = None;
    for bucket in buckets {
        if !bucket.is_object() {
            return format_error(now);
        }
        let Ok(model_id) = text_of(bucket, &["modelId", "model_id"]) else {
            return format_error(now);
        };
        if !model_id.to_lowercase().contains("gemini") {
            continue;
        }
        let Some(remaining) = remaining_fraction(bucket) else {
            continue;
        };
        let used = ((1.0 - remaining) * 100.0).clamp(0.0, 100.0);
        if m1_used.is_none_or(|current| used > current) {
            m1_used = Some(used);
            m1_reset = reset_time(bucket);
        }
    }

    let missing = m1_used.is_none();
    UsageMetrics {
        provider_name: "Antigravity".to_owned(),
        provider_id: "agy".to_owned(),
        metric1_title: "GEMINI 5H".to_owned(),
        metric1_val: m1_used,
        metric1_text: percent_text(m1_used),
        metric1_reset: m1_reset,
        metric2_title: "GEMINI 7D".to_owned(),
        metric2_val: None,
        metric2_text: "--".to_owned(),
        metric2_reset: None,
        metric2_subtext: "Gemini 週額度來源未知".to_owned(),
        badge2_text: "Gemini Models".to_owned(),
        last_updated_time: now.to_owned(),
        error: missing.then(|| NO_DATA_MESSAGE.to_owned()),
        error_code: if missing { "schema" } else { "" }.to_owned(),
        extra_metrics: if missing { Vec::new() } else { empty_rows() },
        ..Default::default()
    }
}

/// Python `_parse_agy_json`: the groups of `retrieveUserQuotaSummary`. Gemini buckets give the main
/// pair, Claude/GPT buckets give `C/G 5H` and `C/G 7D`.
fn parse_agy_json(raw: &Value, now: &str) -> UsageMetrics {
    if !raw.is_object() {
        return format_error(now);
    }
    let from_command = raw.pointer("/command/data/groups");
    let groups = match from_command {
        Some(Value::Array(list)) if !list.is_empty() => Some(list),
        _ => match raw.get("groups") {
            Some(Value::Array(list)) => Some(list),
            Some(Value::Null) | None => None,
            Some(_) => return format_error(now),
        },
    };
    let Some(groups) = groups.filter(|g| !g.is_empty()) else {
        return UsageMetrics {
            provider_name: "Antigravity".to_owned(),
            provider_id: "agy".to_owned(),
            metric1_val: None,
            metric1_text: "--".to_owned(),
            metric2_val: None,
            metric2_text: "--".to_owned(),
            metric2_subtext: "Gemini 週額度來源未知".to_owned(),
            last_updated_time: now.to_owned(),
            error: Some(NO_DATA_MESSAGE.to_owned()),
            error_code: "schema".to_owned(),
            ..Default::default()
        };
    };

    let mut m1_used: Option<f64> = None;
    let mut m1_reset: Option<DateTime<Utc>> = None;
    let mut m2_used: Option<f64> = None;
    let mut m2_reset: Option<DateTime<Utc>> = None;
    let mut third_party_remaining: Option<f64> = None;
    let mut rows = empty_rows();

    for group in groups {
        if !group.is_object() {
            return format_error(now);
        }
        let Ok(name) = text_of(group, &["name", "displayName"]) else {
            return format_error(now);
        };
        let lower = name.to_lowercase();
        let buckets_of = || match group.get("buckets") {
            None => Some(&NO_ITEMS),
            Some(Value::Array(list)) => Some(list),
            Some(_) => None,
        };
        if lower.contains("gemini") && !lower.contains("argon") {
            let Some(buckets) = buckets_of() else {
                return format_error(now);
            };
            for bucket in buckets {
                if !bucket.is_object() {
                    return format_error(now);
                }
                let (Ok(id), Ok(window)) = (
                    text_of(bucket, &["id", "bucketId"]),
                    text_of(bucket, &["window"]),
                ) else {
                    return format_error(now);
                };
                let (id, window) = (id.to_lowercase(), window.to_lowercase());
                let Some(remaining) = remaining_fraction(bucket) else {
                    continue;
                };
                let used = ((1.0 - remaining) * 100.0).clamp(0.0, 100.0);
                let reset = reset_time(bucket);
                if (id.contains("5h") || window.contains("5h"))
                    && m1_used.is_none_or(|current| used > current)
                {
                    m1_used = Some(used);
                    m1_reset = reset;
                } else if (id.contains("week") || window.contains("week"))
                    && m2_used.is_none_or(|current| used > current)
                {
                    m2_used = Some(used);
                    m2_reset = reset;
                }
            }
        } else if (lower.contains("claude") || lower.contains("gpt")) && !lower.contains("argon") {
            let Some(buckets) = buckets_of() else {
                return format_error(now);
            };
            for bucket in buckets {
                if !bucket.is_object() {
                    return format_error(now);
                }
                let Ok(id) = text_of(bucket, &["id", "bucketId"]) else {
                    return format_error(now);
                };
                let id = id.to_lowercase();
                let index = if id.contains("week") {
                    1
                } else if id.contains("5h") {
                    0
                } else {
                    continue;
                };
                let Some(remaining) = remaining_fraction(bucket) else {
                    continue;
                };
                let used = (1.0 - remaining) * 100.0;
                let row = &mut rows[index];
                if row.val.is_none_or(|current| used > current) {
                    row.val = Some(used);
                    row.text = percent_text(Some(used));
                    row.reset = reset_time(bucket);
                }
                if index == 1 {
                    third_party_remaining = row.val.map(|val| 100.0 - val);
                }
            }
        } else {
            info!("[AgyProvider] unknown quota group: {name}");
        }
    }

    let has_error = m1_used.is_none() && m2_used.is_none() && rows.iter().all(|r| r.val.is_none());
    UsageMetrics {
        provider_name: "Antigravity".to_owned(),
        provider_id: "agy".to_owned(),
        metric1_title: "GEMINI 5H".to_owned(),
        metric1_val: m1_used,
        metric1_text: percent_text(m1_used),
        metric1_reset: m1_reset,
        metric2_title: "GEMINI 7D".to_owned(),
        metric2_val: m2_used,
        metric2_text: percent_text(m2_used),
        metric2_reset: m2_reset,
        metric2_subtext: if m2_used.is_none() {
            "Gemini 週額度來源未知".to_owned()
        } else {
            String::new()
        },
        badge1_text: format!("C/G 剩餘: {}", percent_text(third_party_remaining)),
        badge2_text: "Gemini Models".to_owned(),
        last_updated_time: now.to_owned(),
        error: has_error.then(|| NO_DATA_MESSAGE.to_owned()),
        error_code: if has_error { "schema" } else { "" }.to_owned(),
        extra_metrics: if has_error { Vec::new() } else { rows },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agy_statusline::CacheBucket;
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};

    const SUMMARY_FIXTURE: &str = r#"{"groups":[{"name":"Claude and GPT models","buckets":[
        {"id":"3p-weekly","remainingFraction":0.8449,"resetTime":"2026-10-23T12:00:00Z"},
        {"id":"3p-5h","remainingFraction":0.694,"resetTime":"2026-10-16T14:00:00Z"}]}]}"#;

    const QUOTA_FIXTURE: &str = r#"{"buckets":[
        {"modelId":"gemini-3.8-flash-tiered","remainingFraction":0.9743,"resetTime":"2026-10-16T13:41:00Z","tokenType":"WTUS"},
        {"modelId":"gemini-3.8-flash-low","remainingFraction":0.9743,"resetTime":"2026-10-16T13:41:00Z"},
        {"modelId":"claude-sonnet-5-5-high","remainingFraction":0.694,"resetTime":"2026-10-16T14:00:00Z"}]}"#;

    type Script = VecDeque<Result<(u16, String), String>>;

    /// A provider whose POSTs answer from `script` in order; `requests` records `(url, token)`.
    fn provider(
        script: Vec<Result<(u16, String), String>>,
        token: Option<&str>,
        cache: Option<QuotaCache>,
    ) -> (AgyProvider, Arc<Mutex<Vec<(String, String)>>>) {
        let script: Arc<Mutex<Script>> = Arc::new(Mutex::new(script.into()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        let token = token.map(str::to_owned);
        let provider = AgyProvider::from_parts(
            Box::new(move |url, token| {
                log.lock().unwrap().push((url.to_owned(), token.to_owned()));
                script
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| Err("no more replies".to_owned()))
            }),
            Box::new(move || token.clone()),
            Box::new(move || cache.clone()),
        );
        (provider, requests)
    }

    fn ok(body: &str) -> Result<(u16, String), String> {
        Ok((200, body.to_owned()))
    }

    fn status(code: u16) -> Result<(u16, String), String> {
        Ok((code, String::new()))
    }

    fn full_cache() -> QuotaCache {
        let reset = Utc::now() + chrono::Duration::days(7);
        let fractions = [
            ("gemini-5h", 1.0),
            ("gemini-weekly", 0.9914498),
            ("3p-5h", 1.0),
            ("3p-weekly", 0.8469924),
            ("gemini-argon-wtu-weekly", 0.78630334),
        ];
        QuotaCache {
            written_at: Utc::now(),
            buckets: fractions
                .iter()
                .map(|(key, fraction)| {
                    (
                        (*key).to_owned(),
                        CacheBucket {
                            remaining_fraction: *fraction,
                            reset_time: Some(reset),
                        },
                    )
                })
                .collect(),
        }
    }

    fn near(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("a value");
        assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
    }

    fn iso(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn the_cache_complements_the_endpoints_and_the_endpoints_win() {
        let cache = full_cache();
        let weekly_reset = cache.buckets["gemini-weekly"].reset_time;
        let (p, _) = provider(vec![ok(SUMMARY_FIXTURE), ok(QUOTA_FIXTURE)], Some("token"), Some(cache));
        let r = p.fetch_usage();
        assert_eq!((r.error.as_deref(), r.error_code.as_str()), (None, ""));
        near(r.metric1_val, 2.57);
        near(r.extra_metrics[0].val, 30.6);
        assert_eq!(r.extra_metrics[0].reset, Some(iso("2026-10-16T14:00:00+00:00")));
        near(r.metric2_val, 0.85502);
        assert_eq!(r.metric2_reset, weekly_reset);
        let titles: Vec<_> = r.extra_metrics.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(titles, ["C/G 5H", "C/G 7D", "ARGON 7D"]);
        near(r.extra_metrics[2].val, 21.369666);
        near(r.extra_metrics[1].val, 15.51);
        assert_eq!((r.metric1_title.as_str(), r.metric2_title.as_str()), ("GEMINI 5H", "GEMINI 7D"));
        assert_eq!(r.badge1_text, "C/G 剩餘: 84%");
        assert_eq!(r.badge2_text, "Argon 剩餘: 79%");
    }

    #[test]
    fn the_cache_alone_gives_complete_metrics_without_a_token() {
        let cache = full_cache();
        let reset = cache.buckets["gemini-5h"].reset_time;
        let (p, requests) = provider(vec![], None, Some(cache));
        let r = p.fetch_usage();
        assert!(r.error.is_none() && r.error_code.is_empty());
        assert_eq!(r.metric1_val, Some(0.0));
        assert_eq!(r.metric1_text, "0%");
        near(r.extra_metrics[1].val, 15.30076);
        assert_eq!(r.metric1_reset, reset);
        near(r.metric2_val, 0.85502);
        assert_eq!(r.badge1_text, "C/G 剩餘: 85%");
        assert_eq!(r.badge2_text, "Argon 剩餘: 79%");
        assert!(requests.lock().unwrap().is_empty(), "no token, no request");
    }

    #[test]
    fn the_cache_recovers_when_every_request_fails() {
        let (p, _) = provider(
            vec![Err("offline".into()), Err("offline".into())],
            Some("token"),
            Some(full_cache()),
        );
        let r = p.fetch_usage();
        assert!(r.error.is_none());
        assert_eq!(r.metric1_val, Some(0.0));
        near(r.metric2_val, 0.85502);
        assert_eq!(r.badge1_text, "C/G 剩餘: 85%");
        assert_eq!(r.badge2_text, "Argon 剩餘: 79%");
    }

    #[test]
    fn a_missing_weekly_bucket_asks_for_the_status_line() {
        let mut cache = full_cache();
        cache.buckets.remove("gemini-weekly");
        let (p, _) = provider(vec![], None, Some(cache));
        let r = p.fetch_usage();
        assert_eq!(r.metric2_val, None);
        assert_eq!(r.metric2_text, "--");
        assert_eq!(r.metric2_subtext, "需啟用 agy statusLine");
    }

    #[test]
    fn without_a_cache_or_token_the_failure_is_no_token() {
        let (p, _) = provider(vec![], None, None);
        let r = p.fetch_usage();
        assert_eq!(r.error_code, "no_token");
        assert_eq!(r.badge2_text, "Argon: --");
        assert!(r.extra_metrics.is_empty());
    }

    #[test]
    fn without_a_cache_the_http_auth_error_is_kept() {
        let (p, _) = provider(vec![status(401)], Some("token"), None);
        let r = p.fetch_usage();
        assert_eq!(r.error_code, "auth");
        assert_eq!(r.badge2_text, "Argon: --");
        assert!(r.extra_metrics.is_empty());
    }

    #[test]
    fn a_cache_without_buckets_is_a_schema_error() {
        let empty = QuotaCache {
            written_at: Utc::now(),
            buckets: HashMap::new(),
        };
        let (p, _) = provider(vec![], None, Some(empty));
        let r = p.fetch_usage();
        assert_eq!(r.error_code, "schema");
        assert!(r.extra_metrics.is_empty());
        assert_eq!(r.error.as_deref(), Some("未取得有效配額資料"));
    }

    #[test]
    fn both_endpoints_are_posted_in_order_with_the_token() {
        let (p, requests) = provider(vec![ok(SUMMARY_FIXTURE), ok(QUOTA_FIXTURE)], Some("token"), None);
        let r = p.fetch_usage();
        assert!(r.error.is_none());
        near(r.metric1_val, 2.57);
        near(r.extra_metrics[0].val, 30.6);
        assert_eq!(r.extra_metrics[0].reset, Some(iso("2026-10-16T14:00:00+00:00")));
        assert_eq!(r.metric1_text, "3%");
        assert_eq!(r.badge1_text, "C/G 剩餘: 84%");
        assert_eq!(r.badge2_text, "Argon: --");
        assert_eq!(r.extra_metrics[2].val, None);
        assert_eq!(r.extra_metrics[2].subtext, "需啟用 agy statusLine");
        assert_eq!(r.metric2_val, None);
        assert_eq!(r.metric2_text, "--");
        assert_eq!(r.metric2_subtext, "Gemini 週額度來源未知");
        assert_eq!(
            *requests.lock().unwrap(),
            [
                (SUMMARY_URL.to_owned(), "token".to_owned()),
                (QUOTA_URL.to_owned(), "token".to_owned())
            ]
        );
    }

    #[test]
    fn the_most_constrained_gemini_bucket_is_the_five_hour_row() {
        let quota = r#"{"buckets":[
            {"modelId":"gemini-3.8-flash-low","remainingFraction":0.9,"resetTime":"2026-10-16T13:00:00Z"},
            {"modelId":"gemini-3.8-pro-high","remainingFraction":0.25,"resetTime":"2026-10-16T15:00:00Z"}]}"#;
        let (p, _) = provider(vec![ok(SUMMARY_FIXTURE), ok(quota)], None, None);
        let r = p.fetch_via_http("token", "12:00:00");
        assert!(r.error.is_none());
        near(r.metric1_val, 75.0);
        assert_eq!(r.metric1_text, "75%");
        assert_eq!(r.metric1_reset, Some(iso("2026-10-16T15:00:00+00:00")));
    }

    #[test]
    fn an_unknown_group_is_ignored() {
        let summary = r#"{"groups":[
            {"name":"Claude and GPT models","buckets":[{"id":"3p-weekly","remainingFraction":0.8449,"resetTime":"2026-10-23T12:00:00Z"}]},
            {"name":"Gemini 4 Argon","buckets":[{"id":"argon-5h","remainingFraction":0.1}]}]}"#;
        let (p, _) = provider(vec![ok(summary), ok(QUOTA_FIXTURE)], None, None);
        let r = p.fetch_via_http("token", "12:00:00");
        assert!(r.error.is_none());
        near(r.metric1_val, 2.57);
        assert_eq!(r.extra_metrics[0].val, None);
        assert_eq!(r.badge1_text, "C/G 剩餘: 84%");
    }

    #[test]
    fn a_401_from_the_first_endpoint_stops_at_once() {
        let (p, requests) = provider(vec![status(401)], None, None);
        let r = p.fetch_via_http("token", "12:00:00");
        assert_eq!(r.error_code, "auth");
        assert!(r.error.is_some());
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_failing_quota_endpoint_leaves_a_partial_result() {
        let (p, _) = provider(vec![ok(SUMMARY_FIXTURE), status(503)], None, None);
        let r = p.fetch_via_http("token", "12:00:00");
        assert!(r.error.is_none());
        assert_eq!(r.badge1_text, "C/G 剩餘: 84%");
        assert_eq!(r.metric1_val, None);
        assert_eq!(r.metric1_text, "--");
    }

    #[test]
    fn both_endpoints_failing_is_a_schema_error() {
        let (p, _) = provider(vec![status(503), status(503)], None, None);
        let r = p.fetch_via_http("token", "12:00:00");
        assert!(r.error.is_some());
        assert_eq!(r.error_code, "schema");
        assert!(r.extra_metrics.is_empty());
    }

    #[test]
    fn a_body_of_the_wrong_shape_is_a_format_error_not_a_panic() {
        for body in [
            "[]",
            r#"{"groups":"x"}"#,
            r#"{"groups":[1]}"#,
            r#"{"groups":[{"name":3}]}"#,
            r#"{"groups":[{"name":"gemini","buckets":{"a":1}}]}"#,
            r#"{"groups":[{"name":"gemini","buckets":[7]}]}"#,
        ] {
            let r = parse_agy_json(&serde_json::from_str(body).unwrap(), "12:00:00");
            assert_eq!((r.error_code.as_str(), r.error.as_deref()), ("schema", Some("配額資料格式不相容")), "{body}");
        }
        let r = parse_model_buckets(&serde_json::from_str(r#"{"buckets":{}}"#).unwrap(), "12:00:00");
        assert_eq!(r.error_code, "schema");
    }

    #[test]
    fn an_unknown_group_with_odd_buckets_is_ignored_not_an_error() {
        let summary = r#"{"groups":[
            {"name":"Claude and GPT models","buckets":[{"id":"3p-5h","remainingFraction":0.5}]},
            {"name":"Something new","buckets":"later"}]}"#;
        let r = parse_agy_json(&serde_json::from_str(summary).unwrap(), "12:00:00");
        assert!(r.error.is_none(), "{:?}", r.error);
        near(r.extra_metrics[0].val, 50.0);
    }

    #[test]
    fn groups_under_command_data_are_read_too() {
        let wrapped = format!(r#"{{"command":{{"data":{SUMMARY_FIXTURE}}}}}"#);
        let r = parse_agy_json(&serde_json::from_str(&wrapped).unwrap(), "12:00:00");
        assert!(r.error.is_none());
        near(r.extra_metrics[1].val, 15.51);
    }

    #[test]
    fn gemini_week_buckets_fill_the_weekly_row() {
        let summary = r#"{"groups":[{"name":"Gemini models","buckets":[
            {"id":"g-5h","remainingFraction":0.5},
            {"id":"g-week","remainingFraction":0.75,"resetTime":"2026-10-23T12:00:00Z"}]}]}"#;
        let r = parse_agy_json(&serde_json::from_str(summary).unwrap(), "12:00:00");
        near(r.metric1_val, 50.0);
        near(r.metric2_val, 25.0);
        assert_eq!(r.metric2_subtext, "");
        assert_eq!(r.metric2_reset, Some(iso("2026-10-23T12:00:00+00:00")));
    }

    #[test]
    fn os_cred_token_lookup_does_not_panic() {
        let _ = get_access_token();
    }
}
