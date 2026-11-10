"""One menu structure shared by the HUD right-click menu and the tray icon.

Top level stays short; everything else lives in four groups: 介面 / 外觀 / 行為 / 更多.
A setting appears exactly once. The menu is rebuilt each time it is shown, so checks are always current.
"""

from PySide6.QtWidgets import QMenu

from core import agy_statusline
from core import tray_quota
from core.agy_rows import DEFAULT_ORDER as AGY_DEFAULT_ORDER
from core.autostart import is_autostart_enabled
from core.logger import open_log_dir

SIDEBAR_SCALES = (60, 80, 100, 125, 150, 200)
IDLE_LEVELS = (80, 60, 40, 25)
IDLE_DELAYS_MS = (500, 900, 2000, 5000)
OPACITIES = (100, 90, 80, 70, 50, 30)
INTERVALS = (30, 60, 120, 300)
UI_MODES = (("table", "儀表表格 (Modern Table)"),
            ("sidebar", "側邊欄 (Sidebar)"))
SIDEBAR_FORMS = (("strip", "細條 (Strip)"), ("panel", "固定窄欄 (Panel)"))


def _check(menu, label, checked, callback):
    action = menu.addAction(label)
    action.setCheckable(True)
    action.setChecked(bool(checked))
    action.triggered.connect(lambda _checked=False: callback())
    return action


def _choices(menu, items, current, callback):
    for value, label in items:
        _check(menu, label, value == current, lambda v=value: callback(v))


def _build_interface(hud, menu):
    mode = hud.config.get("ui_mode", "sidebar")
    sub = menu.addMenu("介面 (Interface)")
    _choices(sub, UI_MODES, mode, hud._apply_ui_mode)
    if mode == "sidebar":
        sub.addSeparator()
        _choices(sub, SIDEBAR_FORMS, hud.config.get("sidebar_state", "panel"), hud.set_sidebar_state)
        sub.addSeparator()
        current = round(float(hud.config.get("sidebar_scale", 1.0)) * 100)
        size_menu = sub.addMenu("大小 (Ctrl＋滾輪)")
        for pct in SIDEBAR_SCALES:
            _check(size_menu, f"{pct}%", pct == current, lambda p=pct: hud.set_sidebar_scale(p / 100))
    sub.addSeparator()
    _build_agy_rows(hud, sub)


def _build_agy_rows(hud, menu):
    """Shows the current agy row order; the order itself is changed by dragging rows in the detail page."""
    order = hud.agy_order()
    sub = menu.addMenu("AGY 項目順序 (Row order)")
    hint = sub.addAction("在詳細頁面拖拉標題可調整，前兩條為主要顯示")
    hint.setEnabled(False)
    sub.addSeparator()
    for index, title in enumerate(order, start=1):
        sub.addAction(f"{index}. {title}").setEnabled(False)
    sub.addSeparator()
    reset = sub.addAction("還原預設順序")
    reset.setEnabled(tuple(order) != tuple(AGY_DEFAULT_ORDER))
    reset.triggered.connect(hud.reset_agy_order)


def _build_appearance(hud, menu):
    mode = hud.config.get("ui_mode", "sidebar")
    from ui.hud_window import APPEARANCE_LABELS, SCHEME_LABELS
    from ui.styles import APPEARANCES, SCHEMES
    sub = menu.addMenu("外觀 (Appearance)")
    current = hud.config.get("appearance", "auto")
    for value in APPEARANCES:
        _check(sub, APPEARANCE_LABELS[value], value == current, lambda v=value: hud.set_appearance(v))
    if mode == "table":
        sub.addSeparator()
        for value in SCHEMES:
            _check(sub, f"{SCHEME_LABELS[value]}", value == hud.color_scheme(), lambda v=value: hud.set_color_scheme(v))
        _check(sub, "毛玻璃模糊 (Blur)", hud.config.get("table_blur", True), hud.toggle_table_blur)
    sub.addSeparator()
    opacity = hud.config.get("opacity", 0.88)
    opacity_menu = sub.addMenu("視窗透明度 (Opacity)")
    for pct in OPACITIES:
        _check(opacity_menu, f"{pct}%", abs(opacity - pct / 100.0) < 0.05, lambda p=pct: hud._set_opacity(p / 100.0))


def _build_idle_fade(hud, menu):
    sub = menu.addMenu("閒置時淡化 (Idle fade)")
    _check(sub, "啟用", hud.config.get("sidebar_idle_fade", True), hud.toggle_idle_fade)
    sub.addSeparator()
    level = round(hud.idle_fade_level() * 100)
    level_menu = sub.addMenu("淡化到 (Dim to)")
    levels = sorted(set(IDLE_LEVELS) | {level}, reverse=True)
    for pct in levels:
        _check(level_menu, f"{pct}%", pct == level, lambda p=pct: hud.set_idle_fade_level(p))
    delay = hud.idle_fade_delay()
    delay_menu = sub.addMenu("多久後淡化 (Delay)")
    delays = sorted(set(IDLE_DELAYS_MS) | {delay})
    for ms in delays:
        _check(delay_menu, f"{ms / 1000:g} 秒", ms == delay, lambda m=ms: hud.set_idle_fade_delay(m))


def _build_behaviour(hud, menu):
    sub = menu.addMenu("行為 (Behaviour)")
    _check(sub, "滑鼠點擊穿透 (Alt+Shift+C)", hud.config.get("click_through", False), hud.toggle_click_through)
    _check(sub, "視窗永遠置頂 (Always on Top)", hud.config.get("always_on_top", True), hud._toggle_always_on_top)
    _check(sub, "鎖定視窗位置 (Lock Drag)", hud.config.get("locked", False), hud._toggle_lock)
    _build_idle_fade(hud, sub)
    _check(sub, "開機自動啟動 (Start on Boot)", is_autostart_enabled(), hud._toggle_autostart)
    sub.addSeparator()
    interval_menu = sub.addMenu("⏱️ 更新頻率 (Interval)")
    current = hud.config.get("refresh_interval_sec", 60)
    for sec in INTERVALS:
        _check(interval_menu, f"{sec} 秒", current == sec, lambda s=sec: hud._set_interval(s))


