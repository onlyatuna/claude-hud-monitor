import os
import sys
from PySide6.QtWidgets import QSystemTrayIcon, QMenu
from PySide6.QtGui import QIcon, QPixmap, QPainter, QColor, QFont, QFontMetrics, QImage
from PySide6.QtCore import Qt, QRectF

from core import tray_quota
from core.autostart import is_autostart_enabled, set_autostart
from core.logger import open_log_dir
from ui.gauge import ramp_color
from ui.hud_menu import build_hud_menu

ICON_SIZES = (16, 20, 24, 32)  # Windows asks for one of these depending on DPI and never scales an icon up
TILE = QColor(18, 22, 30)


def _digit_font(pixel_size: int) -> QFont:
    font = QFont("Segoe UI")
    font.setPixelSize(pixel_size)
    font.setWeight(QFont.Weight.Bold)
    font.setStyleStrategy(QFont.StyleStrategy.PreferAntialias | QFont.StyleStrategy.NoSubpixelAntialias)
    return font


def render_number_pixmap(text: str, color: str, remaining: bool, size: int) -> QPixmap:
    """Bold number on a dark rounded tile. Remaining mode adds a bar under the digits: at 16 px a bar is the
    only extra mark that stays readable, and "used" (no bar) stays the plain, default look.

    The ink is drawn white on a grey-scale image and used as the alpha of a flat colour layer: Qt would draw
    ClearType fringes on a coloured image even for a transparent icon, which look wrong on any other taskbar colour."""
    pad = max(1, round(size / 16))
    bar = max(2, round(size / 8)) if remaining else 0
    gap = pad if remaining else 0
    avail_w, avail_h = size - 2 * pad, size - 2 * pad - bar - gap
    for px in range(size * 2, 3, -1):  # the largest face whose ink fits
        font = _digit_font(px)
        ink = QFontMetrics(font).tightBoundingRect(text)
        if ink.width() <= avail_w and ink.height() <= avail_h:
            break
    mask = QImage(size, size, QImage.Format.Format_Grayscale8)
    mask.fill(Qt.GlobalColor.black)
    painter = QPainter(mask)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setFont(font)
    painter.setPen(Qt.GlobalColor.white)
    x = pad + (avail_w - ink.width()) / 2 - ink.left()
    y = pad + (avail_h - ink.height()) / 2 - ink.top()
    painter.drawText(round(x), round(y), text)
    if remaining:
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(Qt.GlobalColor.white)
        bar_w = min(avail_w, max(ink.width(), size // 2))
        painter.drawRect(QRectF((size - bar_w) / 2, size - pad - bar, bar_w, bar))
    painter.end()
    layer = QImage(size, size, QImage.Format.Format_ARGB32_Premultiplied)
    layer.fill(QColor(color))
    layer.setAlphaChannel(mask)
    image = QImage(size, size, QImage.Format.Format_ARGB32_Premultiplied)
    image.fill(Qt.GlobalColor.transparent)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setPen(Qt.PenStyle.NoPen)
    painter.setBrush(TILE)
    painter.drawRoundedRect(QRectF(0, 0, size, size), size * 0.22, size * 0.22)
    painter.drawImage(0, 0, layer)
    painter.end()
    return QPixmap.fromImage(image)


def number_icon(text: str, color: str, remaining: bool) -> QIcon:
    icon = QIcon()
    for size in ICON_SIZES:
        icon.addPixmap(render_number_pixmap(text, color, remaining, size))
    return icon


def get_app_icon() -> QIcon:
    if getattr(sys, 'frozen', False) and hasattr(sys, '_MEIPASS'):
        assets_dir = os.path.join(sys._MEIPASS, "assets")
    else:
        base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        assets_dir = os.path.join(base_dir, "assets")

    ico_path = os.path.join(assets_dir, "app_icon.ico")
    png_path = os.path.join(assets_dir, "app_icon.png")
    icns_path = os.path.join(assets_dir, "app_icon.icns")

    if sys.platform == "win32" and os.path.exists(ico_path):
        return QIcon(ico_path)
    elif sys.platform == "darwin" and os.path.exists(icns_path):
        return QIcon(icns_path)
    elif os.path.exists(png_path):
        return QIcon(png_path)
    elif os.path.exists(ico_path):
        return QIcon(ico_path)

    pixmap = QPixmap(64, 64)
    pixmap.fill(Qt.GlobalColor.transparent)
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setBrush(QColor(18, 22, 30))
    painter.setPen(QColor(56, 189, 248, 180))
    painter.drawRoundedRect(4, 4, 56, 56, 12, 12)
    painter.setPen(QColor(56, 189, 248))
    painter.setFont(QFont("Consolas", 28, QFont.Weight.Bold))
    painter.drawText(pixmap.rect(), Qt.AlignmentFlag.AlignCenter, "C")
    painter.end()
    return QIcon(pixmap)

class HUDTrayIcon(QSystemTrayIcon):
    def __init__(self, hud_window, parent=None):
        icon = get_app_icon()
        super().__init__(icon, parent)
        self.hud_window = hud_window
        self.hud_window.set_tray_icon(self)

        self._app_icon = icon
        self._base_tooltip = "AI HUD Monitor (3-in-1)\n• Alt+C: 顯隱\n• Alt+Shift+C: 穿透模式"
        self._signature = None
        self._tooltip = None
        self._init_menu()
        self.activated.connect(self._on_activated)
        self.refresh()

    def refresh(self):
        """Redraw the icon and tooltip for the chosen quota; the icon only when what it shows changed."""
        config = self.hud_window.config
        setting = config.get("tray_quota", tray_quota.OFF)
        mode = config.get("tray_quota_mode", tray_quota.USED)
        reading = tray_quota.select(setting, self.hud_window.tray_quota_readings())
        if reading is None:
            signature, tooltip = (setting,), self._base_tooltip
        else:
            text, color = tray_quota.display(reading, mode)
            if color is None:
                color = ramp_color(reading.used, True)
            signature = (setting, text, color, mode)
            tooltip = f"{tray_quota.tooltip_line(reading, mode)}\n{self._base_tooltip}"
        if signature != self._signature:
            self._signature = signature
            self.setIcon(self._app_icon if reading is None
                         else number_icon(text, color, mode == tray_quota.REMAINING))
        if tooltip != self._tooltip:
            self._tooltip = tooltip
            self.setToolTip(tooltip)

    def _init_menu(self):
        self.menu = QMenu(self.hud_window)
        self.menu.aboutToShow.connect(self._rebuild_menu)
        self._rebuild_menu()
        self.setContextMenu(self.menu)

    def _rebuild_menu(self):
        self.menu.clear()
        build_hud_menu(self.hud_window, tray=True, menu=self.menu)

    def update_menu_state(self):
        """The menu is rebuilt every time it opens, so there is no state to sync."""

    def _on_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            self.hud_window.toggle_visibility()

    def _toggle_autostart(self):
        currently_enabled = is_autostart_enabled()
        new_val = not currently_enabled
        if set_autostart(new_val):
            self.hud_window.config.set("autostart", new_val)
        else:
            self.showMessage("開機啟動", "設定失敗，請檢查系統權限")
        self.update_menu_state()
