import sys
import threading
import time
from PySide6.QtCore import QObject, Signal

from core.logger import logger

MOD_ALT = 0x0001
MOD_CONTROL = 0x0002
MOD_SHIFT = 0x0004
MOD_WIN = 0x0008
MOD_NOREPEAT = 0x4000

_MOD_ORDER = (
    (MOD_CONTROL, "Ctrl"),
    (MOD_ALT, "Alt"),
    (MOD_SHIFT, "Shift"),
    (MOD_WIN, "Meta"),
)

_NAMED_VKS = {
    "SPACE": 0x20,
    "TAB": 0x09,
    "RETURN": 0x0D,
    "ENTER": 0x0D,
    "ESC": 0x1B,
    "ESCAPE": 0x1B,
    "BACKSPACE": 0x08,
    "DELETE": 0x2E,
    "DEL": 0x2E,
    "INSERT": 0x2D,
    "INS": 0x2D,
    "HOME": 0x24,
    "END": 0x23,
    "PGUP": 0x21,
    "PAGEUP": 0x21,
    "PGDOWN": 0x22,
    "PAGEDOWN": 0x22,
    "LEFT": 0x25,
    "UP": 0x26,
    "RIGHT": 0x27,
    "DOWN": 0x28,
}

_MAC_VK = {
    "A": 0, "B": 11, "C": 8, "D": 2, "E": 14, "F": 3, "G": 5, "H": 4,
    "I": 34, "J": 38, "K": 40, "L": 37, "M": 46, "N": 45, "O": 31, "P": 35,
    "Q": 12, "R": 15, "S": 1, "T": 17, "U": 32, "V": 9, "W": 13, "X": 7,
    "Y": 16, "Z": 6,
    "0": 29, "1": 18, "2": 19, "3": 20, "4": 21, "5": 23, "6": 22, "7": 26, "8": 28, "9": 25,
    "F1": 122, "F2": 120, "F3": 99, "F4": 118, "F5": 96, "F6": 97,
    "F7": 98, "F8": 100, "F9": 101, "F10": 109, "F11": 103, "F12": 111,
    "F13": 105, "F14": 107, "F15": 113, "F16": 106, "F17": 64, "F18": 79,
    "F19": 80, "F20": 90,
    "SPACE": 49, "TAB": 48, "RETURN": 36, "ENTER": 36, "ESC": 53, "ESCAPE": 53,
    "BACKSPACE": 51, "DELETE": 117, "DEL": 117, "HOME": 115, "END": 119,
    "PGUP": 116, "PAGEUP": 116, "PGDOWN": 121, "PAGEDOWN": 121,
    "LEFT": 123, "RIGHT": 124, "DOWN": 125, "UP": 126,
}


def _parse_key_token(token: str):
    upper = token.strip().upper()
    if not upper:
        return None
    if len(upper) == 1 and ("A" <= upper <= "Z" or "0" <= upper <= "9"):
        return ord(upper), upper
    if upper.startswith("F") and upper[1:].isdigit():
        num = int(upper[1:])
        if 1 <= num <= 24:
            return 0x6F + num, f"F{num}"
    if upper in _NAMED_VKS:
        return _NAMED_VKS[upper], upper.title()
    return None


def resolve_hotkey(text: str) -> tuple[int, int, str]:
    """Parse portable hotkey text (e.g. 'Alt+C', 'Ctrl+Shift+H') into (win32_mods, vk, label)."""
    raw = text.strip() if isinstance(text, str) else ""
    if len(raw) == 1 and raw.isalnum():
        raw = f"Alt+{raw.upper()}"
    parts = [p.strip() for p in raw.split("+") if p.strip()]
    if len(parts) >= 2:
        mods = 0
        valid = True
        for part in parts[:-1]:
            low = part.lower()
            if low in ("ctrl", "control"):
                mods |= MOD_CONTROL
            elif low == "alt":
                mods |= MOD_ALT
            elif low == "shift":
                mods |= MOD_SHIFT
            elif low in ("meta", "win", "windows", "super"):
                mods |= MOD_WIN
            else:
                valid = False
                break
        parsed_key = _parse_key_token(parts[-1]) if valid else None
        if valid and mods != 0 and parsed_key is not None:
            vk, key_label = parsed_key
            return mods, vk, format_hotkey_label(mods, key_label)
    if raw and raw != "Alt+C":
        logger.warning(f"[Hotkey] {text!r} is not a registrable hotkey, using Alt+C")
    return MOD_ALT, ord("C"), "Alt+C"


def compute_ct_mods(mods: int) -> int:
    """Compute non-conflicting modifiers for the secondary click-through hotkey."""
    if not (mods & MOD_SHIFT):
        return mods | MOD_SHIFT
    if not (mods & MOD_CONTROL):
        return mods | MOD_CONTROL
    if not (mods & MOD_ALT):
        return mods | MOD_ALT
    return mods ^ MOD_SHIFT


