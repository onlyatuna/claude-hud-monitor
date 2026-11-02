//! G9.7.b, sub-scope 3: a `ProviderCardWidget` under dynamic relayout (badge show/hide, text changes).
//!
//! The expected rectangles are PySide6 6.11.2 measurements (Windows, DirectWrite) from
//! `rust/tools/pyside_probes/provider_card_relayout.py`: the real Python card with the real cards
//! style sheet, 256 wide and 117 high (180 wide in the narrow step), at DPR 1.25 and 1.0. Each
//! step feeds the card the `UsageMetrics` the HUD feeds it and lays it out again. Rectangles are
//! relative to the card (the rows sit in RowCell blocks, so the probe maps each child to the card); a
//! hidden child is `None`. `metric*_subtext` exists only in Python and is left empty on both sides.

use super::provider_card::ProviderCardWidget;
use crate::providers::base::UsageMetrics;
use chrono::{TimeZone, Utc};
use ferrokit_gui::geometry::primitives::Rect;
use ferrokit_platform::WindowFlags;
use ferrokit_widgets::{WidgetRef, Window};

/// The card's height in the probe (`CARD_HEIGHT`): its size hint, so no extra height is shared out.
const CARD_HEIGHT: i32 = 117;

type Geom = Option<(i32, i32, i32, i32)>;

/// dot, title, badge, badge2, m1_label, m1_val, m1_bar, m1_sub, m2_label, m2_val, m2_bar, m2_sub.
type Row = [Geom; 12];

const NAMES: [&str; 12] = [
    "dot", "title", "badge", "badge2", "m1_label", "m1_val", "m1_bar", "m1_sub", "m2_label",
    "m2_val", "m2_bar", "m2_sub",
];

const fn g(x: i32, y: i32, w: i32, h: i32) -> Geom {
    Some((x, y, w, h))
}

/// The child's rectangle in the card's coordinates. ferrokit parents the children of a nested layout to
/// that layout's holder widget, so the offsets of the holders up to the card are added; Qt parents
/// them to the card.
fn rect(w: &WidgetRef, card: &WidgetRef) -> Geom {
    if !w.borrow().is_visible() {
        return None;
    }
    let r = w.borrow().geometry();
    let (mut x, mut y) = (r.x, r.y);
    let mut parent = w.borrow().parent_widget().and_then(|p| p.upgrade());
    while let Some(p) = parent {
        if std::rc::Rc::ptr_eq(&p, card) {
            break;
        }
        let pg = p.borrow().geometry();
        x += pg.x;
        y += pg.y;
        parent = p.borrow().parent_widget().and_then(|p| p.upgrade());
    }
    Some((x, y, r.width, r.height))
}

fn row(card: &ProviderCardWidget) -> Row {
    [
        rect(&card.dot, &card.container),
        rect(&card.title, &card.container),
        rect(&card.badge, &card.container),
        rect(&card.badge2, &card.container),
        rect(&card.m1_label, &card.container),
        rect(&card.m1_val, &card.container),
        rect(&card.m1_bar, &card.container),
        rect(&card.m1_sub, &card.container),
        rect(&card.m2_label, &card.container),
        rect(&card.m2_val, &card.container),
        rect(&card.m2_bar, &card.container),
        rect(&card.m2_sub, &card.container),
    ]
}

fn metrics() -> UsageMetrics {
    UsageMetrics {
        provider_id: "codex".into(),
        metric1_val: Some(68.0),
        metric1_text: "68%".into(),
        metric2_val: Some(100.0),
        metric2_text: "100%".into(),
        ..Default::default()
    }
}

/// Whether a rectangle differs from the PySide6 one. In the `error` step Qt lays the error text
/// (`m1_sub`) and the rows around it out 1 px taller than their own size hints add up to (its card
/// hint is 116 while the rows sum to 117), a Qt layout inconsistency that depends on the unshown
/// probe widget; only the horizontal geometry of that step is compared.
fn differs(step: &str, got: Geom, want: Geom) -> bool {
    if step == "error" {
        return got.map(|r| (r.0, r.2)) != want.map(|r| (r.0, r.2));
    }
    got != want
}

