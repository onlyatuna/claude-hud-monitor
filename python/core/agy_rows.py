"""Pick which two of agy's quota rows are the "main" pair (metric1 / metric2); the rest become extras.

Five rows are fixed (AGY_ROW_TITLES). Any further row in the data (a quota bucket this app does not know yet) is
"dynamic": it comes after the fixed rows by default and keeps its place in a saved order while the data has it."""

from dataclasses import replace
from typing import Optional

from core.providers.base import MetricRow, UsageMetrics

AGY_ROW_TITLES = ("GEMINI 5H", "GEMINI 7D", "C/G 5H", "C/G 7D", "ARGON 7D")
DEFAULT_PRIMARY = ("C/G 5H", "ARGON 7D")
DEFAULT_ORDER = DEFAULT_PRIMARY + tuple(t for t in AGY_ROW_TITLES if t not in DEFAULT_PRIMARY)


def normalize_primary(value) -> tuple:
    """Two distinct known row titles, else the default."""
    if isinstance(value, (list, tuple)):
        titles = []
        for item in value:
            title = str(item).upper()
            if title in AGY_ROW_TITLES and title not in titles:
                titles.append(title)
        if len(titles) == 2:
            return tuple(titles)
    return DEFAULT_PRIMARY


def normalize_order(value, dynamic=()) -> tuple:
    """A full ordering of the five fixed rows plus the `dynamic` titles the data currently has. A saved pair
    (older setting) keeps its two rows first and the rest follow in canonical order; anything unusable falls
    back to the default order. Saved titles that are neither fixed nor in `dynamic` are dropped; dynamic rows a
    saved order does not mention yet go last, in data order. With no dynamic rows this is the five-row rule."""
    dynamic = tuple(dict.fromkeys(str(t).upper() for t in dynamic if str(t).upper() not in AGY_ROW_TITLES))
    known = AGY_ROW_TITLES + dynamic
    if isinstance(value, (list, tuple)):
        titles = []
        for item in value:
            title = str(item).upper()
            if title in known and title not in titles:
                titles.append(title)
        if sum(1 for t in titles if t in AGY_ROW_TITLES) == len(AGY_ROW_TITLES):
            return tuple(titles) + tuple(t for t in dynamic if t not in titles)
        if len(titles) == 2:
            return tuple(titles) + tuple(t for t in known if t not in titles)
    return DEFAULT_ORDER + dynamic


def move_row(order, title: str, new_index: int) -> tuple:
    """Move one row to a new position (0-based, among the rows without it)."""
    order = list(normalize_order(order))
    title = str(title).upper()
    if title not in order:
        return tuple(order)
    order.remove(title)
    order.insert(max(0, min(len(order), new_index)), title)
    return tuple(order)


def toggle_primary(current, title: str) -> tuple:
    """Clicking an unselected row makes it the second row (the older second becomes first);
    clicking a selected row moves it to the top."""
    first, second = normalize_primary(current)
    title = str(title).upper()
    if title not in AGY_ROW_TITLES:
        return (first, second)
    if title == first:
        return (first, second)
    if title == second:
        return (second, first)
    return (second, title)


def _row_map(metrics: UsageMetrics) -> dict:
    """Upper-cased title -> row, main pair first then extras (a repeated title keeps its first position)."""
    rows = {
        (metrics.metric1_title or "").upper(): MetricRow(metrics.metric1_title, metrics.metric1_val, metrics.metric1_text,
                                                        metrics.metric1_reset, metrics.metric1_subtext, metrics.main_key(1)),
        (metrics.metric2_title or "").upper(): MetricRow(metrics.metric2_title, metrics.metric2_val, metrics.metric2_text,
                                                        metrics.metric2_reset, metrics.metric2_subtext, metrics.main_key(2)),
    }
    for row in metrics.extra_metrics:
        rows[(row.title or "").upper()] = row
    return rows


def dynamic_titles(metrics: Optional[UsageMetrics]) -> tuple:
    """Titles of the rows the data carries beyond the five fixed ones, in data order. Empty unless the data is
    complete (all five fixed rows present), so an error result's default titles never count."""
    if metrics is None:
        return ()
    rows = _row_map(metrics)
    if not all(title in rows for title in AGY_ROW_TITLES):
        return ()
    return tuple(title for title in rows if title not in AGY_ROW_TITLES)


def arrange_agy(metrics: UsageMetrics, primary) -> UsageMetrics:
    """Return a copy whose metric1/metric2 are the chosen rows and whose extras are all the other rows
    (the fixed ones and any dynamic ones). Unchanged when the data does not carry all five fixed rows
    (errors, older providers)."""
    rows = _row_map(metrics)
    if not all(title in rows for title in AGY_ROW_TITLES):
        return metrics
    order = normalize_order(primary, dynamic_titles(metrics))
    first, second = order[0], order[1]
    a, b = rows[first], rows[second]
    others = [rows[t] for t in order[2:]]
    return replace(
        metrics,
        metric1_title=a.title, metric1_val=a.val, metric1_text=a.text, metric1_reset=a.reset, metric1_subtext=a.subtext,
        metric1_key=a.key,
        metric2_title=b.title, metric2_val=b.val, metric2_text=b.text, metric2_reset=b.reset, metric2_subtext=b.subtext,
        metric2_key=b.key,
        extra_metrics=others,
    )


def arrange_by_order(metrics: UsageMetrics, order) -> UsageMetrics:
    """Generic row ordering for any provider: rows named in `order` come first in that order, the rest keep
    their natural position after them; the first two become metric1/metric2, the others extras."""
    rows = [MetricRow(metrics.metric1_title, metrics.metric1_val, metrics.metric1_text, metrics.metric1_reset,
                      metrics.metric1_subtext, metrics.main_key(1)),
            MetricRow(metrics.metric2_title, metrics.metric2_val, metrics.metric2_text, metrics.metric2_reset,
                      metrics.metric2_subtext, metrics.main_key(2))] + list(metrics.extra_metrics)
    if not isinstance(order, (list, tuple)) or len(rows) < 2:
        return metrics
    wanted = [str(t).upper() for t in order]
    ranked = sorted(enumerate(rows), key=lambda item: (wanted.index((item[1].title or "").upper())
                                                       if (item[1].title or "").upper() in wanted else len(wanted),
                                                       item[0]))
    rows = [row for _i, row in ranked]
    if [r.title for r in rows[:2]] == [metrics.metric1_title, metrics.metric2_title] and \
            [r.title for r in rows[2:]] == [r.title for r in metrics.extra_metrics]:
        return metrics
    a, b = rows[0], rows[1]
    return replace(
        metrics,
        metric1_title=a.title, metric1_val=a.val, metric1_text=a.text, metric1_reset=a.reset, metric1_subtext=a.subtext,
        metric1_key=a.key,
        metric2_title=b.title, metric2_val=b.val, metric2_text=b.text, metric2_reset=b.reset, metric2_subtext=b.subtext,
        metric2_key=b.key,
        extra_metrics=rows[2:],
    )
