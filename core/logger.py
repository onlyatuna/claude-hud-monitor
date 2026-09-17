import os
import sys
import logging
from logging.handlers import RotatingFileHandler
import subprocess

logger = logging.getLogger("ClaudeHUD")

def get_log_dir() -> str:
    # 1. When frozen via PyInstaller (_onefile / _onedir) and portable mode
    if getattr(sys, 'frozen', False):
        exe_dir = os.path.dirname(sys.executable)
        portable_cfg = os.path.join(exe_dir, "config.json")
        if os.path.exists(portable_cfg) and os.access(exe_dir, os.W_OK):
            log_dir = os.path.join(exe_dir, "logs")
            os.makedirs(log_dir, exist_ok=True)
            return log_dir

    # 2. Default app data directory
    if sys.platform == "win32":
        app_data = os.environ.get("APPDATA", os.path.expanduser("~"))
        base_dir = os.path.join(app_data, "ClaudeHUDMonitor")
    elif sys.platform == "darwin":
        base_dir = os.path.expanduser("~/Library/Application Support/ClaudeHUDMonitor")
    else:
        base_dir = os.path.expanduser("~/.config/ClaudeHUDMonitor")

    log_dir = os.path.join(base_dir, "logs")
    os.makedirs(log_dir, exist_ok=True)
    return log_dir

def get_log_file_path() -> str:
    return os.path.join(get_log_dir(), "hud_monitor.log")

def setup_logging(level=logging.INFO) -> logging.Logger:
    log_path = get_log_file_path()
    logger.setLevel(logging.DEBUG)

    if not logger.handlers:
        # Rotating file handler (max 2MB per file, keep 3 backups)
        file_handler = RotatingFileHandler(
            log_path,
            maxBytes=2 * 1024 * 1024,
            backupCount=3,
            encoding="utf-8"
        )
        file_formatter = logging.Formatter(
            "%(asctime)s [%(levelname)s] [%(name)s:%(filename)s:%(lineno)d] %(message)s"
        )
        file_handler.setFormatter(file_formatter)
        file_handler.setLevel(logging.DEBUG)
        logger.addHandler(file_handler)

        # Stream handler for console / dev mode
        if not getattr(sys, 'frozen', False) and sys.stderr is not None:
            console_handler = logging.StreamHandler(sys.stderr)
            console_formatter = logging.Formatter("[%(levelname)s] %(message)s")
            console_handler.setFormatter(console_formatter)
            console_handler.setLevel(level)
            logger.addHandler(console_handler)

    # Intercept uncaught exceptions
    def handle_exception(exc_type, exc_value, exc_traceback):
        if issubclass(exc_type, KeyboardInterrupt):
            sys.__excepthook__(exc_type, exc_value, exc_traceback)
            return
        logger.critical("Unhandled exception:", exc_info=(exc_type, exc_value, exc_traceback))

    sys.excepthook = handle_exception

    return logger

def open_log_dir():
    log_dir = get_log_dir()
    try:
        if sys.platform == "win32":
            os.startfile(log_dir)
        elif sys.platform == "darwin":
            subprocess.Popen(["open", log_dir])
        else:
            subprocess.Popen(["xdg-open", log_dir])
    except Exception as e:
        logger.error(f"Failed to open log directory: {e}")
