import sys
import threading
from PySide6.QtCore import QObject, Signal

from core.logger import logger

class GlobalHotkeyManager(QObject):
    hotkey_triggered = Signal()
    clickthrough_triggered = Signal()
    hotkey_failed = Signal(str)  # Emitted when hotkey registration fails

    def __init__(self):
        super().__init__()
        self._thread = None
        self._thread_id = None
        self._running = False
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

        vk = ord(key_char.upper())

        def message_loop():
            self._thread_id = kernel32.GetCurrentThreadId()

            ok1 = user32.RegisterHotKey(None, HOTKEY_ID_TOGGLE, MOD_ALT | MOD_NOREPEAT, vk)
            self.toggle_registered = bool(ok1)
            if not ok1:
                err = kernel32.GetLastError()
                msg = f"Alt+{key_char.upper()} 全域快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用"
                logger.warning(f"[Hotkey Windows] {msg}")
                self.hotkey_failed.emit(msg)

            ok2 = user32.RegisterHotKey(None, HOTKEY_ID_CLICKTHROUGH, MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, vk)
            self.clickthrough_registered = bool(ok2)
            if not ok2:
                err = kernel32.GetLastError()
                msg = f"Alt+Shift+{key_char.upper()} 穿透模式快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用"
                logger.warning(f"[Hotkey Windows] {msg}")
                self.hotkey_failed.emit(msg)

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
                if self.toggle_registered:
                    user32.UnregisterHotKey(None, HOTKEY_ID_TOGGLE)
                if self.clickthrough_registered:
                    user32.UnregisterHotKey(None, HOTKEY_ID_CLICKTHROUGH)

        self._running = True
        self._thread = threading.Thread(target=message_loop, daemon=True)
        self._thread.start()

    def _start_macos(self, key_char):
        # macOS implementation: attempt using pynput if installed, else inform user
        try:
            from pynput import keyboard
            hotkey_map = {
                f"<alt>+{key_char.lower()}": self.hotkey_triggered.emit,
                f"<alt>+<shift>+{key_char.lower()}": self.clickthrough_triggered.emit
            }
            self._listener = keyboard.GlobalHotKeys(hotkey_map)
            self._listener.daemon = True
            self._listener.start()
            self._running = True
            self.toggle_registered = True
            self.clickthrough_registered = True
            logger.info(f"[Hotkey macOS] Hotkeys registered via pynput for key '{key_char}'.")
        except ImportError:
            msg = "macOS 全域快捷鍵需要 pynput 模組支援，請執行 pip install pynput，並確保於系統設定授予輔助使用 (Accessibility) 權限。"
            logger.warning(f"[Hotkey macOS] {msg}")
            self.hotkey_failed.emit(msg)
        except Exception as e:
            msg = f"macOS 快捷鍵監聽啟動失敗: {e}。請確認已開啟輔助使用權限。"
            logger.error(f"[Hotkey macOS] {msg}", exc_info=True)
            self.hotkey_failed.emit(msg)

    def stop(self):
        if not self._running:
            return
        self._running = False
        if sys.platform == "win32" and self._thread_id:
            import ctypes
            WM_QUIT = 0x0012
            ctypes.windll.user32.PostThreadMessageW(self._thread_id, WM_QUIT, 0, 0)
        elif sys.platform == "darwin" and hasattr(self, "_listener") and self._listener:
            try:
                self._listener.stop()
            except Exception:
                pass
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)

