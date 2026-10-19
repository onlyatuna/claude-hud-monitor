import json
import os
import sys
import tempfile
import threading

from core.logger import logger

DEFAULT_CONFIG = {
    "window_x": None,
    "window_y": None,
    "ui_mode": "sidebar",  # "table" (Modern Table) or "sidebar" (docked strip / panel)
    "sidebar_state": "panel",  # "strip" (icon tiles) or "panel" (full cards, 220px column)
    "sidebar_edge": "right",  # "left" | "right" | "top" | "bottom" | "none" (free position)
    "sidebar_orientation": "vertical",  # follows the edge; remembered while floating
    "sidebar_offset": None,  # distance along the edge from its start; None = bottom/right default
    "sidebar_x": None,  # free position, used when sidebar_edge is "none"
    "sidebar_y": None,
    "sidebar_idle_fade": True,  # dim the bar while idle (no hover, no open detail page)
    "sidebar_idle_level": 0.6,  # fraction of the chosen opacity it dims to (0.2 - 0.95)
    "sidebar_idle_delay_ms": 900,  # how long after the mouse leaves before dimming
    "sidebar_scale": 1.0,  # 0.6 - 2.0, Ctrl+wheel or drag any edge/corner of the bar
    "row_order": {},  # per provider: row titles in the order dragged in the detail page (agy uses agy_order)
    "agy_order": ["C/G 5H", "ARGON 7D", "GEMINI 5H", "GEMINI 7D", "C/G 7D"],  # agy row order; the first two are the main pair
    "provider_order": ["claude", "agy", "codex"],  # tile/card/column order; press-hold and drag a tile to change
    "table_width": 450,
    "table_height": 350,
    "table_blur": True,  # Acrylic / frosted-glass behind the table; off = flat rounded panel
    "table_compact": False,  # collapsed to one dial per service (window dragged below 380x280)
    "table_compact_width": 240,
    "table_compact_height": 96,
    "color_scheme": "scale",  # "scale" or "duo" (for table mode)
    "appearance": "auto",     # "auto", "light" or "dark" (both UI modes)
    "always_on_top": True,
    "opacity": 0.88,
    "click_through": False,
    "refresh_interval_sec": 60,
    "hotkey_enabled": True,
    "hotkey": "Alt+C",
    "locked": False,
    "autostart": False,
    "claude_profile": "auto"
}

UI_MODES = ("table", "sidebar")


def migrate_style(data: dict, saved: dict) -> bool:
    """Retire the Classic Cards style: it becomes the sidebar's fixed panel.

    `saved` is what the file held, so legacy keys (layout_mode, window_x/y) that
    are no longer defaults can still be read. Returns True when `data` changed.
    """
    mode = saved.get("ui_mode")
    if mode is None and ("layout_mode" in saved or saved.get("window_x") is not None):
        mode = "cards"   # files from before the sidebar existed never stored a style
    if mode in UI_MODES:
        return False
    data["ui_mode"] = "sidebar"
    data["sidebar_state"] = "panel"
    if mode == "cards":
        layout = saved.get("layout_mode")
        data["sidebar_orientation"] = layout if layout in ("horizontal", "vertical") else "vertical"
        x, y = saved.get("window_x"), saved.get("window_y")
        if isinstance(x, int) and isinstance(y, int) and not isinstance(x, bool) and not isinstance(y, bool):
            data["sidebar_edge"] = "none"
            data["sidebar_x"], data["sidebar_y"] = x, y
    return True


def get_user_config_dir() -> str:
    if sys.platform == "win32":
        app_data = os.environ.get("APPDATA", os.path.expanduser("~"))
        cfg_dir = os.path.join(app_data, "ClaudeHUDMonitor")
    elif sys.platform == "darwin":
        cfg_dir = os.path.expanduser("~/Library/Application Support/ClaudeHUDMonitor")
    else:
        cfg_dir = os.path.expanduser("~/.config/ClaudeHUDMonitor")

    os.makedirs(cfg_dir, exist_ok=True)
    return cfg_dir

def get_config_path() -> str:
    # 1. When frozen via PyInstaller (_onefile / _onedir)
    # sys._MEIPASS is in %TEMP% and wiped on exit! Never write config to _MEIPASS.
    if getattr(sys, 'frozen', False):
        exe_dir = os.path.dirname(sys.executable)
        portable_cfg = os.path.join(exe_dir, "config.json")
        # If user explicitly placed a portable config.json next to the executable and it is writable
        if os.path.exists(portable_cfg) and os.access(portable_cfg, os.W_OK):
            return portable_cfg
        return os.path.join(get_user_config_dir(), "config.json")

    # 2. When running from Python source (dev mode)
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    local_cfg = os.path.join(base_dir, "config.json")
    if os.path.exists(local_cfg) or os.access(base_dir, os.W_OK):
        return local_cfg

    return os.path.join(get_user_config_dir(), "config.json")

class ConfigManager:
    def __init__(self, path=None):
        self._lock = threading.RLock()
        self.path = os.fspath(path) if path is not None else get_config_path()
        self.data = dict(DEFAULT_CONFIG)
        self.load()

    def load(self):
        with self._lock:
            if os.path.exists(self.path):
                try:
                    with open(self.path, "r", encoding="utf-8") as f:
                        saved = json.load(f)
                    if isinstance(saved, dict):
                        self.data.update(saved)
                        if migrate_style(self.data, saved):
                            self.save()
                except Exception as e:
                    logger.error(f"[Config] Error loading config from {self.path}: {e}", exc_info=True)

    def save(self):
        with self._lock:
            # Atomic replacement keeps the previous settings intact on write failure.
            temporary = None
            try:
                directory = os.path.dirname(os.path.abspath(self.path))
                os.makedirs(directory, exist_ok=True)
                with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=directory,
                                                 prefix=".config-", suffix=".tmp", delete=False) as f:
                    temporary = f.name
                    json.dump(self.data, f, indent=2, ensure_ascii=False)
                    f.flush()
                    os.fsync(f.fileno())
                os.replace(temporary, self.path)
            except Exception as e:
                logger.error(f"[Config] Error saving config to {self.path}: {e}", exc_info=True)
            finally:
                if temporary and os.path.exists(temporary):
                    try:
                        os.unlink(temporary)
                    except OSError:
                        pass

    def get(self, key, default=None):
        with self._lock:
            return self.data.get(key, default)

    def set(self, key, value, auto_save: bool = True):
        with self._lock:
            if self.data.get(key) == value:
                return
            self.data[key] = value
            if auto_save:
                self.save()

    def set_many(self, kv_pairs: dict, auto_save: bool = True):
        with self._lock:
            changed = False
            for k, v in kv_pairs.items():
                if self.data.get(k) != v:
                    self.data[k] = v
                    changed = True
            if changed and auto_save:
                self.save()

    def update(self, values: dict, auto_save: bool = True):
        self.set_many(values, auto_save=auto_save)
