"""Size rules for the Modern Table window (pure functions, no Qt).

Above FULL_MIN the window shows the full table. Dragged below it, the window collapses to one dial per
service; it only expands again once it is HYSTERESIS pixels past the threshold on both sides, so a window
resting on the boundary does not flip back and forth.
"""

FULL_MIN_W, FULL_MIN_H = 380, 280
HYSTERESIS = 16

COMPACT_DIAL_MIN = 48          # smallest dial (the full table's is 84)
COMPACT_GAP = 8                # between dials
COMPACT_MARGIN = 6             # window content margin while collapsed
COMPACT_FRAME = 2              # the panel's 1 px border on each side
DEF_COMPACT_W, DEF_COMPACT_H = 240, 96


def table_state(width: int, height: int, was_compact: bool) -> bool:
    """True when the table window should show the collapsed dial row at this size."""
    if was_compact:
        return not (width >= FULL_MIN_W + HYSTERESIS and height >= FULL_MIN_H + HYSTERESIS)
    return width < FULL_MIN_W or height < FULL_MIN_H


def compact_min_size(count: int = 3) -> tuple:
    """Smallest window that still gives each of `count` dials COMPACT_DIAL_MIN pixels."""
    pad = 2 * COMPACT_MARGIN + COMPACT_FRAME
    return (count * COMPACT_DIAL_MIN + max(0, count - 1) * COMPACT_GAP + pad, COMPACT_DIAL_MIN + pad)
