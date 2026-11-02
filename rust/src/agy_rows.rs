#![allow(dead_code)]
//! Row arrangement for agy's five quota rows and generic per-provider ordering
//! (port of python/core/agy_rows.py).

use crate::providers::base::{MetricRow, UsageMetrics};

pub const AGY_ROW_TITLES: [&str; 5] = ["GEMINI 5H", "GEMINI 7D", "C/G 5H", "C/G 7D", "ARGON 7D"];
pub const DEFAULT_PRIMARY: [&str; 2] = ["C/G 5H", "ARGON 7D"];
pub const DEFAULT_ORDER: [&str; 5] = ["C/G 5H", "ARGON 7D", "GEMINI 5H", "GEMINI 7D", "C/G 7D"];

fn to_strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Distinct known row titles (uppercased) in first-seen order.
fn known_titles(value: &[String]) -> Vec<String> {
    let mut titles: Vec<String> = Vec::new();
    for item in value {
        let title = item.to_uppercase();
        if AGY_ROW_TITLES.contains(&title.as_str()) && !titles.contains(&title) {
            titles.push(title);
        }
    }
    titles
}

/// Python `normalize_primary`: two distinct known row titles, else the default.
pub fn normalize_primary(value: &[String]) -> [String; 2] {
    let mut titles = known_titles(value);
    if titles.len() == 2 {
        let second = titles.pop().unwrap();
        let first = titles.pop().unwrap();
        return [first, second];
    }
    [
        DEFAULT_PRIMARY[0].to_string(),
        DEFAULT_PRIMARY[1].to_string(),
    ]
}

/// Python `normalize_order`: a full ordering of the five rows; a legacy pair keeps its rows first.
pub fn normalize_order(value: &[String]) -> Vec<String> {
    let mut titles = known_titles(value);
    if titles.len() == 5 {
        return titles;
    }
    if titles.len() == 2 {
        for t in AGY_ROW_TITLES {
            if !titles.iter().any(|x| x == t) {
                titles.push(t.to_string());
            }
        }
        return titles;
    }
    to_strings(&DEFAULT_ORDER)
}

/// Python `move_row`: move one row to a 0-based position among the rows without it
/// (negative / too large indices are clamped).
pub fn move_row(order: &[String], title: &str, new_index: i64) -> Vec<String> {
    let mut order = normalize_order(order);
    let title = title.to_uppercase();
    let Some(pos) = order.iter().position(|t| *t == title) else {
        return order;
    };
    order.remove(pos);
    let idx = new_index.clamp(0, order.len() as i64) as usize;
    order.insert(idx, title);
    order
}

/// Python `toggle_primary`.
pub fn toggle_primary(current: &[String], title: &str) -> [String; 2] {
    let [first, second] = normalize_primary(current);
    let title = title.to_uppercase();
    if !AGY_ROW_TITLES.contains(&title.as_str()) || title == first {
        return [first, second];
    }
    if title == second {
        return [second, first];
    }
    [second, title]
}

fn main_rows(m: &UsageMetrics) -> [MetricRow; 2] {
    [
        MetricRow {
            title: m.metric1_title.clone(),
            val: m.metric1_val,
            text: m.metric1_text.clone(),
            reset: m.metric1_reset,
            subtext: m.metric1_subtext.clone(),
        },
        MetricRow {
            title: m.metric2_title.clone(),
            val: m.metric2_val,
            text: m.metric2_text.clone(),
            reset: m.metric2_reset,
            subtext: m.metric2_subtext.clone(),
        },
    ]
}

/// Clone of `m` whose main pair is `rows[0]`/`rows[1]` and extras are `rows[2..]`.
fn with_rows(m: &UsageMetrics, mut rows: Vec<MetricRow>) -> UsageMetrics {
    let extras = rows.split_off(2);
    let b = rows.pop().unwrap();
    let a = rows.pop().unwrap();
    let mut out = m.clone();
    out.metric1_title = a.title;
    out.metric1_val = a.val;
    out.metric1_text = a.text;
    out.metric1_reset = a.reset;
    out.metric1_subtext = a.subtext;
    out.metric2_title = b.title;
    out.metric2_val = b.val;
    out.metric2_text = b.text;
    out.metric2_reset = b.reset;
    out.metric2_subtext = b.subtext;
    out.extra_metrics = extras;
    out
}

/// Python `arrange_agy(metrics, primary)`: metric1/metric2 become the chosen rows, extras the other
/// three. Unchanged (clone) when the data does not carry all five rows.
pub fn arrange_agy(metrics: &UsageMetrics, primary: &[String]) -> UsageMetrics {
    // Insertion-ordered map keyed by uppercased title; later rows overwrite earlier ones.
    let mut rows: Vec<(String, MetricRow)> = Vec::new();
    let [r1, r2] = main_rows(metrics);
    for row in [r1, r2]
        .into_iter()
        .chain(metrics.extra_metrics.iter().cloned())
    {
        let key = row.title.to_uppercase();
        match rows.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = row,
            None => rows.push((key, row)),
        }
    }
    let find = |title: &str| {
        rows.iter()
            .find(|(k, _)| k == title)
            .map(|(_, r)| r.clone())
    };
    if !AGY_ROW_TITLES
        .iter()
        .all(|t| rows.iter().any(|(k, _)| k == t))
    {
        return metrics.clone();
    }
    let order = normalize_order(primary);
    let ordered: Vec<MetricRow> = order.iter().filter_map(|t| find(t)).collect();
    with_rows(metrics, ordered)
}

