import sys
import os
import time
from datetime import datetime
from PySide6.QtCore import Qt, QPoint, QRect, QTimer
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QMenu, QApplication
)
from PySide6.QtGui import QCursor, QGuiApplication

from core.providers import PROVIDERS, UsageMetrics
from core.config_manager import ConfigManager
from core.refresh_controller import RefreshController
from core.autostart import is_autostart_enabled, set_autostart
from core.logger import logger, open_log_dir
from ui.styles import APPEARANCES, SCHEMES, THEMES, get_hud_stylesheet
from ui.usage_table import UsageTable
from ui import vibrancy
from system.memory import trim_memory

MIN_W, MIN_H = 400, 300
DEF_W, DEF_H = 450, 350

SCHEME_LABELS = {"scale": "色階（綠／黃／橘／紅）", "duo": "雙色（內圈／外環）"}
APPEARANCE_LABELS = {"auto": "跟隨系統", "light": "淺色", "dark": "深色"}

if sys.platform == "win32":
    import ctypes
    from ctypes import wintypes
    user32 = ctypes.windll.user32
    GWL_EXSTYLE = -20
    WS_EX_TRANSPARENT = 0x00000020
    WS_EX_LAYERED = 0x00080000
    SWP_NOSIZE = 0x0001
    SWP_NOMOVE = 0x0002
    SWP_NOZORDER = 0x0004
    SWP_FRAMECHANGED = 0x0020

    user32.GetWindowLongW.argtypes = [wintypes.HWND, ctypes.c_int]
    user32.GetWindowLongW.restype = wintypes.LONG
    user32.SetWindowLongW.argtypes = [wintypes.HWND, ctypes.c_int, wintypes.LONG]
    user32.SetWindowLongW.restype = wintypes.LONG
    user32.SetWindowPos.argtypes = [
        wintypes.HWND, wintypes.HWND,
        ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
        wintypes.UINT
    ]
    user32.SetWindowPos.restype = wintypes.BOOL
else:
    user32 = None

