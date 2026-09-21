import json
import ssl
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from core.providers import ClaudeProvider, CodexProvider
from core.providers.base import ssl_context

KEYCHAIN_JSON = json.dumps({'claudeAiOauth': {'accessToken': 'synthetic-keychain'}})


def keychain_result(returncode=0, stdout=KEYCHAIN_JSON):
    return subprocess.CompletedProcess(args=[], returncode=returncode, stdout=stdout, stderr='')


class ClaudeCredentialTests(unittest.TestCase):
    def provider_without_file(self, directory):
        provider = ClaudeProvider()
        provider.CREDENTIALS_PATH = str(Path(directory) / 'missing.json')
        return provider

    def test_macos_falls_back_to_keychain(self):
        with tempfile.TemporaryDirectory() as directory, patch('sys.platform', 'darwin'), patch('subprocess.run', return_value=keychain_result()) as run:
            self.assertEqual(self.provider_without_file(directory).get_access_token(), 'synthetic-keychain')
            self.assertIn('Claude Code-credentials', run.call_args.args[0])

    def test_file_takes_precedence_over_keychain(self):
        with tempfile.TemporaryDirectory() as directory, patch('sys.platform', 'darwin'), patch('subprocess.run') as run:
            path = Path(directory) / '.credentials.json'
            path.write_text(json.dumps({'claudeAiOauth': {'accessToken': 'synthetic-file'}}), encoding='utf-8')
            provider = ClaudeProvider()
            provider.CREDENTIALS_PATH = str(path)
            self.assertEqual(provider.get_access_token(), 'synthetic-file')
            run.assert_not_called()

    def test_keychain_not_used_off_macos(self):
        with tempfile.TemporaryDirectory() as directory, patch('sys.platform', 'win32'), patch('subprocess.run') as run:
            self.assertIsNone(self.provider_without_file(directory).get_access_token())
            run.assert_not_called()

    def test_keychain_failures_return_none(self):
        cases = [keychain_result(returncode=44, stdout=''), keychain_result(stdout='not json'), keychain_result(stdout='[]'),
                 keychain_result(stdout=json.dumps({'claudeAiOauth': 'bad'})), subprocess.TimeoutExpired('security', 5)]
        for outcome in cases:
            kwargs = {'side_effect': outcome} if isinstance(outcome, Exception) else {'return_value': outcome}
            with self.subTest(outcome=outcome), tempfile.TemporaryDirectory() as directory, patch('sys.platform', 'darwin'), patch('subprocess.run', **kwargs):
                self.assertIsNone(self.provider_without_file(directory).get_access_token())


class SSLContextTests(unittest.TestCase):
    def test_context_verifies_certificates(self):
        context = ssl_context()
        self.assertEqual(context.verify_mode, ssl.CERT_REQUIRED)
        self.assertTrue(context.check_hostname)

    def test_providers_pass_context_to_urlopen(self):
        cases = [(ClaudeProvider(), 'get_access_token', 'synthetic'), (CodexProvider(), 'get_auth_data', {'tokens': {'access_token': 'synthetic'}})]
        for provider, method, auth in cases:
            with self.subTest(provider=provider.provider_id), patch.object(provider, method, return_value=auth), patch('urllib.request.urlopen', side_effect=OSError('offline')) as urlopen:
                self.assertEqual(provider.fetch_usage().error_code, 'network')
                self.assertIs(urlopen.call_args.kwargs['context'], ssl_context())
