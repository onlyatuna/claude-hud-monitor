import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from unittest.mock import patch

from core import agy_statusline as agy


def fixture():
    values = [1, 0.8469924, 1, 0.78630334, 0.9914498]
    names = ['3p-5h', '3p-weekly', 'gemini-5h', 'gemini-argon-wtu-weekly', 'gemini-weekly']
    return {'product': 'antigravity', 'version': '1.4.0', 'email': 'synthetic@example.invalid',
            'quota': {name: {'remaining_fraction': value, 'reset_time': '2026-10-23T08:42:04Z',
                             'reset_in_seconds': 583948} for name, value in zip(names, values)}}


class StatuslineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.settings = self.root / 'settings.json'
        self.config = self.root / 'config'
        self.cache = self.root / agy.QUOTA_CACHE_FILENAME
        self.now = datetime(2026, 10, 16, tzinfo=timezone.utc)

    def write_cache(self, quota):
        self.cache.write_text(json.dumps({'written_at': '2026-10-16T00:00:00Z', 'quota': quota}), encoding='utf-8')

    def test_reader_five_buckets(self):
        data = fixture()['quota']
        self.write_cache(data)
        result = agy.read_quota_cache(self.cache, self.now)
        self.assertEqual(result['written_at'], self.now)
        self.assertEqual(set(result['buckets']), set(data))
        for key in data:
            self.assertEqual(result['buckets'][key]['remaining_fraction'], data[key]['remaining_fraction'])
            self.assertIsInstance(result['buckets'][key]['remaining_fraction'], float)

    def test_reader_filters_and_invalid_reset(self):
        quota = {str(x): {'remaining_fraction': x} for x in [1.5, -0.1, True, '0.5']}
        quota['expired'] = {'remaining_fraction': 1, 'reset_time': self.now.isoformat()}
        quota['valid'] = {'remaining_fraction': 0.5, 'reset_time': 'broken'}
        self.write_cache(quota)
        self.assertEqual(agy.read_quota_cache(self.cache, self.now)['buckets'],
                         {'valid': {'remaining_fraction': 0.5, 'reset_time': None}})

    def test_reader_bad_files(self):
        self.assertIsNone(agy.read_quota_cache(self.cache, self.now))
        for content in ['{', '[]', '{}', '{"quota":{}}', '{"quota":{"x":null}}']:
            self.cache.write_text(content, encoding='utf-8')
            self.assertIsNone(agy.read_quota_cache(self.cache, self.now))

    def run_hook(self, text):
        hook = self.root / agy.HOOK_FILENAME
        hook.write_text(agy.HOOK_SOURCE, encoding='utf-8')
        result = subprocess.run([sys.executable, str(hook)], input=text, text=True, capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, ' \n')
        self.assertEqual(result.stderr, '')

    def test_hook_privacy_and_throttle(self):
        data = fixture()
        self.run_hook(json.dumps(data))
        raw = self.cache.read_text(encoding='utf-8')
        result = json.loads(raw)
        self.assertNotIn(data['email'], raw)
        self.assertEqual(set(result), {'written_at', 'quota'})
        self.assertEqual(set(result['quota']), set(data['quota']))
        for bucket in result['quota'].values():
            self.assertEqual(set(bucket), {'remaining_fraction', 'reset_time'})
        mtime = self.cache.stat().st_mtime_ns
        data['quota']['3p-5h']['remaining_fraction'] = 0
        self.run_hook(json.dumps(data))
        self.assertEqual(self.cache.stat().st_mtime_ns, mtime)
        self.assertEqual(self.cache.read_text(encoding='utf-8'), raw)

    def test_hook_invalid_input(self):
        for text in ['not json', '{}', '[]', '{"quota":[]}']:
            self.run_hook(text)
            self.assertFalse(self.cache.exists())

    def install(self):
        return agy.install_statusline(self.settings, self.config, 'python')

    def test_install_and_uninstall(self):
        original = '{"model":"x"}'
        self.settings.write_text(original, encoding='utf-8')
        self.assertEqual(self.install(), 'installed')
        data = json.loads(self.settings.read_text(encoding='utf-8'))
        self.assertEqual(data, {'model': 'x', 'statusLine': {'type': 'command', 'command': f'python {self.config / agy.HOOK_FILENAME}', 'enabled': True}})
        self.assertNotIn('"', data['statusLine']['command'])
        self.assertEqual((self.config / agy.HOOK_FILENAME).read_text(encoding='utf-8'), agy.HOOK_SOURCE)
        backups = list(self.root.glob('settings.json.bak-hud-*'))
        self.assertTrue(backups)
        self.assertEqual(backups[0].read_text(encoding='utf-8'), original)
        self.assertEqual(self.install(), 'already')
        self.assertEqual(agy.uninstall_statusline(self.settings, self.config), 'removed')
        self.assertEqual(json.loads(self.settings.read_text(encoding='utf-8')), {'model': 'x'})
        self.assertTrue((self.config / agy.HOOK_FILENAME).exists())
        self.assertEqual(agy.uninstall_statusline(self.settings, self.config), 'not_installed')

    def test_conflict(self):
        for value in [{'command': 'other'}, None, 'other']:
            raw = json.dumps({'statusLine': value})
            self.settings.write_text(raw, encoding='utf-8')
            self.assertEqual(self.install(), 'conflict')
            self.assertEqual(agy.uninstall_statusline(self.settings, self.config), 'conflict')
            self.assertEqual(self.settings.read_text(encoding='utf-8'), raw)
        self.assertFalse(self.config.exists())

    def test_unsupported_and_missing(self):
        for name in ['has space', 'has"quote', "has'quote"]:
            self.assertEqual(agy.install_statusline(self.settings, self.root / name), 'unsupported_path')
            self.assertFalse((self.root / name).exists())
        self.assertEqual(self.install(), 'error')
        for raw in ['[]', '{']:
            self.settings.write_text(raw, encoding='utf-8')
            self.assertEqual(self.install(), 'error')
            self.assertEqual(self.settings.read_text(encoding='utf-8'), raw)

    def test_cli(self):
        for flag, function, outcomes in [('--install-agy-statusline', 'install_statusline', {'installed': 0, 'already': 0, 'conflict': 1, 'unsupported_path': 1, 'error': 1}),
                                         ('--uninstall-agy-statusline', 'uninstall_statusline', {'removed': 0, 'not_installed': 0, 'conflict': 1, 'error': 1})]:
            for outcome, code in outcomes.items():
                with patch.object(agy, function, return_value=outcome) as call, contextlib.redirect_stdout(io.StringIO()) as output:
                    self.assertEqual(agy.cli([flag]), code)
                    call.assert_called_once_with()
                    self.assertEqual(len(output.getvalue().splitlines()), 1)
        self.assertIsNone(agy.cli([]))


if __name__ == '__main__':
    unittest.main()