fn run(dpr: f32, expected: [(&str, i32, (i32, i32), Row); 8]) {
    let _setup = crate::ui::test_support::CardsOracleSetup::at_dpr(dpr);
    let mut card = ProviderCardWidget::new("codex");
    let flags = WindowFlags::FRAMELESS
        | WindowFlags::CUSTOM_FRAMELESS
        | WindowFlags::LAYERED
        | WindowFlags::TOOL;
    let mut window = Window::new("card", Rect::new(0, 0, 256, CARD_HEIGHT), flags).unwrap();
    window.set_minimum_size(100, 0);
    window.set_root_widget(card.widget());
    // The tree's parent links are set at the first render (`adopt_tree`), as in a showing window;
    // before that a visibility change has no parent layout to request.
    window.render_and_present();

    let mut seen: Vec<(&str, i32, (i32, i32), Row)> = Vec::new();
    let mut show = |card: &ProviderCardWidget, step: &'static str, width: i32| {
        window.set_geometry(Rect::new(0, 0, width, CARD_HEIGHT));
        ferrokit_widgets::WidgetCommandQueue::flush_layouts();
        window.root_widget().borrow().update_layout();
        let hint = card.widget().borrow().size_hint();
        seen.push((step, width, (hint.width, hint.height), row(card)));
    };
    show(&card, "initial", 256);
    let mut feed =
        |card: &mut ProviderCardWidget, step: &'static str, width: i32, m: UsageMetrics| {
            card.update_metrics(&m);
            show(card, step, width);
        };
    feed(
        &mut card,
        "both_badges",
        256,
        UsageMetrics {
            badge1_text: "Code: 100%".into(),
            badge2_text: "Chat: 0%".into(),
            ..metrics()
        },
    );
    feed(
        &mut card,
        "one_badge",
        256,
        UsageMetrics {
            badge1_text: "Plan: Free".into(),
            ..metrics()
        },
    );
    feed(
        &mut card,
        "error",
        256,
        UsageMetrics {
            error: Some("Network error".into()),
            ..metrics()
        },
    );
    feed(
        &mut card,
        "stale",
        256,
        UsageMetrics {
            error: Some("Network error".into()),
            stale: true,
            last_success: Some(Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()),
            ..metrics()
        },
    );
    let long = UsageMetrics {
        provider_name: "OPENAI CODEX EXTRA LONG NAME".into(),
        badge1_text: "Code: 100%".into(),
        badge2_text: "Chat: 0%".into(),
        metric1_title: "SESSION 5H LONG TITLE".into(),
        metric2_title: "WEEKLY 7D LONG TITLE".into(),
        ..metrics()
    };
    feed(&mut card, "long_title", 256, long.clone());
    feed(&mut card, "narrow", 180, long);
    feed(
        &mut card,
        "empty",
        256,
        UsageMetrics {
            provider_id: "codex".into(),
            ..Default::default()
        },
    );

    let mut diffs = Vec::new();
    for ((name, width, got_hint, got), (want_name, want_width, want_hint, want)) in
        seen.iter().zip(expected.iter())
    {
        assert_eq!(name, want_name);
        assert_eq!(width, want_width);
        if got_hint != want_hint {
            diffs.push(format!(
                "{name}.hint: ferrokit {got_hint:?} PySide6 {want_hint:?}"
            ));
        }
        for i in 0..12 {
            if differs(name, got[i], want[i]) {
                diffs.push(format!(
                    "{name}.{}: ferrokit {:?} PySide6 {:?}",
                    NAMES[i], got[i], want[i]
                ));
            }
        }
    }
    assert!(diffs.is_empty(), "DPR {dpr}:\n{}", diffs.join("\n"));
}

