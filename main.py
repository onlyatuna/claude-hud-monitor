import sys
import os

# Ensure local imports work reliably
current_dir = os.path.dirname(os.path.abspath(__file__))
if current_dir not in sys.path:
    sys.path.insert(0, current_dir)

from PySide6.QtWidgets import QApplication
from PySide6.QtCore import Qt

from core.config_manager import ConfigManager
from ui.hud_window import HUDWindow
from ui.tray_icon import HUDTrayIcon
from system.hotkey import GlobalHotkeyManager

def main():
    # Enable High DPI scaling
    QApplication.setHighDpiScaleFactorRoundingPolicy(Qt.HighDpiScaleFactorRoundingPolicy.PassThrough)
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)

    config = ConfigManager()

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
    hud.set_hotkey_manager(hotkey)

    if config.get("hotkey_enabled", True):
        hotkey.hotkey_triggered.connect(hud.toggle_visibility)
        hotkey.clickthrough_triggered.connect(hud.toggle_click_through)

        def on_hotkey_failed(msg):
            tray.showMessage(
                "⚠️ 全域快捷鍵通知",
                f"{msg}\n您仍可透過系統匣圖示完整操作所有功能。",
                tray.icon(),
                5000
            )

        hotkey.hotkey_failed.connect(on_hotkey_failed)
        hotkey.start(key_char="C")

    def on_exit():
        hotkey.stop()

    app.aboutToQuit.connect(on_exit)

    sys.exit(app.exec())

if __name__ == "__main__":
    main()
