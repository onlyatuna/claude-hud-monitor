"""Green -> amber -> red gauge shared by the cards, the detail pages and the sidebar tiles.

A fixed gradient (stops at the status bands 0 / 50 / 75 / 90 %) is revealed up to the current value, so the
leading edge of the bar and the number next to it always have the same colour, and the bar's length and hue
both say how full it is. Colours are nudged (lightness only) until they read on the surface they sit on.
"""

from typing import Optional, Sequence

from PySide6.QtCore import Qt, QRectF
from PySide6.QtGui import QColor, QLinearGradient, QPainter
from PySide6.QtWidgets import QWidget, QSizePolicy

from ui.styles import THEMES

STOPS = ((0, "green"), (50, "yellow"), (75, "orange"), (90, "red"))
# Surfaces the gauge colours must read on (card / flyout / panel backgrounds)
SURFACES = {True: ("#0e1117", "#1e1e24", "#30303a"), False: ("#f8fafc", "#f6f4fa", "#e6e4ed")}


def _luminance(color: str) -> float:
    r, g, b = QColor(color).getRgbF()[:3]
    lin = [v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4 for v in (r, g, b)]
    return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]


def contrast(foreground: str, background: str) -> float:
    low, high = sorted((_luminance(foreground), _luminance(background)))
    return (high + 0.05) / (low + 0.05)


def readable(color: str, dark: bool, surfaces: Optional[Sequence[str]] = None, minimum: float = 3.0) -> str:
    """`color` unchanged when it reads on every surface, otherwise lighter (dark theme) / darker (light theme)
    in the same hue."""
    surfaces = surfaces or SURFACES[dark]
    if all(contrast(color, s) >= minimum for s in surfaces):
        return color
    hue, sat, light, _a = QColor(color).getHslF()
    step = 0.025 if dark else -0.025
    for _ in range(40):
        light = min(1.0, max(0.0, light + step))
        candidate = QColor.fromHslF(max(0.0, hue), sat, light).name()
        if all(contrast(candidate, s) >= minimum for s in surfaces):
            return candidate
    return "#f2f2f7" if dark else "#1f1f24"


def ramp_stops(dark: bool, surfaces: Optional[Sequence[str]] = None):
    scale = THEMES["dark" if dark else "light"]["scale"]
    return [(pos, readable(scale[name], dark, surfaces)) for pos, name in STOPS]


def ramp_color(value: float, dark: bool, surfaces: Optional[Sequence[str]] = None) -> str:
    """Gauge colour at `value` percent (smooth blend between the readable stops)."""
    stops = ramp_stops(dark, surfaces)
    value = max(0.0, min(100.0, float(value)))
    for (p0, c0), (p1, c1) in zip(stops, stops[1:]):
        if value <= p1:
            f = (value - p0) / (p1 - p0) if p1 > p0 else 0.0
            a, b = QColor(c0), QColor(c1)
            return QColor(round(a.red() + (b.red() - a.red()) * f), round(a.green() + (b.green() - a.green()) * f),
                          round(a.blue() + (b.blue() - a.blue()) * f)).name()
    return QColor(stops[-1][1]).name()


class GaugeBar(QWidget):
    """Thin progress bar: rounded track, gradient revealed up to the value. Drop-in for the bits of
    QProgressBar the cards use (setRange / setValue / value / setTextVisible)."""

    def __init__(self, parent=None, height: int = 5):
        super().__init__(parent)
        self._value = 0
        self._minimum, self._maximum = 0, 100
        self.dark = True
        self.muted = False
        self.setFixedHeight(height)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)

    # --- QProgressBar-compatible bits
    def setRange(self, minimum, maximum):
        self._minimum, self._maximum = minimum, max(minimum + 1, maximum)
        self.update()

    def setValue(self, value):
        self._value = value
        self.update()

    def value(self):
        return self._value

    def setTextVisible(self, _visible):
        pass

    def set_dark(self, dark: bool):
        if bool(dark) != self.dark:
            self.dark = bool(dark)
            self.update()

    def fraction(self) -> float:
        span = self._maximum - self._minimum
        return max(0.0, min(1.0, (self._value - self._minimum) / span))

    def paintEvent(self, _event):
        theme = THEMES["dark" if self.dark else "light"]
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.setPen(Qt.PenStyle.NoPen)
        track = QRectF(self.rect())
        radius = track.height() / 2
        p.setBrush(QColor(*theme["track"]))
        p.drawRoundedRect(track, radius, radius)
        fraction = self.fraction()
        if fraction <= 0 or self.muted:
            return
        gradient = QLinearGradient(track.left(), 0, track.right(), 0)
        for pos, color in ramp_stops(self.dark):
            gradient.setColorAt(pos / 100, QColor(color))
        fill = QRectF(track)
        fill.setWidth(max(track.height(), track.width() * fraction))   # keep tiny values visible as a round dot
        p.setBrush(gradient)
        p.drawRoundedRect(fill, radius, radius)
