import sys
import os
import time
from datetime import datetime
from PySide6.QtCore import Qt, QPoint, QRect, QTimer
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QMenu, QPushButton, QFrame, QApplication, QInputDialog
)
from PySide6.QtGui import QCursor, QGuiApplication, QIcon

from core.providers import PROVIDERS, UsageMetrics
from core.config_manager import ConfigManager
from core.refresh_controller import RefreshController
from core.autostart import is_autostart_enabled, set_autostart
from core.logger import logger, open_log_dir
from ui.styles import (
    get_hud_stylesheet, get_cards_stylesheet,
    THEMES, SCHEMES, APPEARANCES
)
from ui.provider_card import ProviderCardWidget
from ui.usage_table import UsageTable
from ui import vibrancy
from ui.menu_icons import menu_icon
from system.memory import trim_memory

SCHEME_LABELS = {"scale": "色階 (Scale)", "duo": "雙色 (Duo)"}
APPEARANCE_LABELS = {"auto": "跟隨系統 (Auto)", "light": "淺色 (Light)", "dark": "深色 (Dark)"}

MIN_TABLE_W, MIN_TABLE_H = 380, 280
DEF_TABLE_W, DEF_TABLE_H = 450, 350

MIN_HORIZ_W, MIN_HORIZ_H = 540, 125
DEF_HORIZ_W, DEF_HORIZ_H = 690, 145

