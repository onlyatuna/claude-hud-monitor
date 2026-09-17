import subprocess
import unittest
from unittest.mock import patch
from core.providers.agy_provider import AgyProvider

class AgyTransportTests(unittest.TestCase):
    def run_provider(self, **kwargs):
        provider = AgyProvider()
        with patch.object(provider, '_get_access_token', return_value=None), patch.object(provider, '_find_agy_binary', return_value='agy'), patch('core.providers.agy_provider.subprocess.run', **kwargs) as run:
            result = provider.fetch_usage()
            self.assertEqual(run.call_args.kwargs['timeout'], 30)
            return result

    def test_timeout(self):
        self.assertEqual(self.run_provider(side_effect=subprocess.TimeoutExpired('agy', 30)).error_code, 'timeout')

    def test_cli_failure_does_not_expose_output(self):
        result = self.run_provider(return_value=subprocess.CompletedProcess([], 1, 'secret', 'secret-token'))
        self.assertEqual(result.error_code, 'cli_exit')
        self.assertNotIn('secret', result.error)

    def test_bad_json(self):
        result = self.run_provider(return_value=subprocess.CompletedProcess([], 0, 'not json', ''))
        self.assertEqual(result.error_code, 'schema')
