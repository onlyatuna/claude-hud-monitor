import sys
import threading
import time
from PySide6.QtCore import QObject, Signal

from core.logger import logger

class GlobalHotkeyManager(QObject):
    hotkey_triggered = Signal()
    clickthrough_triggered = Signal()
    hotkey_failed = Signal(str)  # Emitted when hotkey registration fails
    unavailable = Signal(str)

    def __init__(self):
        super().__init__()
        self._thread = None
        self._thread_id = None
        self._running = False
        self._ready_event = threading.Event()
        self._listener = None
        self.toggle_registered = False
        self.clickthrough_registered = False

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
        kernel32 = ctypes.windll.kernel32

        user32.RegisterHotKey.argtypes = [wintypes.HWND, ctypes.c_int, wintypes.UINT, wintypes.UINT]
        user32.RegisterHotKey.restype = wintypes.BOOL
        user32.UnregisterHotKey.argtypes = [wintypes.HWND, ctypes.c_int]
        user32.UnregisterHotKey.restype = wintypes.BOOL
        user32.PostThreadMessageW.argtypes = [wintypes.DWORD, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
        user32.PostThreadMessageW.restype = wintypes.BOOL
        user32.GetMessageW.argtypes = [ctypes.POINTER(wintypes.MSG), wintypes.HWND, wintypes.UINT, wintypes.UINT]
        user32.GetMessageW.restype = ctypes.c_int
        user32.PeekMessageW.argtypes = [ctypes.POINTER(wintypes.MSG), wintypes.HWND, wintypes.UINT, wintypes.UINT, wintypes.UINT]
        user32.PeekMessageW.restype = wintypes.BOOL

        vk = ord(key_char.upper())

        self._ready_event.clear()
        self._running = True

        def message_loop():
            self._thread_id = kernel32.GetCurrentThreadId()

            # Force creation of message queue before signaling ready
            init_msg = wintypes.MSG()
            user32.PeekMessageW(ctypes.byref(init_msg), None, 0, 0, 0)  # PM_NOREMOVE = 0

            ok1 = user32.RegisterHotKey(None, HOTKEY_ID_TOGGLE, MOD_ALT | MOD_NOREPEAT, vk)
            self.toggle_registered = bool(ok1)
            if not ok1:
                err = kernel32.GetLastError()
                msg = f"Alt+{key_char.upper()} 全域快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用"
                logger.warning(f"[Hotkey Windows] {msg}")
                self.hotkey_failed.emit(msg)
                self.unavailable.emit(f"Alt+{key_char.upper()} 已被占用，請使用系統匣操作")

            ok2 = user32.RegisterHotKey(None, HOTKEY_ID_CLICKTHROUGH, MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, vk)
            self.clickthrough_registered = bool(ok2)
            if not ok2:
                err = kernel32.GetLastError()
                msg = f"Alt+Shift+{key_char.upper()} 穿透模式快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用"
                logger.warning(f"[Hotkey Windows] {msg}")
                self.hotkey_failed.emit(msg)
                self.unavailable.emit(f"Alt+Shift+{key_char.upper()} 已被占用，請使用系統匣操作")

            self._ready_event.set()

            msg = wintypes.MSG()
            try:
                while self._running:
                    ret = user32.GetMessageW(ctypes.byref(msg), None, 0, 0)
                    if ret <= 0:
                        if ret == -1:
                            err = kernel32.GetLastError()
                            logger.error(f"[Hotkey Windows] GetMessageW failed with error {err}")
                        break

                    if msg.message == WM_HOTKEY:
                        if msg.wParam == HOTKEY_ID_TOGGLE:
                            self.hotkey_triggered.emit()
                        elif msg.wParam == HOTKEY_ID_CLICKTHROUGH:
                            self.clickthrough_triggered.emit()
                    user32.TranslateMessage(ctypes.byref(msg))
                    user32.DispatchMessageW(ctypes.byref(msg))
            finally:
                if self.toggle_registered:
                    user32.UnregisterHotKey(None, HOTKEY_ID_TOGGLE)
                if self.clickthrough_registered:
                    user32.UnregisterHotKey(None, HOTKEY_ID_CLICKTHROUGH)

        self._thread = threading.Thread(target=message_loop, daemon=True)
        self._thread.start()
        # Synchronously wait for message queue and hotkey registration to complete
        self._ready_event.wait(timeout=1.5)

    def _start_macos(self, key_char):
        try:
            from pynput import keyboard
            if hasattr(keyboard.Listener, "IS_TRUSTED") and not keyboard.Listener.IS_TRUSTED:
                msg = "macOS 快捷鍵需要輔助使用／輸入監控權限；可使用系統匣操作"
                self.hotkey_failed.emit(msg)
                self.unavailable.emit(msg)
                return
            self._mac_keys = set()
            def on_press(key):
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
            self.toggle_registered = True
            self.clickthrough_registered = True
        except (ImportError, OSError, RuntimeError) as e:
            msg = f"macOS 快捷鍵未啟用: {e}，請檢查 pynput 安裝與系統權限"
            self.hotkey_failed.emit(msg)
            self.unavailable.emit(msg)

    def stop(self):
        if not self._running:
            return
        self._running = False
        if self._listener is not None:
            try:
                self._listener.stop()
            except Exception:
                pass
            self._listener = None
        if sys.platform == "win32":
            self._ready_event.wait(timeout=0.5)
            if self._thread_id:
                import ctypes
                WM_QUIT = 0x0012
                for _ in range(5):
                    if ctypes.windll.user32.PostThreadMessageW(self._thread_id, WM_QUIT, 0, 0):
                        break
                    time.sleep(0.05)
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)
