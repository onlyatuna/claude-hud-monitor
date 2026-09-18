// src/providers/base.rs — UsageMetrics data contract + Provider trait
//
// Mirrors Python core/providers/base.py:
//   - UsageMetrics holds metric1/metric2 (0-100 percent or None), badges, error, stale flag.
//   - Provider is the fetch_usage() trait.
//   - percentage() validation: finite, 0..=100.
//   - format_countdown() converts a future DateTime to a human-readable string.

use chrono::{DateTime, Local, Utc};

/// A validated percentage in [0.0, 100.0].  Returns None for invalid/unavailable values.
pub fn percentage(value: Option<f64>, max: f64) -> Option<f64> {
    let v = value?;
    if v.is_finite() && v >= 0.0 && v <= max {
        Some(v)
    } else {
        None
    }
}

/// Format a float percentage as "72%" or "--" if None.
pub fn percent_text(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{:.0}%", v),
        None => "--".to_owned(),
    }
}

/// Core data contract shared by all providers.
/// `metric1_val` / `metric2_val` are Some(0.0..=100.0) or None (shown as "--").
#[derive(Debug, Clone, Default)]
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

#[allow(dead_code)]
pub fn progress_color_rgb(percent: f64) -> (u8, u8, u8) {
    if percent >= 90.0 {
        (0xef, 0x44, 0x44) // Red
    } else if percent >= 75.0 {
        (0xf5, 0x9e, 0x0b) // Amber
    } else if percent >= 50.0 {
        (0x3b, 0x82, 0xf6) // Blue
    } else {
        (0x10, 0xb9, 0x81) // Green
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
