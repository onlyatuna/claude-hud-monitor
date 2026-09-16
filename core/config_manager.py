import json
import os
import sys

DEFAULT_CONFIG = {
    "active_provider": "claude",  # "claude", "agy", "codex"
    "window_x": None,
    "window_y": None,
    "layout_mode": "vertical",  # "vertical" or "horizontal"
    "vertical_width": 270,
    "vertical_height": 205,
    "horizontal_width": 460,
    "horizontal_height": 110,
    "always_on_top": True,
    "opacity": 0.88,
    "click_through": False,
    "refresh_interval_sec": 60,
    "hotkey_enabled": True,
    "hotkey": "Alt+C",
    "locked": False,
    "autostart": False
}

def get_config_path() -> str:
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    local_cfg = os.path.join(base_dir, "config.json")
    if os.path.exists(local_cfg) or os.access(base_dir, os.W_OK):
        return local_cfg
    
    if sys.platform == "win32":
        app_data = os.environ.get("APPDATA", os.path.expanduser("~"))
        cfg_dir = os.path.join(app_data, "ClaudeHUDMonitor")
    elif sys.platform == "darwin":
        cfg_dir = os.path.expanduser("~/Library/Application Support/ClaudeHUDMonitor")
    else:
        cfg_dir = os.path.expanduser("~/.config/ClaudeHUDMonitor")

    os.makedirs(cfg_dir, exist_ok=True)
    return os.path.join(cfg_dir, "config.json")

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
                print(f"[Config] Error loading config: {e}")

    def save(self):
        try:
            with open(self.path, "w", encoding="utf-8") as f:
                json.dump(self.data, f, indent=2, ensure_ascii=False)
        except Exception as e:
            print(f"[Config] Error saving config: {e}")

    def get(self, key, default=None):
        return self.data.get(key, default)

    def set(self, key, value):
        self.data[key] = value
        self.save()
