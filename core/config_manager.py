import json
import os
import sys
import tempfile

DEFAULT_CONFIG = {
    "active_provider": "claude",  # "claude", "agy", "codex"
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

def get_config_path() -> str:
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    local_cfg = os.path.join(base_dir, "config.json")
    if not getattr(sys, "frozen", False) and (os.path.exists(local_cfg) or os.access(base_dir, os.W_OK)):
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
    def __init__(self, path=None):
        self.path = os.fspath(path) if path is not None else get_config_path()
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
        except OSError as e:
            print(f"[Config] Error saving config: {type(e).__name__}")
        finally:
            if temporary and os.path.exists(temporary):
                os.unlink(temporary)

    def get(self, key, default=None):
        return self.data.get(key, default)

    def update(self, values):
        if any(self.data.get(key) != value for key, value in values.items()):
            self.data.update(values)
            self.save()

    def set(self, key, value):
        self.update({key: value})
