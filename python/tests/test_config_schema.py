import json
import os
import tempfile
import unittest
from pathlib import Path

from core import config_manager as cm
from core.config_schema import sanitize


def fixed(**saved):
    data = dict(cm.DEFAULT_CONFIG)
    data.update(saved)
    return data, sanitize(data, cm.DEFAULT_CONFIG)


class SanitizeTests(unittest.TestCase):
    def test_defaults_and_valid_values_are_left_alone(self):
        data, changed = fixed()
        self.assertEqual(changed, [])
        self.assertEqual(data, cm.DEFAULT_CONFIG)
        valid = dict(opacity=0.5, refresh_interval_sec=120, always_on_top=False, window_x=-1200, window_y=40,
                     table_width=520, table_height=410, sidebar_scale=1.5, sidebar_edge='left', sidebar_offset=40,
                     sidebar_state='strip', appearance='dark', color_scheme='duo', hotkey='Alt+C',
                     provider_order=['codex', 'claude', 'agy'], row_order={'claude': ['WEEKLY 7D', 'SESSION 5H']})
        data, changed = fixed(**valid)
        self.assertEqual(changed, [])
        for key, value in valid.items():
            self.assertEqual(data[key], value, key)

    def test_whole_numbers_in_range_are_not_reported_as_repairs(self):
        data, changed = fixed(opacity=1, sidebar_scale=2, refresh_interval_sec=60)
        self.assertEqual(changed, [])
        self.assertEqual(data['opacity'], 1)

    def test_unknown_keys_are_kept(self):
        data, changed = fixed(layout_mode='horizontal', future_setting={'a': 1})
        self.assertEqual(changed, [])
        self.assertEqual(data['layout_mode'], 'horizontal')
        self.assertEqual(data['future_setting'], {'a': 1})

    def test_the_values_that_used_to_crash_the_window_at_startup(self):
        for key, bad in (('opacity', 'abc'), ('opacity', None), ('opacity', [1]), ('refresh_interval_sec', '60x'),
                         ('refresh_interval_sec', None), ('refresh_interval_sec', [60])):
            with self.subTest(key=key, bad=bad):
                data, changed = fixed(**{key: bad})
                self.assertIn(key, changed)
                self.assertEqual(data[key], cm.DEFAULT_CONFIG[key])

    def test_numbers_are_clamped_or_coerced(self):
        self.assertEqual(fixed(opacity=0)[0]['opacity'], cm.DEFAULT_CONFIG['opacity'])
        self.assertEqual(fixed(opacity=-3)[0]['opacity'], cm.DEFAULT_CONFIG['opacity'])
        self.assertEqual(fixed(opacity=5)[0]['opacity'], 1.0)
        self.assertEqual(fixed(opacity=0.01)[0]['opacity'], 0.1)
        self.assertEqual(fixed(opacity=float('nan'))[0]['opacity'], cm.DEFAULT_CONFIG['opacity'])
        self.assertEqual(fixed(opacity=True)[0]['opacity'], cm.DEFAULT_CONFIG['opacity'])
        self.assertEqual(fixed(refresh_interval_sec='90')[0]['refresh_interval_sec'], 90)
        self.assertEqual(fixed(refresh_interval_sec=60.0)[0]['refresh_interval_sec'], 60)
        self.assertEqual(fixed(refresh_interval_sec=0)[0]['refresh_interval_sec'], 20)
        self.assertEqual(fixed(refresh_interval_sec=-5)[0]['refresh_interval_sec'], 20)
        self.assertEqual(fixed(refresh_interval_sec=10**9)[0]['refresh_interval_sec'], 86400)
        self.assertEqual(fixed(sidebar_scale='big')[0]['sidebar_scale'], 1.0)
        self.assertEqual(fixed(sidebar_scale=9)[0]['sidebar_scale'], 2.0)
        self.assertEqual(fixed(sidebar_scale=0.1)[0]['sidebar_scale'], 0.6)
        self.assertEqual(fixed(sidebar_idle_level='x')[0]['sidebar_idle_level'], 0.6)
        self.assertEqual(fixed(sidebar_idle_level=0.99)[0]['sidebar_idle_level'], 0.95)
        self.assertEqual(fixed(sidebar_idle_delay_ms='slow')[0]['sidebar_idle_delay_ms'], 900)
        self.assertEqual(fixed(sidebar_idle_delay_ms=5)[0]['sidebar_idle_delay_ms'], 200)
        for bad in (None, 5, '', '  ', ['claude:5h']):
            self.assertEqual(fixed(tray_quota=bad)[0]['tray_quota'], 'off', repr(bad))

    def test_booleans_accept_real_booleans_and_the_words_true_and_false(self):
        for key in ('always_on_top', 'click_through', 'locked', 'autostart', 'hotkey_enabled', 'sidebar_idle_fade',
                    'table_blur', 'table_compact'):
            default = cm.DEFAULT_CONFIG[key]
            with self.subTest(key=key):
                self.assertIs(fixed(**{key: True})[0][key], True)
                self.assertIs(fixed(**{key: False})[0][key], False)
                self.assertIs(fixed(**{key: 'true'})[0][key], True)
                self.assertIs(fixed(**{key: 'False'})[0][key], False)
                for bad in ('maybe', 1, 0, None, [], {}):
                    self.assertIs(fixed(**{key: bad})[0][key], default, repr(bad))

    def test_a_string_false_no_longer_locks_the_window(self):
        self.assertIs(fixed(locked='false')[0]['locked'], False)

    def test_positions_are_integers_or_none(self):
        for key in ('window_x', 'window_y', 'sidebar_x', 'sidebar_y'):
            with self.subTest(key=key):
                self.assertEqual(fixed(**{key: -1200})[0][key], -1200)
                self.assertEqual(fixed(**{key: 30.0})[0][key], 30)
                self.assertIsNone(fixed(**{key: None})[0][key])
                for bad in ('left', [1], True, 1.5, float('inf')):
                    self.assertIsNone(fixed(**{key: bad})[0][key], repr(bad))

    def test_window_sizes_must_be_positive_integers(self):
        for key in ('table_width', 'table_height', 'table_compact_width', 'table_compact_height'):
            with self.subTest(key=key):
                self.assertEqual(fixed(**{key: 500})[0][key], 500)
                for bad in ('wide', None, 0, -4, True, [5], 10**6):
                    self.assertEqual(fixed(**{key: bad})[0][key], cm.DEFAULT_CONFIG[key], repr(bad))

    def test_choices_fall_back_to_the_default(self):
        cases = {'sidebar_state': ('strip', 'panel'), 'sidebar_edge': ('left', 'right', 'top', 'bottom', 'none'),
                 'sidebar_orientation': ('horizontal', 'vertical'), 'color_scheme': ('scale', 'duo'),
                 'appearance': ('auto', 'light', 'dark'), 'tray_quota_mode': ('used', 'remaining')}
        for key, allowed in cases.items():
            with self.subTest(key=key):
                for value in allowed:
                    self.assertEqual(fixed(**{key: value})[0][key], value)
                for bad in ('sideways', 3, None, ['left']):
                    self.assertEqual(fixed(**{key: bad})[0][key], cm.DEFAULT_CONFIG[key], repr(bad))

    def test_sidebar_offset_is_none_an_integer_or_end(self):
        self.assertEqual(fixed(sidebar_offset=40)[0]['sidebar_offset'], 40)
        self.assertEqual(fixed(sidebar_offset='end')[0]['sidebar_offset'], 'end')
        self.assertIsNone(fixed(sidebar_offset=None)[0]['sidebar_offset'])
        for bad in ('far', True, 2.5, [3]):
            self.assertIsNone(fixed(sidebar_offset=bad)[0]['sidebar_offset'], repr(bad))

    def test_text_settings_need_non_empty_strings(self):
        self.assertEqual(fixed(hotkey='')[0]['hotkey'], 'Alt+C')
        self.assertEqual(fixed(hotkey=7)[0]['hotkey'], 'Alt+C')
        self.assertEqual(fixed(claude_profile='  ')[0]['claude_profile'], 'auto')
        self.assertEqual(fixed(claude_profile=None)[0]['claude_profile'], 'auto')
        self.assertEqual(fixed(claude_profile='work')[0]['claude_profile'], 'work')

    def test_orders_must_be_well_formed(self):
        self.assertEqual(fixed(provider_order='claude')[0]['provider_order'], cm.DEFAULT_CONFIG['provider_order'])
        self.assertEqual(fixed(provider_order=['a', 'b', 'c'])[0]['provider_order'], cm.DEFAULT_CONFIG['provider_order'])
        self.assertEqual(fixed(provider_order=['claude', 'claude', 'agy'])[0]['provider_order'],
                         cm.DEFAULT_CONFIG['provider_order'])
        self.assertEqual(fixed(row_order='x')[0]['row_order'], {})
        self.assertEqual(fixed(row_order={'claude': 'x'})[0]['row_order'], {})
        self.assertEqual(fixed(row_order={'claude': ['A'], 'codex': [1], 5: ['B']})[0]['row_order'], {'claude': ['A']})
        self.assertEqual(fixed(agy_order='x')[0]['agy_order'], cm.DEFAULT_CONFIG['agy_order'])

    def test_changed_lists_the_repaired_keys_only(self):
        _, changed = fixed(opacity='x', locked='maybe', window_x=5)
        self.assertEqual(sorted(changed), ['locked', 'opacity'])