/// Python `arrange_by_order`: rows named in `order` come first in that order, the rest keep their
/// natural position after them; the first two become metric1/metric2, the others extras.
/// Returns an unchanged clone when the arrangement does not change the titles.
pub fn arrange_by_order(metrics: &UsageMetrics, order: &[String]) -> UsageMetrics {
    let [r1, r2] = main_rows(metrics);
    let rows: Vec<MetricRow> = [r1, r2]
        .into_iter()
        .chain(metrics.extra_metrics.iter().cloned())
        .collect();
    let wanted: Vec<String> = order.iter().map(|t| t.to_uppercase()).collect();
    let rank = |row: &MetricRow| {
        let t = row.title.to_uppercase();
        wanted.iter().position(|w| *w == t).unwrap_or(wanted.len())
    };
    let mut ranked: Vec<(usize, MetricRow)> = rows.into_iter().enumerate().collect();
    // stable sort by (rank, original index)
    ranked.sort_by_key(|(i, row)| (rank(row), *i));
    let rows: Vec<MetricRow> = ranked.into_iter().map(|(_, r)| r).collect();
    let unchanged = rows[0].title == metrics.metric1_title
        && rows[1].title == metrics.metric2_title
        && rows[2..]
            .iter()
            .map(|r| &r.title)
            .eq(metrics.extra_metrics.iter().map(|r| &r.title));
    if unchanged {
        return metrics.clone();
    }
    with_rows(metrics, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        to_strings(items)
    }

    fn agy_metrics() -> UsageMetrics {
        UsageMetrics {
            provider_id: "agy".into(),
            metric1_title: "GEMINI 5H".into(),
            metric1_val: Some(0.0),
            metric1_text: "0%".into(),
            metric2_title: "GEMINI 7D".into(),
            metric2_val: Some(1.0),
            metric2_text: "1%".into(),
            extra_metrics: vec![
                MetricRow::new("C/G 5H", Some(3.0), "3%"),
                MetricRow::new("C/G 7D", Some(15.0), "15%"),
                MetricRow::new("ARGON 7D", Some(21.0), "21%"),
            ],
            ..Default::default()
        }
    }

    /// Stand-in for python's `assertIs`: same titles/vals/texts on main + extra rows.
    fn same(a: &UsageMetrics, b: &UsageMetrics) -> bool {
        let rows = |m: &UsageMetrics| {
            let [r1, r2] = main_rows(m);
            [r1, r2]
                .into_iter()
                .chain(m.extra_metrics.iter().cloned())
                .collect::<Vec<_>>()
        };
        a.provider_id == b.provider_id && a.error == b.error && rows(a) == rows(b)
    }

    #[test]
    fn default_pair_is_cg_and_argon() {
        let result = arrange_agy(&agy_metrics(), &[]);
        assert_eq!(
            (result.metric1_title.as_str(), result.metric1_val),
            ("C/G 5H", Some(3.0))
        );
        assert_eq!(
            (result.metric2_title.as_str(), result.metric2_val),
            ("ARGON 7D", Some(21.0))
        );
        let extras: Vec<&str> = result
            .extra_metrics
            .iter()
            .map(|r| r.title.as_str())
            .collect();
        assert_eq!(extras, ["GEMINI 5H", "GEMINI 7D", "C/G 7D"]);
        assert_eq!(result.provider_id, "agy");
    }

    #[test]
    fn any_pair_keeps_all_five_rows() {
        for first in AGY_ROW_TITLES {
            for second in AGY_ROW_TITLES {
                if first == second {
                    continue;
                }
                let result = arrange_agy(&agy_metrics(), &s(&[first, second]));
                let mut titles = vec![result.metric1_title.clone(), result.metric2_title.clone()];
                titles.extend(result.extra_metrics.iter().map(|r| r.title.clone()));
                titles.sort();
                let mut expected = s(&AGY_ROW_TITLES);
                expected.sort();
                assert_eq!(titles, expected);
                assert_eq!(
                    (result.metric1_title.as_str(), result.metric2_title.as_str()),
                    (first, second)
                );
            }
        }
    }

    #[test]
    fn incomplete_or_error_data_is_left_alone() {
        let data = UsageMetrics {
            provider_id: "agy".into(),
            error: Some("offline".into()),
            ..Default::default()
        };
        assert!(same(&arrange_agy(&data, &s(&DEFAULT_PRIMARY)), &data));
        let mut partial = agy_metrics();
        partial.extra_metrics.truncate(2);
        assert!(same(&arrange_agy(&partial, &s(&DEFAULT_PRIMARY)), &partial));
    }

    #[test]
    fn bad_settings_fall_back_to_default() {
        let default = s(&DEFAULT_PRIMARY);
        let bads: Vec<Vec<String>> = vec![
            vec![],
            s(&["x"]),
            s(&["C/G 5H"]),
            s(&["C/G 5H", "C/G 5H"]),
            s(&["nope", "ARGON 7D"]),
            s(&["5"]),
        ];
        for bad in bads {
            assert_eq!(normalize_primary(&bad).to_vec(), default);
        }
        assert_eq!(
            normalize_primary(&s(&["argon 7d", "gemini 5h"])).to_vec(),
            s(&["ARGON 7D", "GEMINI 5H"])
        );
    }

    #[test]
    fn toggle_replaces_oldest_or_moves_to_top() {
        let cur = s(&["C/G 5H", "ARGON 7D"]);
        assert_eq!(
            toggle_primary(&cur, "GEMINI 7D").to_vec(),
            s(&["ARGON 7D", "GEMINI 7D"])
        );
        assert_eq!(
            toggle_primary(&cur, "ARGON 7D").to_vec(),
            s(&["ARGON 7D", "C/G 5H"])
        );
        assert_eq!(
            toggle_primary(&cur, "C/G 5H").to_vec(),
            s(&["C/G 5H", "ARGON 7D"])
        );
        assert_eq!(
            toggle_primary(&cur, "bogus").to_vec(),
            s(&["C/G 5H", "ARGON 7D"])
        );
    }

    #[test]
    fn default_order_starts_with_the_default_pair() {
        assert_eq!(DEFAULT_ORDER[..2], DEFAULT_PRIMARY);
        let mut a = DEFAULT_ORDER.to_vec();
        let mut b = AGY_ROW_TITLES.to_vec();
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }

    #[test]
    fn normalize_order_accepts_full_orders_and_legacy_pairs() {
        let full = s(&["GEMINI 7D", "C/G 7D", "ARGON 7D", "C/G 5H", "GEMINI 5H"]);
        assert_eq!(normalize_order(&full), full);
        let pair = normalize_order(&s(&["ARGON 7D", "GEMINI 5H"]));
        assert_eq!(pair[..2].to_vec(), s(&["ARGON 7D", "GEMINI 5H"]));
        assert_eq!(pair.len(), 5);
        let mut four_bogus = full[..4].to_vec();
        four_bogus.push("bogus".into());
        let bads: Vec<Vec<String>> = vec![vec![], s(&["x"]), vec![], s(&["C/G 5H"]), four_bogus];
        for bad in bads {
            assert_eq!(normalize_order(&bad), s(&DEFAULT_ORDER));
        }
    }

    #[test]
    fn move_row_cases() {
        let order = s(&DEFAULT_ORDER);
        assert_eq!(move_row(&order, "GEMINI 7D", 0)[0], "GEMINI 7D");
        assert_eq!(move_row(&order, "C/G 5H", 4).last().unwrap(), "C/G 5H");
        let mut moved = move_row(&order, "ARGON 7D", 2);
        moved.sort();
        let mut expected = s(&AGY_ROW_TITLES);
        expected.sort();
        assert_eq!(moved, expected);
        assert_eq!(move_row(&order, "nope", 0), order);
        assert_eq!(move_row(&order, "GEMINI 7D", -3)[0], "GEMINI 7D");
    }

    #[test]
    fn arrange_uses_the_full_order() {
        let order = s(&["GEMINI 7D", "C/G 7D", "ARGON 7D", "C/G 5H", "GEMINI 5H"]);
        let result = arrange_agy(&agy_metrics(), &order);
        assert_eq!(
            (result.metric1_title.as_str(), result.metric2_title.as_str()),
            ("GEMINI 7D", "C/G 7D")
        );
        let extras: Vec<&str> = result
            .extra_metrics
            .iter()
            .map(|r| r.title.as_str())
            .collect();
        assert_eq!(extras, ["ARGON 7D", "C/G 5H", "GEMINI 5H"]);
    }

    #[test]
    fn arrange_by_order_for_two_row_providers() {
        let data = UsageMetrics {
            provider_id: "claude".into(),
            metric1_title: "SESSION 5H".into(),
            metric1_val: Some(27.0),
            metric2_title: "WEEKLY 7D".into(),
            metric2_val: Some(9.0),
            ..Default::default()
        };
        let swapped = arrange_by_order(&data, &s(&["WEEKLY 7D", "SESSION 5H"]));
        assert_eq!(
            (
                swapped.metric1_title.as_str(),
                swapped.metric1_val,
                swapped.metric2_title.as_str()
            ),
            ("WEEKLY 7D", Some(9.0), "SESSION 5H")
        );
        assert!(same(
            &arrange_by_order(&data, &s(&["SESSION 5H", "WEEKLY 7D"])),
            &data
        ));
        assert!(same(&arrange_by_order(&data, &[]), &data));
        assert_eq!(
            arrange_by_order(&data, &s(&["unknown"])).metric1_title,
            "SESSION 5H"
        );
    }
}
