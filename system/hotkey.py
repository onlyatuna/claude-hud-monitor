import ctypes
from ctypes import wintypes
import threading
from PySide6.QtCore import QObject, Signal

WM_HOTKEY = 0x0312
MOD_ALT = 0x0001
MOD_CONTROL = 0x0002
MOD_SHIFT = 0x0004
MOD_WIN = 0x0008
MOD_NOREPEAT = 0x4000

HOTKEY_ID_TOGGLE = 9527
HOTKEY_ID_CLICKTHROUGH = 9528

user32 = ctypes.windll.user32

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

        self._running = True
        vk = ord(key_char.upper())
        self._thread = threading.Thread(target=self._message_loop, args=(vk,), daemon=True)
        self._thread.start()

    def _message_loop(self, vk):
        self._thread_id = ctypes.windll.kernel32.GetCurrentThreadId()

        # 1. Register Alt + C for Show/Hide
        success1 = user32.RegisterHotKey(None, HOTKEY_ID_TOGGLE, MOD_ALT | MOD_NOREPEAT, vk)
        if success1:
            print("[Hotkey] Registered Alt+C (Toggle HUD)")
        else:
            print(f"[Hotkey] Failed to register Alt+C (error {ctypes.GetLastError()})")

        # 2. Register Alt + Shift + C for Click-Through Toggle
        success2 = user32.RegisterHotKey(None, HOTKEY_ID_CLICKTHROUGH, MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, vk)
        if success2:
            print("[Hotkey] Registered Alt+Shift+C (Toggle Click-Through)")
        else:
            print(f"[Hotkey] Failed to register Alt+Shift+C (error {ctypes.GetLastError()})")

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
            print("[Hotkey] Hotkeys unregistered.")

    def stop(self):
        if not self._running:
            return
        self._running = False
        if self._thread_id:
            WM_QUIT = 0x0012
            user32.PostThreadMessageW(self._thread_id, WM_QUIT, 0, 0)
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)
