import sys
import threading
from PySide6.QtCore import QObject, Signal

class GlobalHotkeyManager(QObject):
    hotkey_triggered = Signal()
    clickthrough_triggered = Signal()
    unavailable = Signal(str)

    def __init__(self):
        super().__init__()
        self._thread = None
        self._thread_id = None
        self._running = False
        self._listener = None

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
            if not user32.RegisterHotKey(None, HOTKEY_ID_TOGGLE, MOD_ALT | MOD_NOREPEAT, vk):
                self.unavailable.emit("Alt+C 已被占用，請使用系統匣操作")
            if not user32.RegisterHotKey(None, HOTKEY_ID_CLICKTHROUGH, MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, vk):
                self.unavailable.emit("Alt+Shift+C 已被占用，請使用系統匣操作")

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
        try:
            from pynput import keyboard
            if not keyboard.Listener.IS_TRUSTED:
                self.unavailable.emit("macOS 快捷鍵需要輔助使用／輸入監控權限；可使用系統匣操作")
                return
            self._mac_keys = set()
            def on_press(key):
                # Physical C avoids Option+C being translated into another character.
                token = getattr(key, "vk", None)
                if token is None:
                    token = key
                repeated = token in self._mac_keys
                self._mac_keys.add(token)
                alt = any(k in self._mac_keys for k in (keyboard.Key.alt, keyboard.Key.alt_l, keyboard.Key.alt_r))
                shift = any(k in self._mac_keys for k in (keyboard.Key.shift, keyboard.Key.shift_l, keyboard.Key.shift_r))
                other = any(k in self._mac_keys for k in (keyboard.Key.ctrl, keyboard.Key.ctrl_l, keyboard.Key.ctrl_r, keyboard.Key.cmd, keyboard.Key.cmd_l, keyboard.Key.cmd_r))
                if token == 8 and alt and not other and not repeated:
                    (self.clickthrough_triggered if shift else self.hotkey_triggered).emit()
            def on_release(key):
                token = getattr(key, "vk", None)
                self._mac_keys.discard(key if token is None else token)
            self._listener = keyboard.Listener(on_press=on_press, on_release=on_release)
            self._listener.start()
            self._running = True
        except (ImportError, OSError, RuntimeError):
            self.unavailable.emit("macOS 快捷鍵未啟用，請檢查 pynput 安裝與系統權限")

    def stop(self):
        if not self._running:
            return
        self._running = False
        if self._listener is not None:
            self._listener.stop()
            self._listener = None
        if sys.platform == "win32" and self._thread_id:
            import ctypes
            WM_QUIT = 0x0012
            ctypes.windll.user32.PostThreadMessageW(self._thread_id, WM_QUIT, 0, 0)
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)
