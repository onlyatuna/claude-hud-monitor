"""Pick which two of agy's five quota rows are the "main" pair (metric1 / metric2); the rest become extras."""

from dataclasses import replace
from typing import Optional

from core.providers.base import MetricRow, UsageMetrics

AGY_ROW_TITLES = ("GEMINI 5H", "GEMINI 7D", "C/G 5H", "C/G 7D", "ARGON 7D")
DEFAULT_PRIMARY = ("C/G 5H", "ARGON 7D")


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


def arrange_agy(metrics: UsageMetrics, primary) -> UsageMetrics:
    """Return a copy whose metric1/metric2 are the chosen rows and whose extras are the other three.
    Unchanged when the data does not carry all five rows (errors, older providers)."""
    rows = {
        (metrics.metric1_title or "").upper(): MetricRow(metrics.metric1_title, metrics.metric1_val, metrics.metric1_text,
                                                        metrics.metric1_reset, metrics.metric1_subtext),
        (metrics.metric2_title or "").upper(): MetricRow(metrics.metric2_title, metrics.metric2_val, metrics.metric2_text,
                                                        metrics.metric2_reset, metrics.metric2_subtext),
    }
    for row in metrics.extra_metrics:
        rows[(row.title or "").upper()] = row
    if not all(title in rows for title in AGY_ROW_TITLES):
        return metrics
    first, second = normalize_primary(primary)
    a, b = rows[first], rows[second]
    others = [rows[t] for t in AGY_ROW_TITLES if t not in (first, second)]
    return replace(
        metrics,
        metric1_title=a.title, metric1_val=a.val, metric1_text=a.text, metric1_reset=a.reset, metric1_subtext=a.subtext,
        metric2_title=b.title, metric2_val=b.val, metric2_text=b.text, metric2_reset=b.reset, metric2_subtext=b.subtext,
        extra_metrics=others,
    )