def _build_tray_quota(hud, menu):
    """Which quota's number the tray icon shows; rows are the live ones, so the labels carry current values."""
    sub = menu.addMenu("系統匣數字 (Tray number)")
    setting = hud.config.get("tray_quota", tray_quota.OFF)
    rows = hud.tray_quota_readings()
    _check(sub, "關閉 (Off)", setting == tray_quota.OFF, lambda: hud.set_tray_quota(tray_quota.OFF))
    _check(sub, "自動：最緊的額度 (Auto)", setting == tray_quota.AUTO, lambda: hud.set_tray_quota(tray_quota.AUTO))
    if setting not in (tray_quota.OFF, tray_quota.AUTO) and all(r.key != setting for r in rows):
        _check(sub, f"{setting}  unavailable", True, lambda: None).setEnabled(False)
    header = None
    for row in rows:
        if row.provider_id != header:
            header = row.provider_id
            sub.addSeparator()
            _note(sub, row.provider_name)
        _check(sub, tray_quota.menu_label(row), row.key == setting, lambda k=row.key: hud.set_tray_quota(k))
    sub.addSeparator()
    mode = hud.config.get("tray_quota_mode", tray_quota.USED)
    _check(sub, "顯示已用 (Show used)", mode == tray_quota.USED, lambda: hud.set_tray_quota_mode(tray_quota.USED))
    _check(sub, "顯示剩餘 (Show remaining)", mode == tray_quota.REMAINING,
           lambda: hud.set_tray_quota_mode(tray_quota.REMAINING))


def _build_claude_accounts(hud, menu):
    from core.providers.claude_provider import discover_profiles, resolve_active_profile
    sub = menu.addMenu("Claude 帳號 (Claude Account)")
    current = hud.config.get("claude_profile", "auto")
    is_auto = current == "auto"
    active, _ = resolve_active_profile(current)
    title = (f"智慧自動追蹤 (目前: {active.short_name})" if is_auto and active.id != "default"
             else "智慧自動追蹤 (最近活躍)")
    _check(sub, f"{title}", is_auto, lambda: hud._set_claude_profile("auto"))
    sub.addSeparator()
    for profile in discover_profiles()[:40]:
        selected = not is_auto and (current == profile.id or (current == ".claude" and profile.id == "default"))
        _check(sub, profile.display_name, selected, lambda pid=profile.id: hud._set_claude_profile(pid))


def _note(menu, text):
    action = menu.addAction(text)
    action.setEnabled(False)
    return action


def _build_agy_statusline(hud, menu):
    """One click to enable the agy statusLine hook that feeds Gemini's weekly quota and Argon."""
    status = hud.agy_statusline_status()
    sub = menu.addMenu("AGY 週額度掛鉤 (statusLine)")
    _note(sub, agy_statusline.STATUS_LABELS.get(status, agy_statusline.STATUS_LABELS["error"]))
    if status == "installed":
        written = agy_statusline.cache_written_at()
        _note(sub, f"資料最後更新：{written.astimezone().strftime('%m-%d %H:%M')}" if written
              else "尚未收到資料（需開啟 agy 互動模式）")
    sub.addSeparator()
    if status == "installed":
        sub.addAction("停用掛鉤 (Disable)").triggered.connect(lambda: hud.run_agy_statusline("uninstall"))
    elif status == "needs_update":
        sub.addAction("更新掛鉤 (Update)").triggered.connect(lambda: hud.run_agy_statusline("install"))
    else:
        enable = sub.addAction("啟用 Gemini 週額度與 Argon (Enable)")
        enable.setEnabled(status == "not_installed")
        enable.triggered.connect(lambda: hud.run_agy_statusline("install"))
    sub.addSeparator()
    _note(sub, "需要 PATH 上有 python；啟用後請重開 agy 互動模式")


def _build_more(hud, menu):
    sub = menu.addMenu("更多 (More)")
    _build_claude_accounts(hud, sub)
    _build_agy_statusline(hud, sub)
    sub.addSeparator()
    sub.addAction("開啟記錄檔目錄 (Open Logs)").triggered.connect(open_log_dir)
    sub.addAction("重設預設尺寸與位置").triggered.connect(hud._reset_geometry)
    sub.addAction("隱藏 HUD (Alt+C 重新喚出)").triggered.connect(hud.hide)


def build_hud_menu(hud, parent=None, tray=False, menu=None) -> QMenu:
    """tray=True adds show/hide on top and drops the 更多 group (rarely needed from the tray).
    Pass `menu` to fill an existing QMenu (the tray rebuilds its own menu each time it opens)."""
    if menu is None:
        menu = QMenu(parent if parent is not None else hud)
    if tray:
        menu.addAction("顯示 / 隱藏 HUD (Alt+C)").triggered.connect(hud.toggle_visibility)
    menu.addAction("立即重新整理所有 AI (Refresh All)").triggered.connect(hud.trigger_async_refresh)
    menu.addSeparator()
    _build_interface(hud, menu)
    _build_appearance(hud, menu)
    _build_behaviour(hud, menu)
    if tray:
        _build_tray_quota(hud, menu)
    if not tray:
        _build_more(hud, menu)
    menu.addSeparator()
    menu.addAction("結束程式 (Exit)").triggered.connect(hud.close_application)
    return menu
