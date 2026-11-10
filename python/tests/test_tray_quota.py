import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest.mock import patch

from PySide6.QtWidgets import QApplication
from core import tray_quota as tq
from core.config_manager import ConfigManager
from core.providers.base import MetricRow, UsageMetrics

APP = QApplication.instance() or QApplication([])
NOW = datetime(2026, 1, 1, 12, 0, tzinfo=timezone.utc)
FABLE = 'claude:weekly_scoped:fable'


def claude(five=30, week=60, fable=84, **kw):
    return UsageMetrics(provider_id='claude', metric1_title='SESSION 5H', metric1_val=five, metric1_key='claude:5h',
                        metric2_title='WEEKLY 7D', metric2_val=week, metric2_key='claude:7d',
                        extra_metrics=[MetricRow('FABLE 7D', fable, key=FABLE)] if fable is not None else [], **kw)


def codex(used=42, **kw):
    return UsageMetrics(provider_id='codex', metric1_title='WINDOW 5H', metric1_val=used, metric2_val=None, **kw)


def rows(**metrics):
    return tq.readings(metrics, list(metrics), NOW)


class LogicTests(unittest.TestCase):
    def test_auto_picks_the_fullest_row_and_skips_offline_providers(self):
        found = tq.select('auto', rows(claude=claude(), codex=codex(97, error='down')))
        self.assertEqual((found.key, found.used), (FABLE, 84))
        self.assertEqual(tq.select('auto', rows(claude=claude(), codex=codex(97, error='x', stale=True))).key,
                         'codex:WINDOW 5H')
        self.assertEqual(tq.select('auto', rows(codex=codex(None))).state, tq.NONE)
        self.assertIsNone(tq.select('off', rows(claude=claude())))

    def test_numbers_fit_two_digits_in_both_modes(self):
        self.assertEqual([tq.number_text(v, 'used') for v in (None, 0, 7.4, 41.6, 100)], ['--', '0', '7', '42', '99'])
        self.assertEqual([tq.number_text(v, 'remaining') for v in (0, 16, 58, 100)], ['99', '84', '42', '0'])

    def test_missing_selection_is_unavailable_and_keeps_the_setting(self):
        with tempfile.TemporaryDirectory() as directory:
            config = ConfigManager(Path(directory) / 'config.json')
            config.set('tray_quota', FABLE)
            reading = tq.select(config.get('tray_quota'), rows(claude=claude(fable=None)))
            self.assertEqual((reading.state, tq.display(reading, 'used')[0]), (tq.UNAVAILABLE, '--'))
            self.assertEqual(config.get('tray_quota'), FABLE)

    def test_selection_on_an_offline_provider_shows_offline_not_unavailable(self):
        offline = UsageMetrics(provider_id='claude', error='not signed in')  # error result: default titles, no keys
        for key in ('claude:5h', FABLE):
            reading = tq.select(key, rows(claude=offline))
            self.assertEqual((reading.state, tq.display(reading, 'used')[0]), (tq.OFFLINE, '!'))
            self.assertEqual(tq.tooltip_line(reading, 'used'), 'Claude: offline')

    def test_states_follow_the_sidebar_and_age(self):
        def shown(m):
            return tq.display(tq.select('codex:WINDOW 5H', rows(codex=m)), 'used')
        self.assertEqual(shown(codex(42)), ('42', None))
        self.assertEqual(shown(codex(42, error='timeout', stale=True, last_success=NOW)), ('42', tq.AMBER))
        self.assertEqual(shown(codex(42, last_success=NOW - timedelta(minutes=31))), ('42', tq.GREY))
        self.assertEqual(shown(codex(None)), ('--', tq.GREY))
        self.assertEqual(shown(codex(None, error='not signed in'))[0], '!')


class TrayUiTests(unittest.TestCase):
    def _hud(self, directory, **settings):
        from ui.hud_window import HUDWindow
        config = ConfigManager(Path(directory) / 'config.json')
        config.update(settings)
        hud = HUDWindow(config)
        hud._raw_metrics.update({'claude': claude(), 'codex': codex()})
        return config, hud

    def _close(self, hud):
        hud.refresh_controller.stop()
        hud.countdown_timer.stop()
        hud.geometry_timer.stop()
        hud.close()

    def _submenu(self, hud):
        from ui.hud_menu import build_hud_menu
        menu = build_hud_menu(hud, tray=True)
        return next(a.menu() for a in menu.actions() if a.menu() and 'Tray number' in a.text())

    def test_menu_checks_follow_config_and_choosing_persists(self):
        import re
        emoji = re.compile('[\U0001F300-\U0001FAFF\u2190-\u21FF\u2600-\u27BF\u2B00-\u2BFF]')
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, tray_quota=FABLE, tray_quota_mode='remaining')
            try:
                sub = self._submenu(hud)
                checked = [a.text() for a in sub.actions() if a.isChecked()]
                self.assertEqual(checked, ['FABLE 7D  84%', '顯示剩餘 (Show remaining)'])
                self.assertFalse([a.text() for a in sub.actions() if emoji.search(a.text())])
                next(a for a in sub.actions() if a.text() == 'WINDOW 5H  42%').trigger()
                self.assertEqual(config.get('tray_quota'), 'codex:WINDOW 5H')
                next(a for a in self._submenu(hud).actions() if 'Show used' in a.text()).trigger()
                self.assertEqual(config.get('tray_quota_mode'), 'used')
                hud._raw_metrics['claude'] = claude(fable=None)
                config.set('tray_quota', FABLE)
                gone = next(a for a in self._submenu(hud).actions() if 'unavailable' in a.text())
                self.assertTrue(gone.isChecked() and not gone.isEnabled())
                self.assertEqual(config.get('tray_quota'), FABLE)
            finally:
                self._close(hud)

    def test_icon_is_static_when_off_and_redrawn_only_when_the_reading_changes(self):
        from ui.tray_icon import HUDTrayIcon
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory)
            tray = HUDTrayIcon(hud)
            try:
                self.assertEqual(tray.toolTip(), tray._base_tooltip)
                off = tray.icon().pixmap(32, 32).toImage()
                self.assertEqual(off, tray._app_icon.pixmap(32, 32).toImage())
                hud.set_tray_quota(FABLE)
                self.assertNotEqual(tray.icon().pixmap(32, 32).toImage(), off)
                self.assertTrue(tray.toolTip().startswith('Claude FABLE 7D: 84% used\n'))
                with patch.object(tray, 'setIcon') as set_icon:
                    hud._publish(hud._raw_metrics['claude'])
                    set_icon.assert_not_called()
                    hud.set_tray_quota_mode('remaining')
                    set_icon.assert_called_once()
                self.assertTrue(tray.toolTip().startswith('Claude FABLE 7D: 16% left\n'))
            finally:
                tray.hide()
                self._close(hud)


if __name__ == '__main__':
    unittest.main()
