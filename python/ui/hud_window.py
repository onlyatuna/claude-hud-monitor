import sys
import os
import time
from datetime import datetime
from PySide6.QtCore import Qt, QEvent, QPoint, QRect, QTimer
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QMenu, QApplication
)
from PySide6.QtGui import QCursor, QGuiApplication, QIcon, QRegion

from core.providers import PROVIDERS, UsageMetrics
from core.agy_rows import arrange_agy, arrange_by_order, normalize_order, DEFAULT_ORDER as AGY_DEFAULT_ORDER
from core.config_manager import ConfigManager, UI_MODES
from core.refresh_controller import RefreshController
from core.autostart import is_autostart_enabled, set_autostart
from core.logger import logger, open_log_dir
from ui.styles import (
    get_hud_stylesheet, get_cards_stylesheet,
    THEMES, SCHEMES, APPEARANCES
)
from ui.provider_card import ProviderCardWidget
from ui.usage_table import UsageTable
from ui.sidebar import SidebarWidget, DEFAULT_ORDER
from ui.dock_geometry import (EDGES, dock_to_edge, offset_along_edge, orientation_for_edge, pick_work_area,
                              snap_rect, snap_rect_corner, step_scale, clamp_scale)
from ui.hud_menu import build_hud_menu
from ui.table_layout import (FULL_MIN_W, FULL_MIN_H, DEF_COMPACT_W, DEF_COMPACT_H, COMPACT_MARGIN,
                             compact_min_size, table_state)
from ui import vibrancy
from ui.round_mask import rounded_rects
from system.memory import trim_memory

SCHEME_LABELS = {"scale": "色階 (Scale)", "duo": "雙色 (Duo)"}
APPEARANCE_LABELS = {"auto": "跟隨系統 (Auto)", "light": "淺色 (Light)", "dark": "深色 (Dark)"}

MIN_TABLE_W, MIN_TABLE_H = FULL_MIN_W, FULL_MIN_H   # below this the table collapses to dials (ui/table_layout.py)
DEF_TABLE_W, DEF_TABLE_H = 450, 350

