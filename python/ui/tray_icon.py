import os
import sys
from PySide6.QtWidgets import QSystemTrayIcon, QMenu
from PySide6.QtGui import QIcon, QPixmap, QPainter, QColor, QFont
from PySide6.QtCore import Qt


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

        self.setToolTip("AI HUD Monitor (3-in-1)\n• Alt+C: 顯隱\n• Alt+Shift+C: 穿透模式")
        self._init_menu()
        self.activated.connect(self._on_activated)

    def _init_menu(self):
        # Same native-styled menu as the HUD right-click menu; rebuilt on every open so checks/profiles are current.
        self.menu = QMenu(self.hud_window)
        self.menu.aboutToShow.connect(self._rebuild_menu)
        self.setContextMenu(self.menu)

    def _rebuild_menu(self):
        self.menu.clear()
        self.hud_window.populate_context_menu(self.menu)

    def update_menu_state(self):
        # Nothing to sync: the menu is rebuilt from config each time it is shown.
        pass

    def _on_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            self.hud_window.toggle_visibility()
