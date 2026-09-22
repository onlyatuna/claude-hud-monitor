import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest.mock import patch

from PySide6.QtWidgets import QApplication
from core.config_manager import ConfigManager
from core.providers.base import UsageMetrics
from ui.styles import THEMES
from ui.usage_table import UsageTable

APP = QApplication.instance() or QApplication([])


def make_table(scheme='scale', appearance='dark'):
    return UsageTable(THEMES[appearance], scheme)


class TableTests(unittest.TestCase):
    def test_error_recovery_clears_offline_state(self):
        table = make_table()
        col = table.columns['agy']
        table.update_metrics(UsageMetrics(provider_id='agy', error='timeout'))
        self.assertEqual(col.badge.text(), 'OFFLINE')
        self.assertTrue(col.dial.muted)
        table.update_metrics(UsageMetrics(provider_id='agy', metric1_val=20, metric1_text='20%'))
        self.assertNotEqual(col.badge.text(), 'OFFLINE')
        self.assertFalse(col.dial.muted)
        self.assertEqual(col.m2_val.text(), '--')
        self.assertEqual(col.header.toolTip(), '')

    def test_absent_timestamp_clears_previous_countdown(self):
        table = make_table()
        col = table.columns['claude']
        table.update_metrics(UsageMetrics(provider_id='claude', metric2_reset=datetime.now(timezone.utc) + timedelta(days=2)))
        self.assertNotEqual(col.m2_countdown.text(), '--:--:--')
        table.update_metrics(UsageMetrics(provider_id='claude'))
        self.assertEqual(col.m2_countdown.text(), '--:--:--')
        self.assertEqual(col.m1_reset.text(), '--:--')

    def test_stale_data_remains_visible_and_labelled(self):
        table = make_table()
        col = table.columns['agy']
        table.update_metrics(UsageMetrics(provider_id='agy', metric1_val=25, metric1_text='25%', metric2_val=10,
                                          metric2_text='10%', error='timeout', stale=True,
                                          last_success=datetime.now(timezone.utc)))
        self.assertEqual(col.dial.inner_text, '25%')
        self.assertEqual(col.m2_val.text(), '10 %')
        self.assertIn('STALE', col.badge.text())
        self.assertIn('timeout', col.header.toolTip())

    def test_ahead_of_pace_shows_overage_on_weekly_pill(self):
        table = make_table()
        col = table.columns['codex']
        # 1 of 7 days elapsed (~14% pace) but 62% used
        table.update_metrics(UsageMetrics(provider_id='codex', metric2_val=62, metric2_text='62%',
                                          metric2_reset=datetime.now(timezone.utc) + timedelta(days=6)))
        self.assertIn('▲48', col.m2_val.text())
        self.assertIn('用完', col.m2_val.toolTip())

    def test_non_default_window_gets_caption(self):
        table = make_table()
        col = table.columns['codex']
        table.update_metrics(UsageMetrics(provider_id='codex', metric1_title='WINDOW 1D', metric1_val=5,
                                          metric1_text='5%', metric2_title='WINDOW 30D', metric2_val=10,
                                          metric2_text='10%'))
        self.assertEqual(col.dial.caption, '1D')
        self.assertIn('30D', col.m2_val.text())

    def test_scale_and_duo_colour_the_dial_differently(self):
        data = UsageMetrics(provider_id='claude', metric1_val=95, metric1_text='95%', metric2_val=10, metric2_text='10%')
        scale, duo = make_table('scale'), make_table('duo')
        scale.update_metrics(data)
        duo.update_metrics(data)
        dark = THEMES['dark']
        self.assertEqual(scale.columns['claude'].dial.inner[2], dark['scale']['red'])
        self.assertEqual(scale.columns['claude'].dial.outer[2], dark['scale']['green'])
        self.assertEqual(duo.columns['claude'].dial.inner[2], dark['duo']['inner'])
        self.assertEqual(duo.columns['claude'].dial.outer[2], dark['duo']['outer'])


class HudTests(unittest.TestCase):
    def _hud(self, directory, **settings):
        from ui.hud_window import HUDWindow
        config = ConfigManager(Path(directory) / 'config.json')
        config.update(settings)
        return config, HUDWindow(config)

    def _close(self, hud):
        hud.refresh_controller.stop()
        hud.countdown_timer.stop()
        hud.geometry_timer.stop()
        hud.close()

    def test_theme_switch_keeps_latest_data_and_persists(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory)
            hud._on_data_fetched(UsageMetrics(provider_id='claude', metric1_val=42, metric1_text='42%'))
            hud.set_color_scheme('duo')
            hud.set_appearance('light')
            self.assertEqual(hud.table.columns['claude'].dial.inner_text, '42%')
            self.assertEqual(hud.table.columns['claude'].dial.t, THEMES['light'])
            self.assertEqual(ConfigManager(config.path).get('color_scheme'), 'duo')
            self.assertEqual(ConfigManager(config.path).get('appearance'), 'light')
            self._close(hud)

    def test_invalid_theme_values_are_ignored(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, color_scheme='neon')
            self.assertEqual(hud.color_scheme(), 'scale')
            hud.set_appearance('sepia')
            self.assertEqual(config.get('appearance'), 'auto')
            self._close(hud)

    def test_saved_size_is_restored_and_undersized_values_reset(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, table_width=520, table_height=410)
            self.assertEqual((hud.width(), hud.height()), (520, 410))
            self._close(hud)
            _, hud = self._hud(directory, table_width=50, table_height=50)
            self.assertEqual((hud.width(), hud.height()), (450, 350))
            self._close(hud)
