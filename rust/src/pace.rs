//! Time and pace helpers for the usage table, matching Python core/pace.py.
//!
//! "Pace" compares usage with how far through its window we are: at even pace a
//! window that is 40% elapsed should be 40% used. Usage above that mark means the
//! quota runs out before the window resets if the current rate continues.

use chrono::{DateTime, Datelike, Local, Timelike, Utc};

pub const WEEKDAYS: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];

pub const SCALE_THRESHOLDS: [(f64, &str); 4] = [
    (90.0, "red"),
    (75.0, "orange"),
    (50.0, "yellow"),
    (0.0, "green"),
];

pub const MIN_ELAPSED_FOR_PACE: f64 = 0.05;
pub const FIVE_HOURS: i64 = 5 * 3600;
pub const ONE_WEEK: i64 = 7 * 86400;

pub fn remaining_seconds(target: Option<DateTime<Utc>>) -> Option<i64> {
    let target = target?;
    let now = Utc::now();
    let diff = (target - now).num_seconds();
    Some(diff.max(0))
}

pub fn format_countdown_hm(target: Option<DateTime<Utc>>) -> String {
    let sec = match remaining_seconds(target) {
        Some(s) => s,
        None => return "--:--".to_string(),
    };
    format!("{:02}:{:02}", sec / 3600, (sec % 3600) / 60)
}

pub fn format_countdown_dhm(target: Option<DateTime<Utc>>) -> String {
    let sec = match remaining_seconds(target) {
        Some(s) => s,
        None => return "--:--:--".to_string(),
    };
    format!(
        "{:02}:{:02}:{:02}",
        sec / 86400,
        (sec % 86400) / 3600,
        (sec % 3600) / 60
    )
}

pub fn format_reset_time(target: Option<DateTime<Utc>>, with_day: bool) -> String {
    let target = match target {
        Some(t) => t,
        None => return "--:--".to_string(),
    };
    let local: DateTime<Local> = DateTime::from(target);
    let hm = format!("{:02}:{:02}", local.hour(), local.minute());
    if with_day {
        // weekday().number_from_monday() is 1..=7
        let weekday_idx = (local.weekday().number_from_monday() - 1) as usize;
        let day_str = WEEKDAYS.get(weekday_idx).copied().unwrap_or("?");
        format!("週{} {}", day_str, hm)
    } else {
        hm
    }
}

pub fn short_window(title: &str) -> String {
    title
        .replace("WINDOW ", "")
        .replace("SESSION ", "")
        .replace("WEEKLY ", "")
}

pub fn window_caption(title: &str, expected: &str) -> String {
    let short = short_window(title);
    if title.is_empty() || short == expected {
        String::new()
    } else {
        short
    }
}

pub fn window_seconds(title: &str, default: i64) -> i64 {
    let short = short_window(title).trim().to_string();
    if short.is_empty() {
        return default;
    }

    let mut num_str = String::new();
    let mut unit_char = None;
    for c in short.chars() {
        if c.is_ascii_digit() || c == '.' {
            num_str.push(c);
        } else if c.is_alphabetic() {
            unit_char = Some(c.to_ascii_uppercase());
        }
    }

    if let (Ok(num), Some(unit)) = (num_str.parse::<f64>(), unit_char) {
        let mult = match unit {
            'D' => 86400.0,
            'H' => 3600.0,
            'M' => 60.0,
            _ => return default,
        };
        (num * mult) as i64
    } else {
        default
    }
}

pub fn elapsed_fraction(reset: Option<DateTime<Utc>>, window_sec: i64) -> Option<f64> {
    let remaining = remaining_seconds(reset)?;
    if window_sec <= 0 {
        return None;
    }
    let frac = 1.0 - (remaining as f64 / window_sec as f64);
    Some(frac.clamp(0.0, 1.0))
}

pub fn pace_mark(elapsed: Option<f64>) -> Option<f64> {
    let elapsed = elapsed?;
    if elapsed < MIN_ELAPSED_FOR_PACE {
        return None;
    }
    Some(elapsed * 100.0)
}

pub fn scale_level(percent: Option<f64>) -> Option<&'static str> {
    let percent = percent?;
    for (bound, level) in SCALE_THRESHOLDS {
        if percent >= bound {
            return Some(level);
        }
    }
    None
}

