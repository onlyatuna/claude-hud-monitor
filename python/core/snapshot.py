"""`main.py --snapshot [DIR]`: render every HUD style to PNG files, offscreen, with synthetic data.

For reviewing a UI change without a Windows desktop (or without touching your own HUD): it uses a throw-away
config, no providers and no network, and never creates a tray icon, hotkey or single-instance lock. The files
are the same views the tests assert on, so a before/after pair shows what a change did.
"""
import os
import sys
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path

DEFAULT_DIR = Path("snapshots")


def parse_args(argv) -> Path:
    """The directory after --snapshot, or ./snapshots when none (or when the next word is another flag)."""
    if "--snapshot" in argv:
        index = argv.index("--snapshot") + 1
        if index < len(argv) and not argv[index].startswith("--"):
            return Path(argv[index])
    return DEFAULT_DIR


def _metrics(states: bool):
    from core.providers.base import MetricRow, UsageMetrics
    now = datetime.now(timezone.utc)
    soon, later = now + timedelta(hours=2, minutes=15), now + timedelta(days=3, hours=4)

    def usage(pid, five, week, **extra):
        return UsageMetrics(provider_id=pid, metric1_title="SESSION 5H", metric1_val=five, metric1_text=f"{five}%",
                            metric1_reset=soon, metric2_title="WEEKLY 7D", metric2_val=week, metric2_text=f"{week}%",
                            metric2_reset=later, **extra)

    agy_rows = [MetricRow("C/G 5H", 30, "30%", soon), MetricRow("C/G 7D", 15, "15%", later),
                MetricRow("ARGON 7D", 60, "60%", later)]
    if not states:
        return [usage("claude", 42, 71), usage("codex", 12, 35), usage("agy", 88, 20, extra_metrics=agy_rows)]
    return [usage("claude", 93, 64),
            usage("codex", 25, 40, error="timeout", stale=True, last_success=now - timedelta(minutes=7)),
            UsageMetrics(provider_id="agy", error="agy is not signed in")]


# name -> (settings, data variant, window size or None, grab the context menu instead of the window)
SCENARIOS = {
    "table_full_dark": ({"ui_mode": "table", "appearance": "dark"}, False, None, False),
    "table_full_light": ({"ui_mode": "table", "appearance": "light"}, False, None, False),
    "table_states_dark": ({"ui_mode": "table", "appearance": "dark"}, True, None, False),
    "table_compact_dark": ({"ui_mode": "table", "appearance": "dark", "table_compact": True}, False, None, False),
    "table_compact_min_light": ({"ui_mode": "table", "appearance": "light", "table_compact": True}, True, "min", False),
    "sidebar_panel_vertical_dark": ({"ui_mode": "sidebar", "sidebar_state": "panel", "appearance": "dark"},
                                    False, None, False),
    "sidebar_panel_vertical_light": ({"ui_mode": "sidebar", "sidebar_state": "panel", "appearance": "light"},
                                     False, None, False),
    "sidebar_panel_horizontal_dark": ({"ui_mode": "sidebar", "sidebar_state": "panel", "appearance": "dark",
                                       "sidebar_edge": "bottom", "sidebar_orientation": "horizontal"}, False, None, False),
    "sidebar_strip_vertical_dark": ({"ui_mode": "sidebar", "sidebar_state": "strip", "appearance": "dark"},
                                    True, None, False),
    "sidebar_strip_horizontal_dark": ({"ui_mode": "sidebar", "sidebar_state": "strip", "appearance": "dark",
                                       "sidebar_edge": "bottom", "sidebar_orientation": "horizontal"}, False, None, False),
    "menu_dark": ({"ui_mode": "table", "appearance": "dark"}, False, None, True),
}


def _render(name, settings, states, size, menu, out_dir: Path, app) -> Path:
    from core.config_manager import ConfigManager
    from ui.hud_menu import build_hud_menu
    from ui.hud_window import HUDWindow
    from ui.table_layout import compact_min_size
    with tempfile.TemporaryDirectory(prefix="hud-snapshot-") as directory:
        config = ConfigManager(Path(directory) / "config.json")
        config.update({"opacity": 1.0, "sidebar_idle_fade": False, **settings})
        hud = HUDWindow(config, providers={})
        try:
            hud.show()
            app.processEvents()
            for metrics in _metrics(states):
                hud._on_data_fetched(metrics)
            if size == "min":
                hud.resize(*compact_min_size(len(hud.table.columns)))
            for _ in range(3):
                app.processEvents()
            if menu:
                popup = build_hud_menu(hud)
                popup.ensurePolished()
                popup.adjustSize()
                image = popup.grab()
            else:
                image = hud.grab()
            path = out_dir / f"{name}.png"
            if not image.save(str(path), "PNG"):
                raise RuntimeError(f"could not write {path}")
            return path
        finally:
            hud.refresh_controller.stop()
            hud.countdown_timer.stop()
            hud.geometry_timer.stop()
            hud.close()
            hud.deleteLater()
            app.processEvents()


def run(out_dir=DEFAULT_DIR) -> list:
    """Write one PNG per scenario into `out_dir` (created if needed); returns the paths."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication(sys.argv[:1])
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    return [_render(name, *spec, out_dir, app) for name, spec in SCENARIOS.items()]
