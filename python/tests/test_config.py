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


class StyleMigrationTests(unittest.TestCase):
    def _load(self, saved):
        directory = tempfile.mkdtemp()
        path = Path(directory) / 'config.json'
        if saved is not None:
            path.write_text(json.dumps(saved), encoding='utf-8')
        return path, cm.ConfigManager(path)

    def test_fresh_install_defaults_to_sidebar_panel(self):
        _, config = self._load(None)
        self.assertEqual(config.get('ui_mode'), 'sidebar')
        self.assertEqual(config.get('sidebar_state'), 'panel')

    def test_cards_become_sidebar_panel_keeping_position_and_orientation(self):
        _, config = self._load({'ui_mode': 'cards', 'layout_mode': 'horizontal', 'window_x': 120, 'window_y': 80,
                                'opacity': 0.7})
        self.assertEqual(config.get('ui_mode'), 'sidebar')
        self.assertEqual(config.get('sidebar_state'), 'panel')
        self.assertEqual(config.get('sidebar_orientation'), 'horizontal')
        self.assertEqual(config.get('sidebar_edge'), 'none')
        self.assertEqual((config.get('sidebar_x'), config.get('sidebar_y')), (120, 80))
        self.assertEqual(config.get('opacity'), 0.7)

    def test_vertical_cards_become_vertical_panel(self):
        _, config = self._load({'ui_mode': 'cards', 'layout_mode': 'vertical', 'window_x': 5, 'window_y': 6})
        self.assertEqual(config.get('sidebar_orientation'), 'vertical')

    def test_cards_without_saved_position_keep_default_edge(self):
        _, config = self._load({'ui_mode': 'cards', 'layout_mode': 'vertical'})
        self.assertEqual(config.get('sidebar_edge'), 'right')
        self.assertIsNone(config.get('sidebar_x'))

    def test_legacy_file_without_ui_mode_counts_as_cards(self):
        _, config = self._load({'layout_mode': 'horizontal', 'window_x': 10, 'window_y': 20})
        self.assertEqual(config.get('ui_mode'), 'sidebar')
        self.assertEqual(config.get('sidebar_orientation'), 'horizontal')
        self.assertEqual((config.get('sidebar_x'), config.get('sidebar_y')), (10, 20))

    def test_table_and_sidebar_settings_are_untouched(self):
        _, table = self._load({'ui_mode': 'table', 'window_x': 1, 'window_y': 2, 'layout_mode': 'horizontal'})
        self.assertEqual(table.get('ui_mode'), 'table')
        self.assertEqual(table.get('sidebar_edge'), 'right')
        _, strip = self._load({'ui_mode': 'sidebar', 'sidebar_state': 'strip', 'sidebar_edge': 'left'})
        self.assertEqual((strip.get('sidebar_state'), strip.get('sidebar_edge')), ('strip', 'left'))

    def test_unknown_mode_falls_back_to_default_style(self):
        for bad in ('grid', '', None, 3):
            with self.subTest(bad=bad):
                _, config = self._load({'ui_mode': bad})
                self.assertEqual((config.get('ui_mode'), config.get('sidebar_state')), ('sidebar', 'panel'))

    def test_migration_is_saved_and_idempotent(self):
        path, config = self._load({'ui_mode': 'cards', 'layout_mode': 'vertical', 'window_x': 7, 'window_y': 9})
        on_disk = json.loads(path.read_text(encoding='utf-8'))
        self.assertEqual(on_disk['ui_mode'], 'sidebar')
        self.assertEqual(on_disk['layout_mode'], 'vertical')   # legacy keys stay for downgrades
        again = cm.ConfigManager(path)
        self.assertEqual(again.data, config.data)
