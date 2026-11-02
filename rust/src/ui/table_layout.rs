//! Size rules for the Modern Table window (pure functions; Python `ui/table_layout.py`).
//!
//! Above `FULL_MIN` the window shows the full table. Dragged below it, the window collapses to one dial per
//! service; it only expands again once it is `HYSTERESIS` pixels past the threshold on both sides, so a
//! window resting on the boundary does not flip back and forth.

pub const FULL_MIN_W: i32 = 380;
pub const FULL_MIN_H: i32 = 280;
pub const HYSTERESIS: i32 = 16;

/// Smallest dial (the full table's is 84).
pub const COMPACT_DIAL_MIN: i32 = 48;
/// Between dials.
pub const COMPACT_GAP: i32 = 8;
/// Window content margin while collapsed.
pub const COMPACT_MARGIN: i32 = 6;
/// The panel's 1 px border on each side.
pub const COMPACT_FRAME: i32 = 2;
pub const DEF_COMPACT_W: i32 = 240;
pub const DEF_COMPACT_H: i32 = 96;

/// True when the table window should show the collapsed dial row at this size.
pub fn table_state(width: i32, height: i32, was_compact: bool) -> bool {
    if was_compact {
        !(width >= FULL_MIN_W + HYSTERESIS && height >= FULL_MIN_H + HYSTERESIS)
    } else {
        width < FULL_MIN_W || height < FULL_MIN_H
    }
}

/// Smallest window that still gives each of `count` dials `COMPACT_DIAL_MIN` pixels.
pub fn compact_min_size(count: i32) -> (i32, i32) {
    let pad = 2 * COMPACT_MARGIN + COMPACT_FRAME;
    (
        count * COMPACT_DIAL_MIN + (count - 1).max(0) * COMPACT_GAP + pad,
        COMPACT_DIAL_MIN + pad,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_size_threshold_is_the_old_minimum() {
        assert_eq!((FULL_MIN_W, FULL_MIN_H), (380, 280));
    }

    #[test]
    fn collapses_below_either_dimension() {
        assert!(!table_state(380, 280, false));
        assert!(table_state(379, 400, false));
        assert!(table_state(600, 279, false));
        assert!(table_state(200, 90, false));
    }

    #[test]
    fn full_window_stays_full_above_the_threshold() {
        assert!(!table_state(450, 350, false));
        assert!(!table_state(380, 280, false));
    }

    #[test]
    fn compact_window_needs_the_hysteresis_margin_to_expand() {
        let margin = HYSTERESIS;
        assert_eq!(margin, 16);
        assert!(table_state(380, 280, true));
        assert!(table_state(380 + margin - 1, 280 + margin, true));
        assert!(table_state(380 + margin, 280 + margin - 1, true));
        assert!(!table_state(380 + margin, 280 + margin, true));
        assert!(!table_state(800, 600, true));
    }

    #[test]
    fn no_flicker_while_resizing_across_the_threshold() {
        let mut state = false;
        let mut seen = Vec::new();
        let widths = (360..=420).rev().step_by(4).chain((360..420).step_by(4));
        for width in widths {
            state = table_state(width, 400, state);
            seen.push(state);
        }
        let flips = seen.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(flips, 2);
    }

    #[test]
    fn compact_minimum_size_is_small_but_holds_three_dials() {
        let (w, h) = compact_min_size(3);
        assert!(w < FULL_MIN_W);
        assert!(h < FULL_MIN_H);
        assert!(w >= 3 * COMPACT_DIAL_MIN);
        assert!(h >= COMPACT_DIAL_MIN);
        assert!(compact_min_size(4).0 > w);
        assert_eq!((w, h), (174, 62));
    }

    #[test]
    fn default_compact_size_is_not_below_the_minimum() {
        let (w, h) = compact_min_size(3);
        assert!(DEF_COMPACT_W >= w);
        assert!(DEF_COMPACT_H >= h);
    }
}
