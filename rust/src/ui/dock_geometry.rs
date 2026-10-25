#![allow(dead_code)]
//! Pure geometry helpers for the docked HUD window (port of python/ui/dock_geometry.py, no GUI deps).

/// (x, y, w, h)
pub type Rect = (i32, i32, i32, i32);

pub const EDGES: [&str; 4] = ["left", "right", "top", "bottom"];
pub const SCALE_MIN: f64 = 0.6;
pub const SCALE_MAX: f64 = 2.0;
pub const SCALE_STEP: f64 = 0.1;
pub const DEFAULT_SNAP_THRESHOLD: i32 = 24;
pub const DEFAULT_MARGIN: i32 = 12;

/// Python's offset: an int distance from the start of the edge, or "end" (flush to the far end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offset {
    Start(i32),
    End,
}

/// Python `_clamp`: `max(low, min(value, high))` (low wins when high < low).
fn clamp(value: i32, low: i32, high: i32) -> i32 {
    low.max(value.min(high))
}

fn edge_name(name: &str) -> Option<&'static str> {
    EDGES.iter().copied().find(|e| *e == name)
}

/// Python `dock_rect`: flush to the work area's right edge, `bottom_offset` above its bottom edge.
pub fn dock_rect(work: Rect, width: i32, height: i32, bottom_offset: i32) -> Rect {
    let (wx, wy, ww, wh) = work;
    let w = clamp(width, 1, ww);
    let h = clamp(height, 1, wh);
    let offset = clamp(bottom_offset, 0, wh - h);
    (wx + ww - w, wy + wh - offset - h, w, h)
}

/// Python `constrain_to_edge`: x forced flush right, y clamped inside the work area.
pub fn constrain_to_edge(rect: Rect, work: Rect) -> Rect {
    let (_, ry, rw, rh) = rect;
    let (wx, wy, ww, wh) = work;
    let w = clamp(rw, 1, ww);
    let h = clamp(rh, 1, wh);
    (wx + ww - w, clamp(ry, wy, wy + wh - h), w, h)
}

/// Python `bottom_offset_from_rect`.
pub fn bottom_offset_from_rect(rect: Rect, work: Rect) -> i32 {
    let (_, ry, _, rh) = rect;
    let (_, wy, _, wh) = work;
    0.max((wy + wh) - (ry + rh))
}

/// Python `pick_work_area`: containing area, else the one whose centre is nearest; None when empty.
pub fn pick_work_area(point: (i32, i32), work_areas: &[Rect]) -> Option<Rect> {
    let (px, py) = point;
    if let Some(wa) = work_areas
        .iter()
        .find(|(wx, wy, ww, wh)| *wx <= px && px < wx + ww && *wy <= py && py < wy + wh)
    {
        return Some(*wa);
    }
    let dist = |wa: &Rect| {
        let dx = px as f64 - (wa.0 as f64 + wa.2 as f64 / 2.0);
        let dy = py as f64 - (wa.1 as f64 + wa.3 as f64 / 2.0);
        dx * dx + dy * dy
    };
    let mut best: Option<Rect> = None;
    for wa in work_areas {
        // strict `<` keeps the first minimum, like python's min()
        if best.map_or(true, |b| dist(wa) < dist(&b)) {
            best = Some(*wa);
        }
    }
    best
}

