"""Rounded-rectangle window mask as pixel-row rectangles (pure geometry, no Qt).

Windows' Acrylic blurs the whole window rectangle, so the corners that the panel's CSS rounds off would still
show square blur. Clipping the window itself to this shape cuts the blur with it. The edge is not anti-aliased.
"""
import math


def rounded_rects(width: int, height: int, radius: int) -> list:
    """Non-overlapping (x, y, w, h) rectangles that cover a width x height box with rounded corners."""
    if width <= 0 or height <= 0:
        return []
    r = max(0, min(int(radius), width // 2, height // 2))
    if r == 0:
        return [(0, 0, width, height)]

    insets = []
    for y in range(r):                       # a pixel is kept when its centre lies inside the corner circle
        dy = r - y - 0.5
        dx = math.sqrt(max(0.0, r * r - dy * dy))
        insets.append(min(r, max(0, math.ceil(r - dx - 0.5))))

    bands = []                               # merge consecutive rows with the same inset
    for y, inset in enumerate(insets):
        if bands and bands[-1][0] == inset:
            bands[-1][2] += 1
        else:
            bands.append([inset, y, 1])

    rects = []
    for inset, y, rows in bands:
        if width - 2 * inset > 0:
            rects.append((inset, y, width - 2 * inset, rows))
    if height - 2 * r > 0:
        rects.append((0, r, width, height - 2 * r))
    for inset, y, rows in reversed(bands):
        if width - 2 * inset > 0:
            rects.append((inset, height - y - rows, width - 2 * inset, rows))
    return rects
