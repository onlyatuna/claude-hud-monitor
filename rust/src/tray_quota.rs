//! Which quota's number is drawn on the tray icon: key format, row lookup, number text, state and
//! colour. Pure logic, no Qt; the icon drawing lives in `ui/tray_number_icon.rs`.
//!
//! Config (`config.json`, shared with the Python HUD):
//! - `tray_quota`: `"off"` (default), `"auto"` (the tightest quota) or a row key.
//! - `tray_quota_mode`: `"used"` (default) or `"remaining"`.
//!
//! A row key is the row's `MetricRow.key` qualified with its provider: unchanged when it already
//! starts with `<provider_id>:` (every Claude key: `claude:5h`, `claude:7d`,
//! `claude:weekly_scoped:fable`), else `<provider_id>:<row key>` (`codex:WINDOW 5H`, `agy:C/G 5H`).

use chrono::{DateTime, Duration, Utc};

use crate::providers::base::{UsageMetrics, percent_text};

pub const QUOTA_OFF: &str = "off";
pub const QUOTA_AUTO: &str = "auto";
pub const MODE_USED: &str = "used";
pub const MODE_REMAINING: &str = "remaining";

/// A reading older than this (since `last_success`) is drawn grey.
const OLD_AFTER_MINUTES: i64 = 30;

/// Room for the quota line in the tooltip: Windows cuts a tooltip at 127 UTF-16 units and the
/// three fixed lines below it take about 55.
const TOOLTIP_LINE_UNITS: usize = 60;

const RED: [u8; 3] = [0xef, 0x44, 0x44];
const AMBER: [u8; 3] = [0xf5, 0x9e, 0x0b];
const GREY: [u8; 3] = [0x8b, 0x94, 0x9e];

/// Any non-blank string is a valid `tray_quota`: `off`, `auto` or a row key. A key that matches no
/// current row is kept (see `State::Unavailable`).
pub fn valid_quota(value: &str) -> bool {
    !value.trim().is_empty()
}

pub fn valid_mode(value: &str) -> bool {
    value == MODE_USED || value == MODE_REMAINING
}

/// The qualified key of a row.
pub fn qualify(provider_id: &str, row_key: &str) -> String {
    let prefix = format!("{provider_id}:");
    if row_key.starts_with(&prefix) {
        row_key.to_owned()
    } else {
        format!("{prefix}{row_key}")
    }
}

