import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import json
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import MagicMock, patch

from core import agy_statusline as agy

HOOK = agy.HOOK_FILENAME


class Workspace(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.agy_dir = self.root / 'agy'
        self.agy_dir.mkdir()
        self.settings = self.agy_dir / 'settings.json'
        self.config = self.root / 'config'
        self.hook = self.config / HOOK

    def write_settings(self, data):
        self.settings.write_text(json.dumps(data), encoding='utf-8')

    def read_settings(self):
        return json.loads(self.settings.read_text(encoding='utf-8'))

    def install(self, python='python'):
        return agy.install_statusline(self.settings, self.config, python)

    def status(self):
        return agy.statusline_status(self.settings, self.config)


class StatusTests(Workspace):
    def test_no_agy_settings_file(self):
        self.assertEqual(self.status(), 'agy_missing')
        self.agy_dir.rmdir()
        self.assertEqual(self.status(), 'agy_missing')

    def test_not_installed(self):
        self.write_settings({'model': 'x'})
        self.assertEqual(self.status(), 'not_installed')

    def test_installed_then_removed(self):
        self.write_settings({})
        self.assertEqual(self.install(), 'installed')
        self.assertEqual(self.status(), 'installed')
        agy.uninstall_statusline(self.settings, self.config)
        self.assertEqual(self.status(), 'not_installed')

    def test_someone_elses_statusline_is_a_conflict(self):
        self.write_settings({'statusLine': {'type': 'command', 'command': 'other'}})
        self.assertEqual(self.status(), 'conflict')

    def test_missing_or_outdated_hook_needs_an_update(self):
        self.write_settings({})
        self.install()
        self.hook.write_text('# an older hook', encoding='utf-8')
        self.assertEqual(self.status(), 'needs_update')
        self.hook.unlink()
        self.assertEqual(self.status(), 'needs_update')

    def test_command_pointing_somewhere_else_or_disabled_needs_an_update(self):
        self.write_settings({})
        self.install()
        data = self.read_settings()
        data['statusLine']['enabled'] = False
        self.write_settings(data)
        self.assertEqual(self.status(), 'needs_update')
        self.write_settings({'statusLine': {'type': 'command', 'command': f'python {self.root / "elsewhere" / HOOK}',
                                            'enabled': True}})
        self.assertEqual(self.status(), 'needs_update')

    def test_broken_settings(self):
        for raw in ('{', '[]'):
            self.settings.write_text(raw, encoding='utf-8')
            self.assertEqual(self.status(), 'error')


class InstallTests(Workspace):
    def test_missing_agy_settings_are_not_invented(self):
        self.assertEqual(self.install(), 'agy_missing')
        self.assertFalse(self.settings.exists())
        self.assertFalse(self.config.exists())
        self.agy_dir.rmdir()
        self.assertEqual(self.install(), 'agy_missing')

    def test_already_only_when_the_hook_is_current_too(self):
        self.write_settings({})
        self.assertEqual(self.install(), 'installed')
        self.assertEqual(self.install(), 'already')
        self.hook.write_text('# stale', encoding='utf-8')
        before = self.settings.read_text(encoding='utf-8')
        backups = len(list(self.agy_dir.glob('settings.json.bak-hud-*')))
        self.assertEqual(self.install(), 'updated')
        self.assertEqual(self.hook.read_text(encoding='utf-8'), agy.HOOK_SOURCE)
        self.assertEqual(self.settings.read_text(encoding='utf-8'), before)           # settings untouched
        self.assertEqual(len(list(self.agy_dir.glob('settings.json.bak-hud-*'))), backups)
        self.assertEqual(self.install(), 'already')

    def test_a_deleted_hook_is_written_again(self):
        self.write_settings({})
        self.install()
        self.hook.unlink()
        self.assertEqual(self.install(), 'updated')
        self.assertTrue(self.hook.exists())

    def test_a_command_for_another_location_is_replaced_with_a_backup(self):
        self.write_settings({'statusLine': {'type': 'command', 'command': f'python {self.root / "old" / HOOK}',
                                            'enabled': True}})
        self.assertEqual(self.install(), 'updated')
        self.assertEqual(self.read_settings()['statusLine']['command'], f'python {self.hook}')
        self.assertTrue(list(self.agy_dir.glob('settings.json.bak-hud-*')))

    def test_nothing_is_changed_when_python_cannot_be_found(self):
        self.write_settings({'model': 'x'})
        raw = self.settings.read_text(encoding='utf-8')
        with patch.object(agy, 'find_python', return_value=None):
            self.assertEqual(agy.install_statusline(self.settings, self.config), 'python_missing')
        self.assertEqual(self.settings.read_text(encoding='utf-8'), raw)
        self.assertFalse(list(self.agy_dir.glob('settings.json.bak-hud-*')))
        self.assertFalse(self.config.exists())

    def test_the_detected_python_is_the_one_written(self):
        self.write_settings({})
        with patch.object(agy, 'find_python', return_value='python3'):
            self.assertEqual(agy.install_statusline(self.settings, self.config), 'installed')
        self.assertEqual(self.read_settings()['statusLine']['command'], f'python3 {self.hook}')

    def test_python_is_not_needed_to_report_a_conflict_or_an_existing_install(self):
        self.write_settings({'statusLine': {'type': 'command', 'command': 'other'}})
        with patch.object(agy, 'find_python', return_value=None):
            self.assertEqual(agy.install_statusline(self.settings, self.config), 'conflict')
        self.write_settings({})
        self.install()
        with patch.object(agy, 'find_python', return_value=None):
            self.assertEqual(agy.install_statusline(self.settings, self.config), 'already')

    def test_uninstall_without_agy_settings_has_nothing_to_remove(self):
        self.assertEqual(agy.uninstall_statusline(self.settings, self.config), 'not_installed')


class FindPythonTests(unittest.TestCase):
    def ok(self, code=0):
        return MagicMock(returncode=code)

    def test_platform_names(self):
        with patch.object(agy.shutil, 'which', side_effect=lambda n: f'/bin/{n}'), \
                patch.object(agy.subprocess, 'run', return_value=self.ok()):
            self.assertEqual(agy.find_python('win32'), 'python')
            self.assertEqual(agy.find_python('darwin'), 'python3')
            self.assertEqual(agy.find_python('linux'), 'python3')

    def test_not_on_the_path(self):
        with patch.object(agy.shutil, 'which', return_value=None):
            self.assertIsNone(agy.find_python('win32'))

    def test_a_store_stub_that_cannot_run_python_does_not_count(self):
        with patch.object(agy.shutil, 'which', return_value='C:/WindowsApps/python.exe'), \
                patch.object(agy.subprocess, 'run', return_value=self.ok(9009)):
            self.assertIsNone(agy.find_python('win32'))

    def test_a_hung_or_unlaunchable_python_does_not_count(self):
        for error in (subprocess.TimeoutExpired('python', 10), OSError('no')):
            with patch.object(agy.shutil, 'which', return_value='/bin/python3'), \
                    patch.object(agy.subprocess, 'run', side_effect=error):
                self.assertIsNone(agy.find_python('linux'))

    def test_the_check_runs_python_for_real(self):
        with patch.object(agy.shutil, 'which', return_value=sys.executable):
            self.assertIn(agy.find_python('linux'), ('python3',))


class ShortPathTests(Workspace):
    def spaced(self):
        return self.root / 'John Smith' / 'config'

    def test_a_path_with_spaces_is_still_refused_outside_windows(self):
        with patch.object(agy.sys, 'platform', 'linux'):
            self.assertEqual(agy.install_statusline(self.settings, self.spaced(), 'python'), 'unsupported_path')
        self.assertFalse(self.spaced().exists())

    def test_windows_uses_the_8_3_name_when_there_is_one(self):
        self.write_settings({})
        config = self.spaced()
        short = self.root / 'JOHNSM~1' / 'config'

        def fake_short(path):
            return str(short)

        with patch.object(agy.sys, 'platform', 'win32'), patch.object(agy, '_short_path', side_effect=fake_short):
            self.assertEqual(agy.install_statusline(self.settings, config, 'python'), 'installed')
            self.assertEqual(agy.statusline_status(self.settings, config), 'installed')
        self.assertEqual(self.read_settings()['statusLine']['command'], f'python {short / HOOK}')
        self.assertTrue((config / HOOK).exists())                       # the file is written at the real path

    def test_a_refused_attempt_leaves_no_directories_behind(self):
        self.write_settings({})
        config = self.spaced()
        for answer in (None, str(config)):
            with patch.object(agy.sys, 'platform', 'win32'), patch.object(agy, '_short_path', return_value=answer):
                self.assertEqual(agy.install_statusline(self.settings, config, 'python'), 'unsupported_path')
            self.assertFalse(config.exists())
            self.assertFalse(config.parent.exists())          # the intermediate directory it made is gone too

    def test_a_directory_that_was_already_there_is_kept(self):
        self.write_settings({})
        config = self.spaced()
        config.mkdir(parents=True)
        with patch.object(agy.sys, 'platform', 'win32'), patch.object(agy, '_short_path', return_value=None):
            self.assertEqual(agy.install_statusline(self.settings, config, 'python'), 'unsupported_path')
        self.assertTrue(config.is_dir())

    def test_the_status_check_never_creates_anything(self):
        self.write_settings({'statusLine': {'type': 'command', 'command': f'python {self.root / "JOHNSM~1" / HOOK}',
                                            'enabled': True}})
        config = self.spaced()
        with patch.object(agy.sys, 'platform', 'win32'), patch.object(agy, '_short_path', return_value=str(self.root / 'JOHNSM~1')):
            self.assertEqual(agy.statusline_status(self.settings, config), 'needs_update')
        self.assertFalse(config.exists())
        self.assertFalse(config.parent.exists())

    def test_windows_without_short_names_is_refused(self):
        self.write_settings({})
        for answer in (None, str(self.spaced())):
            with patch.object(agy.sys, 'platform', 'win32'), patch.object(agy, '_short_path', return_value=answer):
                self.assertEqual(agy.install_statusline(self.settings, self.spaced(), 'python'), 'unsupported_path')
        self.assertNotIn('statusLine', self.read_settings())


class CacheAgeTests(Workspace):
    def test_written_at(self):
        cache = self.root / agy.QUOTA_CACHE_FILENAME
        self.assertIsNone(agy.cache_written_at(cache))
        cache.write_text(json.dumps({'written_at': '2026-11-02T08:30:00+00:00', 'quota': {}}), encoding='utf-8')
        self.assertEqual(agy.cache_written_at(cache), datetime(2026, 11, 2, 8, 30, tzinfo=timezone.utc))
        for raw in ('{', '[]', '{"written_at": 5}', '{"written_at": "soon"}'):
            cache.write_text(raw, encoding='utf-8')
            self.assertIsNone(agy.cache_written_at(cache))


class MessageTests(unittest.TestCase):
    def test_every_result_has_one_line_of_text(self):
        codes = ['installed', 'updated', 'already', 'removed', 'not_installed', 'conflict', 'unsupported_path',
                 'agy_missing', 'python_missing', 'error']
        for code in codes:
            self.assertIn(code, agy.RESULT_MESSAGES)
            self.assertTrue(agy.RESULT_MESSAGES[code])
            self.assertNotIn('\n', agy.RESULT_MESSAGES[code])
        self.assertEqual(set(agy.SUCCESS_RESULTS), {'installed', 'updated', 'already', 'removed', 'not_installed'})

    def test_every_status_has_a_label(self):
        for status in ('installed', 'needs_update', 'not_installed', 'conflict', 'agy_missing', 'error'):
            self.assertTrue(agy.STATUS_LABELS[status])

    def test_cli_exit_codes_follow_the_success_list(self):
        import contextlib
        import io
        for outcome in ('installed', 'updated', 'already', 'agy_missing', 'python_missing', 'conflict'):
            with patch.object(agy, 'install_statusline', return_value=outcome), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(agy.cli(['--install-agy-statusline']), 0 if outcome in agy.SUCCESS_RESULTS else 1)


class MenuTests(Workspace):
    """The right-click menu: 更多 > AGY, one click to enable, update or disable the hook."""

    def setUp(self):
        super().setUp()
        from PySide6.QtWidgets import QApplication
        self.app = QApplication.instance() or QApplication([])
        patches = [patch.object(agy, 'agy_settings_path', return_value=str(self.settings)),
                   patch.object(agy, 'get_user_config_dir', return_value=str(self.config)),
                   patch.object(agy, 'quota_cache_path', return_value=str(self.root / agy.QUOTA_CACHE_FILENAME)),
                   patch.object(agy, 'find_python', return_value='python'),
                   patch('ui.hud_window.PROVIDERS', {})]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)

    def hud(self):
        from core.config_manager import ConfigManager
        from ui.hud_window import HUDWindow
        config = ConfigManager(self.root / 'hud.json')
        config.update({'ui_mode': 'table'})
        hud = HUDWindow(config, providers={})
        self.addCleanup(self._close, hud)
        return hud

    @staticmethod
    def _close(hud):
        hud.refresh_controller.stop()
        hud.countdown_timer.stop()
        hud.geometry_timer.stop()
        hud.close()

    def agy_menu(self, hud):
        from PySide6.QtCore import QPoint
        from PySide6.QtGui import QContextMenuEvent
        from PySide6.QtWidgets import QMenu
        captured = []

        class RecordingMenu(QMenu):
            def exec(self, *args):
                captured.append(self)

        with patch('ui.hud_menu.QMenu', RecordingMenu):
            hud.contextMenuEvent(QContextMenuEvent(QContextMenuEvent.Reason.Mouse, QPoint(1, 1), QPoint(1, 1)))
        parent_menu = next(a.menu() for a in captured[0].actions() if a.menu() and ('Account' in a.text() or 'More' in a.text()))
        return next(a.menu() for a in parent_menu.actions() if a.menu() and 'AGY' in a.text() and 'Row' not in a.text())

    @staticmethod
    def rows(menu):
        return [(a.text(), a.isEnabled()) for a in menu.actions() if not a.isSeparator()]

    def action(self, menu, word):
        return next(a for a in menu.actions() if word in a.text())

    def test_not_installed_offers_to_enable(self):
        self.write_settings({})
        menu = self.agy_menu(self.hud())
        rows = self.rows(menu)
        self.assertIn('未啟用', rows[0][0])
        self.assertFalse(rows[0][1])
        enable = self.action(menu, 'Enable')
        self.assertTrue(enable.isEnabled())
        self.assertFalse(any('Disable' in text for text, _ in rows))
        self.assertTrue(any('python' in text for text, _ in rows))

    def test_enabling_installs_the_hook_and_says_so(self):
        self.write_settings({'model': 'x'})
        hud = self.hud()
        hud.tray_icon = MagicMock()
        self.action(self.agy_menu(hud), 'Enable').trigger()
        self.assertEqual(self.read_settings()['statusLine']['command'], f'python {self.hook}')
        self.assertTrue(self.hook.exists())
        hud.tray_icon.showMessage.assert_called_once()
        self.assertIn(agy.RESULT_MESSAGES['installed'], hud.tray_icon.showMessage.call_args.args)

    def test_installed_offers_to_disable_and_shows_when_data_last_arrived(self):
        self.write_settings({})
        self.install()
        menu = self.agy_menu(self.hud())
        self.assertIn('已啟用', self.rows(menu)[0][0])
        self.assertTrue(any('尚未收到資料' in text for text, _ in self.rows(menu)))
        (self.root / agy.QUOTA_CACHE_FILENAME).write_text(
            json.dumps({'written_at': '2026-11-02T08:30:00+00:00', 'quota': {}}), encoding='utf-8')
        menu = self.agy_menu(self.hud())
        self.assertTrue(any('最後更新' in text for text, _ in self.rows(menu)))
        self.assertFalse(any('Enable' in text for text, _ in self.rows(menu)))
        hud = self.hud()
        hud.tray_icon = MagicMock()
        self.action(self.agy_menu(hud), 'Disable').trigger()
        self.assertNotIn('statusLine', self.read_settings())
        self.assertIn(agy.RESULT_MESSAGES['removed'], hud.tray_icon.showMessage.call_args.args)

    def test_a_stale_hook_offers_an_update(self):
        self.write_settings({})
        self.install()
        self.hook.write_text('# old', encoding='utf-8')
        hud = self.hud()
        hud.tray_icon = MagicMock()
        menu = self.agy_menu(hud)
        self.assertIn('需要更新', self.rows(menu)[0][0])
        self.action(menu, 'Update').trigger()
        self.assertEqual(self.hook.read_text(encoding='utf-8'), agy.HOOK_SOURCE)
        self.assertEqual(agy.statusline_status(self.settings, self.config), 'installed')

    def test_someone_elses_statusline_is_never_overwritten(self):
        self.write_settings({'statusLine': {'type': 'command', 'command': 'other'}})
        menu = self.agy_menu(self.hud())
        self.assertIn('其他 statusLine', self.rows(menu)[0][0])
        self.assertFalse(self.action(menu, 'Enable').isEnabled())

    def test_without_agy_nothing_can_be_enabled(self):
        menu = self.agy_menu(self.hud())
        self.assertIn('找不到', self.rows(menu)[0][0])
        self.assertFalse(self.action(menu, 'Enable').isEnabled())

    def test_a_missing_python_is_reported_and_nothing_is_written(self):
        self.write_settings({})
        hud = self.hud()
        hud.tray_icon = MagicMock()
        with patch.object(agy, 'find_python', return_value=None):
            self.action(self.agy_menu(hud), 'Enable').trigger()
        self.assertNotIn('statusLine', self.read_settings())
        self.assertIn(agy.RESULT_MESSAGES['python_missing'], hud.tray_icon.showMessage.call_args.args)

    def test_without_a_tray_icon_the_result_is_only_logged(self):
        self.write_settings({})
        hud = self.hud()
        hud.tray_icon = None
        with self.assertLogs('ClaudeHUD', level='INFO') as logs:
            self.assertEqual(hud.run_agy_statusline('install'), 'installed')
        self.assertTrue(any('statusLine' in line for line in logs.output))

    def test_menu_text_is_plain(self):
        import re
        self.write_settings({})
        for text, _ in self.rows(self.agy_menu(self.hud())):
            self.assertIsNone(re.search('[\U0001F300-\U0001FAFF\u2600-\u27BF\uFE0F]', text), text)

    def test_the_tray_menu_stays_short(self):
        from PySide6.QtWidgets import QMenu
        from ui.hud_menu import build_hud_menu
        hud = self.hud()
        menu = build_hud_menu(hud, tray=True)
        self.assertFalse(any('More' in a.text() for a in menu.actions()))


if __name__ == '__main__':
    unittest.main()
