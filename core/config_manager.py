import json
import os
import sys

from core.logger import logger

DEFAULT_CONFIG = {
    "window_x": None,
    "window_y": None,
    "layout_mode": "vertical",  # "vertical" or "horizontal"
    "vertical_width": 280,
    "vertical_height": 410,
    "horizontal_width": 690,
    "horizontal_height": 145,
    "always_on_top": True,
    "opacity": 0.88,
    "click_through": False,
    "refresh_interval_sec": 60,
    "hotkey_enabled": True,
    "hotkey": "Alt+C",
    "locked": False,
    "autostart": False
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
    def __init__(self):
        self.path = get_config_path()
        self.data = dict(DEFAULT_CONFIG)
        self.load()

    def load(self):
        if os.path.exists(self.path):
            try:
                with open(self.path, "r", encoding="utf-8") as f:
                    saved = json.load(f)
                    self.data.update(saved)
            except Exception as e:
                logger.error(f"[Config] Error loading config from {self.path}: {e}", exc_info=True)

    def save(self):
        cfg_dir = os.path.dirname(self.path)
        os.makedirs(cfg_dir, exist_ok=True)
        temp_path = f"{self.path}.{os.getpid()}.tmp"
        try:
            with open(temp_path, "w", encoding="utf-8") as f:
                json.dump(self.data, f, indent=2, ensure_ascii=False)
                f.flush()
                os.fsync(f.fileno())
            os.replace(temp_path, self.path)
        except Exception as e:
            logger.error(f"[Config] Error saving config to {self.path}: {e}", exc_info=True)
            if os.path.exists(temp_path):
                try:
                    os.remove(temp_path)
                except OSError:
                    pass

    def get(self, key, default=None):
        return self.data.get(key, default)

    def set(self, key, value, auto_save: bool = True):
        self.data[key] = value
        if auto_save:
            self.save()

    def set_many(self, kv_pairs: dict, auto_save: bool = True):
        for k, v in kv_pairs.items():
            self.data[k] = v
        if auto_save:
            self.save()

