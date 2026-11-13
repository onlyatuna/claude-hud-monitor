"""Which quota's number the tray icon shows, and how it reads. Pure logic, no Qt.

Setting `tray_quota`: "off", "auto" (the fullest row) or a row key. A row key is the MetricRow key, prefixed
with `<provider_id>:` unless it already starts with that (Claude keys do: `claude:5h`, `claude:weekly_scoped:fable`;
Codex/AGY rows have key == title, so `codex:WINDOW 5H`, `agy:C/G 5H`). Setting `tray_quota_mode`: "used" | "remaining".
"""

from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from typing import Optional

OFF, AUTO = "off", "auto"
USED, REMAINING = "used", "remaining"
STALE_AFTER = timedelta(minutes=30)  # a reading older than this is drawn grey
AMBER, GREY, RED = "#f59e0b", "#8b949e", "#ef4444"
PROVIDER_NAMES = {"claude": "Claude", "codex": "Codex", "agy": "AGY"}

OK, STALE, OLD, NONE, OFFLINE, UNAVAILABLE = "ok", "stale", "old", "none", "offline", "unavailable"


@dataclass(frozen=True)
class Reading:
    key: str
    provider_id: str
    title: str
    used: Optional[float]
    state: str
    name: str = ""   # display name of an extra account (its tile text); empty = the built-in provider names

    @property
    def provider_name(self) -> str:
        return self.name or PROVIDER_NAMES.get(self.provider_id, self.provider_id.title())


def qualified_key(provider_id: str, row_key: str) -> str:
    return row_key if row_key.startswith(provider_id + ":") else f"{provider_id}:{row_key}"


def _state(metrics, used, now) -> str:
    if metrics.error and not metrics.stale:  # same test as the sidebar's `offline`
        return OFFLINE
    if used is None:
        return NONE
    if metrics.last_success is not None and now - metrics.last_success > STALE_AFTER:
        return OLD
    return STALE if metrics.stale else OK


def readings(raw_metrics: dict, provider_order, now: Optional[datetime] = None, names: Optional[dict] = None) -> list:
    """Every row of every provider, in provider order (main pair, then extras); `raw_metrics` is unarranged.
    A provider that is offline has no real rows (its error result carries default titles and no keys), so it
    contributes one placeholder: key `<provider_id>:`, empty title, state OFFLINE."""
    now = now or datetime.now(timezone.utc)
    names = names or {}
    result = []
    for provider_id in provider_order:
        m = raw_metrics.get(provider_id)
        if m is None:
            continue
        if _state(m, 0, now) == OFFLINE:
            result.append(Reading(provider_id + ":", provider_id, "", None, OFFLINE, names.get(provider_id, "")))
            continue
        rows = [(m.main_key(1), m.metric1_title, m.metric1_val), (m.main_key(2), m.metric2_title, m.metric2_val)]
        rows += [(r.key, r.title, r.val) for r in m.extra_metrics]
        for key, title, used in rows:
            if not key:  # a provider with a single window has no second row
                continue
            result.append(Reading(qualified_key(provider_id, key), provider_id, title, used, _state(m, used, now),
                                  names.get(provider_id, "")))
    return result


def select(setting: str, rows: list) -> Optional[Reading]:
    """The row the setting points at; None for "off". A key that is not in `rows` gives an UNAVAILABLE reading
    (the stored setting is never touched). "auto" is the highest used% among the rows that have a number
    (offline providers skipped, ties go to the first), or a NONE reading when there is none."""
    if setting == OFF:
        return None
    if setting == AUTO:
        live = [r for r in rows if r.used is not None and r.state != OFFLINE]
        return max(live, key=lambda r: r.used) if live else Reading(AUTO, "", "", None, NONE)
    for r in rows:
        if r.key == setting:
            return r
    for r in rows:  # the provider of the selected row is offline: show that, not "unavailable"
        if r.state == OFFLINE and setting.startswith(r.key):
            return Reading(setting, r.provider_id, "", None, OFFLINE, r.name)
    return Reading(setting, "", "", None, UNAVAILABLE)


def number_text(used: Optional[float], mode: str) -> str:
    """Digits for the icon: used (or 100 - used) rounded like percent_text, no % sign, at most 2 digits."""
    if used is None:
        return "--"
    n = int(f"{used:.0f}")
    return str(max(0, min(99, 100 - n if mode == REMAINING else n)))


def display(reading: Reading, mode: str):
    """(text, colour) for the icon. colour None = take the gauge ramp at the used value."""
    state = reading.state
    if state == OFFLINE:
        return "!", RED
    if state in (NONE, UNAVAILABLE) or reading.used is None:
        return "--", GREY
    text = number_text(reading.used, mode)
    if state == OLD:
        return text, GREY
    return text, AMBER if state == STALE else None


def tooltip_line(reading: Reading, mode: str) -> str:
    if reading.state == UNAVAILABLE:
        return "Tray number: selection unavailable"
    name = f"{reading.provider_name} {reading.title}".strip()
    if reading.state == OFFLINE:
        return f"{name}: offline"
    if not reading.title:  # auto with no number anywhere
        return "Tray number: --"
    if reading.used is None:
        return f"{name}: --"
    n = max(0, min(100, int(f"{reading.used:.0f}")))
    value = f"{100 - n}% left" if mode == REMAINING else f"{n}% used"
    return f"{name}: {value}" + (" (stale)" if reading.state in (STALE, OLD) else "")


def menu_label(reading: Reading) -> str:
    """Row title plus its USED percent, whatever the display mode."""
    return f"{reading.title}  {'--' if reading.used is None else f'{reading.used:.0f}%'}"