def format_hotkey_label(mods: int, key_label: str) -> str:
    parts = [name for bit, name in _MOD_ORDER if mods & bit]
    parts.append(key_label)
    return "+".join(parts)


def format_hotkey_labels(hotkey_str: str) -> tuple[str, str]:
    mods, _vk, toggle_label = resolve_hotkey(hotkey_str)
    key_label = toggle_label.rsplit("+", 1)[-1]
    ct_mods = compute_ct_mods(mods)
    return toggle_label, format_hotkey_label(ct_mods, key_label)

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

    def start(self, hotkey_str="Alt+C"):
        if self._running:
            return

        if sys.platform == "win32":
            self._start_windows(hotkey_str)
        elif sys.platform == "darwin":
            self._start_macos(hotkey_str)

    def _start_windows(self, hotkey_str):
        import ctypes
        from ctypes import wintypes

        WM_HOTKEY = 0x0312
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

        mods, vk, toggle_label = resolve_hotkey(hotkey_str)
        ct_mods = compute_ct_mods(mods)
        key_label = toggle_label.rsplit("+", 1)[-1]
        ct_label = format_hotkey_label(ct_mods, key_label)

        self._ready_event.clear()
        self._running = True

        def message_loop():
            self._thread_id = kernel32.GetCurrentThreadId()

            # Force creation of message queue before signaling ready
            init_msg = wintypes.MSG()
            user32.PeekMessageW(ctypes.byref(init_msg), None, 0, 0, 0)  # PM_NOREMOVE = 0

            ok1 = user32.RegisterHotKey(None, HOTKEY_ID_TOGGLE, mods | MOD_NOREPEAT, vk)
            self.toggle_registered = bool(ok1)
            if not ok1:
                err = kernel32.GetLastError()
                msg = f"{toggle_label} 全域快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用"
                logger.warning(f"[Hotkey Windows] {msg}")
                self.hotkey_failed.emit(msg)
                self.unavailable.emit(f"{toggle_label} 已被占用，請使用系統匣操作")

            ok2 = user32.RegisterHotKey(None, HOTKEY_ID_CLICKTHROUGH, ct_mods | MOD_NOREPEAT, vk)
            self.clickthrough_registered = bool(ok2)
            if not ok2:
                err = kernel32.GetLastError()
                msg = f"{ct_label} 穿透模式快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用"
                logger.warning(f"[Hotkey Windows] {msg}")
                self.hotkey_failed.emit(msg)
                self.unavailable.emit(f"{ct_label} 已被占用，請使用系統匣操作")

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

    def _start_macos(self, hotkey_str):
        try:
            from pynput import keyboard
            if hasattr(keyboard.Listener, "IS_TRUSTED") and not keyboard.Listener.IS_TRUSTED:
                msg = "macOS 快捷鍵需要輔助使用／輸入監控權限；可使用系統匣操作"
                self.hotkey_failed.emit(msg)
                self.unavailable.emit(msg)
                return
            mods, vk, toggle_label = resolve_hotkey(hotkey_str)
            ct_mods = compute_ct_mods(mods)
            key_label = toggle_label.rsplit("+", 1)[-1]
            self._mac_keys = set()

            def current_mods():
                state = 0
                if any(k in self._mac_keys for k in (keyboard.Key.alt, keyboard.Key.alt_l, keyboard.Key.alt_r)):
                    state |= MOD_ALT
                if any(k in self._mac_keys for k in (keyboard.Key.shift, keyboard.Key.shift_l, keyboard.Key.shift_r)):
                    state |= MOD_SHIFT
                if any(k in self._mac_keys for k in (keyboard.Key.ctrl, keyboard.Key.ctrl_l, keyboard.Key.ctrl_r)):
                    state |= MOD_CONTROL
                if any(k in self._mac_keys for k in (keyboard.Key.cmd, keyboard.Key.cmd_l, keyboard.Key.cmd_r)):
                    state |= MOD_WIN
                return state

            mac_vk = _MAC_VK.get(key_label.upper())

            def matches_key(key, token):
                if mac_vk is not None and token == mac_vk:
                    return True
                char = getattr(key, "char", None)
                if isinstance(char, str) and char.upper() == key_label.upper():
                    return True
                return False

            def on_press(key):
                token = getattr(key, "vk", None)
                if token is None:
                    token = key
                repeated = token in self._mac_keys
                self._mac_keys.add(token)
                if repeated or not matches_key(key, token):
                    return
                active_mods = current_mods()
                if active_mods == ct_mods:
                    self.clickthrough_triggered.emit()
                elif active_mods == mods:
                    self.hotkey_triggered.emit()
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
