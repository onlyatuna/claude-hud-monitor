"""One menu structure shared by the HUD right-click menu and the tray icon.

Top level stays short; everything else lives in four groups: 介面 / 外觀 / 行為 / 更多.
A setting appears exactly once. The menu is rebuilt each time it is shown, so checks are always current.
"""

from PySide6.QtWidgets import QMenu

from core.agy_rows import AGY_ROW_TITLES, normalize_primary
from core.autostart import is_autostart_enabled
from core.logger import open_log_dir

SIDEBAR_SCALES = (60, 80, 100, 125, 150, 200)
OPACITIES = (100, 90, 80, 70, 50, 30)
INTERVALS = (30, 60, 120, 300)
UI_MODES = (("cards", "🗂️ 傳統卡片 (Classic Cards)"),
            ("table", "📊 儀表表格 (Modern Table)"),
            ("sidebar", "📌 側邊欄 (Sidebar)"))
SIDEBAR_FORMS = (("strip", "細條 (Strip)"), ("panel", "固定窄欄 (Panel)"))
LAYOUTS = (("horizontal", "💻 橫向三欄並排 (Horizontal Triple)"),
           ("vertical", "📱 直立三層堆疊 (Vertical Stack)"))


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
    mode = hud.config.get("ui_mode", "cards")
    sub = menu.addMenu("🎭 介面 (Interface)")
    _choices(sub, UI_MODES, mode, hud._apply_ui_mode)
    if mode == "cards":
        sub.addSeparator()
        _choices(sub, LAYOUTS, hud.config.get("layout_mode", "horizontal"), hud._apply_cards_layout_mode)
    elif mode == "sidebar":
        sub.addSeparator()
        _choices(sub, SIDEBAR_FORMS, hud.config.get("sidebar_state", "strip"), hud.set_sidebar_state)
        sub.addSeparator()
        current = round(float(hud.config.get("sidebar_scale", 1.0)) * 100)
        size_menu = sub.addMenu("🔎 大小 (Ctrl＋滾輪)")
        for pct in SIDEBAR_SCALES:
            _check(size_menu, f"{pct}%", pct == current, lambda p=pct: hud.set_sidebar_scale(p / 100))
    sub.addSeparator()
    _build_agy_rows(hud, sub)


def _build_agy_rows(hud, menu):
    first, second = normalize_primary(hud.config.get("agy_primary"))
    sub = menu.addMenu("🧭 AGY 主要兩條 (Main rows)")
    for title in AGY_ROW_TITLES:
        mark = " ①" if title == first else (" ②" if title == second else "")
        _check(sub, f"{title}{mark}", title in (first, second), lambda t=title: hud.toggle_agy_row(t))


def _build_appearance(hud, menu):
    mode = hud.config.get("ui_mode", "cards")
    from ui.hud_window import APPEARANCE_LABELS, SCHEME_LABELS
    from ui.styles import APPEARANCES, SCHEMES
    sub = menu.addMenu("🌓 外觀 (Appearance)")
    current = hud.config.get("appearance", "auto")
    for value in APPEARANCES:
        _check(sub, APPEARANCE_LABELS[value], value == current, lambda v=value: hud.set_appearance(v))
    if mode in ("table", "sidebar"):
        sub.addSeparator()
        for value in SCHEMES:
            _check(sub, f"🎨 {SCHEME_LABELS[value]}", value == hud.color_scheme(), lambda v=value: hud.set_color_scheme(v))
    sub.addSeparator()
    opacity = hud.config.get("opacity", 0.88)
    opacity_menu = sub.addMenu("🪟 視窗透明度 (Opacity)")
    for pct in OPACITIES:
        _check(opacity_menu, f"{pct}%", abs(opacity - pct / 100.0) < 0.05, lambda p=pct: hud._set_opacity(p / 100.0))


def _build_behaviour(hud, menu):
    sub = menu.addMenu("⚙️ 行為 (Behaviour)")
    _check(sub, "👻 滑鼠點擊穿透 (Alt+Shift+C)", hud.config.get("click_through", False), hud.toggle_click_through)
    _check(sub, "📌 視窗永遠置頂 (Always on Top)", hud.config.get("always_on_top", True), hud._toggle_always_on_top)
    _check(sub, "🔒 鎖定視窗位置 (Lock Drag)", hud.config.get("locked", False), hud._toggle_lock)
    _check(sub, "🚀 開機自動啟動 (Start on Boot)", is_autostart_enabled(), hud._toggle_autostart)
    sub.addSeparator()
    interval_menu = sub.addMenu("⏱️ 更新頻率 (Interval)")
    current = hud.config.get("refresh_interval_sec", 60)
    for sec in INTERVALS:
        _check(interval_menu, f"{sec} 秒", current == sec, lambda s=sec: hud._set_interval(s))


def _build_claude_accounts(hud, menu):
    from core.providers.claude_provider import discover_profiles, resolve_active_profile
    sub = menu.addMenu("✳️ Claude 帳號 (Claude Account)")
    current = hud.config.get("claude_profile", "auto")
    is_auto = current == "auto"
    active, _ = resolve_active_profile(current)
    title = (f"智慧自動追蹤 (目前: {active.short_name})" if is_auto and active.id != "default"
             else "智慧自動追蹤 (最近活躍)")
    _check(sub, f"🎯 {title}", is_auto, lambda: hud._set_claude_profile("auto"))
    sub.addSeparator()
    for profile in discover_profiles()[:40]:
        selected = not is_auto and (current == profile.id or (current == ".claude" and profile.id == "default"))
        _check(sub, profile.display_name, selected, lambda pid=profile.id: hud._set_claude_profile(pid))


def _build_more(hud, menu):
    sub = menu.addMenu("⋯ 更多 (More)")
    _build_claude_accounts(hud, sub)
    sub.addSeparator()
    sub.addAction("📂 開啟記錄檔目錄 (Open Logs)").triggered.connect(open_log_dir)
    sub.addAction("📐 重設預設尺寸與位置").triggered.connect(hud._reset_geometry)
    sub.addAction("👁️ 隱藏 HUD (Alt+C 重新喚出)").triggered.connect(hud.hide)


def build_hud_menu(hud, parent=None, tray=False, menu=None) -> QMenu:
    """tray=True adds show/hide on top and drops the 更多 group (rarely needed from the tray).
    Pass `menu` to fill an existing QMenu (the tray rebuilds its own menu each time it opens)."""
    if menu is None:
        menu = QMenu(parent if parent is not None else hud)
    if tray:
        menu.addAction("👁️ 顯示 / 隱藏 HUD (Alt+C)").triggered.connect(hud.toggle_visibility)
    menu.addAction("🔄 立即重新整理所有 AI (Refresh All)").triggered.connect(hud.trigger_async_refresh)
    menu.addSeparator()
    _build_interface(hud, menu)
    _build_appearance(hud, menu)
    _build_behaviour(hud, menu)
    if not tray:
        _build_more(hud, menu)
    menu.addSeparator()
    menu.addAction("❌ 結束程式 (Exit)").triggered.connect(hud.close_application)
    return menu
