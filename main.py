import sys
import os

# Ensure local imports work reliably
current_dir = os.path.dirname(os.path.abspath(__file__))
if current_dir not in sys.path:
    sys.path.insert(0, current_dir)

from PySide6.QtWidgets import QApplication
from PySide6.QtCore import Qt, qInstallMessageHandler, QtMsgType

from core.logger import setup_logging, logger
from core.config_manager import ConfigManager
from core.diagnostics import configure_logging
from ui.hud_window import HUDWindow
from ui.tray_icon import HUDTrayIcon, get_app_icon
from system.hotkey import GlobalHotkeyManager

def qt_message_handler(mode, context, message):
    if mode == QtMsgType.QtDebugMsg:
        logger.debug(f"[Qt] {message}")
    elif mode == QtMsgType.QtInfoMsg:
        logger.info(f"[Qt] {message}")
    elif mode == QtMsgType.QtWarningMsg:
        logger.warning(f"[Qt] {message}")
    elif mode == QtMsgType.QtCriticalMsg:
        logger.error(f"[Qt] {message}")
    elif mode == QtMsgType.QtFatalMsg:
        logger.critical(f"[Qt] {message}")

def main():
    setup_logging()
    logger.info("=== Claude HUD Monitor starting ===")
    logger.info(f"Python version: {sys.version}, Platform: {sys.platform}")

    if sys.platform == "win32":
        try:
            import ctypes
            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID("ClaudeHUD.Monitor.App")
        except Exception:
            pass

    qInstallMessageHandler(qt_message_handler)

    # Enable High DPI scaling
    QApplication.setHighDpiScaleFactorRoundingPolicy(Qt.HighDpiScaleFactorRoundingPolicy.PassThrough)
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)

    app_icon = get_app_icon()
    app.setWindowIcon(app_icon)

    config = ConfigManager()
    logger.info(f"Loaded config from: {config.path}")
    configure_logging(config.path)

    # Create HUD Window
    hud = HUDWindow(config)
    hud.setWindowIcon(app_icon)
    hud.show()
    logger.info("HUD window created and shown")

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
            logger.warning(f"Hotkey registration issue: {msg}")
            tray.showMessage(
                "⚠️ 全域快捷鍵通知",
                f"{msg}\n您仍可透過系統匣圖示完整操作所有功能。",
                tray.icon(),
                5000
            )

        hotkey.hotkey_failed.connect(on_hotkey_failed)
        if hasattr(hotkey, "unavailable"):
            hotkey.unavailable.connect(on_hotkey_failed)
        hotkey.start(key_char="C")
        logger.info("Hotkey manager started")

    def on_exit():
        logger.info("Application shutting down...")
        hotkey.stop()
        if hasattr(hud, "refresh_controller"):
            hud.refresh_controller.stop()

    app.aboutToQuit.connect(on_exit)

    exit_code = app.exec()
    logger.info(f"=== Claude HUD Monitor exited with code {exit_code} ===")
    sys.exit(exit_code)

if __name__ == "__main__":
    if "--smoke-test" in sys.argv:
        from core.smoke_check import run
        sys.exit(run())
    main()
