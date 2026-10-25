// src/providers/base.rs — UsageMetrics data contract + Provider trait
//
// Mirrors Python core/providers/base.py:
//   - UsageMetrics holds metric1/metric2 (0-100 percent or None), badges, error, stale flag.
//   - Provider is the fetch_usage() trait.
//   - percentage() validation: finite, 0..=100.
//   - format_countdown() converts a future DateTime to a human-readable string.

use chrono::{DateTime, Local, Utc};

/// A validated percentage in [0.0, 100.0].  Returns None for invalid/unavailable values.
/// Allows a 1e-6 epsilon for floating-point calculation inaccuracies and clamps to [0.0, max].
pub fn percentage(value: Option<f64>, max: f64) -> Option<f64> {
    let v = value?;
    if v.is_finite() && v >= -1e-6 && v <= (max + 1e-6) {
        Some(v.clamp(0.0, max))
    } else {
        None
    }
}

/// Python `retry_delay`: `Retry-After` as non-negative finite seconds or an HTTP date.
pub fn retry_delay(value: &str) -> Option<f64> {
    if value.is_empty() {
        return None;
    }
    if let Ok(seconds) = value.trim().parse::<f64>() {
        return (seconds.is_finite() && seconds >= 0.0).then_some(seconds);
    }
    let target = DateTime::parse_from_rfc2822(value.trim()).ok()?;
    let ms = target
        .with_timezone(&Utc)
        .signed_duration_since(Utc::now())
        .num_milliseconds();
    Some((ms as f64 / 1000.0).max(0.0))
}

/// Python `data.get(key) or {}` followed by `.get(..)`: a missing/null/falsy value is an empty
/// object (`Some(Null)`), an object is itself, anything else is a schema error (`None`).
pub fn object_field(data: &serde_json::Value, key: &str) -> Option<serde_json::Value> {
    use serde_json::Value;
    match data.get(key) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Some(Value::Null),
        Some(Value::Object(_)) => data.get(key).cloned(),
        Some(Value::String(s)) if s.is_empty() => Some(Value::Null),
        Some(Value::Array(a)) if a.is_empty() => Some(Value::Null),
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => Some(Value::Null),
        _ => None,
    }
}

/// The shared `配額資料格式不相容` schema failure of Python `safe_parse`.
pub fn format_error(provider_id: &str, provider_name: &str, now_str: &str) -> UsageMetrics {
    UsageMetrics {
        provider_id: provider_id.to_owned(),
        provider_name: provider_name.to_owned(),
        last_updated_time: now_str.to_owned(),
        error: Some("配額資料格式不相容".to_owned()),
        error_code: "schema".to_owned(),
        ..Default::default()
    }
}

/// Format a float percentage as "72%" or "--" if None.
pub fn percent_text(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{:.0}%", v),
        None => "--".to_owned(),
    }
}

/// One extra quota row (Python `MetricRow`): agy's five buckets beyond the main pair.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricRow {
    pub title: String,
    pub val: Option<f64>,
    pub text: String,
    pub reset: Option<DateTime<Utc>>,
    pub subtext: String,
}

impl MetricRow {
    pub fn new(title: &str, val: Option<f64>, text: &str) -> Self {
        Self {
            title: title.to_owned(),
            val,
            text: text.to_owned(),
            reset: None,
            subtext: String::new(),
        }
    }
}

impl Default for MetricRow {
    fn default() -> Self {
        Self::new("", None, "--")
    }
}

/// Core data contract shared by all providers.
/// `metric1_val` / `metric2_val` are Some(0.0..=100.0) or None (shown as "--").
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UsageMetrics {
    pub provider_id: String,
    pub provider_name: String,

    pub metric1_title: String,
    pub metric1_val: Option<f64>,
    pub metric1_text: String,
    pub metric1_reset: Option<DateTime<Utc>>,
    pub metric1_subtext: String,

    pub metric2_title: String,
    pub metric2_val: Option<f64>,
    pub metric2_text: String,
    pub metric2_reset: Option<DateTime<Utc>>,
    pub metric2_subtext: String,

    pub badge1_text: String,
    pub badge2_text: String,

    pub last_updated_time: String,
    pub error: Option<String>,
    pub error_code: String,
    pub retry_after: Option<f64>,
    pub stale: bool,
    pub last_success: Option<DateTime<Utc>>,
    /// Rows beyond the main pair (agy's other quota buckets); empty for providers with two rows.
    pub extra_metrics: Vec<MetricRow>,
}
impl Default for UsageMetrics {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            provider_name: String::new(),
            metric1_title: "SESSION 5H".to_string(),
            metric1_val: None,
            metric1_text: "--".to_string(),
            metric1_reset: None,
            metric1_subtext: String::new(),
            metric2_title: "WEEKLY 7D".to_string(),
            metric2_val: None,
            metric2_text: "--".to_string(),
            metric2_reset: None,
            metric2_subtext: String::new(),
            badge1_text: String::new(),
            badge2_text: String::new(),
            last_updated_time: String::new(),
            error: None,
            error_code: String::new(),
            retry_after: None,
            stale: false,
            last_success: None,
            extra_metrics: Vec::new(),
        }
    }
}