pub fn runout_text(percent: Option<f64>, elapsed: Option<f64>, window_sec: i64) -> String {
    let mark = match pace_mark(elapsed) {
        Some(m) => m,
        None => return String::new(),
    };
    let percent = match percent {
        Some(p) => p,
        None => return String::new(),
    };
    let elapsed = match elapsed {
        Some(e) => e,
        None => return String::new(),
    };

    if percent <= mark {
        return "照目前速度，重設前不會用完".to_string();
    }

    let rate = percent / (elapsed * window_sec as f64);
    if rate <= 0.0 {
        return String::new();
    }

    let remaining_seconds = ((100.0 - percent) / rate) as i64;
    let local = Local::now() + chrono::Duration::seconds(remaining_seconds);
    let weekday_idx = (local.weekday().number_from_monday() - 1) as usize;
    let day_str = WEEKDAYS.get(weekday_idx).copied().unwrap_or("?");
    format!(
        "照目前速度，預計 週{} {:02}:{:02} 用完",
        day_str,
        local.hour(),
        local.minute()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn in_time(dur: Duration) -> DateTime<Utc> {
        Utc::now() + dur
    }

    #[test]
    fn test_countdown_formats() {
        assert_eq!(
            format_countdown_hm(Some(in_time(
                Duration::hours(2) + Duration::minutes(14) + Duration::seconds(30)
            ))),
            "02:14"
        );
        assert_eq!(
            format_countdown_dhm(Some(in_time(
                Duration::days(3)
                    + Duration::hours(5)
                    + Duration::minutes(41)
                    + Duration::seconds(30)
            ))),
            "03:05:41"
        );
        assert_eq!(format_countdown_hm(None), "--:--");
        assert_eq!(format_countdown_dhm(None), "--:--:--");
        assert_eq!(
            format_countdown_hm(Some(in_time(Duration::minutes(-5)))),
            "00:00"
        );
    }

    #[test]
    fn test_reset_time_weekday() {
        let target = in_time(Duration::days(2));
        let without_day = format_reset_time(Some(target), false);
        assert!(without_day.len() == 5 && without_day.contains(':'));

        let with_day = format_reset_time(Some(target), true);
        assert!(with_day.starts_with("週"));
        assert!(with_day.contains(':'));
    }

    #[test]
    fn test_window_parsing() {
        assert_eq!(window_seconds("SESSION 5H", 0), 5 * 3600);
        assert_eq!(window_seconds("WINDOW 30D", 0), 30 * 86400);
        assert_eq!(window_seconds("WINDOW 90M", 0), 90 * 60);
        assert_eq!(window_seconds("PRIMARY", 123), 123);
        assert_eq!(window_caption("WEEKLY 7D", "7D"), "");
        assert_eq!(window_caption("WINDOW 7D", "7D"), "");
        assert_eq!(window_caption("WINDOW 1D", "5H"), "1D");
    }

    #[test]
    fn test_pace_mark() {
        assert_eq!(pace_mark(None), None);
        assert_eq!(pace_mark(Some(0.02)), None);
        assert!((pace_mark(Some(0.5)).unwrap() - 50.0).abs() < 1e-4);

        let elapsed = elapsed_fraction(
            Some(in_time(Duration::hours(2) + Duration::minutes(30))),
            FIVE_HOURS,
        );
        assert!((elapsed.unwrap() - 0.5).abs() < 0.05);
        assert_eq!(elapsed_fraction(None, FIVE_HOURS), None);
    }

    #[test]
    fn test_scale_levels() {
        assert_eq!(scale_level(None), None);
        let vals = [0.0, 49.9, 50.0, 74.9, 75.0, 89.9, 90.0, 100.0];
        let expected = [
            "green", "green", "yellow", "yellow", "orange", "orange", "red", "red",
        ];
        for (v, exp) in vals.iter().zip(expected.iter()) {
            assert_eq!(scale_level(Some(*v)), Some(*exp));
        }
    }

    #[test]
    fn test_runout_text() {
        assert_eq!(
            runout_text(Some(30.0), Some(0.5), ONE_WEEK),
            "照目前速度，重設前不會用完"
        );
        let text = runout_text(Some(80.0), Some(0.5), ONE_WEEK);
        assert!(text.contains("用完"));
        assert_eq!(runout_text(Some(80.0), Some(0.01), ONE_WEEK), "");
        assert_eq!(runout_text(None, Some(0.5), ONE_WEEK), "");
    }
}