QWIDGETSIZE_MAX = 16777215
SIDEBAR_SNAP_PX = 24
RESIZE_GRIP_PX = 7
IDLE_FADE_FACTOR = 0.6
IDLE_FADE_DELAY_MS = 900
IDLE_LEVELS = (80, 60, 40, 25)       # % of the chosen opacity the idle bar dims to
IDLE_DELAYS_MS = (500, 900, 2000, 5000)

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

        self._blur_active = False        # native blur is on behind the table
        self._blur_suspended = False     # ... but paused while the window is moved or resized
        self._mask_key = None
        self._blur_resume_timer = QTimer(self)
        self._blur_resume_timer.setSingleShot(True)
        self._blur_resume_timer.setInterval(220)
        self._blur_resume_timer.timeout.connect(self._resume_blur)

        self._latest_metrics = {}
        self._raw_metrics = {}
        self._provider_errors = {}
        self.providers = PROVIDERS if providers is None else providers
        for p in self.providers.values():
            if hasattr(p, "config"):
                p.config = self.config
        self.refresh_controller = RefreshController(
            self.providers, config.get("refresh_interval_sec", 60), self)
        self.refresh_controller.updated.connect(self._on_data_fetched)
        self.refresh_controller.busy_changed.connect(self._on_busy_changed)

        # Usage Table (Modern Table UI)
        self.table = UsageTable(self.theme(), self.color_scheme(), provider_ids=self.provider_order(), parent=self)

        # Sidebar (docked strip/panel); its panel shows the full provider cards
        self.sidebar_cards = {pid: ProviderCardWidget(pid) for pid in ("claude", "agy", "codex")}
        self.sidebar = SidebarWidget(self.sidebar_cards, parent=self)
        self.sidebar.hide()
        self.sidebar.stateChangeRequested.connect(self.set_sidebar_state)
        self.sidebar.contextMenuRequested.connect(self._show_sidebar_menu)
        self.sidebar.scaleStepRequested.connect(self._on_sidebar_scale_step)
        self.sidebar.dragRequested.connect(self._start_sidebar_drag)
        self.sidebar.orderChanged.connect(self.set_provider_order)
        for pid, card in self.sidebar_cards.items():
            card.rowOrderChanged.connect(lambda titles, pid=pid: self.set_row_order(pid, titles))
        self.sidebar.set_order(self.provider_order())
        self._resize = None
        self._grip_cursor = False
        self._fade_timer = QTimer(self)
        self._fade_timer.setSingleShot(True)
        self._fade_timer.timeout.connect(self._apply_opacity)
        QApplication.instance().installEventFilter(self)
        self._docking = False
        self._snap_timer = QTimer(self)
        self._snap_timer.setSingleShot(True)
        self._snap_timer.setInterval(180)
        self._snap_timer.timeout.connect(self._finish_sidebar_move)

        self._init_window_flags()
        self._init_ui_skeleton()
        self._apply_ui_mode(self.config.get("ui_mode", "sidebar"), initial=True)
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

        self.time_label = QLabel("--:--:--")
        self.time_label.setObjectName("HeaderStatus")


    def _clear_layout(self, layout):
        keep_widgets = {
            getattr(self, "status_dot", None),
            getattr(self, "title_label", None),
            getattr(self, "ghost_label", None),
            getattr(self, "time_label", None),
            getattr(self, "table", None),
            getattr(self, "sidebar", None),
            *getattr(self, "sidebar_cards", {}).values()
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
        if self.config.get("ui_mode") == "sidebar":
            self._apply_theme()
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
        ui_mode = self.config.get("ui_mode", "sidebar")
        t = self.theme()
        if ui_mode == "table":
            dark = (t is THEMES["dark"])
            blur = bool(self.config.get("table_blur", True))
            if not blur:
                vibrancy.clear(self)
            vibrant = bool(blur and self.isVisible() and vibrancy.apply(self, dark, t["radius"]))
            self._blur_active, self._blur_suspended = vibrant, False
            self.setStyleSheet(get_hud_stylesheet(t, vibrant))
        else:
            self._blur_active = False
            vibrancy.clear(self)
            dark = t is THEMES["dark"]
            self.setStyleSheet(get_cards_stylesheet(dark=dark))
            self.sidebar.set_appearance(dark, 'scale')   # status colours only; the duo scheme is for the table
            for card in self.sidebar_cards.values():
                card.set_appearance(dark)
        self._refresh_mask()

    def _refresh_mask(self):
        """Clip the table window to rounded corners where native blur would otherwise show square corners."""
        if self.config.get("ui_mode", "sidebar") == "table" and vibrancy.mask_needed(self._blur_active):
            key = (self.width(), self.height(), self.theme()["radius"])
            if key != self._mask_key:
                region = QRegion()
                for rect in rounded_rects(*key):
                    region = region.united(QRegion(*rect))
                self.setMask(region)
                self._mask_key = key
        elif self._mask_key is not None:
            self.clearMask()
            self._mask_key = None

    def toggle_table_blur(self):
        self.config.set("table_blur", not self.config.get("table_blur", True))
        self._apply_theme()
        if self.tray_icon:
            self.tray_icon.update_menu_state()

    def _suspend_blur(self):
        """Acrylic makes dragging and resizing stutter: drop the blur until the movement stops."""
        if not self._blur_active or self.config.get("ui_mode", "sidebar") != "table":
            return
        if not self._blur_suspended:
            self._blur_suspended = bool(vibrancy.suspend(self, self.theme() is THEMES["dark"]))
        if self._blur_suspended:
            self._blur_resume_timer.start()

    def _resume_blur(self):
        if self._blur_suspended and self._blur_active:
            vibrancy.apply(self, self.theme() is THEMES["dark"], self.theme()["radius"])
        self._blur_suspended = False

    def _rebuild_table(self):
        if self.config.get("ui_mode", "sidebar") == "table":
            self._persist_geometry()
        t = self.theme()
        s = self.color_scheme()
        old_table = self.table
        self.table = UsageTable(t, s, provider_ids=self.provider_order(), parent=self)
        for m in self._latest_metrics.values():
            self.table.update_metrics(m)

        if self.config.get("ui_mode", "sidebar") == "table":
            self._apply_ui_mode("table")
        if old_table:
            old_table.deleteLater()

    def _apply_ui_mode(self, mode: str, initial=False):
        if mode not in UI_MODES:
            mode = "sidebar"      # the retired Classic Cards style now lives in the sidebar panel
        if not initial:
            if self.config.get("ui_mode") != mode:
                self._persist_geometry()
                self.config.set("ui_mode", mode)
            if self.tray_icon:
                self.tray_icon.update_menu_state()

        self._restoring_geometry = True
        self._clear_layout(self.inner_layout)
        self.sidebar.close_flyout()
        self.sidebar.hide()
        self.table.hide()
        self.setMinimumSize(0, 0)
        self.setMaximumSize(QWIDGETSIZE_MAX, QWIDGETSIZE_MAX)
        self.inner_layout.setContentsMargins(12, 8, 12, 10)
        self.inner_layout.setSpacing(6)

        if mode == "sidebar":
            self._apply_sidebar_mode()
            self._restoring_geometry = False
            self._save_geometry()
            self._republish_all()
            return

        # Header Bar
        header_layout = QHBoxLayout()
        header_layout.setSpacing(6)
        header_layout.addWidget(self.status_dot)
        header_layout.addWidget(self.title_label)
        header_layout.addWidget(self.ghost_label)
        header_layout.addStretch()
        header_layout.addWidget(self.time_label)
        self.inner_layout.addLayout(header_layout)

        self.title_label.setText("AI AGENT HUD (TABLE)")
        self._apply_theme()
        self.inner_layout.addWidget(self.table)
        self.table.show()

        compact = bool(self.config.get("table_compact", False))
        self._set_table_compact(compact)
        w, h = self._saved_table_size(compact)
        self.resize(w, h)

        if initial:
            x = self.config.get("window_x")
            y = self.config.get("window_y")
            self._restore_or_default_position(x, y, w, h)
        else:
            self._ensure_within_screen(w, h)

        self._restoring_geometry = False
        self._save_geometry()
        self._republish_all()

    def _saved_table_size(self, compact: bool) -> tuple:
        """The remembered window size for the collapsed or the full table (undersized values reset)."""
        if compact:
            min_w, min_h = compact_min_size(len(self.table.columns))
            w = self.config.get("table_compact_width", DEF_COMPACT_W)
            h = self.config.get("table_compact_height", DEF_COMPACT_H)
            return (w if isinstance(w, int) and w >= min_w else DEF_COMPACT_W,
                    h if isinstance(h, int) and h >= min_h else DEF_COMPACT_H)
        w = self.config.get("table_width", DEF_TABLE_W)
        h = self.config.get("table_height", DEF_TABLE_H)
        return (w if isinstance(w, int) and w >= MIN_TABLE_W else DEF_TABLE_W,
                h if isinstance(h, int) and h >= MIN_TABLE_H else DEF_TABLE_H)

    def _set_table_compact(self, compact: bool):
        """Collapse the table to one dial per service (or restore it) and adapt the window chrome."""
        self.table.set_compact(compact)
        self.setMinimumSize(*compact_min_size(len(self.table.columns)))   # the full table may be dragged down to it
        self.inner_layout.setContentsMargins(*((COMPACT_MARGIN,) * 4 if compact else (12, 8, 12, 10)))
        self.inner_layout.setSpacing(0 if compact else 6)
        self._sync_header()
        self.config.set("table_compact", bool(compact))

    def _sync_header(self):
        compact = self.config.get("ui_mode") == "table" and self.table.is_compact()
        for widget in (self.status_dot, self.title_label, self.time_label):
            widget.setVisible(not compact)
        self.ghost_label.setVisible(self.config.get("click_through", False) and not compact)

    def _sync_table_state(self):
        if self._restoring_geometry or self.config.get("ui_mode") != "table":
            return
        compact = table_state(self.width(), self.height(), self.table.is_compact())
        if compact != self.table.is_compact():
            self._set_table_compact(compact)

    # ================= Sidebar mode =================
    def _apply_sidebar_mode(self):
        self.inner_layout.setContentsMargins(0, 0, 0, 0)
        self.inner_layout.setSpacing(0)
        state = self.config.get("sidebar_state", "panel")
        self.inner_layout.addWidget(self.sidebar)
        self.sidebar.show()
        self._fade_timer.start(1500)      # stay fully visible for a moment after the mode is chosen
        self.sidebar.set_scale(self._sidebar_scale())
        self.sidebar.set_orientation(self._sidebar_orientation())
        self.sidebar.set_dock_edge(self._sidebar_edge())
        self.sidebar.set_state(state if state in ("strip", "panel") else "strip")
        self._apply_theme()
        self._dock_sidebar()

    def _sidebar_scale(self) -> float:
        value = self.config.get("sidebar_scale", 1.0)
        try:
            return max(0.6, min(2.0, round(float(value), 2)))
        except (TypeError, ValueError):
            return 1.0

    def _sidebar_edge(self):
        edge = self.config.get("sidebar_edge", "right")
        return edge if edge in EDGES else None

    def _sidebar_orientation(self) -> str:
        return orientation_for_edge(self._sidebar_edge(), self.config.get("sidebar_orientation", "vertical"))

    def _sidebar_work_area(self):
        areas = [(g.x(), g.y(), g.width(), g.height())
                 for g in (s.availableGeometry() for s in QGuiApplication.screens())]
        if not areas:
            return None
        center = self.geometry().center()
        return pick_work_area((center.x(), center.y()), areas)

    def _dock_sidebar(self):
        """Place the bar on its saved edge (or saved free position) of the current screen."""
        work = self._sidebar_work_area()
        if work is None:
            return
        size = self.sidebar.preferred_size()
        edge = self._sidebar_edge()
        if edge is not None:
            offset = self.config.get("sidebar_offset")
            rect = dock_to_edge(work, edge, size.width(), size.height(),
                                offset if isinstance(offset, int) or offset == "end" else None)
        else:
            x, y = self.config.get("sidebar_x"), self.config.get("sidebar_y")
            if not isinstance(x, int) or not isinstance(y, int):
                x, y = work[0] + work[2], work[1] + work[3]
            rect, _ = snap_rect((x, y, size.width(), size.height()), work, -10**6)
        self._docking = True
        try:
            self.setFixedSize(rect[2], rect[3])
            self.setGeometry(*rect)
        finally:
            self._docking = False

    def _finish_sidebar_move(self):
        """After a drag: snap to the nearest screen edge (or stay free) and remember the position."""
        if self.config.get("ui_mode") != "sidebar" or self._docking:
            return
        if QGuiApplication.mouseButtons() != Qt.MouseButton.NoButton:
            self._snap_timer.start()
            return
        work = self._sidebar_work_area()
        if work is None:
            return
        g = self.geometry()
        current = self.config.get("sidebar_orientation", "vertical")
        # near a corner the bar keeps its orientation and sticks to the corner instead of turning sideways
        rect, edge, offset = snap_rect_corner((g.x(), g.y(), g.width(), g.height()), work, SIDEBAR_SNAP_PX, prefer=current)
        orientation = orientation_for_edge(edge, current)
        updates = {"sidebar_edge": edge or "none", "sidebar_orientation": orientation}
        if edge is not None:
            updates["sidebar_offset"] = offset
        else:
            updates["sidebar_x"], updates["sidebar_y"] = rect[0], rect[1]
        self.config.set_many(updates)
        self.sidebar.set_orientation(orientation)
        self.sidebar.set_dock_edge(edge)
        self._dock_sidebar()

    def _start_sidebar_drag(self):
        if self.config.get("locked", False):
            return
        handle = self.windowHandle()
        if handle:
            handle.startSystemMove()

    def _resize_zone(self, pos: QPoint):
        g = self.geometry()
        if not g.adjusted(-2, -2, 2, 2).contains(pos):
            return None
        m = RESIZE_GRIP_PX
        zone = ""
        if pos.x() - g.left() < m:
            zone += "l"
        elif g.right() - pos.x() < m:
            zone += "r"
        if pos.y() - g.top() < m:
            zone += "t"
        elif g.bottom() - pos.y() < m:
            zone += "b"
        return zone or None

    def _set_grip_cursor(self, zone):
        if zone is None:
            if self._grip_cursor:
                QApplication.restoreOverrideCursor()
                self._grip_cursor = False
            return
        if zone in ("l", "r"):
            shape = Qt.CursorShape.SizeHorCursor
        elif zone in ("t", "b"):
            shape = Qt.CursorShape.SizeVerCursor
        elif zone in ("lt", "rb"):
            shape = Qt.CursorShape.SizeFDiagCursor
        else:
            shape = Qt.CursorShape.SizeBDiagCursor
        if self._grip_cursor:
            QApplication.changeOverrideCursor(shape)
        else:
            QApplication.setOverrideCursor(shape)
            self._grip_cursor = True

    def _begin_resize(self, pos: QPoint, zone: str):
        self.sidebar.close_flyout()
        self._snap_timer.stop()
        self._resize = {"start": pos, "geom": QRect(self.geometry()), "scale": self._sidebar_scale(), "zone": zone}
        self.grabMouse()

    def _resize_to(self, pos: QPoint):
        r = self._resize
        g, zone = r["geom"], r["zone"]
        dx, dy = pos.x() - r["start"].x(), pos.y() - r["start"].y()
        ratios = []
        if "r" in zone:
            ratios.append((g.width() + dx) / g.width())
        if "l" in zone:
            ratios.append((g.width() - dx) / g.width())
        if "b" in zone:
            ratios.append((g.height() + dy) / g.height())
        if "t" in zone:
            ratios.append((g.height() - dy) / g.height())
        ratio = max(ratios, key=lambda v: abs(v - 1.0))
        scale = max(0.6, min(2.0, round(r["scale"] * ratio, 2)))
        if scale == self.sidebar.scale():
            return
        self.sidebar.set_scale(scale)
        size = self.sidebar.preferred_size()
        w, h = size.width(), size.height()
        x = g.x() if "r" in zone else (g.right() + 1 - w if "l" in zone else g.center().x() - w // 2)
        y = g.y() if "b" in zone else (g.bottom() + 1 - h if "t" in zone else g.center().y() - h // 2)
        self._docking = True
        try:
            self.setFixedSize(w, h)
            self.setGeometry(x, y, w, h)
        finally:
            self._docking = False

    def _end_resize(self):
        self.releaseMouse()
        self._resize = None
        self._set_grip_cursor(None)
        self.config.set("sidebar_scale", round(self.sidebar.scale(), 2))
        self._finish_sidebar_move()

    def eventFilter(self, obj, event):
        if self.config.get("ui_mode") != "sidebar":
            return False
        kind = event.type()
        if self._resize is not None:
            if kind == QEvent.Type.MouseMove:
                self._resize_to(event.globalPosition().toPoint())
                return True
            if kind == QEvent.Type.MouseButtonRelease:
                self._end_resize()
                return True
            return False
        if self.config.get("locked", False):
            return False
        if kind == QEvent.Type.Leave and obj is self:
            self._set_grip_cursor(None)
        elif kind in (QEvent.Type.MouseMove, QEvent.Type.MouseButtonPress) and isinstance(obj, QWidget) \
                and (obj is self or self.isAncestorOf(obj)):
            pos = event.globalPosition().toPoint()
            zone = self._resize_zone(pos)
            if kind == QEvent.Type.MouseMove and event.buttons() == Qt.MouseButton.NoButton:
                self._set_grip_cursor(zone)
            elif kind == QEvent.Type.MouseButtonPress and event.button() == Qt.MouseButton.LeftButton and zone:
                self._begin_resize(pos, zone)
                return True
        return False

    def set_sidebar_scale(self, scale: float):
        try:
            scale = max(0.6, min(2.0, round(float(scale), 2)))
        except (TypeError, ValueError):
            scale = 1.0
        self.config.set("sidebar_scale", scale)
        if self.config.get("ui_mode") == "sidebar":
            self.sidebar.set_scale(self._sidebar_scale())
            self._dock_sidebar()

    def _on_sidebar_scale_step(self, steps: int):
        self.set_sidebar_scale(step_scale(self._sidebar_scale(), steps))

    def set_sidebar_state(self, state: str):
        if state not in ("strip", "panel") or self.config.get("ui_mode") != "sidebar":
            return
        self.config.set("sidebar_state", state)
        self.sidebar.set_state(state)
        self._dock_sidebar()

    def _fit_sidebar(self):
        """Keep the docked window exactly as big as the panel needs (agy shows five bars once its data arrives).

        New rows are measured before their style is applied (about 2 px taller each), so the window is docked
        again once the event loop has polished them.
        """
        if self.config.get("ui_mode", "sidebar") != "sidebar" or self._resize is not None:
            return
        if self.sidebar.state() == "panel":
            self._dock_sidebar()
            QTimer.singleShot(0, self._settle_sidebar)

    def _settle_sidebar(self):
        if self.config.get("ui_mode", "sidebar") == "sidebar" and self._resize is None \
                and self.sidebar.state() == "panel" and self.size() != self.sidebar.preferred_size():
            self._dock_sidebar()

    def _show_sidebar_menu(self, global_pos):
        build_hud_menu(self, self).exec(global_pos)

    def toggle_ui_mode(self):
        cur = self.config.get("ui_mode", "sidebar")
        self._apply_ui_mode("sidebar" if cur == "table" else "table")

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

    def provider_order(self) -> list:
        order = self.config.get("provider_order", list(DEFAULT_ORDER))
        if isinstance(order, list) and sorted(order) == sorted(DEFAULT_ORDER):
            return list(order)
        return list(DEFAULT_ORDER)

    def set_provider_order(self, order):
        order = list(order)
        if sorted(order) != sorted(DEFAULT_ORDER) or order == self.provider_order():
            return
        self.config.set("provider_order", order)
        self.sidebar.set_order(order)
        if self.config.get("ui_mode", "sidebar") == "table":
            self._rebuild_table()

    def agy_order(self) -> tuple:
        return normalize_order(self.config.get("agy_order", self.config.get("agy_primary")))

    def set_agy_order(self, titles):
        order = normalize_order(titles)
        if order == self.agy_order():
            return
        self.config.set("agy_order", list(order))
        raw = self._raw_metrics.get("agy")
        if raw is not None:
            self._publish(self._arranged(raw))

    def set_row_order(self, provider_id: str, titles):
        if provider_id == "agy":
            self.set_agy_order(titles)
            return
        titles = [str(t) for t in titles]
        orders = dict(self.config.get("row_order", {}) or {})
        if orders.get(provider_id) == titles:
            return
        orders[provider_id] = titles
        self.config.set("row_order", orders)
        raw = self._raw_metrics.get(provider_id)
        if raw is not None:
            self._publish(self._arranged(raw))

    def _republish_all(self):
        for raw in list(self._raw_metrics.values()):
            self._publish(self._arranged(raw))

    def reset_agy_order(self):
        self.set_agy_order(AGY_DEFAULT_ORDER)

    def _on_data_fetched(self, metrics: UsageMetrics):
        self._raw_metrics[metrics.provider_id] = metrics
        self._publish(self._arranged(metrics))

    def _arranged(self, metrics: UsageMetrics) -> UsageMetrics:
        if metrics.provider_id == "agy":
            return arrange_agy(metrics, self.agy_order())
        order = (self.config.get("row_order", {}) or {}).get(metrics.provider_id)
        # the table's dial is fixed (inner = 5 h, outer = week), so custom row order only applies elsewhere
        if order and self.config.get("ui_mode", "sidebar") != "table":
            return arrange_by_order(metrics, order)
        return metrics

    def _publish(self, metrics: UsageMetrics):
        self._latest_metrics[metrics.provider_id] = metrics
        if metrics.provider_id in self.sidebar_cards:
            self.sidebar_cards[metrics.provider_id].update_metrics(metrics)
        self.sidebar.update_metrics(metrics)
        self._fit_sidebar()
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

        for card in self.sidebar_cards.values():
            card.update_countdown()
        self.sidebar.update_countdown()
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
        self._sync_header()
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
        if self.config.get("locked", False) or self.config.get("ui_mode") == "sidebar":
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
                self._suspend_blur()
                if wh:
                    wh.startSystemResize(self.current_edge)
                event.accept()
                return
            elif not self.config.get("locked", False):
                wh = self.windowHandle()
                self._suspend_blur()
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
        self._sync_table_state()
        self._refresh_mask()
        if self._blur_suspended:
            self._blur_resume_timer.start()
        self._save_geometry()

    def moveEvent(self, event):
        super().moveEvent(event)
        if self._blur_suspended:
            self._blur_resume_timer.start()
        if self.config.get("ui_mode") == "sidebar" and not self._docking and not self._restoring_geometry:
            self._snap_timer.start()
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
        if self.config.get("ui_mode", "sidebar") != "table":
            return  # sidebar position is saved when a drag settles (_finish_sidebar_move)
        updates = {"window_x": pos.x(), "window_y": pos.y(), "table_compact": self.table.is_compact()}
        if self.table.is_compact():
            min_w, min_h = compact_min_size(len(self.table.columns))
            updates["table_compact_width"] = max(min_w, size.width())
            updates["table_compact_height"] = max(min_h, size.height())
        else:
            updates["table_width"] = max(MIN_TABLE_W, size.width())
            updates["table_height"] = max(MIN_TABLE_H, size.height())
        self.config.set_many(updates)

    # ================= Context Menu =================
    def contextMenuEvent(self, event):
        build_hud_menu(self, self).exec(event.globalPos())

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
        self._apply_opacity()

    def _idle_fade_active(self) -> bool:
        return (self.config.get("ui_mode") == "sidebar" and self.config.get("sidebar_idle_fade", True)
                and not self.config.get("click_through", False) and not self.underMouse()
                and self._resize is None and self.sidebar._active is None)

    def _apply_opacity(self):
        """Window opacity = the user's setting, dimmed while the sidebar is idle (no hover, no open detail page)."""
        base = float(self.config.get("opacity", 0.88))
        self.setWindowOpacity(base * self.idle_fade_level() if self._idle_fade_active() else base)

    def idle_fade_level(self) -> float:
        """Fraction of the chosen opacity the idle sidebar dims to (0.2 - 0.95)."""
        try:
            return max(0.2, min(0.95, float(self.config.get("sidebar_idle_level", IDLE_FADE_FACTOR))))
        except (TypeError, ValueError):
            return IDLE_FADE_FACTOR

    def idle_fade_delay(self) -> int:
        try:
            return max(200, min(30000, int(self.config.get("sidebar_idle_delay_ms", IDLE_FADE_DELAY_MS))))
        except (TypeError, ValueError):
            return IDLE_FADE_DELAY_MS

    def toggle_idle_fade(self):
        self.config.set("sidebar_idle_fade", not self.config.get("sidebar_idle_fade", True))
        self._apply_opacity()

    def set_idle_fade_level(self, percent: int):
        self.config.set("sidebar_idle_level", max(20, min(95, int(percent))) / 100)
        self._apply_opacity()

    def set_idle_fade_delay(self, milliseconds: int):
        self.config.set("sidebar_idle_delay_ms", max(200, min(30000, int(milliseconds))))

    def enterEvent(self, event):
        super().enterEvent(event)
        self._fade_timer.stop()
        self._apply_opacity()

    def leaveEvent(self, event):
        super().leaveEvent(event)
        self._fade_timer.start(self.idle_fade_delay())

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
        if self.config.get("ui_mode", "sidebar") == "sidebar":
            self.config.set_many({"sidebar_edge": "right", "sidebar_orientation": "vertical", "sidebar_offset": None,
                                  "sidebar_x": None, "sidebar_y": None, "sidebar_scale": 1.0})
            self.sidebar.set_scale(1.0)
            self.sidebar.set_orientation("vertical")
            self.sidebar.set_dock_edge("right")
            self._dock_sidebar()
            return
        w, h = DEF_TABLE_W, DEF_TABLE_H
        if self.table.is_compact():
            self._set_table_compact(False)
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