fn edge_distances(rect: Rect, work: Rect) -> [(&'static str, i32); 4] {
    let (rx, ry, rw, rh) = rect;
    let (wx, wy, ww, wh) = work;
    [
        ("left", rx - wx),
        ("right", (wx + ww) - (rx + rw)),
        ("top", ry - wy),
        ("bottom", (wy + wh) - (ry + rh)),
    ]
}

/// First minimum by distance (python `min(key=...)` keeps the earliest on ties).
fn nearest(candidates: &[(&'static str, i32)]) -> &'static str {
    let mut best = candidates[0];
    for c in &candidates[1..] {
        if c.1 < best.1 {
            best = *c;
        }
    }
    best.0
}

/// Python `snap_rect`.
pub fn snap_rect(rect: Rect, work: Rect, threshold: i32) -> (Rect, Option<&'static str>) {
    let (rx, ry, rw, rh) = rect;
    let (wx, wy, ww, wh) = work;
    let w = clamp(rw, 1, ww);
    let h = clamp(rh, 1, wh);
    let mut x = clamp(rx, wx, wx + ww - w);
    let mut y = clamp(ry, wy, wy + wh - h);

    let candidates: Vec<(&'static str, i32)> = edge_distances(rect, work)
        .into_iter()
        .filter(|(_, d)| *d <= threshold)
        .collect();
    if candidates.is_empty() {
        return ((x, y, w, h), None);
    }
    let best = nearest(&candidates);
    match best {
        "left" => x = wx,
        "right" => x = wx + ww - w,
        "top" => y = wy,
        _ => y = wy + wh - h,
    }
    ((x, y, w, h), Some(best))
}

/// Python `snap_rect_corner`: corner-aware snap honouring `prefer` ("vertical"/"horizontal"),
/// also reporting the offset along the chosen edge.
#[allow(unused_assignments)] // mirrors python's assign-then-adjust flow
pub fn snap_rect_corner(
    rect: Rect,
    work: Rect,
    threshold: i32,
    prefer: Option<&str>,
) -> (Rect, Option<&'static str>, Option<Offset>) {
    let (snapped, _) = snap_rect(rect, work, threshold);
    let (rx, ry, _, rh) = rect;
    let rw = rect.2;
    let (wx, wy, ww, wh) = work;
    let mut candidates: Vec<(&'static str, i32)> = edge_distances(rect, work)
        .into_iter()
        .filter(|(_, d)| *d <= threshold)
        .collect();
    if candidates.is_empty() {
        return (snapped, None, None);
    }
    if let Some(p @ ("vertical" | "horizontal")) = prefer {
        let axis: [&str; 2] = if p == "vertical" { ["left", "right"] } else { ["top", "bottom"] };
        let preferred: Vec<_> = candidates.iter().copied().filter(|(e, _)| axis.contains(e)).collect();
        if !preferred.is_empty() {
            candidates = preferred;
        }
    }
    let edge = nearest(&candidates);
    let (mut x, mut y, w, h) = snapped;
    let offset;
    if edge == "left" || edge == "right" {
        x = if edge == "left" { wx } else { wx + ww - w };
        let to_end = (wy + wh) - (ry + rh);
        let near_end = to_end <= threshold;
        let near_start = ry - wy <= threshold;
        y = clamp(ry, wy, wy + wh - h);
        if near_end && (to_end <= ry - wy || !near_start) {
            y = wy + wh - h;
            offset = Offset::End;
        } else if near_start {
            y = wy;
            offset = Offset::Start(0);
        } else {
            offset = Offset::Start(y - wy);
        }
    } else {
        y = if edge == "top" { wy } else { wy + wh - h };
        let to_end = (wx + ww) - (rx + rw);
        let near_end = to_end <= threshold;
        let near_start = rx - wx <= threshold;
        x = clamp(rx, wx, wx + ww - w);
        if near_end && (to_end <= rx - wx || !near_start) {
            x = wx + ww - w;
            offset = Offset::End;
        } else if near_start {
            x = wx;
            offset = Offset::Start(0);
        } else {
            offset = Offset::Start(x - wx);
        }
    }
    ((x, y, w, h), Some(edge), Some(offset))
}

/// Python `orientation_for_edge`: left/right -> vertical, top/bottom -> horizontal, else `current`.
pub fn orientation_for_edge(edge: Option<&str>, current: &'static str) -> &'static str {
    match edge {
        Some("left") | Some("right") => "vertical",
        Some("top") | Some("bottom") => "horizontal",
        _ => current,
    }
}

/// Python `dock_to_edge`; None for an invalid edge (python raises ValueError).
pub fn dock_to_edge(
    work: Rect,
    edge: &str,
    width: i32,
    height: i32,
    offset: Option<Offset>,
    margin: i32,
) -> Option<Rect> {
    let edge = edge_name(edge)?;
    let (wx, wy, ww, wh) = work;
    let w = clamp(width, 1, ww);
    let h = clamp(height, 1, wh);
    let raw = |len: i32| match offset {
        None => len - margin,
        Some(Offset::End) => len,
        Some(Offset::Start(n)) => n,
    };
    if edge == "left" || edge == "right" {
        let o = clamp(raw(wh - h), 0, wh - h);
        let x = if edge == "left" { wx } else { wx + ww - w };
        Some((x, wy + o, w, h))
    } else {
        let o = clamp(raw(ww - w), 0, ww - w);
        let y = if edge == "top" { wy } else { wy + wh - h };
        Some((wx + o, y, w, h))
    }
}

/// Python `offset_along_edge`; None for an invalid edge.
pub fn offset_along_edge(rect: Rect, work: Rect, edge: &str) -> Option<i32> {
    let edge = edge_name(edge)?;
    let (rx, ry, _, _) = rect;
    let (wx, wy, _, _) = work;
    Some(if edge == "left" || edge == "right" {
        0.max(ry - wy)
    } else {
        0.max(rx - wx)
    })
}

/// Python `clamp_scale`: NaN -> 1.0; clamp to [SCALE_MIN, SCALE_MAX]; round to 0.1.
pub fn clamp_scale(scale: f64) -> f64 {
    if scale.is_nan() {
        return 1.0;
    }
    let clamped = SCALE_MIN.max(scale.min(SCALE_MAX));
    (clamped * 10.0).round() / 10.0
}

/// Python `step_scale`.
pub fn step_scale(scale: f64, steps: i32) -> f64 {
    clamp_scale(scale + steps as f64 * SCALE_STEP)
}

/// Python `round()` for floats: round half to even.
fn py_round(x: f64) -> i32 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        (r - x.signum()) as i32
    } else {
        r as i32
    }
}

/// Python `scale_size`.
pub fn scale_size(base_w: i32, base_h: i32, scale: f64) -> (i32, i32) {
    let s = clamp_scale(scale);
    (
        1.max(py_round(base_w as f64 * s)),
        1.max(py_round(base_h as f64 * s)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WA: Rect = (0, 0, 1920, 1040);

    fn dte(wa: Rect, e: &str, w: i32, h: i32, o: Option<Offset>) -> Rect {
        dock_to_edge(wa, e, w, h, o, DEFAULT_MARGIN).unwrap()
    }

    #[test]
    fn dock_rect_standard_1920x1040() {
        assert_eq!(dock_rect(WA, 92, 270, 12), (1828, 758, 92, 270));
    }

    #[test]
    fn dock_rect_secondary_monitor_offset() {
        let rect = dock_rect((1920, 0, 1920, 1080), 92, 270, 12);
        assert_eq!(rect.0, 1920 + 1920 - 92);
        assert_eq!(rect, (3748, 798, 92, 270));
    }

    #[test]
    fn dock_rect_clamps_oversized_dimensions_and_offset() {
        let wa = (100, 50, 800, 600);
        assert_eq!(dock_rect(wa, 92, 900, 12), (100 + 800 - 92, 50, 92, 600));
        assert_eq!(dock_rect(wa, 1200, 200, 12), (100, 50 + 600 - 12 - 200, 800, 200));
        assert_eq!(dock_rect(wa, 92, 200, 9999), (100 + 800 - 92, 50, 92, 200));
        assert_eq!(dock_rect(wa, 0, -10, -5), (100 + 800 - 1, 50 + 600 - 1, 1, 1));
    }

    #[test]
    fn dock_rect_dpi_scaling_scenarios() {
        let wa_125 = (0, 0, 1536, 816);
        let r = dock_rect(wa_125, 92, 270, 12);
        assert_eq!(r, (1444, 534, 92, 270));
        assert_eq!(r.0 + r.2, wa_125.0 + wa_125.2);
        assert!(r.0 >= wa_125.0);
        assert!(r.1 >= wa_125.1);
        assert!(r.1 + r.3 <= wa_125.1 + wa_125.3);

        let wa_150 = (0, 0, 1280, 680);
        let r = dock_rect(wa_150, 92, 270, 12);
        assert_eq!(r, (1188, 398, 92, 270));
        assert_eq!(r.0 + r.2, wa_150.0 + wa_150.2);
        assert!(r.0 >= wa_150.0);
        assert!(r.1 >= wa_150.1);
        assert!(r.1 + r.3 <= wa_150.1 + wa_150.3);
    }

    #[test]
    fn constrain_to_edge_cases() {
        let wa = (0, 40, 1920, 1000);
        assert_eq!(constrain_to_edge((500, 300, 92, 270), wa), (1828, 300, 92, 270));
        assert_eq!(constrain_to_edge((500, -20, 92, 270), wa), (1828, 40, 92, 270));
        assert_eq!(constrain_to_edge((500, 900, 92, 270), wa), (1828, 1040 - 270, 92, 270));
        assert_eq!(constrain_to_edge((10, 10, 3000, 2000), wa), (0, 40, 1920, 1000));
    }

    #[test]
    fn bottom_offset_from_rect_cases() {
        assert_eq!(bottom_offset_from_rect((1828, 758, 92, 270), WA), 12);
        assert_eq!(bottom_offset_from_rect((1828, 770, 92, 270), WA), 0);
        assert_eq!(bottom_offset_from_rect((1828, 850, 92, 270), WA), 0);
    }

    #[test]
    fn pick_work_area_cases() {
        let primary = (0, 0, 1920, 1040);
        let secondary_right = (1920, 0, 1920, 1080);
        let secondary_left = (-1920, 0, 1920, 1080);
        let areas = [primary, secondary_right, secondary_left];
        assert_eq!(pick_work_area((2500, 500), &areas), Some(secondary_right));
        assert_eq!(pick_work_area((-500, 500), &areas), Some(secondary_left));
        assert_eq!(pick_work_area((1920, 500), &areas), Some(secondary_right));

        let gapped = [(0, 0, 1920, 1040), (2100, 0, 1920, 1040)];
        assert_eq!(pick_work_area((1950, 520), &gapped), Some(gapped[0]));
        assert_eq!(pick_work_area((2050, 520), &gapped), Some(gapped[1]));

        assert_eq!(pick_work_area((100, 100), &[]), None);
    }

    #[test]
    fn snap_rect_cases() {
        let t = DEFAULT_SNAP_THRESHOLD;
        assert_eq!(snap_rect((1900, 300, 64, 200), WA, t), ((1856, 300, 64, 200), Some("right")));
        assert_eq!(snap_rect((10, 500, 64, 200), WA, t), ((0, 500, 64, 200), Some("left")));
        assert_eq!(snap_rect((500, 8, 200, 64), WA, t), ((500, 0, 200, 64), Some("top")));
        assert_eq!(snap_rect((500, 980, 200, 64), WA, t), ((500, 976, 200, 64), Some("bottom")));
        assert_eq!(snap_rect((800, 400, 64, 200), WA, t), ((800, 400, 64, 200), None));
        assert_eq!(snap_rect((5, 5, 64, 200), WA, t), ((0, 5, 64, 200), Some("left")));
        assert_eq!(snap_rect((-500, -100, 64, 200), WA, t), ((0, 0, 64, 200), Some("left")));
        let wa_sec = (1920, 0, 1920, 1080);
        assert_eq!(snap_rect((1930, 200, 64, 200), wa_sec, t), ((1920, 200, 64, 200), Some("left")));
        assert_eq!(snap_rect((3800, 200, 64, 200), wa_sec, t), ((3776, 200, 64, 200), Some("right")));
    }

    #[test]
    fn orientation_for_edge_cases() {
        assert_eq!(orientation_for_edge(Some("left"), "vertical"), "vertical");
        assert_eq!(orientation_for_edge(Some("right"), "horizontal"), "vertical");
        assert_eq!(orientation_for_edge(Some("top"), "vertical"), "horizontal");
        assert_eq!(orientation_for_edge(Some("bottom"), "vertical"), "horizontal");
        assert_eq!(orientation_for_edge(None, "vertical"), "vertical");
        assert_eq!(orientation_for_edge(None, "horizontal"), "horizontal");
        assert_eq!(orientation_for_edge(Some("unknown"), "horizontal"), "horizontal");
        assert_eq!(orientation_for_edge(Some("invalid"), "vertical"), "vertical");
    }

    #[test]
    fn dock_to_edge_cases() {
        assert_eq!(dte(WA, "right", 92, 270, None), dock_rect(WA, 92, 270, 12));
        let wa_offset = (100, 50, 1920, 1040);
        assert_eq!(dte(wa_offset, "left", 92, 270, Some(Offset::Start(100))), (100, 150, 92, 270));
        assert_eq!(dte(WA, "top", 270, 64, None), (1920 - 270 - 12, 0, 270, 64));
        assert_eq!(
            dte(WA, "bottom", 270, 64, Some(Offset::Start(99999))),
            (1920 - 270, 1040 - 64, 270, 64)
        );
        assert_eq!(dte(WA, "left", 5000, 5000, None), (0, 0, 1920, 1040));
        assert_eq!(dock_to_edge(WA, "center", 92, 270, None, DEFAULT_MARGIN), None);

        for wa in [(0, 0, 1536, 816), (0, 0, 1280, 680)] {
            for edge in EDGES {
                let r = dte(wa, edge, 92, 270, None);
                assert!(r.0 >= wa.0);
                assert!(r.1 >= wa.1);
                assert!(r.0 + r.2 <= wa.0 + wa.2);
                assert!(r.1 + r.3 <= wa.1 + wa.3);
            }
        }
    }

    #[test]
    fn offset_along_edge_round_trip() {
        let wa = (100, 50, 1920, 1040);
        for edge in EDGES {
            let (w, h) = if edge == "left" || edge == "right" { (92, 270) } else { (270, 64) };
            let rect = dte(wa, edge, w, h, Some(Offset::Start(180)));
            assert_eq!(offset_along_edge(rect, wa, edge), Some(180));
        }
        assert_eq!(offset_along_edge((0, 0, 92, 270), wa, "invalid"), None);
    }

    #[test]
    fn scale_helpers() {
        assert_eq!(clamp_scale(0.3), 0.6);
        assert_eq!(clamp_scale(5.0), 2.0);
        assert_eq!(clamp_scale(1.04), 1.0);
        assert_eq!(clamp_scale(1.06), 1.1);
        assert_eq!(clamp_scale(f64::NAN), 1.0);

        assert_eq!(step_scale(1.0, 3), 1.3);
        assert_eq!(step_scale(0.6, -1), 0.6);
        assert_eq!(step_scale(2.0, 1), 2.0);

        assert_eq!(scale_size(64, 68, 1.5), (96, 102));
        assert_eq!(scale_size(64, 68, 0.1), (38, 41));
    }

    #[test]
    fn py_round_is_bankers() {
        assert_eq!(py_round(0.5), 0);
        assert_eq!(py_round(1.5), 2);
        assert_eq!(py_round(2.5), 2);
        assert_eq!(py_round(-0.5), 0);
        assert_eq!(py_round(-1.5), -2);
        assert_eq!(py_round(38.4), 38);
    }

    // ---- TestSnapCorner ----

    #[test]
    fn vertical_bar_in_bottom_right_corner_stays_vertical() {
        let (rect, edge, offset) = snap_rect_corner((1850, 790, 64, 240), WA, 24, Some("vertical"));
        assert_eq!((edge, offset), (Some("right"), Some(Offset::End)));
        assert_eq!(rect, (1856, 800, 64, 240));
    }

    #[test]
    fn horizontal_bar_in_bottom_right_corner_stays_horizontal() {
        let (rect, edge, offset) = snap_rect_corner((1700, 975, 240, 64), WA, 24, Some("horizontal"));
        assert_eq!((edge, offset), (Some("bottom"), Some(Offset::End)));
        assert_eq!(rect, (1680, 976, 240, 64));
    }

    #[test]
    fn without_preference_nearest_edge_wins() {
        let (_, edge, _) = snap_rect_corner((1850, 790, 64, 240), WA, 24, None);
        assert_eq!(edge, Some("right"));
        let (_, edge, _) = snap_rect_corner((1840, 800, 64, 240), WA, 24, None);
        assert_eq!(edge, Some("bottom"));
    }

    #[test]
    fn top_corners_and_plain_edge_offset() {
        assert_eq!(
            snap_rect_corner((1850, 6, 64, 240), WA, 24, Some("vertical")),
            ((1856, 0, 64, 240), Some("right"), Some(Offset::Start(0)))
        );
        assert_eq!(
            snap_rect_corner((1850, 300, 64, 240), WA, 24, Some("vertical")),
            ((1856, 300, 64, 240), Some("right"), Some(Offset::Start(300)))
        );
    }

    #[test]
    fn nothing_near_returns_none() {
        let (rect, edge, offset) = snap_rect_corner((800, 400, 64, 240), WA, 24, Some("vertical"));
        assert_eq!((edge, offset), (None, None));
        assert_eq!(rect, (800, 400, 64, 240));
    }

    #[test]
    fn dock_to_edge_accepts_end() {
        assert_eq!(dte(WA, "right", 64, 240, Some(Offset::End)), (1856, 800, 64, 240));
        assert_eq!(dte(WA, "bottom", 240, 64, Some(Offset::End)), (1680, 976, 240, 64));
    }
}