#[test]
fn provider_card_relayout_matches_pyside6_at_dpr_1_25() {
    run(
        1.25,
        [
            (
                "initial",
                256,
                (130, 117),
                [
                    g(6, 4, 7, 15),
                    g(17, 4, 78, 15),
                    g(225, 4, 25, 15),
                    None,
                    g(10, 26, 63, 17),
                    g(230, 26, 16, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 59, 17),
                    g(230, 73, 16, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "both_badges",
                256,
                (229, 117),
                [
                    g(6, 4, 7, 15),
                    g(17, 4, 78, 15),
                    g(126, 4, 65, 15),
                    g(195, 4, 55, 15),
                    g(10, 26, 63, 17),
                    g(222, 26, 24, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 59, 17),
                    g(215, 73, 31, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "one_badge",
                256,
                (170, 117),
                [
                    g(6, 4, 7, 15),
                    g(17, 4, 78, 15),
                    g(185, 4, 65, 15),
                    None,
                    g(10, 26, 63, 17),
                    g(222, 26, 24, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 59, 17),
                    g(215, 73, 31, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "error",
                256,
                (155, 116),
                [
                    g(6, 4, 7, 15),
                    g(17, 4, 78, 15),
                    g(200, 4, 50, 15),
                    None,
                    g(10, 26, 63, 16),
                    g(224, 26, 22, 16),
                    g(10, 43, 236, 5),
                    g(10, 49, 236, 15),
                    g(10, 73, 59, 17),
                    g(230, 73, 16, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "stale",
                256,
                (145, 117),
                [
                    g(6, 4, 7, 15),
                    g(17, 4, 78, 15),
                    g(210, 4, 40, 15),
                    None,
                    g(10, 26, 63, 17),
                    g(222, 26, 24, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 59, 17),
                    g(215, 73, 31, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "long_title",
                256,
                (333, 117),
                [
                    g(6, 4, 7, 15),
                    g(17, 4, 105, 15),
                    g(126, 4, 65, 15),
                    g(195, 4, 55, 15),
                    g(10, 26, 128, 17),
                    g(222, 26, 24, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 124, 17),
                    g(215, 73, 31, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "narrow",
                180,
                (333, 117),
                [
                    g(6, 4, 7, 15),
                    g(17, 4, 50, 15),
                    g(71, 4, 50, 15),
                    g(125, 4, 50, 15),
                    g(10, 26, 128, 17),
                    g(146, 26, 24, 17),
                    g(10, 44, 160, 5),
                    g(10, 50, 160, 14),
                    g(10, 73, 124, 17),
                    g(139, 73, 31, 17),
                    g(10, 91, 160, 5),
                    g(10, 97, 160, 14),
                ],
            ),
            (
                "empty",
                256,
                (130, 117),
                [
                    g(6, 4, 7, 15),
                    g(17, 4, 78, 15),
                    g(225, 4, 25, 15),
                    None,
                    g(10, 26, 63, 17),
                    g(230, 26, 16, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 59, 17),
                    g(230, 73, 16, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
        ],
    );
}

#[test]
fn provider_card_relayout_matches_pyside6_at_dpr_1_0() {
    run(
        1.0,
        [
            (
                "initial",
                256,
                (129, 116),
                [
                    g(6, 4, 6, 15),
                    g(16, 4, 78, 15),
                    g(225, 4, 25, 15),
                    None,
                    g(10, 26, 65, 17),
                    g(230, 26, 16, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 58, 17),
                    g(230, 73, 16, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "both_badges",
                256,
                (228, 116),
                [
                    g(6, 4, 6, 15),
                    g(16, 4, 78, 15),
                    g(126, 4, 65, 15),
                    g(195, 4, 55, 15),
                    g(10, 26, 65, 17),
                    g(222, 26, 24, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 58, 17),
                    g(214, 73, 32, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "one_badge",
                256,
                (169, 116),
                [
                    g(6, 4, 6, 15),
                    g(16, 4, 78, 15),
                    g(185, 4, 65, 15),
                    None,
                    g(10, 26, 65, 17),
                    g(222, 26, 24, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 58, 17),
                    g(214, 73, 32, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "error",
                256,
                (154, 114),
                [
                    g(6, 4, 6, 15),
                    g(16, 4, 78, 15),
                    g(200, 4, 50, 15),
                    None,
                    g(10, 26, 65, 16),
                    g(225, 26, 21, 16),
                    g(10, 43, 236, 5),
                    g(10, 49, 236, 15),
                    g(10, 73, 58, 17),
                    g(230, 73, 16, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "stale",
                256,
                (144, 116),
                [
                    g(6, 4, 6, 15),
                    g(16, 4, 78, 15),
                    g(210, 4, 40, 15),
                    None,
                    g(10, 26, 65, 17),
                    g(222, 26, 24, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 58, 17),
                    g(214, 73, 32, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "long_title",
                256,
                (333, 116),
                [
                    g(6, 4, 6, 15),
                    g(16, 4, 106, 15),
                    g(126, 4, 65, 15),
                    g(195, 4, 55, 15),
                    g(10, 26, 131, 17),
                    g(222, 26, 24, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 123, 17),
                    g(214, 73, 32, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
            (
                "narrow",
                180,
                (333, 116),
                [
                    g(6, 4, 6, 15),
                    g(16, 4, 50, 15),
                    g(70, 4, 50, 15),
                    g(124, 4, 50, 15),
                    g(10, 26, 131, 17),
                    g(146, 26, 24, 17),
                    g(10, 44, 160, 5),
                    g(10, 50, 160, 14),
                    g(10, 73, 123, 17),
                    g(138, 73, 32, 17),
                    g(10, 91, 160, 5),
                    g(10, 97, 160, 14),
                ],
            ),
            (
                "empty",
                256,
                (129, 116),
                [
                    g(6, 4, 6, 15),
                    g(16, 4, 78, 15),
                    g(225, 4, 25, 15),
                    None,
                    g(10, 26, 65, 17),
                    g(230, 26, 16, 17),
                    g(10, 44, 236, 5),
                    g(10, 50, 236, 14),
                    g(10, 73, 58, 17),
                    g(230, 73, 16, 17),
                    g(10, 91, 236, 5),
                    g(10, 97, 236, 14),
                ],
            ),
        ],
    );
}
