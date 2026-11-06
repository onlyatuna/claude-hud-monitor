"""Translucent outline shown while the sidebar is dragged near a screen edge: where the bar will dock."""

from PySide6.QtCore import Qt, QRect, QRectF
from PySide6.QtGui import QColor, QPainter, QPen
from PySide6.QtWidgets import QWidget

from ui.sidebar import SIDEBAR_TOKENS

# Shared with the Rust HUD (rust/src/ui/dock_preview.rs): keep the values identical.
DOCK_PREVIEW_FILL_ALPHA = 46        # 0-255, the focus colour of the sidebar theme behind the outline
DOCK_PREVIEW_BORDER_ALPHA = 235
DOCK_PREVIEW_BORDER_PX = 2
# corner radius = the sidebar's own `radius` token (6) * the sidebar scale, as the bar itself is rounded


def preview_colors(dark: bool):
    """(fill, border) colours of the preview in the dark / light sidebar theme."""
    base = SIDEBAR_TOKENS['dark' if dark else 'light']['focus']
    fill, border = QColor(base), QColor(base)
    fill.setAlpha(DOCK_PREVIEW_FILL_ALPHA)
    border.setAlpha(DOCK_PREVIEW_BORDER_ALPHA)
    return fill, border


def preview_radius(dark: bool, scale: float) -> float:
    return SIDEBAR_TOKENS['dark' if dark else 'light']['radius'] * scale


class DockPreview(QWidget):
    """Frameless, click-through, non-activating top-level that outlines the docked rectangle of the dragged HUD."""

    def __init__(self, owner=None):
        super().__init__(owner)
        self._dark, self._scale, self._on_top = True, 1.0, None
        self._apply_flags(True)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground, True)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating, True)
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, True)
        self.setFocusPolicy(Qt.FocusPolicy.NoFocus)

    def _apply_flags(self, on_top: bool):
        if on_top == self._on_top:
            return
        self._on_top = on_top
        flags = (Qt.WindowType.FramelessWindowHint | Qt.WindowType.Tool | Qt.WindowType.WindowDoesNotAcceptFocus
                 | Qt.WindowType.WindowTransparentForInput)
        if on_top:
            flags |= Qt.WindowType.WindowStaysOnTopHint
        self.setWindowFlags(flags)

    def show_at(self, rect: QRect, dark: bool, scale: float, on_top: bool):
        """Outline `rect` (global coordinates); the HUD itself is not moved and does not lose the focus."""
        self._apply_flags(on_top)
        self._dark, self._scale = bool(dark), float(scale)
        if self.geometry() != rect:
            self.setGeometry(rect)
        self.update()
        if not self.isVisible():
            self.show()

    def paintEvent(self, event):
        fill, border = preview_colors(self._dark)
        radius = preview_radius(self._dark, self._scale)
        inset = DOCK_PREVIEW_BORDER_PX / 2
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setBrush(fill)
        painter.setPen(QPen(border, DOCK_PREVIEW_BORDER_PX))
        painter.drawRoundedRect(QRectF(self.rect()).adjusted(inset, inset, -inset, -inset), radius, radius)
