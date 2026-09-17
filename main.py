import sys
import os

# Ensure local imports work reliably
current_dir = os.path.dirname(os.path.abspath(__file__))
if current_dir not in sys.path:
    sys.path.insert(0, current_dir)

from PySide6.QtWidgets import QApplication
from PySide6.QtCore import Qt

from core.config_manager import ConfigManager
from core.diagnostics import configure_logging
from ui.hud_window import HUDWindow
from ui.tray_icon import HUDTrayIcon
from system.hotkey import GlobalHotkeyManager

def main():
    # Enable High DPI scaling
    QApplication.setHighDpiScaleFactorRoundingPolicy(Qt.HighDpiScaleFactorRoundingPolicy.PassThrough)
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)

    config = ConfigManager()
    configure_logging(config.path)

    # Create HUD Window
    hud = HUDWindow(config)
    hud.show()

    # Create System Tray Icon
    tray = HUDTrayIcon(hud)
    tray.show()

    # Register Global Hotkeys:
    # Alt + C -> Toggle HUD Show / Hide
    # Alt + Shift + C -> Toggle Click-Through Ghost Mode
    hotkey = GlobalHotkeyManager()
    if config.get("hotkey_enabled", True):
        hotkey.hotkey_triggered.connect(hud.toggle_visibility)
        hotkey.clickthrough_triggered.connect(hud.toggle_click_through)
        hotkey.unavailable.connect(lambda message: tray.showMessage("快捷鍵", message))
        hotkey.start(key_char="C")

    def on_exit():
        hotkey.stop()
        hud.refresh_controller.stop()

    app.aboutToQuit.connect(on_exit)

    sys.exit(app.exec())

if __name__ == "__main__":
    if "--smoke-test" in sys.argv:
        from core.smoke_check import run
        sys.exit(run())
    main()