class ConfigLoadTests(unittest.TestCase):
    def _load(self, saved):
        directory = tempfile.mkdtemp()
        path = Path(directory) / 'config.json'
        path.write_text(json.dumps(saved), encoding='utf-8')
        return path, cm.ConfigManager(path)

    def test_load_repairs_bad_values_in_memory_and_keeps_the_file_until_the_next_save(self):
        path, config = self._load({'ui_mode': 'table', 'opacity': 'abc', 'refresh_interval_sec': '60x', 'locked': 'false',
                                   'my_note': 'keep me'})
        self.assertEqual(config.get('opacity'), cm.DEFAULT_CONFIG['opacity'])
        self.assertEqual(config.get('refresh_interval_sec'), cm.DEFAULT_CONFIG['refresh_interval_sec'])
        self.assertIs(config.get('locked'), False)
        self.assertEqual(json.loads(path.read_text(encoding='utf-8'))['opacity'], 'abc')   # not rewritten behind the user's back
        config.set('appearance', 'dark')
        saved = json.loads(path.read_text(encoding='utf-8'))
        self.assertEqual(saved['opacity'], cm.DEFAULT_CONFIG['opacity'])
        self.assertEqual(saved['my_note'], 'keep me')

    def test_load_logs_which_settings_were_repaired(self):
        with self.assertLogs('ClaudeHUD', level='WARNING') as logs:
            self._load({'ui_mode': 'table', 'opacity': 'abc'})
        self.assertTrue(any('opacity' in line for line in logs.output))

    def test_style_migration_still_works_alongside_the_repairs(self):
        _, config = self._load({'ui_mode': 'cards', 'layout_mode': 'horizontal', 'window_x': 10, 'window_y': 20,
                                'opacity': 'abc'})
        self.assertEqual(config.get('ui_mode'), 'sidebar')
        self.assertEqual((config.get('sidebar_x'), config.get('sidebar_y')), (10, 20))
        self.assertEqual(config.get('opacity'), cm.DEFAULT_CONFIG['opacity'])


class HudStartupTests(unittest.TestCase):
    """The hand-edited values that used to crash the window before it appeared."""

    def test_window_starts_with_the_old_crash_values(self):
        os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
        from PySide6.QtWidgets import QApplication
        from ui.hud_window import HUDWindow
        QApplication.instance() or QApplication([])
        for mode in ('table', 'sidebar'):
            for key, bad in (('opacity', 'abc'), ('opacity', None), ('refresh_interval_sec', '60x')):
                with self.subTest(mode=mode, key=key, bad=bad), tempfile.TemporaryDirectory() as directory:
                    path = Path(directory) / 'config.json'
                    path.write_text(json.dumps({'ui_mode': mode, key: bad}), encoding='utf-8')
                    hud = HUDWindow(cm.ConfigManager(path), providers={})
                    try:
                        hud.show()
                    finally:
                        hud.refresh_controller.stop()
                        hud.countdown_timer.stop()
                        hud.geometry_timer.stop()
                        hud.close()


if __name__ == '__main__':
    unittest.main()