class HUDWindow(QWidget):
    RESIZE_MARGIN = 8

    def __init__(self, config: ConfigManager, tray_icon_ref=None, providers=None):
        super().__init__()
        self.config = config
        self.tray_icon = tray_icon_ref
        self.hotkey_manager = None

        self._restoring_geometry = True
        self.geometry_timer = QTimer(self)
        self.geometry_timer.setSingleShot(True)
        self.geometry_timer.setInterval(250)
        self.geometry_timer.timeout.connect(self._persist_geometry)
        self.current_edge = None

        self._provider_errors = {}
        self._latest_metrics = {}
        self.table = None
        self._vibrant = False
        self.refresh_controller = RefreshController(
            PROVIDERS if providers is None else providers, config.get("refresh_interval_sec", 60), self)
        self.refresh_controller.updated.connect(self._on_data_fetched)
        self.refresh_controller.busy_changed.connect(self._on_busy_changed)

        self._init_window_flags()
        self._init_ui_skeleton()
        self._apply_theme()
        self._restore_initial_geometry()
        self._setup_timers()

        hints = QGuiApplication.styleHints()
        if hasattr(hints, "colorSchemeChanged"):
            hints.colorSchemeChanged.connect(self._on_system_color_scheme_changed)

        if self.config.get("click_through", False):
            # Delay startup click-through to verify hotkeys and prevent lockout
            QTimer.singleShot(300, self._safe_init_click_through)

        # Initial fetch for all providers
        self.refresh_controller.start()

        # Release initialization cold pages after UI and DirectWrite/fonts stabilize
        QTimer.singleShot(2500, trim_memory)

    def set_hotkey_manager(self, hotkey_mgr):
        self.hotkey_manager = hotkey_mgr

    def set_tray_icon(self, tray):
        self.tray_icon = tray

    def _init_window_flags(self):
        flags = Qt.WindowType.FramelessWindowHint | Qt.WindowType.Tool
        if self.config.get("always_on_top", True):
            flags |= Qt.WindowType.WindowStaysOnTopHint

        self.setWindowFlags(flags)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground, True)
        self.setMouseTracking(True)

        opacity = self.config.get("opacity", 0.88)
        self.setWindowOpacity(opacity)

    def _init_ui_skeleton(self):
        self.root_layout = QVBoxLayout(self)
        self.root_layout.setContentsMargins(0, 0, 0, 0)

        self.container = QWidget(self)
        self.container.setObjectName("CentralWidget")
        self.container.setMouseTracking(True)
        self.root_layout.addWidget(self.container)

        self.inner_layout = QVBoxLayout(self.container)
        self.inner_layout.setContentsMargins(12, 8, 12, 10)
        self.inner_layout.setSpacing(4)

        self.status_dot = QLabel("●")
        self.status_dot.setStyleSheet("color: #30d158; font-size: 9px;")

        self.title_label = QLabel("AI Agent HUD")
        self.title_label.setObjectName("HeaderTitle")

        self.ghost_label = QLabel("👻")
        self.ghost_label.setToolTip("滑鼠穿透中 (Alt+Shift+C 解除)")
        self.ghost_label.setVisible(False)

        self.time_label = QLabel("--:--:--")
        self.time_label.setObjectName("HeaderStatus")

        header_layout = QHBoxLayout()
        header_layout.setSpacing(6)
        header_layout.addWidget(self.status_dot)
        header_layout.addWidget(self.title_label)
        header_layout.addWidget(self.ghost_label)
        header_layout.addStretch()
        header_layout.addWidget(self.time_label)
        self.inner_layout.addLayout(header_layout)

    # ================= Theme (appearance × colour scheme) =================
    def resolved_appearance(self) -> str:
        appearance = self.config.get("appearance", "auto")
        if appearance in ("light", "dark"):
            return appearance
        hints = QGuiApplication.styleHints()
        if hasattr(hints, "colorScheme") and hints.colorScheme() == Qt.ColorScheme.Dark:
            return "dark"
        return "light"

    def color_scheme(self) -> str:
        scheme = self.config.get("color_scheme", "scale")
        return scheme if scheme in SCHEMES else "scale"

    def _theme(self) -> dict:
        return THEMES[self.resolved_appearance()]

    def _refresh_backdrop(self):
        """Native blur where available, and the panel opacity that matches it."""
        theme = self._theme()
        self._vibrant = self.isVisible() and vibrancy.apply(
            self, dark=self.resolved_appearance() == "dark", corner_radius=theme["radius"])
        self.setStyleSheet(get_hud_stylesheet(theme, self._vibrant))

    def _apply_theme(self):
        self._refresh_backdrop()
        # The table bakes colours into painted glyphs, so rebuild it and replay the latest data.
        if self.table is not None:
            self.inner_layout.removeWidget(self.table)
            self.table.deleteLater()
        self.table = UsageTable(self._theme(), self.color_scheme(), parent=self.container)
        self.inner_layout.addWidget(self.table, 1)
        for metrics in self._latest_metrics.values():
            self.table.update_metrics(metrics)

    def set_color_scheme(self, scheme: str):
        if scheme in SCHEMES and scheme != self.color_scheme():
            self.config.set("color_scheme", scheme)
            self._apply_theme()

    def set_appearance(self, appearance: str):
        if appearance in APPEARANCES and appearance != self.config.get("appearance", "auto"):
            self.config.set("appearance", appearance)
            self._apply_theme()

    def _on_system_color_scheme_changed(self, *_):
        if self.config.get("appearance", "auto") == "auto":
            self._apply_theme()

    def showEvent(self, event):
        super().showEvent(event)
        # Native blur needs a native window; flag changes recreate it, so (re)apply on every show.
        QTimer.singleShot(0, self._refresh_backdrop)

    def _restore_initial_geometry(self):
        self.setMinimumSize(MIN_W, MIN_H)
        w = self.config.get("table_width", DEF_W)
        h = self.config.get("table_height", DEF_H)
        w = w if isinstance(w, int) and w >= MIN_W else DEF_W
        h = h if isinstance(h, int) and h >= MIN_H else DEF_H
        self.resize(w, h)
        self._restore_or_default_position(self.config.get("window_x"), self.config.get("window_y"), w, h)
        self._restoring_geometry = False
        self._save_geometry()

    def _restore_or_default_position(self, x, y, w, h):
        is_visible = False
        if x is not None and y is not None:
            candidate_rect = QRect(x, y, w, h)
            for screen in QGuiApplication.screens():
                screen_geom = screen.availableGeometry()
                intersection = screen_geom.intersected(candidate_rect)
                if intersection.width() >= 50 and intersection.height() >= 30:
                    is_visible = True
                    break

        if is_visible:
            self.move(x, y)
        else:
            primary_screen = QGuiApplication.primaryScreen()
            if primary_screen:
                screen_geom = primary_screen.availableGeometry()
                default_x = max(screen_geom.x() + 10, screen_geom.x() + screen_geom.width() - w - 40)
                default_y = max(screen_geom.y() + 10, screen_geom.y() + 50)
                self.move(default_x, default_y)
            else:
                self.move(100, 100)

    def _safe_init_click_through(self):
        # Safety check: if hotkey registration failed, prevent user from being permanently locked out
        if self.hotkey_manager and not self.hotkey_manager.clickthrough_registered:
            self.config.set("click_through", False)
            if self.tray_icon:
                self.tray_icon.showMessage(
                    "⚠️ 穿透模式已暫停",
                    "全域快捷鍵註冊失敗，已自動停用啟動時穿透模式以防視窗鎖死。\n您仍可由系統匣右鍵選單手動開啟。",
                    self.tray_icon.icon(),
                    5000
                )
            return
        self.set_click_through(True, notify=False)

    def _setup_timers(self):
        self.countdown_timer = QTimer(self)
        self.countdown_timer.timeout.connect(self._update_all_countdowns)
        self.countdown_timer.start(1000)

    def trigger_async_refresh(self):
        self.refresh_controller.refresh()

    def _on_data_fetched(self, metrics: UsageMetrics):
        self._latest_metrics[metrics.provider_id] = metrics
        self.table.update_metrics(metrics)
        self._provider_errors[metrics.provider_id] = bool(metrics.error)
        self.time_label.setText(datetime.now().strftime("%H:%M:%S"))

    def _on_busy_changed(self, busy):
        color = "#64d2ff" if busy else ("#ff9f0a" if any(self._provider_errors.values()) else "#30d158")
        self.status_dot.setStyleSheet(f"color: {color}; font-size: 9px;")
        if not busy:
            # Reclaim transient JSON/HTTP buffers after batch refresh cycle completes
            QTimer.singleShot(1000, trim_memory)

    def _update_all_countdowns(self):
        # Auto-detect system wake from sleep/suspend
        cur_ts = time.time()
        if hasattr(self, "_last_countdown_ts"):
            gap = cur_ts - self._last_countdown_ts
            if gap > 15.0:  # System was asleep or suspended for >15 seconds
                self.trigger_async_refresh()
        self._last_countdown_ts = cur_ts

        self.table.update_countdowns()

    # ================= Click-Through Mode =================
    def _apply_native_click_through(self, enable: bool):
        # Cross-platform click-through handling
        if sys.platform == "win32" and user32:
            hwnd = int(self.winId())
            style = user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
            if enable:
                new_style = style | WS_EX_TRANSPARENT | WS_EX_LAYERED
            else:
                new_style = style & ~WS_EX_TRANSPARENT
            user32.SetWindowLongW(hwnd, GWL_EXSTYLE, new_style)
            # Instruct DWM to immediately update frame and mouse hit-testing
            user32.SetWindowPos(
                hwnd, None, 0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED
            )
        elif sys.platform == "darwin":
            self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, enable)
            try:
                import ctypes
                import ctypes.util
                objc_path = ctypes.util.find_library('objc')
                if objc_path:
                    objc = ctypes.cdll.LoadLibrary(objc_path)
                    objc.objc_getClass.restype = ctypes.c_void_p
                    objc.objc_getClass.argtypes = [ctypes.c_char_p]
                    objc.sel_registerName.restype = ctypes.c_void_p
                    objc.sel_registerName.argtypes = [ctypes.c_char_p]
                    objc.objc_msgSend.restype = ctypes.c_void_p
                    objc.objc_msgSend.argtypes = [ctypes.c_void_p, ctypes.c_void_p]

                    view_ptr = ctypes.c_void_p(int(self.winId()))
                    sel_window = objc.sel_registerName(b"window")
                    window_ptr = objc.objc_msgSend(view_ptr, sel_window)
                    if window_ptr:
                        sel_setIgnoresMouseEvents = objc.sel_registerName(b"setIgnoresMouseEvents:")
                        msg_send_bool = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_bool)(objc.objc_msgSend)
                        msg_send_bool(window_ptr, sel_setIgnoresMouseEvents, enable)
            except Exception as e:
                logger.warning(f"[ClickThrough macOS] Failed to set ignoresMouseEvents: {e}")
        else:
            visible = self.isVisible()
            self.setWindowFlag(Qt.WindowType.WindowTransparentForInput, enable)
            self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, enable)
            if visible:
                self.show()

    def set_click_through(self, enable: bool, notify: bool = True):
        self.config.set("click_through", enable)
        self.ghost_label.setVisible(enable)
        self._apply_native_click_through(enable)

        if enable and notify and self.tray_icon:
            self.tray_icon.showMessage(
                "👻 滑鼠穿透模式已啟用",
                "點擊將直接穿透 HUD。\n如需調整設定或移動，請按 Alt+Shift+C 或右鍵點擊系統匣圖示取消。",
                self.tray_icon.icon(),
                4000
            )

        if self.tray_icon:
            self.tray_icon.update_menu_state()

    def toggle_click_through(self):
        cur = self.config.get("click_through", False)
        self.set_click_through(not cur)

    # ================= Resizing & Dragging Engine =================
    def _calc_edge(self, pos: QPoint):
        if self.config.get("locked", False):
            return None

        w, h = self.width(), self.height()
        m = self.RESIZE_MARGIN
        x, y = pos.x(), pos.y()

        edge = 0
        if x < m:
            edge |= Qt.Edge.LeftEdge.value
        elif x > w - m:
            edge |= Qt.Edge.RightEdge.value

        if y < m:
            edge |= Qt.Edge.TopEdge.value
        elif y > h - m:
            edge |= Qt.Edge.BottomEdge.value

        return Qt.Edge(edge) if edge != 0 else None

    def mouseMoveEvent(self, event):
        pos = event.position().toPoint()
        edge = self._calc_edge(pos)
        self.current_edge = edge

        if edge is None:
            self.setCursor(Qt.CursorShape.ArrowCursor)
        elif edge in (Qt.Edge.LeftEdge, Qt.Edge.RightEdge):
            self.setCursor(Qt.CursorShape.SizeHorCursor)
        elif edge in (Qt.Edge.TopEdge, Qt.Edge.BottomEdge):
            self.setCursor(Qt.CursorShape.SizeVerCursor)
        elif (Qt.Edge.TopEdge.value | Qt.Edge.LeftEdge.value) == edge.value or \
             (Qt.Edge.BottomEdge.value | Qt.Edge.RightEdge.value) == edge.value:
            self.setCursor(Qt.CursorShape.SizeFDiagCursor)
        else:
            self.setCursor(Qt.CursorShape.SizeBDiagCursor)

        super().mouseMoveEvent(event)

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            if self.current_edge is not None:
                wh = self.windowHandle()
                if wh:
                    wh.startSystemResize(self.current_edge)
                event.accept()
                return
            elif not self.config.get("locked", False):
                wh = self.windowHandle()
                if wh:
                    wh.startSystemMove()
                event.accept()
                return
        super().mousePressEvent(event)

    def mouseReleaseEvent(self, event):
        self._persist_geometry()
        super().mouseReleaseEvent(event)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        self._save_geometry()

    def moveEvent(self, event):
        super().moveEvent(event)
        self._save_geometry()

    def closeEvent(self, event):
        self._persist_geometry()
        super().closeEvent(event)

    def hideEvent(self, event):
        super().hideEvent(event)
        # Deep sleep trim when HUD is minimized to background tray
        QTimer.singleShot(150, trim_memory)

    def mouseDoubleClickEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.trigger_async_refresh()
            event.accept()

    def _save_geometry(self):
        if not self._restoring_geometry:
            self.geometry_timer.start()

    def _persist_geometry(self):
        if self._restoring_geometry:
            return
        self.geometry_timer.stop()
        pos, size = self.pos(), self.size()
        self.config.set_many({
            "window_x": pos.x(),
            "window_y": pos.y(),
            "table_width": max(MIN_W, size.width()),
            "table_height": max(MIN_H, size.height()),
        })

    # ================= Context Menu =================
    def contextMenuEvent(self, event):
        menu = QMenu(self)

        refresh_act = menu.addAction("🔄 立即重新整理所有 AI (Refresh All)")
        refresh_act.triggered.connect(self.trigger_async_refresh)

        menu.addSeparator()

        self.add_theme_menus(menu)

        # Click-through toggle
        ghost_act = menu.addAction("👻 滑鼠點擊穿透 (Alt+Shift+C)")
        ghost_act.setCheckable(True)
        ghost_act.setChecked(self.config.get("click_through", False))
        ghost_act.triggered.connect(self.toggle_click_through)

        # Always on top
        aot_act = menu.addAction("📌 視窗永遠置頂 (Always on Top)")
        aot_act.setCheckable(True)
        aot_act.setChecked(self.config.get("always_on_top", True))
        aot_act.triggered.connect(self._toggle_always_on_top)

        # Lock position
        lock_act = menu.addAction("🔒 鎖定視窗位置 (Lock Drag)")
        lock_act.setCheckable(True)
        lock_act.setChecked(self.config.get("locked", False))
        lock_act.triggered.connect(self._toggle_lock)

        # Opacity submenu
        opacity_menu = menu.addMenu("🌗 視窗透明度 (Opacity)")
        current_op = self.config.get("opacity", 0.88)
        for pct in [100, 90, 80, 70, 50, 30]:
            val = pct / 100.0
            act = opacity_menu.addAction(f"{pct}%")
            act.setCheckable(True)
            act.setChecked(abs(current_op - val) < 0.05)
            act.triggered.connect(lambda checked, v=val: self._set_opacity(v))

        # Refresh interval submenu
        interval_menu = menu.addMenu("⏱️ 更新頻率 (Interval)")
        cur_int = self.config.get("refresh_interval_sec", 60)
        for sec in [30, 60, 120, 300]:
            act = interval_menu.addAction(f"{sec} 秒")
            act.setCheckable(True)
            act.setChecked(cur_int == sec)
            act.triggered.connect(lambda checked, s=sec: self._set_interval(s))

        # Autostart (cross-platform)
        autostart_act = menu.addAction("🚀 開機自動啟動 (Start on Boot)")
        autostart_act.setCheckable(True)
        autostart_act.setChecked(is_autostart_enabled())
        autostart_act.triggered.connect(self._toggle_autostart)

        menu.addSeparator()

        log_act = menu.addAction("📂 開啟記錄檔目錄 (Open Logs)")
        log_act.triggered.connect(open_log_dir)

        reset_act = menu.addAction("📐 重設預設尺寸與位置")
        reset_act.triggered.connect(self._reset_geometry)

        hide_act = menu.addAction("👁️ 隱藏 HUD (Alt+C 重新喚出)")
        hide_act.triggered.connect(self.hide)

        exit_act = menu.addAction("❌ 結束程式 (Exit)")
        exit_act.triggered.connect(self.close_application)

        menu.exec(event.globalPos())

    def add_theme_menus(self, menu: QMenu):
        """Colour scheme and appearance submenus (used by both the HUD and the tray menu)."""
        def build(title, options, labels, current, apply):
            sub = menu.addMenu(title)
            for value in options:
                act = sub.addAction(labels[value])
                act.setCheckable(True)
                act.setData(value)
                act.triggered.connect(lambda checked, v=value: apply(v))

            def sync():
                for act in sub.actions():
                    act.setChecked(act.data() == current())
            sync()
            # The tray menu is built once, so re-sync whenever the submenu opens
            sub.aboutToShow.connect(sync)
            return sub

        build("🎨 配色 (Colors)", SCHEMES, SCHEME_LABELS, self.color_scheme, self.set_color_scheme)
        build("🌓 外觀 (Appearance)", APPEARANCES, APPEARANCE_LABELS,
              lambda: self.config.get("appearance", "auto"), self.set_appearance)

    def _toggle_always_on_top(self):
        new_val = not self.config.get("always_on_top", True)
        self.config.set("always_on_top", new_val)
        self.setWindowFlag(Qt.WindowType.WindowStaysOnTopHint, new_val)
        self.show()
        if self.config.get("click_through", False):
            self._apply_native_click_through(True)
        if self.tray_icon:
            self.tray_icon.update_menu_state()

    def _toggle_lock(self):
        new_val = not self.config.get("locked", False)
        self.config.set("locked", new_val)

    def _set_opacity(self, value: float):
        self.config.set("opacity", value)
        self.setWindowOpacity(value)

    def _set_interval(self, seconds: int):
        self.config.set("refresh_interval_sec", seconds)
        self.refresh_controller.set_interval(seconds)

    def _toggle_autostart(self):
        currently_enabled = is_autostart_enabled()
        new_val = not currently_enabled
        if set_autostart(new_val):
            self.config.set("autostart", new_val)
        elif self.tray_icon:
            self.tray_icon.showMessage("開機啟動", "設定失敗，請檢查系統權限")
        if self.tray_icon:
            self.tray_icon.update_menu_state()

    def _reset_geometry(self):
        w, h = DEF_W, DEF_H
        self.resize(w, h)
        primary_screen = QGuiApplication.primaryScreen()
        screen_geom = primary_screen.availableGeometry() if primary_screen else self.screen().availableGeometry()
        self.move(screen_geom.x() + screen_geom.width() - w - 40, screen_geom.y() + 50)
        self._persist_geometry()

    def toggle_visibility(self):
        if self.isVisible():
            self.hide()
        else:
            self.show()
            self.activateWindow()

    def close_application(self):
        self._persist_geometry()
        self.countdown_timer.stop()
        self.refresh_controller.stop()
        self.close()
        app = QApplication.instance()
        if app:
            app.quit()
