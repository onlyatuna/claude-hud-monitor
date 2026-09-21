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

    pub metric2_title: String,
    pub metric2_val: Option<f64>,
    pub metric2_text: String,
    pub metric2_reset: Option<DateTime<Utc>>,

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
