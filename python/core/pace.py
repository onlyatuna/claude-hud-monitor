"""
Pure time / pace helpers for the usage table (no Qt).

"Pace" compares usage with how far through its window we are: at even pace a
window that is 40% elapsed should be 40% used. Usage above that mark means the
quota runs out before the window resets if the current rate continues.
"""
import re
from datetime import datetime, timezone
from typing import Optional

WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"]

# Colour levels for the "scale" scheme: [0,50) green, [50,75) yellow, [75,90) orange, [90,100] red
SCALE_THRESHOLDS = [(90, "red"), (75, "orange"), (50, "yellow"), (0, "green")]

# Too early in a window the pace estimate is noise, so no mark is shown.
MIN_ELAPSED_FOR_PACE = 0.05

FIVE_HOURS = 5 * 3600
ONE_WEEK = 7 * 86400
_UNIT_SECONDS = {"D": 86400, "H": 3600, "M": 60}


def _now_for(target: datetime) -> datetime:
    return datetime.now(timezone.utc) if target.tzinfo else datetime.now()


def remaining_seconds(target: Optional[datetime]) -> Optional[int]:
    if not target:
        return None
    return max(0, int((target - _now_for(target)).total_seconds()))


def format_countdown_hm(target: Optional[datetime]) -> str:
    sec = remaining_seconds(target)
    if sec is None:
        return "--:--"
    return f"{sec // 3600:02d}:{sec % 3600 // 60:02d}"


def format_countdown_dhm(target: Optional[datetime]) -> str:
    sec = remaining_seconds(target)
    if sec is None:
        return "--:--:--"
    return f"{sec // 86400:02d}:{sec % 86400 // 3600:02d}:{sec % 3600 // 60:02d}"


def format_reset_time(target: Optional[datetime], with_day: bool) -> str:
    if not target:
        return "--:--"
    local = target.astimezone() if target.tzinfo else target
    hm = local.strftime("%H:%M")
    return f"週{WEEKDAYS[local.weekday()]} {hm}" if with_day else hm


def short_window(title: str) -> str:
    """'WINDOW 30D' / 'SESSION 5H' / 'WEEKLY 7D' -> '30D' / '5H' / '7D'."""
    return (title or "").replace("WINDOW ", "").replace("SESSION ", "").replace("WEEKLY ", "")


def window_caption(title: str, expected: str) -> str:
    """Short caption, only when a provider's window differs from the row it is shown in."""
    short = short_window(title)
    return "" if not title or short == expected else short


def window_seconds(title: str, default: int) -> int:
    match = re.search(r"(\d+(?:\.\d+)?)\s*([DHM])$", short_window(title))
    return int(float(match.group(1)) * _UNIT_SECONDS[match.group(2)]) if match else default


def elapsed_fraction(reset: Optional[datetime], window_sec: int) -> Optional[float]:
    """How far through the current window we are (0..1), or None without a reset time."""
    remaining = remaining_seconds(reset)
    if remaining is None or window_sec <= 0:
        return None
    return min(1.0, max(0.0, 1 - remaining / window_sec))


def pace_mark(elapsed: Optional[float]) -> Optional[float]:
    """Even-pace usage percent for right now, or None when it isn't meaningful yet."""
    if elapsed is None or elapsed < MIN_ELAPSED_FOR_PACE:
        return None
    return elapsed * 100


def scale_level(percent: Optional[float]) -> Optional[str]:
    if percent is None:
        return None
    return next(level for bound, level in SCALE_THRESHOLDS if percent >= bound)


def runout_text(percent: Optional[float], elapsed: Optional[float], window_sec: int) -> str:
    mark = pace_mark(elapsed)
    if percent is None or mark is None:
        return ""
    if percent <= mark:
        return "照目前速度，重設前不會用完"
    rate = percent / (elapsed * window_sec)  # percent per second so far
    local = datetime.fromtimestamp(datetime.now().timestamp() + (100 - percent) / rate)
    return f"照目前速度，預計 週{WEEKDAYS[local.weekday()]} {local.strftime('%H:%M')} 用完"