MIN_VERT_W, MIN_VERT_H = 250, 320
DEF_VERT_W, DEF_VERT_H = 280, 410

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

        self._latest_metrics = {}
        self._provider_errors = {}
        self.providers = PROVIDERS if providers is None else providers
        for p in self.providers.values():
            if hasattr(p, "config"):
                p.config = self.config
        self.refresh_controller = RefreshController(
            self.providers, config.get("refresh_interval_sec", 60), self)
        self.refresh_controller.updated.connect(self._on_data_fetched)
        self.refresh_controller.busy_changed.connect(self._on_busy_changed)

        # Provider Cards (Classic UI)
        self.cards = {
            "claude": ProviderCardWidget("claude"),
            "agy": ProviderCardWidget("agy"),
            "codex": ProviderCardWidget("codex")
        }

        # Usage Table (Modern Table UI)
        self.table = UsageTable(self.theme(), self.color_scheme(), parent=self)

        self._init_window_flags()
        self._init_ui_skeleton()
        self._apply_ui_mode(self.config.get("ui_mode", "cards"), initial=True)
        self._setup_timers()
        QGuiApplication.styleHints().colorSchemeChanged.connect(self._on_system_appearance_changed)

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
        self.inner_layout.setSpacing(6)

        # Header bar widgets
        self.status_dot = QLabel("●")
        self.status_dot.setStyleSheet("color: #10b981; font-size: 11px;")

        self.title_label = QLabel("AI AGENT HUD")
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

        # Container for Cards
        self.cards_container = QWidget(self)
        self.cards_layout = QVBoxLayout(self.cards_container)
        self.cards_layout.setContentsMargins(0, 0, 0, 0)
        self.cards_layout.setSpacing(6)

    def _clear_layout(self, layout):
        keep_widgets = {
            getattr(self, "status_dot", None),
            getattr(self, "title_label", None),
            getattr(self, "ghost_label", None),
            getattr(self, "layout_toggle_btn", None),
            getattr(self, "time_label", None),
            getattr(self, "cards_container", None),
            getattr(self, "table", None),
            *getattr(self, "cards", {}).values()
        }
        while layout.count():
            item = layout.takeAt(0)
            widget = item.widget()
            if widget:
                widget.setParent(None)
                if widget not in keep_widgets:
                    widget.deleteLater()
            sub_layout = item.layout()
            if sub_layout:
                self._clear_layout(sub_layout)
                sub_layout.deleteLater()

    # ================= UI Mode & Themes =================
    def color_scheme(self) -> str:
        val = self.config.get("color_scheme", "scale")
        return val if val in SCHEMES else "scale"

    def set_color_scheme(self, scheme: str):
        if scheme not in SCHEMES:
            return
        self.config.set("color_scheme", scheme)
        self._rebuild_table()

    def set_appearance(self, appearance: str):
        if appearance not in APPEARANCES:
            return
        self.config.set("appearance", appearance)
        self._apply_theme()
        self._rebuild_table()

    def _on_system_appearance_changed(self, *_):
        if self.config.get("appearance", "auto") == "auto":
            QTimer.singleShot(0, self._refresh_system_appearance)

    def _refresh_system_appearance(self):
        if self.config.get("appearance", "auto") == "auto":
            self._apply_theme()
            self._rebuild_table()

    def theme(self) -> dict:
        app_setting = self.config.get("appearance", "auto")
        if app_setting == "light":
            return THEMES["light"]
        if app_setting == "dark":
            return THEMES["dark"]
        return THEMES["dark"] if self._system_is_dark() else THEMES["light"]

    def _system_is_dark(self) -> bool:
        scheme = QGuiApplication.styleHints().colorScheme()
        if scheme != Qt.ColorScheme.Unknown:
            return scheme == Qt.ColorScheme.Dark
        app = QApplication.instance()
        pal = app.palette() if app else None
        if pal:
            return pal.window().color().lightness() < 128
        return True

    def _apply_theme(self):
        ui_mode = self.config.get("ui_mode", "cards")
        t = self.theme()
        if ui_mode == "table":
            dark = (t is THEMES["dark"])
            vibrant = bool(self.isVisible() and vibrancy.apply(self, dark, t["radius"]))
            self.setStyleSheet(get_hud_stylesheet(t, vibrant))
        else:
            vibrancy.clear(self)
            dark = t is THEMES["dark"]
            self.setStyleSheet(get_cards_stylesheet(dark=dark))
            for card in self.cards.values():
                card.set_appearance(dark)

    def _rebuild_table(self):
        t = self.theme()
        s = self.color_scheme()
        old_table = self.table
        self.table = UsageTable(t, s, parent=self)
        for m in self._latest_metrics.values():
            self.table.update_metrics(m)

        ui_mode = self.config.get("ui_mode", "cards")
        if ui_mode == "table":
            self._apply_ui_mode("table")
        if old_table:
            old_table.deleteLater()

    def _apply_ui_mode(self, mode: str, initial=False):
        if not initial:
            if self.config.get("ui_mode") != mode:
                self._persist_geometry()
                self.config.set("ui_mode", mode)
            if self.tray_icon:
                self.tray_icon.update_menu_state()

        self._restoring_geometry = True
        self._clear_layout(self.inner_layout)

        # Header Bar
        header_layout = QHBoxLayout()
        header_layout.setSpacing(6)
        header_layout.addWidget(self.status_dot)
        header_layout.addWidget(self.title_label)
        header_layout.addWidget(self.ghost_label)
        header_layout.addWidget(self.layout_toggle_btn)
        header_layout.addStretch()
        header_layout.addWidget(self.time_label)
        self.inner_layout.addLayout(header_layout)

        if mode == "table":
            self.title_label.setText("AI AGENT HUD (TABLE)")
            self.layout_toggle_btn.setVisible(False)
            self._apply_theme()
            self.inner_layout.addWidget(self.table)

            self.setMinimumSize(MIN_TABLE_W, MIN_TABLE_H)
            w = self.config.get("table_width", DEF_TABLE_W)
            h = self.config.get("table_height", DEF_TABLE_H)
            if w < MIN_TABLE_W:
                w = DEF_TABLE_W
            if h < MIN_TABLE_H:
                h = DEF_TABLE_H
            self.resize(w, h)
        else:
            self.title_label.setText("AI AGENT HUD (3-IN-1)")
            self.layout_toggle_btn.setVisible(True)
            self._apply_theme()
            self.inner_layout.addWidget(self.cards_container)
            card_layout_mode = self.config.get("layout_mode", "horizontal")
            w, h = self._apply_cards_layout(card_layout_mode)

        if initial:
            x = self.config.get("window_x")
            y = self.config.get("window_y")
            self._restore_or_default_position(x, y, w, h)
        else:
            self._ensure_within_screen(w, h)

        self._restoring_geometry = False
        self._save_geometry()

    def _apply_cards_layout(self, mode: str) -> tuple[int, int]:
        self._clear_layout(self.cards_layout)
        provider_ids = ["claude", "agy", "codex"]

        for card in self.cards.values():
            card.layout_mode = mode
            card.update_metrics(card.current_metrics)

        if mode == "horizontal":
            self.setMinimumSize(MIN_HORIZ_W, MIN_HORIZ_H)
            w = self.config.get("horizontal_width", DEF_HORIZ_W)
            h = self.config.get("horizontal_height", DEF_HORIZ_H)
            if w < MIN_HORIZ_W:
                w = DEF_HORIZ_W
            if h < MIN_HORIZ_H:
                h = DEF_HORIZ_H
            self.resize(w, h)

            body_layout = QHBoxLayout()
            body_layout.setSpacing(8)
            for i, pid in enumerate(provider_ids):
                body_layout.addWidget(self.cards[pid], 1)
                if i < len(provider_ids) - 1:
                    divider = QFrame()
                    divider.setObjectName("Divider")
                    divider.setFrameShape(QFrame.Shape.VLine)
                    body_layout.addWidget(divider)
            self.cards_layout.addLayout(body_layout)
        else:
            self.setMinimumSize(MIN_VERT_W, MIN_VERT_H)
            w = self.config.get("vertical_width", DEF_VERT_W)
            h = self.config.get("vertical_height", DEF_VERT_H)
            if w < MIN_VERT_W:
                w = DEF_VERT_W
            if h < MIN_VERT_H:
                h = DEF_VERT_H
            self.resize(w, h)

            for i, pid in enumerate(provider_ids):
                self.cards_layout.addWidget(self.cards[pid])
                if i < len(provider_ids) - 1:
                    h_div = QFrame()
                    h_div.setObjectName("HorizontalDivider")
                    h_div.setFrameShape(QFrame.Shape.HLine)
                    self.cards_layout.addWidget(h_div)

        return w, h

    def toggle_ui_mode(self):
        cur = self.config.get("ui_mode", "cards")
        new_mode = "table" if cur == "cards" else "cards"
        self._apply_ui_mode(new_mode)

    def toggle_layout_mode(self):
        cur = self.config.get("layout_mode", "horizontal")
        new_mode = "vertical" if cur == "horizontal" else "horizontal"
        self._apply_cards_layout_mode(new_mode)

    def _ensure_within_screen(self, w: int, h: int):
        current_center = self.geometry().center()
        target_screen = None
        for screen in QGuiApplication.screens():
            if screen.availableGeometry().contains(current_center):
                target_screen = screen
                break
        if not target_screen:
            target_screen = self.screen() or QGuiApplication.primaryScreen()

        if target_screen:
            avail = target_screen.availableGeometry()
            cur_x = self.x()
            cur_y = self.y()

            if cur_x + w > avail.right():
                cur_x = max(avail.left(), avail.right() - w)
            if cur_x < avail.left():
                cur_x = avail.left()

            if cur_y + h > avail.bottom():
                cur_y = max(avail.top(), avail.bottom() - h)
            if cur_y < avail.top():
                cur_y = avail.top()

            self.move(cur_x, cur_y)

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
        if metrics.provider_id in self.cards:
            self.cards[metrics.provider_id].update_metrics(metrics)
        self.table.update_metrics(metrics)
        self._provider_errors[metrics.provider_id] = bool(metrics.error)
        self.time_label.setText(datetime.now().strftime("%H:%M:%S"))

    def _on_busy_changed(self, busy):
        color = "#38bdf8" if busy else ("#f59e0b" if any(self._provider_errors.values()) else "#10b981")
        self.status_dot.setStyleSheet(f"color: {color}; font-size: 11px;")
        if not busy:
            QTimer.singleShot(1000, trim_memory)

    def _update_all_countdowns(self):
        cur_ts = time.time()
        if hasattr(self, "_last_countdown_ts"):
            gap = cur_ts - self._last_countdown_ts
            if gap > 15.0:
                self.trigger_async_refresh()
        self._last_countdown_ts = cur_ts

        for card in self.cards.values():
            card.update_countdown()
        self.table.update_countdowns()

    # ================= Click-Through Mode =================
    def _apply_native_click_through(self, enable: bool):
        if sys.platform == "win32" and user32:
            hwnd = int(self.winId())
            style = user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
            if enable:
                new_style = style | WS_EX_TRANSPARENT | WS_EX_LAYERED
            else:
                new_style = style & ~WS_EX_TRANSPARENT
            user32.SetWindowLongW(hwnd, GWL_EXSTYLE, new_style)
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

    def showEvent(self, event):
        super().showEvent(event)
        QTimer.singleShot(0, self._apply_theme)

    def hideEvent(self, event):
        super().hideEvent(event)
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
        ui_mode = self.config.get("ui_mode", "cards")
        updates = {
            "window_x": pos.x(),
            "window_y": pos.y(),
        }
        if ui_mode == "table":
            updates["table_width"] = max(MIN_TABLE_W, size.width())
            updates["table_height"] = max(MIN_TABLE_H, size.height())
        else:
            layout_mode = self.config.get("layout_mode", "horizontal")
            if layout_mode == "horizontal":
                updates["horizontal_width"] = max(MIN_HORIZ_W, size.width())
                updates["horizontal_height"] = max(MIN_HORIZ_H, size.height())
            else:
                updates["vertical_width"] = max(MIN_VERT_W, size.width())
                updates["vertical_height"] = max(MIN_VERT_H, size.height())
        self.config.set_many(updates)

    # ================= Context Menu =================
    def add_theme_menus(self, menu: QMenu):
        """Colour scheme and appearance submenus (used by both HUD and tray)."""
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
            sub.aboutToShow.connect(sync)
            return sub

        colors = build("🎨 配色 (Colors)", SCHEMES, SCHEME_LABELS, self.color_scheme, self.set_color_scheme)
        def sync_colors():
            colors.menuAction().setVisible(self.config.get("ui_mode", "cards") == "table")
        sync_colors()
        menu.aboutToShow.connect(sync_colors)
        build("🌓 外觀 (Appearance)", APPEARANCES, APPEARANCE_LABELS,
              lambda: self.config.get("appearance", "auto"), self.set_appearance)

    def contextMenuEvent(self, event):
        menu = QMenu(self)
        self.populate_context_menu(menu)
        menu.exec(event.globalPos())

    def populate_context_menu(self, menu: QMenu):
        """Fill `menu` with the full HUD menu (shared by the right-click menu and the tray icon)."""
        refresh_act = menu.addAction(menu_icon("refresh"), "立即重新整理所有 AI (Refresh All)")
        refresh_act.triggered.connect(self.trigger_async_refresh)

        menu.addSeparator()

        # Claude Account Submenu
        claude_menu = menu.addMenu(menu_icon("claude"), "Claude 帳號 (Claude Account)")
        from core.providers.claude_provider import discover_profiles, resolve_active_profile
        cur_profile = self.config.get("claude_profile", "auto")
        is_auto = (cur_profile == "auto")
        active_prof, _ = resolve_active_profile(cur_profile)

        auto_title = f"智慧自動追蹤 (目前: {active_prof.short_name})" if (is_auto and active_prof.id != "default") else "智慧自動追蹤 (最近活躍)"
        auto_act = claude_menu.addAction(menu_icon("target"), auto_title)
        auto_act.setCheckable(True)
        auto_act.setChecked(is_auto)
        auto_act.triggered.connect(lambda: self._set_claude_profile("auto"))

        claude_menu.addSeparator()

        for prof in discover_profiles()[:40]:
            is_selected = (not is_auto and (cur_profile == prof.id or (cur_profile == ".claude" and prof.id == "default")))
            p_act = claude_menu.addAction(prof.display_name)
            p_act.setCheckable(True)
            p_act.setChecked(is_selected)
            p_act.triggered.connect(lambda checked, pid=prof.id: self._set_claude_profile(pid))

        menu.addSeparator()

        # AGY Account Submenu
        agy_menu = menu.addMenu(menu_icon("agy"), "AGY 帳號 (AGY Account)")

        from core.providers.agy_provider import discover_profiles as discover_agy_profiles, resolve_active_profile as resolve_active_agy_profile
        cur_agy_profile = self.config.get("agy_profile", "auto")
        is_agy_auto = (cur_agy_profile == "auto")
        active_agy_prof, _ = resolve_active_agy_profile(cur_agy_profile)

        agy_auto_title = f"智慧自動追蹤 (目前: {active_agy_prof.short_name})" if (is_agy_auto and active_agy_prof.id != "default") else "智慧自動追蹤 (目前活躍)"
        agy_auto_act = agy_menu.addAction(menu_icon("target"), agy_auto_title)
        agy_auto_act.setCheckable(True)
        agy_auto_act.setChecked(is_agy_auto)
        agy_auto_act.triggered.connect(lambda: self._set_agy_profile("auto"))

        agy_menu.addSeparator()

        for prof in discover_agy_profiles()[:40]:
            is_selected = (not is_agy_auto and (cur_agy_profile == prof.id or cur_agy_profile == prof.short_name))
            p_act = agy_menu.addAction(prof.display_name)
            p_act.setCheckable(True)
            p_act.setChecked(is_selected)
            p_act.triggered.connect(lambda checked, pid=prof.id: self._set_agy_profile(pid))

        agy_menu.addSeparator()
        save_act = agy_menu.addAction(menu_icon("save"), "將當前帳號儲存為...")
        save_act.triggered.connect(self._prompt_save_agy_profile)

        menu.addSeparator()

        # Layout Switch Submenu
        layout_menu = menu.addMenu(menu_icon("layout"), "顯示佈局 (Layout)")
        cur_layout = self.config.get("layout_mode", "horizontal")
        horiz_act = layout_menu.addAction(menu_icon("laptop"), "橫向三欄並排 (Horizontal Triple)")
        horiz_act.setCheckable(True)
        horiz_act.setChecked(cur_layout == "horizontal")
        horiz_act.triggered.connect(lambda: self._apply_cards_layout_mode("horizontal"))

        vert_act = layout_menu.addAction(menu_icon("phone"), "直立三層堆疊 (Vertical Stack)")
        vert_act.setCheckable(True)
        vert_act.setChecked(cur_layout == "vertical")
        vert_act.triggered.connect(lambda: self._apply_cards_layout_mode("vertical"))

        # Click-through toggle
        ghost_act = menu.addAction(menu_icon("ghost"), "滑鼠點擊穿透 (Alt+Shift+C)")
        ghost_act.setCheckable(True)
        ghost_act.setChecked(self.config.get("click_through", False))
        ghost_act.triggered.connect(self.toggle_click_through)

        # Always on top
        aot_act = menu.addAction(menu_icon("pin"), "視窗永遠置頂 (Always on Top)")
        aot_act.setCheckable(True)
        aot_act.setChecked(self.config.get("always_on_top", True))
        aot_act.triggered.connect(self._toggle_always_on_top)

        # Lock position
        lock_act = menu.addAction(menu_icon("lock"), "鎖定視窗位置 (Lock Drag)")
        lock_act.setCheckable(True)
        lock_act.setChecked(self.config.get("locked", False))
        lock_act.triggered.connect(self._toggle_lock)

        # Opacity submenu
        opacity_menu = menu.addMenu(menu_icon("opacity"), "視窗透明度 (Opacity)")
        current_op = self.config.get("opacity", 0.88)
        for pct in [100, 90, 80, 70, 50, 30]:
            val = pct / 100.0
            act = opacity_menu.addAction(f"{pct}%")
            act.setCheckable(True)
            act.setChecked(abs(current_op - val) < 0.05)
            act.triggered.connect(lambda checked, v=val: self._set_opacity(v))

        # Refresh interval submenu
        interval_menu = menu.addMenu(menu_icon("timer"), "更新頻率 (Interval)")
        cur_int = self.config.get("refresh_interval_sec", 60)
        for sec in [30, 60, 120, 300]:
            act = interval_menu.addAction(f"{sec} 秒")
            act.setCheckable(True)
            act.setChecked(cur_int == sec)
            act.triggered.connect(lambda checked, s=sec: self._set_interval(s))

        # Autostart (cross-platform)
        autostart_act = menu.addAction(menu_icon("rocket"), "開機自動啟動 (Start on Boot)")
        autostart_act.setCheckable(True)
        autostart_act.setChecked(is_autostart_enabled())
        autostart_act.triggered.connect(self._toggle_autostart)

        menu.addSeparator()

        log_act = menu.addAction(menu_icon("folder"), "開啟記錄檔目錄 (Open Logs)")
        log_act.triggered.connect(open_log_dir)

        reset_act = menu.addAction(menu_icon("reset"), "重設預設尺寸與位置")
        reset_act.triggered.connect(self._reset_geometry)

        hide_act = menu.addAction(menu_icon("eye"), "隱藏 HUD (Alt+C 重新喚出)")
        hide_act.triggered.connect(self.hide)

        exit_act = menu.addAction(menu_icon("exit"), "結束程式 (Exit)")
        exit_act.triggered.connect(self.close_application)

    def _apply_cards_layout_mode(self, mode: str):
        self._persist_geometry()
        self.config.set("layout_mode", mode)
        self._apply_ui_mode("cards")
        if self.tray_icon:
            self.tray_icon.update_menu_state()
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

    def _set_claude_profile(self, profile_id: str):
        self.config.set("claude_profile", profile_id)
        if hasattr(self, "providers") and isinstance(self.providers, dict):
            cp = self.providers.get("claude")
            if cp and hasattr(cp, "profile_preference"):
                cp.profile_preference = profile_id
        self.trigger_async_refresh()

    def _set_agy_profile(self, profile_id: str):
        self.config.set("agy_profile", profile_id)
        if hasattr(self, "providers") and isinstance(self.providers, dict):
            ap = self.providers.get("agy")
            if ap and hasattr(ap, "profile_preference"):
                ap.profile_preference = profile_id
        from core.providers.agy_provider import switch_active_profile
        if profile_id != "auto":
            switch_active_profile(profile_id)
        self.trigger_async_refresh()

    def _prompt_save_agy_profile(self):
        alias, ok = QInputDialog.getText(self, "儲存 AGY 帳號", "請輸入此帳號的別名 (例如 work, personal):")
        if ok and alias.strip():
            from core.providers.agy_provider import save_current_as_profile
            prof = save_current_as_profile(alias.strip())
            if prof:
                self._set_agy_profile(prof.id)

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
        ui_mode = self.config.get("ui_mode", "cards")
        if ui_mode == "table":
            w, h = DEF_TABLE_W, DEF_TABLE_H
        else:
            layout_mode = self.config.get("layout_mode", "horizontal")
            if layout_mode == "horizontal":
                w, h = DEF_HORIZ_W, DEF_HORIZ_H
            else:
                w, h = DEF_VERT_W, DEF_VERT_H

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
