//! Rounded-rectangle window mask as pixel-row rectangles (pure geometry; Python `ui/round_mask.py`).
//!
//! Windows' Acrylic blurs the whole window rectangle, so the corners that the panel's rounded border cuts
//! off would still show square blur. Clipping the window itself to this shape cuts the blur with it. The
//! edge is not anti-aliased.

/// `(x, y, width, height)`.
pub type MaskRect = (i32, i32, i32, i32);

/// Non-overlapping rectangles that cover a `width` x `height` box with rounded corners.
pub fn rounded_rects(width: i32, height: i32, radius: i32) -> Vec<MaskRect> {
    if width <= 0 || height <= 0 {
        return Vec::new();
    }
    let r = radius.min(width / 2).min(height / 2).max(0);
    if r == 0 {
        return vec![(0, 0, width, height)];
    }

    // A pixel is kept when its centre lies inside the corner circle.
    let insets: Vec<i32> = (0..r)
        .map(|y| {
            let dy = f64::from(r - y) - 0.5;
            let rf = f64::from(r);
            let dx = (rf * rf - dy * dy).max(0.0).sqrt();
            ((rf - dx - 0.5).ceil() as i32).clamp(0, r)
        })
        .collect();

    // Merge consecutive rows with the same inset: (inset, first row, rows).
    let mut bands: Vec<(i32, i32, i32)> = Vec::new();
    for (y, &inset) in insets.iter().enumerate() {
        match bands.last_mut() {
            Some(last) if last.0 == inset => last.2 += 1,
            _ => bands.push((inset, y as i32, 1)),
        }
    }

    let mut rects = Vec::new();
    for &(inset, y, rows) in &bands {
        if width - 2 * inset > 0 {
            rects.push((inset, y, width - 2 * inset, rows));
        }
    }
    if height - 2 * r > 0 {
        rects.push((0, r, width, height - 2 * r));
    }
    for &(inset, y, rows) in bands.iter().rev() {
        if width - 2 * inset > 0 {
            rects.push((inset, height - y - rows, width - 2 * inset, rows));
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covered(rects: &[MaskRect], x: i32, y: i32) -> bool {
        rects
            .iter()
            .any(|&(rx, ry, rw, rh)| rx <= x && x < rx + rw && ry <= y && y < ry + rh)
    }

    #[test]
    fn corners_are_cut_and_edges_and_centre_are_kept() {
        let (w, h) = (450, 350);
        let rects = rounded_rects(w, h, 12);
        for corner in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            assert!(!covered(&rects, corner.0, corner.1), "{corner:?}");
        }
        for kept in [
            (w / 2, 0),
            (w / 2, h - 1),
            (0, h / 2),
            (w - 1, h / 2),
            (w / 2, h / 2),
            (12, 12),
            (4, 4),
        ] {
            assert!(covered(&rects, kept.0, kept.1), "{kept:?}");
        }
        assert!(!covered(&rects, 2, 2));
    }

    #[test]
    fn shape_is_symmetric() {
        let (w, h) = (200, 120);
        let rects = rounded_rects(w, h, 12);
        for x in 0..24 {
            for y in 0..24 {
                assert_eq!(covered(&rects, x, y), covered(&rects, w - 1 - x, y));
                assert_eq!(covered(&rects, x, y), covered(&rects, x, h - 1 - y));
            }
        }
    }

    #[test]
    fn rects_stay_inside_and_do_not_overlap() {
        for (w, h, r) in [(450, 350, 12), (174, 62, 12), (100, 40, 40), (30, 30, 12)] {
            let mut cells = std::collections::HashSet::new();
            for (rx, ry, rw, rh) in rounded_rects(w, h, r) {
                assert!(rx >= 0 && ry >= 0 && rx + rw <= w && ry + rh <= h);
                assert!(rw > 0 && rh > 0);
                for y in ry..ry + rh {
                    for x in rx..rx + rw {
                        assert!(cells.insert((x, y)), "overlap at {x},{y}");
                    }
                }
            }
        }
    }

    #[test]
    fn area_matches_a_rounded_rectangle() {
        let (w, h, r) = (450, 350, 12);
        let area: i32 = rounded_rects(w, h, r)
            .iter()
            .map(|&(_, _, rw, rh)| rw * rh)
            .sum();
        let expected = f64::from(w * h) - (4.0 - std::f64::consts::PI) * f64::from(r * r);
        assert!((f64::from(area) / expected - 1.0).abs() < 0.002);
    }

    #[test]
    fn radius_is_clamped_to_half_the_short_side() {
        let rects = rounded_rects(100, 40, 400);
        assert!(covered(&rects, 50, 20));
        assert!(!covered(&rects, 0, 0));
        assert!(covered(&rects, 50, 0));
    }

    #[test]
    fn zero_radius_is_the_plain_rectangle() {
        assert_eq!(rounded_rects(80, 50, 0), vec![(0, 0, 80, 50)]);
    }

    #[test]
    fn degenerate_sizes_give_no_rects() {
        assert!(rounded_rects(0, 50, 12).is_empty());
        assert!(rounded_rects(50, 0, 12).is_empty());
    }
}
