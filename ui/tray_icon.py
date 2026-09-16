import os
import sys
from PySide6.QtWidgets import QSystemTrayIcon, QMenu
from PySide6.QtGui import QIcon, QPixmap, QPainter, QColor, QFont
from PySide6.QtCore import Qt

from core.autostart import is_autostart_enabled, set_autostart

def get_app_icon() -> QIcon:
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    ico_path = os.path.join(base_dir, "assets", "app_icon.ico")
    png_path = os.path.join(base_dir, "assets", "app_icon.png")

    if os.path.exists(ico_path):
        return QIcon(ico_path)
    elif os.path.exists(png_path):
        return QIcon(png_path)

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
        self.menu = QMenu()

        toggle_act = self.menu.addAction("👁️ 顯示 / 隱藏 HUD (Alt+C)")
        toggle_act.triggered.connect(self.hud_window.toggle_visibility)

        refresh_act = self.menu.addAction("🔄 立即重新整理所有 AI (Refresh All)")
        refresh_act.triggered.connect(self.hud_window.trigger_async_refresh)

        self.menu.addSeparator()

        # Layout submenu
        self.layout_menu = self.menu.addMenu("📐 顯示佈局 (Layout)")
        cur_layout = self.hud_window.config.get("layout_mode", "horizontal")
        self.horiz_act = self.layout_menu.addAction("💻 橫向三欄並排 (Horizontal Triple)")
        self.horiz_act.setCheckable(True)
        self.horiz_act.setChecked(cur_layout == "horizontal")
        self.horiz_act.triggered.connect(lambda: self.hud_window._apply_layout_mode("horizontal"))

        self.vert_act = self.layout_menu.addAction("📱 直立三層堆疊 (Vertical Stack)")
        self.vert_act.setCheckable(True)
        self.vert_act.setChecked(cur_layout == "vertical")
        self.vert_act.triggered.connect(lambda: self.hud_window._apply_layout_mode("vertical"))

        # Click-through toggle
        self.clickthrough_act = self.menu.addAction("👻 滑鼠點擊穿透 (Alt+Shift+C)")
        self.clickthrough_act.setCheckable(True)
        self.clickthrough_act.setChecked(self.hud_window.config.get("click_through", False))
        self.clickthrough_act.triggered.connect(self.hud_window.toggle_click_through)

        # Always on top
        self.aot_act = self.menu.addAction("📌 視窗永遠置頂")
        self.aot_act.setCheckable(True)
        self.aot_act.setChecked(self.hud_window.config.get("always_on_top", True))
        self.aot_act.triggered.connect(self.hud_window._toggle_always_on_top)

        if sys.platform == "win32":
            self.autostart_act = self.menu.addAction("🚀 開機自動啟動")
            self.autostart_act.setCheckable(True)
            self.autostart_act.setChecked(is_autostart_enabled())
            self.autostart_act.triggered.connect(self._toggle_autostart)

        self.menu.addSeparator()

        exit_act = self.menu.addAction("❌ 結束程式 (Exit)")
        exit_act.triggered.connect(self.hud_window.close_application)

        self.setContextMenu(self.menu)

    def update_menu_state(self):
        cur_layout = self.hud_window.config.get("layout_mode", "horizontal")
        self.horiz_act.setChecked(cur_layout == "horizontal")
        self.vert_act.setChecked(cur_layout == "vertical")
        self.clickthrough_act.setChecked(self.hud_window.config.get("click_through", False))
        self.aot_act.setChecked(self.hud_window.config.get("always_on_top", True))

    def _on_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            self.hud_window.toggle_visibility()

    def _toggle_autostart(self):
        currently_enabled = is_autostart_enabled()
        new_val = not currently_enabled
        set_autostart(new_val)
        self.hud_window.config.set("autostart", new_val)