impl UsageMetrics {
    pub fn error_result(
        provider_id: &str,
        provider_name: &str,
        error: &str,
        error_code: &str,
    ) -> Self {
        Self {
            provider_id: provider_id.to_owned(),
            provider_name: provider_name.to_owned(),
            metric1_title: "SESSION 5H".to_owned(),
            metric2_title: "WEEKLY 7D".to_owned(),
            metric1_text: "--".to_owned(),
            metric2_text: "--".to_owned(),
            last_updated_time: now_str(),
            error: Some(error.to_owned()),
            error_code: error_code.to_owned(),
            ..Default::default()
        }
    }
}

pub fn now_str() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

/// Format a future UTC DateTime into a countdown string.
/// Returns "即將重設" when expired, "--" when None.
pub fn format_countdown(target: Option<DateTime<Utc>>) -> String {
    let Some(t) = target else {
        return "--".to_owned();
    };
    let now = Utc::now();
    let diff = t.signed_duration_since(now);
    let total_secs = diff.num_seconds();

    if total_secs <= 0 {
        return "即將重設".to_owned();
    }

    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if days > 0 {
        format!("{}天 {}時 {}分", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {:02}m {:02}s", hours, mins, secs)
    } else {
        format!("{}m {:02}s", mins, secs)
    }
}

/// Provider trait — one implementation per AI service.
#[allow(dead_code)]
pub trait Provider {
    fn provider_id(&self) -> &str;
    fn display_name(&self) -> &str;
    /// Blocking fetch; called from a background thread.
    fn fetch_usage(&self) -> UsageMetrics;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_percentage_valid_and_invalid() {
        assert_eq!(percentage(Some(50.0), 100.0), Some(50.0));
        assert_eq!(percentage(Some(0.0), 100.0), Some(0.0));
        assert_eq!(percentage(Some(100.0), 100.0), Some(100.0));
        assert_eq!(percentage(Some(1.0000000000000002), 1.0), Some(1.0));
        assert_eq!(percentage(Some(-0.0000001), 1.0), Some(0.0));
        assert_eq!(percentage(Some(-1.0), 100.0), None);
        assert_eq!(percentage(Some(105.0), 100.0), None);
        assert_eq!(percentage(Some(f64::NAN), 100.0), None);
        assert_eq!(percentage(Some(f64::INFINITY), 100.0), None);
        assert_eq!(percentage(None, 100.0), None);
    }

    #[test]
    fn test_retry_delay() {
        assert_eq!(retry_delay("30"), Some(30.0));
        assert_eq!(retry_delay("0"), Some(0.0));
        assert_eq!(retry_delay(""), None);
        assert_eq!(retry_delay("-1"), None);
        assert_eq!(retry_delay("nan"), None);
        assert_eq!(retry_delay("inf"), None);
        assert_eq!(retry_delay("garbage"), None);
        assert_eq!(retry_delay("Wed, 21 Oct 2015 07:28:00 GMT"), Some(0.0));
        let future = (Utc::now() + chrono::Duration::seconds(120)).to_rfc2822();
        let d = retry_delay(&future).unwrap();
        assert!(d > 100.0 && d <= 120.0, "{d}");
    }

    #[test]
    fn test_object_field_shapes() {
        let v = serde_json::json!({"a": {"x": 1}, "b": null, "c": "text", "d": [], "e": 5});
        assert!(object_field(&v, "a").unwrap().is_object());
        assert!(object_field(&v, "b").unwrap().is_null());
        assert!(object_field(&v, "missing").unwrap().is_null());
        assert!(object_field(&v, "d").unwrap().is_null());
        assert!(object_field(&v, "c").is_none());
        assert!(object_field(&v, "e").is_none());
    }

    #[test]
    fn test_percent_text() {
        assert_eq!(percent_text(Some(42.6)), "43%");
        assert_eq!(percent_text(Some(0.0)), "0%");
        assert_eq!(percent_text(None), "--");
    }

    #[test]
    fn test_format_countdown() {
        assert_eq!(format_countdown(None), "--");

        // Expired target
        let past = Utc::now() - Duration::seconds(10);
        assert_eq!(format_countdown(Some(past)), "即將重設");

        // Future target (1 day, 2 hours, 3 mins)
        let future_days =
            Utc::now() + Duration::days(1) + Duration::hours(2) + Duration::minutes(3);
        let res_days = format_countdown(Some(future_days));
        assert!(res_days.contains("天") && res_days.contains("時"));

        // Future target (2 hours, 30 mins)
        let future_hours = Utc::now() + Duration::hours(2) + Duration::minutes(30);
        let res_hours = format_countdown(Some(future_hours));
        assert!(res_hours.contains("2h") && res_hours.contains("m"));

        // Future target (45 mins)
        let future_mins = Utc::now() + Duration::minutes(45);
        let res_mins = format_countdown(Some(future_mins));
        assert!(res_mins.contains("45m") || res_mins.contains("44m"));
    }
}
