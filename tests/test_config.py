import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from core import config_manager as cm

class ConfigTests(unittest.TestCase):
    def test_frozen_path_is_persistent(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(cm.sys, 'frozen', True, create=True), patch.object(cm.sys, 'platform', 'win32'), patch.dict(os.environ, {'APPDATA': directory}), patch.object(cm, '__file__', os.path.join(directory, '_MEI123', 'core', 'config_manager.py')):
                path = cm.get_config_path()
                self.assertEqual(path, os.path.join(directory, 'ClaudeHUDMonitor', 'config.json'))
                cm.ConfigManager(path).set('opacity', 0.55)
                self.assertEqual(cm.ConfigManager(path).get('opacity'), 0.55)

    def test_failed_replace_preserves_settings(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'config.json'
            config = cm.ConfigManager(path)
            config.set('opacity', 0.5)
            with patch.object(cm.os, 'replace', side_effect=OSError):
                config.set('opacity', 0.6)
            self.assertEqual(json.loads(path.read_text())['opacity'], 0.5)
            self.assertEqual(len(list(Path(directory).iterdir())), 1)
