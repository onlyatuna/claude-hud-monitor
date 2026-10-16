"""Pure geometry helpers for the right-edge docked HUD window (no Qt)."""

import math

Rect = tuple[int, int, int, int]  # (x, y, w, h)
EDGES = ("left", "right", "top", "bottom")
SCALE_MIN, SCALE_MAX, SCALE_STEP = 0.6, 2.0, 0.1


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


def snap_rect(rect: Rect, work_area: Rect, threshold: int = 24) -> tuple[Rect, "str | None"]:
    """各邊距離：left = rect.x - wa.x；right = (wa.x + wa.w) - (rect.x + rect.w)；top = rect.y - wa.y；bottom = (wa.y + wa.h) - (rect.y + rect.h)。
    距離 <= threshold（含負值，也就是已超出該邊）的邊是候選；取距離最小者；距離相同時優先順序 left、right、top、bottom。
    有候選：回傳 (貼齊該邊的矩形, 邊名)。貼齊＝該軸對齊那一邊，另一軸夾在工作區內（rect 比工作區大時夾到工作區大小）。
    沒有候選：回傳 (完全夾在工作區內的矩形, None)。w、h 一律保持（除非超過工作區才夾）。"""
    rx, ry, rw, rh = rect
    wx, wy, ww, wh = work_area
    w = _clamp(rw, 1, ww)
    h = _clamp(rh, 1, wh)
    x = _clamp(rx, wx, wx + ww - w)
    y = _clamp(ry, wy, wy + wh - h)

    distances = (
        ("left", rx - wx),
        ("right", (wx + ww) - (rx + rw)),
        ("top", ry - wy),
        ("bottom", (wy + wh) - (ry + rh)),
    )
    candidates = [(edge, dist) for edge, dist in distances if dist <= threshold]
    if not candidates:
        return ((x, y, w, h), None)

    best_edge, _ = min(candidates, key=lambda item: item[1])
    if best_edge == "left":
        x = wx
    elif best_edge == "right":
        x = wx + ww - w
    elif best_edge == "top":
        y = wy
    elif best_edge == "bottom":
        y = wy + wh - h
    return ((x, y, w, h), best_edge)


def orientation_for_edge(edge: "str | None", current: str = "vertical") -> str:
    """left/right -> "vertical"；top/bottom -> "horizontal"；None（或不認得的值）-> current。"""
    if edge in ("left", "right"):
        return "vertical"
    if edge in ("top", "bottom"):
        return "horizontal"
    return current


def dock_to_edge(work_area: Rect, edge: str, width: int, height: int, offset: "int | None" = None, margin: int = 12) -> Rect:
    """依邊產生貼齊矩形。offset 是沿著該邊、從起點算起的距離（left/right 邊從工作區上緣算起；top/bottom 邊從工作區左緣算起）。
    offset 為 None 時採預設：left/right 邊貼底（下緣離工作區下緣 margin）、top/bottom 邊貼右（右緣離工作區右緣 margin）。
    width、height 先夾在 [1, 工作區對應邊長]；offset 夾在 [0, 邊長 - 矩形長]；結果必定完全落在工作區內。edge 不合法 raise ValueError。"""
    if edge not in EDGES:
        raise ValueError(f"Invalid edge: {edge!r}")
    wx, wy, ww, wh = work_area
    w = _clamp(width, 1, ww)
    h = _clamp(height, 1, wh)
    if edge in ("left", "right"):
        raw_offset = (wh - h - margin) if offset is None else offset
        clamped_offset = _clamp(raw_offset, 0, wh - h)
        x = wx if edge == "left" else wx + ww - w
        y = wy + clamped_offset
    else:
        raw_offset = (ww - w - margin) if offset is None else offset
        clamped_offset = _clamp(raw_offset, 0, ww - w)
        x = wx + clamped_offset
        y = wy if edge == "top" else wy + wh - h
    return (x, y, w, h)


def offset_along_edge(rect: Rect, work_area: Rect, edge: str) -> int:
    """dock_to_edge 的反函式：left/right 邊回傳 rect.y - wa.y；top/bottom 邊回傳 rect.x - wa.x；最小 0。edge 不合法 raise ValueError。"""
    if edge not in EDGES:
        raise ValueError(f"Invalid edge: {edge!r}")
    rx, ry, _, _ = rect
    wx, wy, _, _ = work_area
    if edge in ("left", "right"):
        return max(0, ry - wy)
    return max(0, rx - wx)


def clamp_scale(scale: float) -> float:
    """夾在 [SCALE_MIN, SCALE_MAX]，並四捨五入到 0.1 的倍數（結果用 round(x, 1)）。非數字或 NaN 回 1.0。"""
    if isinstance(scale, bool) or not isinstance(scale, (int, float)) or math.isnan(scale):
        return 1.0
    clamped = max(SCALE_MIN, min(float(scale), SCALE_MAX))
    return round(clamped, 1)


def step_scale(scale: float, steps: int) -> float:
    """clamp_scale(scale + steps * SCALE_STEP)。"""
    return clamp_scale(scale + steps * SCALE_STEP)


def scale_size(base_width: int, base_height: int, scale: float) -> tuple[int, int]:
    """(round(base_width * s), round(base_height * s))，s = clamp_scale(scale)；每一邊至少 1。"""
    s = clamp_scale(scale)
    return (max(1, round(base_width * s)), max(1, round(base_height * s)))

