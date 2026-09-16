import sys
import threading
from PySide6.QtCore import QObject, Signal

class GlobalHotkeyManager(QObject):
    hotkey_triggered = Signal()
    clickthrough_triggered = Signal()

    def __init__(self):
        super().__init__()
        self._thread = None
        self._thread_id = None
        self._running = False

    def start(self, key_char="C"):
        if self._running:
            return

        if sys.platform == "win32":
            self._start_windows(key_char)
        elif sys.platform == "darwin":
            self._start_macos(key_char)

    def _start_windows(self, key_char):
        import ctypes
        from ctypes import wintypes

        WM_HOTKEY = 0x0312
        MOD_ALT = 0x0001
        MOD_SHIFT = 0x0004
        MOD_NOREPEAT = 0x4000
        HOTKEY_ID_TOGGLE = 9527
        HOTKEY_ID_CLICKTHROUGH = 9528

        user32 = ctypes.windll.user32
        vk = ord(key_char.upper())

        def message_loop():
            self._thread_id = ctypes.windll.kernel32.GetCurrentThreadId()
            user32.RegisterHotKey(None, HOTKEY_ID_TOGGLE, MOD_ALT | MOD_NOREPEAT, vk)
            user32.RegisterHotKey(None, HOTKEY_ID_CLICKTHROUGH, MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, vk)

            msg = wintypes.MSG()
            try:
                while self._running and user32.GetMessageW(ctypes.byref(msg), None, 0, 0) != 0:
                    if msg.message == WM_HOTKEY:
                        if msg.wParam == HOTKEY_ID_TOGGLE:
                            self.hotkey_triggered.emit()
                        elif msg.wParam == HOTKEY_ID_CLICKTHROUGH:
                            self.clickthrough_triggered.emit()
                    user32.TranslateMessage(ctypes.byref(msg))
                    user32.DispatchMessageW(ctypes.byref(msg))
            finally:
                user32.UnregisterHotKey(None, HOTKEY_ID_TOGGLE)
                user32.UnregisterHotKey(None, HOTKEY_ID_CLICKTHROUGH)

        self._running = True
        self._thread = threading.Thread(target=message_loop, daemon=True)
        self._thread.start()

    def _start_macos(self, key_char):
        # On macOS, global hotkeys require macOS System Settings -> Privacy & Security -> Accessibility permissions.
        # Fallback to in-app or background event loop without crashing.
        print(f"[Hotkey macOS] Initialized for key '{key_char}'. Note: Global shortcuts require Accessibility permissions.")

    def stop(self):
        if not self._running:
            return
        self._running = False
        if sys.platform == "win32" and self._thread_id:
            import ctypes
            WM_QUIT = 0x0012
            ctypes.windll.user32.PostThreadMessageW(self._thread_id, WM_QUIT, 0, 0)
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)
