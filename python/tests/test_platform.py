import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import plistlib
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch, MagicMock
from PySide6.QtWidgets import QApplication
from core import autostart
from system.hotkey import GlobalHotkeyManager

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
