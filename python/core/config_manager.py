import json
import os
import sys
import tempfile
import threading

from core.logger import logger

DEFAULT_CONFIG = {
    "window_x": None,
    "window_y": None,
    "ui_mode": "cards",  # "cards" (Classic Cards) or "table" (Modern Table)
    "layout_mode": "vertical",  # "vertical" or "horizontal" (for cards mode)
    "vertical_width": 280,
    "vertical_height": 410,
    "horizontal_width": 690,
    "horizontal_height": 145,
    "table_width": 450,
    "table_height": 350,
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
    "claude_profile": "auto",
    "agy_profile": "auto"
}

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
                        self.data.update(saved)
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
