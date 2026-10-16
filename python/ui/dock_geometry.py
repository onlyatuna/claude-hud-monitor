"""Pure geometry helpers for the right-edge docked HUD window (no Qt)."""

Rect = tuple[int, int, int, int]  # (x, y, w, h)


def _clamp(value: int, low: int, high: int) -> int:
    return max(low, min(value, high))


def dock_rect(work_area: Rect, width: int, height: int, bottom_offset: int = 12) -> Rect:
    """右緣貼齊 work_area 右緣、下緣離 work_area 下緣 bottom_offset 的矩形。
    width 夾在 [1, work_area.w]；height 夾在 [1, work_area.h]；bottom_offset 先夾在 [0, work_area.h - height]。
    結果必定完全落在 work_area 內。"""
    wx, wy, ww, wh = work_area
    w = _clamp(width, 1, ww)
    h = _clamp(height, 1, wh)
    offset = _clamp(bottom_offset, 0, wh - h)
    x = wx + ww - w
    y = wy + wh - offset - h
    return (x, y, w, h)


def constrain_to_edge(rect: Rect, work_area: Rect) -> Rect:
    """x 強制右緣貼齊（x = work_area 右緣 - w）；y 夾在 [work_area.y, work_area 下緣 - h]；w、h 保持
    （但 w、h 超過 work_area 時夾到 work_area 大小）。"""
    _, ry, rw, rh = rect
    wx, wy, ww, wh = work_area
    w = _clamp(rw, 1, ww)
    h = _clamp(rh, 1, wh)
    x = wx + ww - w
    y = _clamp(ry, wy, wy + wh - h)
    return (x, y, w, h)


def bottom_offset_from_rect(rect: Rect, work_area: Rect) -> int:
    """work_area 下緣 - rect 下緣，最小 0（rect 在工作區下方外面也回 0）。用來把拖曳後的位置存成設定。"""
    _, ry, _, rh = rect
    _, wy, _, wh = work_area
    return max(0, (wy + wh) - (ry + rh))


def pick_work_area(point: tuple[int, int], work_areas: list[Rect]) -> Rect:
    """回傳包含 point 的工作區；沒有任何一個包含時回傳「中心離 point 最近」的那個；
    work_areas 為空時 raise ValueError。多螢幕時 work_areas 的 x 可為負數。"""
    if not work_areas:
        raise ValueError("work_areas must not be empty")
    px, py = point
    for wa in work_areas:
        wx, wy, ww, wh = wa
        if wx <= px < wx + ww and wy <= py < wy + wh:
            return wa
    return min(
        work_areas,
        key=lambda wa: (px - (wa[0] + wa[2] / 2)) ** 2 + (py - (wa[1] + wa[3] / 2)) ** 2,
    )