/// Provider name as the tray shows it.
pub fn display_name(provider_id: &str) -> String {
    match provider_id {
        "claude" => "Claude".to_owned(),
        "codex" => "Codex".to_owned(),
        "agy" => "AGY".to_owned(),
        other => {
            let mut chars = other.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

/// The tray name of the provider behind `m`: a built-in one by its id, an extra account by its tile
/// text, `elide(label, 8)`. Its provider reports `elide(label, 13)` as its name and cutting that
/// to 8 gives the same text as cutting the label itself.
pub fn display_name_of(m: &UsageMetrics) -> String {
    if m.provider_id.contains('@') && !m.provider_name.is_empty() {
        return crate::accounts::elide(&m.provider_name, crate::accounts::TILE_LABEL_MAX);
    }
    display_name(&m.provider_id)
}

/// One selectable quota row of a provider's latest (unarranged) data.
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaRow {
    pub key: String,
    pub title: String,
    pub val: Option<f64>,
}

/// Every row of `m`: the main pair, then the extras.
pub fn rows(m: &UsageMetrics) -> Vec<QuotaRow> {
    // A provider that failed with nothing to show reports default rows without their keys, so
    // it has no real rows: one placeholder stands for all of them (see `resolve`, the menu).
    if is_offline(m) {
        return vec![QuotaRow {
            key: format!("{}:", m.provider_id),
            title: String::new(),
            val: None,
        }];
    }
    let id = &m.provider_id;
    let main_key = |key: &str, title: &str| qualify(id, if key.is_empty() { title } else { key });
    let mut out: Vec<QuotaRow> = [
        (&m.metric1_key, &m.metric1_title, m.metric1_val),
        (&m.metric2_key, &m.metric2_title, m.metric2_val),
    ]
    .into_iter()
    // A provider with a single window leaves its second main row empty: no row, no key.
    .filter(|(key, title, _)| !(key.is_empty() && title.is_empty()))
    .map(|(key, title, val)| QuotaRow {
        key: main_key(key, title),
        title: title.clone(),
        val,
    })
    .collect();
    out.extend(m.extra_metrics.iter().map(|r| QuotaRow {
        key: qualify(id, &r.key),
        title: r.title.clone(),
        val: r.val,
    }));
    out
}

/// The sidebar's `offline = bool(error and not stale)`: failed with nothing to show.
pub fn is_offline(m: &UsageMetrics) -> bool {
    m.error.is_some() && !m.stale
}

/// The number, without the `%`: `used` rounded like `percent_text`. Three digits do not fit a
/// 16 px icon, so used mode stops at `99`; remaining mode is `100 - used`, also at most `99`
/// (`0` when the quota is used up).
pub fn number_text(used: f64, mode: &str) -> String {
    let n = format!("{used:.0}")
        .parse::<f64>()
        .unwrap_or(0.0)
        .clamp(0.0, 100.0) as i32;
    if mode == MODE_REMAINING {
        (100 - n).min(99).to_string()
    } else {
        n.min(99).to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// A fresh reading: coloured by the gauge ramp.
    Ok,
    /// The last fetch failed but the previous value is kept: amber.
    Stale,
    /// The reading is older than 30 minutes: grey (wins over `Stale`).
    Old,
    /// The provider failed and has nothing to show: `!`.
    Offline,
    /// The row exists but has no value: `--`.
    NoReading,
    /// The selected key is not in the data (a model scope went away): `--`, setting untouched.
    Unavailable,
}

/// What the icon and tooltip show.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// Provider name and row title, e.g. `Claude FABLE 7D`; empty when there is no row to name
    /// (unavailable selection, auto with nothing).
    pub label: String,
    pub used: Option<f64>,
    pub state: State,
}

/// Resolves the setting against the providers' latest unarranged data, in provider order.
/// `None` for `off`. `auto` is the row with the highest used % among the finite values of the
/// providers that are not offline (a tie goes to the first); with none it is `NoReading`.
pub fn resolve(setting: &str, data: &[UsageMetrics], now: DateTime<Utc>) -> Option<Reading> {
    if setting == QUOTA_OFF {
        return None;
    }
    // Nothing has been fetched yet (start-up): not a missing selection.
    if data.is_empty() {
        return Some(Reading {
            label: String::new(),
            used: None,
            state: State::NoReading,
        });
    }
    let reading = |m: &UsageMetrics, row: &QuotaRow| {
        let used = row.val.filter(|v| v.is_finite());
        let state = if is_offline(m) {
            State::Offline
        } else if used.is_none() {
            State::NoReading
        } else if m
            .last_success
            .is_some_and(|t| now - t > Duration::minutes(OLD_AFTER_MINUTES))
        {
            State::Old
        } else if m.stale {
            State::Stale
        } else {
            State::Ok
        };
        Reading {
            label: format!("{} {}", display_name_of(m), row.title),
            used,
            state,
        }
    };

    if setting == QUOTA_AUTO {
        let mut best: Option<(&UsageMetrics, QuotaRow, f64)> = None;
        for m in data.iter().filter(|m| !is_offline(m)) {
            for row in rows(m) {
                let Some(v) = row.val.filter(|v| v.is_finite()) else {
                    continue;
                };
                if best.as_ref().is_none_or(|(_, _, top)| v > *top) {
                    best = Some((m, row, v));
                }
            }
        }
        return Some(match best {
            Some((m, row, _)) => reading(m, &row),
            None => Reading {
                label: String::new(),
                used: None,
                state: State::NoReading,
            },
        });
    }

    for m in data {
        if let Some(row) = rows(m).into_iter().find(|r| r.key == setting) {
            return Some(reading(m, &row));
        }
        // Any key of an offline provider is "offline", not "gone" (`claude:5h` is not among the
        // placeholder's keys).
        if is_offline(m) && setting.starts_with(&format!("{}:", m.provider_id)) {
            return Some(Reading {
                label: display_name_of(m),
                used: None,
                state: State::Offline,
            });
        }
    }
    Some(Reading {
        label: String::new(),
        used: None,
        state: State::Unavailable,
    })
}

/// How the icon looks: the text, its colour and whether the remaining-mode bar is drawn. Equal
/// specs draw equal icons, so the tray only redraws when it changes.
#[derive(Debug, Clone, PartialEq)]
pub struct IconSpec {
    pub text: String,
    /// `[r, g, b]`.
    pub rgb: [u8; 3],
    pub remaining: bool,
}

impl Reading {
    pub fn icon_spec(&self, mode: &str) -> IconSpec {
        let ramp = |used: f64| {
            let c = crate::ui::gauge::ramp_color(used, true, None);
            [c.red() as u8, c.green() as u8, c.blue() as u8]
        };
        let (text, rgb, numeric) = match (self.state, self.used) {
            (State::Offline, _) => ("!".to_owned(), RED, false),
            (State::NoReading | State::Unavailable, _) | (_, None) => {
                ("--".to_owned(), GREY, false)
            }
            (State::Old, Some(v)) => (number_text(v, mode), GREY, true),
            (State::Stale, Some(v)) => (number_text(v, mode), AMBER, true),
            (State::Ok, Some(v)) => (number_text(v, mode), ramp(v), true),
        };
        IconSpec {
            text,
            rgb,
            remaining: numeric && mode == MODE_REMAINING,
        }
    }

    /// First tooltip line, e.g. `Claude FABLE 7D: 84% used` or `16% left`. A long label is cut so
    /// the whole tooltip stays inside the 127 UTF-16 units Windows allows.
    pub fn tooltip_line(&self, mode: &str) -> String {
        let tail = match (self.state, self.used) {
            (State::Unavailable, _) => return "Tray number: selection unavailable".to_owned(),
            (_, _) if self.label.is_empty() => return "Tray number: --".to_owned(),
            (State::Offline, _) => "offline".to_owned(),
            (_, None) => "--".to_owned(),
            (state, Some(v)) => {
                let n = number_text(v, mode);
                let unit = if mode == MODE_REMAINING {
                    "left"
                } else {
                    "used"
                };
                let old = matches!(state, State::Stale | State::Old);
                format!("{n}% {unit}{}", if old { " (stale)" } else { "" })
            }
        };
        let room = TOOLTIP_LINE_UNITS.saturating_sub(tail.len() + 2);
        let mut label = String::new();
        let mut units = 0;
        for c in self.label.chars() {
            units += c.len_utf16();
            if units > room {
                label.push('…');
                break;
            }
            label.push(c);
        }
        format!("{label}: {tail}")
    }
}

/// Menu label of a row: its live title and the used percent, e.g. `FABLE 7D  84%`.
pub fn menu_label(row: &QuotaRow) -> String {
    format!("{}  {}", row.title, percent_text(row.val))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::MetricRow;

    fn claude(m1: Option<f64>, m2: Option<f64>) -> UsageMetrics {
        UsageMetrics {
            provider_id: "claude".into(),
            provider_name: "Claude".into(),
            metric1_title: "SESSION 5H".into(),
            metric2_title: "WEEKLY 7D".into(),
            metric1_key: "claude:5h".into(),
            metric2_key: "claude:7d".into(),
            metric1_val: m1,
            metric2_val: m2,
            extra_metrics: vec![MetricRow {
                key: "claude:weekly_scoped:fable".into(),
                ..MetricRow::new("FABLE 7D", Some(84.0), "84%")
            }],
            ..Default::default()
        }
    }

    fn codex(used: f64) -> UsageMetrics {
        UsageMetrics {
            provider_id: "codex".into(),
            provider_name: "Codex".into(),
            metric1_title: "WINDOW 5H".into(),
            metric1_val: Some(used),
            ..Default::default()
        }
    }

    #[test]
    fn auto_picks_the_highest_used_across_providers_and_skips_offline_ones() {
        let now = Utc::now();
        let data = [claude(Some(10.0), Some(20.0)), codex(91.0)];
        let r = resolve("auto", &data, now).unwrap();
        assert_eq!((r.label.as_str(), r.used), ("Codex WINDOW 5H", Some(91.0)));
        // Claude's FABLE 84 wins once Codex has failed with nothing to show.
        let mut failed = codex(0.0);
        failed.error = Some("boom".into());
        let r = resolve("auto", &[claude(Some(10.0), Some(20.0)), failed], now).unwrap();
        assert_eq!((r.label.as_str(), r.used), ("Claude FABLE 7D", Some(84.0)));
    }

    #[test]
    fn number_follows_the_mode_and_never_has_three_digits() {
        assert_eq!(number_text(42.4, MODE_USED), "42");
        assert_eq!(number_text(42.4, MODE_REMAINING), "58");
        assert_eq!(number_text(100.0, MODE_USED), "99");
        assert_eq!(number_text(100.0, MODE_REMAINING), "0");
        assert_eq!(number_text(0.0, MODE_REMAINING), "99");
    }

    #[test]
    fn missing_selection_is_unavailable_and_states_follow_the_sidebar() {
        let now = Utc::now();
        let data = [claude(Some(10.0), None)];
        let r = resolve("claude:weekly_scoped:gone", &data, now).unwrap();
        assert_eq!(r.state, State::Unavailable);
        assert_eq!(r.icon_spec(MODE_USED).text, "--");
        assert_eq!(
            r.tooltip_line(MODE_USED),
            "Tray number: selection unavailable"
        );

        let r = resolve("claude:7d", &data, now).unwrap();
        assert_eq!(r.icon_spec(MODE_USED).text, "--");

        let mut stale = claude(Some(61.0), Some(5.0));
        stale.error = Some("rate limited".into());
        stale.stale = true;
        let spec = resolve("claude:5h", &[stale.clone()], now)
            .unwrap()
            .icon_spec(MODE_USED);
        assert_eq!((spec.text.as_str(), spec.rgb), ("61", AMBER));

        stale.last_success = Some(now - Duration::minutes(31));
        let spec = resolve("claude:5h", &[stale], now)
            .unwrap()
            .icon_spec(MODE_USED);
        assert_eq!(spec.rgb, GREY);

        let offline = UsageMetrics::error_result("claude", "Claude", "no login", "auth");
        let spec = resolve("claude:5h", &[offline], now)
            .unwrap()
            .icon_spec(MODE_USED);
        assert_eq!(spec.text, "!");
        assert!(resolve("off", &data, now).is_none());
    }

    #[test]
    fn keys_labels_and_tooltip() {
        let data = [claude(Some(10.0), Some(20.0)), codex(5.0)];
        let keys: Vec<String> = data.iter().flat_map(rows).map(|r| r.key).collect();
        assert!(keys.contains(&"claude:weekly_scoped:fable".to_owned()));
        assert!(keys.contains(&"codex:WINDOW 5H".to_owned()));
        assert_eq!(menu_label(&rows(&data[0])[2]), "FABLE 7D  84%");
        let r = resolve("claude:weekly_scoped:fable", &data, Utc::now()).unwrap();
        assert_eq!(r.tooltip_line(MODE_USED), "Claude FABLE 7D: 84% used");
        assert_eq!(r.tooltip_line(MODE_REMAINING), "Claude FABLE 7D: 16% left");
    }

    #[test]
    fn a_dynamic_agy_row_is_selectable_until_it_disappears() {
        let mut carbon = MetricRow::new("CARBON 7D", Some(40.0), "40%");
        carbon.key = "agy:carbon-7d".into();
        let mut agy = UsageMetrics {
            provider_id: "agy".into(),
            metric1_title: "GEMINI 5H".into(),
            extra_metrics: vec![carbon],
            ..Default::default()
        };
        let now = Utc::now();
        let r = resolve("agy:carbon-7d", std::slice::from_ref(&agy), now).unwrap();
        assert_eq!(
            (r.label.as_str(), r.used, r.state),
            ("AGY CARBON 7D", Some(40.0), State::Ok)
        );
        agy.extra_metrics.clear();
        let r = resolve("agy:carbon-7d", &[agy], now).unwrap();
        assert_eq!(r.state, State::Unavailable);
        assert_eq!(r.icon_spec(MODE_USED).text, "--");
    }
}
