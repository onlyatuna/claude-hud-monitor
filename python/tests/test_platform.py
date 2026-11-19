import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import plistlib
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch, MagicMock
from PySide6.QtWidgets import QApplication
from core import autostart
from system.hotkey import (
    MOD_ALT,
    MOD_CONTROL,
    MOD_SHIFT,
    MOD_WIN,
    GlobalHotkeyManager,
    compute_ct_mods,
    format_hotkey_labels,
    resolve_hotkey,
)

APP = QApplication.instance() or QApplication([])

class PlatformTests(unittest.TestCase):
    def test_mac_source_autostart_has_script_and_escapes_xml(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(autostart.sys, 'platform', 'darwin'), patch.object(autostart.sys, 'frozen', False, create=True), patch.object(autostart.sys, 'executable', '/Python & Tools/python'), patch.object(autostart.sys, 'argv', ['/project & test/main.py']), patch.object(autostart.os.path, 'expanduser', return_value=directory):
            self.assertTrue(autostart.set_autostart(True))
            with open(Path(directory) / 'com.claudehud.plist', 'rb') as f:
                data = plistlib.load(f)
            self.assertEqual(data['ProgramArguments'], ['/Python & Tools/python', os.path.abspath('/project & test/main.py')])
    def test_mac_permission_failure_is_reported(self):
        keyboard = MagicMock()
        keyboard.Listener.IS_TRUSTED = False
        with patch.dict('sys.modules', {'pynput': MagicMock(keyboard=keyboard)}):
            manager = GlobalHotkeyManager()
            messages = []
            manager.unavailable.connect(messages.append)
            manager._start_macos('C')
            self.assertEqual(len(messages), 1)
            keyboard.Listener.assert_not_called()
    def test_mac_shortcuts_are_distinct(self):
        keyboard = MagicMock()
        keyboard.Listener.IS_TRUSTED = True
        with patch.dict('sys.modules', {'pynput': MagicMock(keyboard=keyboard)}):
            manager = GlobalHotkeyManager()
            show, ghost = [], []
            manager.hotkey_triggered.connect(lambda: show.append(True))
            manager.clickthrough_triggered.connect(lambda: ghost.append(True))
            manager._start_macos('C')
            callbacks = keyboard.Listener.call_args.kwargs
            # Key enums have no vk; model these explicitly for callback testing.
            for name in ['alt', 'alt_l', 'alt_r', 'shift', 'shift_l', 'shift_r', 'ctrl', 'ctrl_l', 'ctrl_r', 'cmd', 'cmd_l', 'cmd_r']:
                getattr(keyboard.Key, name).vk = None
            key = MagicMock(vk=8)
            callbacks['on_press'](keyboard.Key.alt)
            callbacks['on_press'](key)
            callbacks['on_press'](key)
            callbacks['on_release'](key)
            callbacks['on_press'](keyboard.Key.shift)
            callbacks['on_press'](key)
            self.assertEqual(len(show), 1)
            self.assertEqual(len(ghost), 1)
            manager.stop()

    def test_hotkey_string_parsing_and_clickthrough_modifiers(self):
        mods, vk, label = resolve_hotkey("Control+H")
        self.assertEqual((mods, vk, label), (MOD_CONTROL, ord("H"), "Ctrl+H"))
        self.assertEqual(format_hotkey_labels("Control+H"), ("Ctrl+H", "Ctrl+Shift+H"))
        self.assertEqual(format_hotkey_labels("Win+Shift+K"), ("Shift+Meta+K", "Ctrl+Shift+Meta+K"))
        self.assertEqual(compute_ct_mods(MOD_CONTROL | MOD_ALT | MOD_SHIFT), MOD_CONTROL | MOD_ALT)
        self.assertEqual(resolve_hotkey("invalid-chord"), (MOD_ALT, ord("C"), "Alt+C"))
        self.assertEqual(resolve_hotkey("C"), (MOD_ALT, ord("C"), "Alt+C"))
        self.assertEqual(resolve_hotkey(123), (MOD_ALT, ord("C"), "Alt+C"))
        self.assertEqual(MOD_WIN & compute_ct_mods(MOD_WIN), MOD_WIN)

    def test_mac_custom_hotkey_uses_macos_virtual_keycode(self):
        keyboard = MagicMock()
        keyboard.Listener.IS_TRUSTED = True
        with patch.dict('sys.modules', {'pynput': MagicMock(keyboard=keyboard)}):
            manager = GlobalHotkeyManager()
            show, ghost = [], []
            manager.hotkey_triggered.connect(lambda: show.append(True))
            manager.clickthrough_triggered.connect(lambda: ghost.append(True))
            manager._start_macos('Ctrl+H')
            callbacks = keyboard.Listener.call_args.kwargs
            for name in ['alt', 'alt_l', 'alt_r', 'shift', 'shift_l', 'shift_r', 'ctrl', 'ctrl_l', 'ctrl_r', 'cmd', 'cmd_l', 'cmd_r']:
                getattr(keyboard.Key, name).vk = None
            key_h = MagicMock(vk=4, char='\x08')
            callbacks['on_press'](keyboard.Key.ctrl)
            callbacks['on_press'](key_h)
            callbacks['on_release'](key_h)
            callbacks['on_press'](keyboard.Key.shift)
            callbacks['on_press'](key_h)
            self.assertEqual(len(show), 1)
            self.assertEqual(len(ghost), 1)
            manager.stop()
