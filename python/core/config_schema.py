"""Repair hand-edited or damaged settings when the config file is loaded.

Every known key gets a rule: a value of the wrong type or outside its range is replaced by the default (or
clamped where a nearby valid value is obvious) instead of reaching the UI, where it used to crash the window
before it appeared (opacity "abc", refresh_interval_sec "60x"). Unknown keys are never touched.
"""
import math

_NOTHING = object()


def _is_number(value) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _boolean(value, default):
    if isinstance(value, bool):
        return value
    if isinstance(value, str) and value.strip().lower() in ("true", "false"):
        return value.strip().lower() == "true"
    return default


def _float_between(low, high, *, reject_non_positive=False):
    def rule(value, default):
        if not _is_number(value) or not math.isfinite(value) or (reject_non_positive and value <= 0):
            return default
        return value if low <= value <= high else float(min(high, max(low, value)))
    return rule


def _int_between(low, high):
    def rule(value, default):
        if isinstance(value, str):
            try:
                value = float(value.strip())
            except ValueError:
                return default
        elif not _is_number(value):
            return default
        if not math.isfinite(value):
            return default
        return int(min(high, max(low, round(value))))
    return rule


def _coordinate(value, default):
    if _is_number(value) and math.isfinite(value) and value == int(value):
        return int(value)
    return None


def _window_size(value, default):
    if _is_number(value) and math.isfinite(value) and value == int(value) and 0 < value <= 20000:
        return int(value)
    return default


def _choice(*allowed):
    def rule(value, default):
        return value if isinstance(value, str) and value in allowed else default
    return rule


def _sidebar_offset(value, default):
    if value == "end" or (isinstance(value, int) and not isinstance(value, bool)):
        return value
    return None


def _text(value, default):
    return value if isinstance(value, str) and value.strip() else default


def _provider_order(value, default):
    if isinstance(value, list) and all(isinstance(v, str) for v in value) and sorted(value) == sorted(default):
        return list(value)
    return list(default)


def _row_order(value, default):
    if not isinstance(value, dict):
        return {}
    return {key: list(rows) for key, rows in value.items()
            if isinstance(key, str) and isinstance(rows, list) and all(isinstance(r, str) for r in rows)}


def _agy_order(value, default):
    if isinstance(value, list) and all(isinstance(v, str) for v in value):
        return list(value)
    return list(default)


RULES = {
    "window_x": _coordinate, "window_y": _coordinate, "sidebar_x": _coordinate, "sidebar_y": _coordinate,
    "sidebar_state": _choice("strip", "panel"),
    "sidebar_edge": _choice("left", "right", "top", "bottom", "none"),
    "sidebar_orientation": _choice("horizontal", "vertical"),
    "sidebar_offset": _sidebar_offset,
    "sidebar_idle_fade": _boolean,
    "sidebar_idle_level": _float_between(0.2, 0.95),
    "sidebar_idle_delay_ms": _int_between(200, 30000),
    "sidebar_scale": _float_between(0.6, 2.0),
    "row_order": _row_order, "agy_order": _agy_order, "provider_order": _provider_order,
    "table_width": _window_size, "table_height": _window_size,
    "table_blur": _boolean, "table_compact": _boolean,
    "table_compact_width": _window_size, "table_compact_height": _window_size,
    "color_scheme": _choice("scale", "duo"),
    "appearance": _choice("auto", "light", "dark"),
    "always_on_top": _boolean, "click_through": _boolean, "locked": _boolean, "autostart": _boolean,
    "hotkey_enabled": _boolean,
    "opacity": _float_between(0.1, 1.0, reject_non_positive=True),
    "refresh_interval_sec": _int_between(20, 86400),
    "hotkey": _text, "claude_profile": _text, "tray_quota": _text,
    "tray_quota_mode": _choice("used", "remaining"),
}


def _same(a, b) -> bool:
    return type(a) is type(b) and a == b


def sanitize(data: dict, defaults: dict) -> list:
    """Repair `data` in place; returns the keys that had to be changed."""
    changed = []
    for key, rule in RULES.items():
        if key not in data:
            continue
        old = data[key]
        new = rule(old, defaults.get(key))
        if not _same(old, new):
            data[key] = new
            changed.append(key)
    return changed
