import sys
import os
import ctypes
import threading
import time
from datetime import datetime
from PySide6.QtCore import Qt, QPoint, QTimer, Signal, QObject
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QMenu, QPushButton, QFrame
)
from PySide6.QtGui import QCursor

from core.providers import PROVIDERS, UsageMetrics
from core.config_manager import ConfigManager
from core.autostart import is_autostart_enabled, set_autostart
from ui.styles import get_hud_stylesheet
from ui.provider_card import ProviderCardWidget

user32 = ctypes.windll.user32 if sys.platform == "win32" else None
GWL_EXSTYLE = -20
WS_EX_TRANSPARENT = 0x00000020
WS_EX_LAYERED = 0x00080000

class WorkerSignals(QObject):
    data_fetched = Signal(UsageMetrics)

class HUDWindow(QWidget):
    RESIZE_MARGIN = 8

    def __init__(self, config: ConfigManager, tray_icon_ref=None):
        super().__init__()
        self.config = config
        self.tray_icon = tray_icon_ref

        self.signals = WorkerSignals()
        self.signals.data_fetched.connect(self._on_data_fetched)

        self.current_edge = None
        self.is_fetching = False

        # Provider Cards
        self.cards = {
            "claude": ProviderCardWidget("claude"),
            "agy": ProviderCardWidget("agy"),
            "codex": ProviderCardWidget("codex")
        }

        self._init_window_flags()
        self._init_ui_skeleton()
        self._apply_layout_mode(self.config.get("layout_mode", "horizontal"), initial=True)
        self._setup_timers()

        if self.config.get("click_through", False):
            QTimer.singleShot(200, lambda: self.set_click_through(True))

        # Initial fetch for all providers
        self.trigger_async_refresh()

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
        self.setStyleSheet(get_hud_stylesheet())

        self.root_layout = QVBoxLayout(self)
        self.root_layout.setContentsMargins(0, 0, 0, 0)

        self.container = QWidget(self)
        self.container.setObjectName("CentralWidget")
        self.container.setMouseTracking(True)
        self.root_layout.addWidget(self.container)

        self.inner_layout = QVBoxLayout(self.container)
        self.inner_layout.setContentsMargins(12, 8, 12, 10)
        self.inner_layout.setSpacing(6)

        # Header bar widgets
        self.status_dot = QLabel("●")
        self.status_dot.setStyleSheet("color: #10b981; font-size: 11px;")

        self.title_label = QLabel("AI AGENT HUD (3-IN-1)")
        self.title_label.setObjectName("HeaderTitle")

        self.ghost_label = QLabel("👻")
        self.ghost_label.setToolTip("滑鼠穿透中 (Alt+Shift+C 解除)")
        self.ghost_label.setVisible(False)

        self.layout_toggle_btn = QPushButton("⇄")
        self.layout_toggle_btn.setObjectName("LayoutToggleBtn")
        self.layout_toggle_btn.setToolTip("切換 橫向並排 / 直式堆疊 佈局")
        self.layout_toggle_btn.clicked.connect(self.toggle_layout_mode)

        self.time_label = QLabel("--:--:--")
        self.time_label.setObjectName("HeaderStatus")

    def _clear_layout(self, layout):
        while layout.count():
            item = layout.takeAt(0)
            widget = item.widget()
            if widget:
                widget.setParent(None)
            sub_layout = item.layout()
            if sub_layout:
                self._clear_layout(sub_layout)

    def _apply_layout_mode(self, mode: str, initial=False):
        self.config.set("layout_mode", mode)
        self._clear_layout(self.inner_layout)

        # Common Header
        header_layout = QHBoxLayout()
        header_layout.setSpacing(6)
        header_layout.addWidget(self.status_dot)
        header_layout.addWidget(self.title_label)
        header_layout.addWidget(self.ghost_label)
        header_layout.addWidget(self.layout_toggle_btn)
        header_layout.addStretch()
        header_layout.addWidget(self.time_label)
        self.inner_layout.addLayout(header_layout)

        if mode == "horizontal":
            # Horizontal: 3 side-by-side columns
            self.setMinimumSize(540, 125)
            if not initial:
                w = self.config.get("horizontal_width", 690)
                h = self.config.get("horizontal_height", 145)
                self.resize(w, h)

            body_layout = QHBoxLayout()
            body_layout.setSpacing(8)

            provider_ids = ["claude", "agy", "codex"]
            for i, pid in enumerate(provider_ids):
                body_layout.addWidget(self.cards[pid], 1)
                if i < len(provider_ids) - 1:
                    divider = QFrame()
                    divider.setObjectName("Divider")
                    divider.setFrameShape(QFrame.Shape.VLine)
                    body_layout.addWidget(divider)

            self.inner_layout.addLayout(body_layout)

        else:
            # Vertical: 3 stacked rows
            self.setMinimumSize(250, 320)
            if not initial:
                w = self.config.get("vertical_width", 280)
                h = self.config.get("vertical_height", 410)
                self.resize(w, h)

            provider_ids = ["claude", "agy", "codex"]
            for i, pid in enumerate(provider_ids):
                self.inner_layout.addWidget(self.cards[pid])
                if i < len(provider_ids) - 1:
                    h_div = QFrame()
                    h_div.setStyleSheet("background-color: rgba(255, 255, 255, 0.08); max-height: 1px; min-height: 1px;")
                    h_div.setFrameShape(QFrame.Shape.HLine)
                    self.inner_layout.addWidget(h_div)

        if initial:
            if mode == "horizontal":
                w = self.config.get("horizontal_width", 690)
                h = self.config.get("horizontal_height", 145)
            else:
                w = self.config.get("vertical_width", 280)
                h = self.config.get("vertical_height", 410)
            self.resize(w, h)

            x = self.config.get("window_x")
            y = self.config.get("window_y")
            if x is not None and y is not None:
                self.move(x, y)
            else:
                screen = self.screen().geometry()
                self.move(screen.width() - w - 40, 50)

    def toggle_layout_mode(self):
        cur = self.config.get("layout_mode", "horizontal")
        new_mode = "vertical" if cur == "horizontal" else "horizontal"
        self._apply_layout_mode(new_mode)

    def _setup_timers(self):
        self.countdown_timer = QTimer(self)
        self.countdown_timer.timeout.connect(self._update_all_countdowns)
        self.countdown_timer.start(1000)

        interval = max(20, self.config.get("refresh_interval_sec", 60)) * 1000
        self.fetch_timer = QTimer(self)
        self.fetch_timer.timeout.connect(self.trigger_async_refresh)
        self.fetch_timer.start(interval)

    def trigger_async_refresh(self):
        if self.is_fetching:
            return
        self.is_fetching = True
        self.status_dot.setStyleSheet("color: #38bdf8; font-size: 11px;")

        def run():
            try:
                threads = []
                for pid, provider in PROVIDERS.items():
                    def fetch_one(p=provider):
                        try:
                            metrics = p.fetch_usage()
                            self.signals.data_fetched.emit(metrics)
                        except Exception as e:
                            err = UsageMetrics(provider_id=p.provider_id, error=str(e))
                            self.signals.data_fetched.emit(err)

                    t = threading.Thread(target=fetch_one, daemon=True)
                    threads.append(t)
                    t.start()

                for t in threads:
                    t.join(timeout=10.0)
            finally:
                self.is_fetching = False

        threading.Thread(target=run, daemon=True).start()

    def _on_data_fetched(self, metrics: UsageMetrics):
        if metrics.provider_id in self.cards:
            self.cards[metrics.provider_id].update_metrics(metrics)

        now_str = datetime.now().strftime("%H:%M:%S")
        self.time_label.setText(now_str)
        self.status_dot.setStyleSheet("color: #10b981; font-size: 11px;")

    def _update_all_countdowns(self):
        # Auto-detect system wake from sleep/suspend
        cur_ts = time.time()
        if hasattr(self, "_last_countdown_ts"):
            gap = cur_ts - self._last_countdown_ts
            if gap > 15.0:  # System was asleep or suspended for >15 seconds
                self.is_fetching = False  # Clear any stuck lock
                self.trigger_async_refresh()
        self._last_countdown_ts = cur_ts

        for card in self.cards.values():
            card.update_countdown()

    # ================= Click-Through Mode =================
    def set_click_through(self, enable: bool):
        self.config.set("click_through", enable)
        self.ghost_label.setVisible(enable)

        # Cross-platform click-through handling
        if sys.platform == "win32" and user32:
            hwnd = int(self.winId())
            style = user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
            if enable:
                user32.SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_TRANSPARENT | WS_EX_LAYERED)
            else:
                user32.SetWindowLongW(hwnd, GWL_EXSTYLE, style & ~WS_EX_TRANSPARENT)
        else:
            # macOS / Linux native Qt event pass-through
            self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, enable)

        if enable and self.tray_icon:
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
        self._save_geometry()
        super().mouseReleaseEvent(event)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        self._save_geometry()

    def moveEvent(self, event):
        super().moveEvent(event)
        pos = self.pos()
        self.config.set("window_x", pos.x())
        self.config.set("window_y", pos.y())

    def mouseDoubleClickEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.trigger_async_refresh()
            event.accept()

    def _save_geometry(self):
        pos = self.pos()
        size = self.size()
        self.config.set("window_x", pos.x())
        self.config.set("window_y", pos.y())
        mode = self.config.get("layout_mode", "horizontal")
        if mode == "horizontal":
            self.config.set("horizontal_width", size.width())
            self.config.set("horizontal_height", size.height())
        else:
            self.config.set("vertical_width", size.width())
            self.config.set("vertical_height", size.height())

    # ================= Context Menu =================
    def contextMenuEvent(self, event):
        menu = QMenu(self)

        refresh_act = menu.addAction("🔄 立即重新整理所有 AI (Refresh All)")
        refresh_act.triggered.connect(self.trigger_async_refresh)

        menu.addSeparator()

        # Layout Switch Submenu
        layout_menu = menu.addMenu("📐 顯示佈局 (Layout)")
        cur_layout = self.config.get("layout_mode", "horizontal")
        horiz_act = layout_menu.addAction("💻 橫向三欄並排 (Horizontal Triple)")
        horiz_act.setCheckable(True)
        horiz_act.setChecked(cur_layout == "horizontal")
        horiz_act.triggered.connect(lambda: self._apply_layout_mode("horizontal"))

        vert_act = layout_menu.addAction("📱 直立三層堆疊 (Vertical Stack)")
        vert_act.setCheckable(True)
        vert_act.setChecked(cur_layout == "vertical")
        vert_act.triggered.connect(lambda: self._apply_layout_mode("vertical"))

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

        reset_act = menu.addAction("📐 重設預設尺寸與位置")
        reset_act.triggered.connect(self._reset_geometry)

        hide_act = menu.addAction("👁️ 隱藏 HUD (Alt+C 重新喚出)")
        hide_act.triggered.connect(self.hide)

        exit_act = menu.addAction("❌ 結束程式 (Exit)")
        exit_act.triggered.connect(self.close_application)

        menu.exec(event.globalPos())

    def _toggle_always_on_top(self):
        new_val = not self.config.get("always_on_top", True)
        self.config.set("always_on_top", new_val)
        self.setWindowFlag(Qt.WindowType.WindowStaysOnTopHint, new_val)
        self.show()

    def _toggle_lock(self):
        new_val = not self.config.get("locked", False)
        self.config.set("locked", new_val)

    def _set_opacity(self, value: float):
        self.config.set("opacity", value)
        self.setWindowOpacity(value)

    def _set_interval(self, seconds: int):
        self.config.set("refresh_interval_sec", seconds)
        self.fetch_timer.setInterval(seconds * 1000)

    def _toggle_autostart(self):
        currently_enabled = is_autostart_enabled()
        new_val = not currently_enabled
        set_autostart(new_val)
        self.config.set("autostart", new_val)

    def _reset_geometry(self):
        mode = self.config.get("layout_mode", "horizontal")
        if mode == "horizontal":
            self.resize(690, 145)
        else:
            self.resize(280, 410)
        screen = self.screen().geometry()
        self.move(screen.width() - self.width() - 40, 50)
        self._save_geometry()

    def toggle_visibility(self):
        if self.isVisible():
            self.hide()
        else:
            self.show()
            self.activateWindow()

    def close_application(self):
        self.countdown_timer.stop()
        self.fetch_timer.stop()
        self.close()
        sys.exit(0)
