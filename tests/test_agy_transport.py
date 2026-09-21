import subprocess
import unittest
from unittest.mock import patch
from urllib.error import HTTPError
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

    def test_http_uses_fallback_endpoint(self):
        class Response:
            status = 200
            def __enter__(self):
                return self
            def __exit__(self, *args):
                return False
            def read(self):
                return b'{"groups":[{"name":"Gemini","buckets":[{"id":"5h","remainingFraction":0.5}]}]}'

        provider = AgyProvider()
        with patch('core.providers.agy_provider.urllib.request.urlopen', side_effect=[HTTPError(provider.API_URLS[0], 503, 'down', {}, None), Response()]) as urlopen:
            result = provider._fetch_via_http('token', '12:00:00')
        self.assertEqual(result.metric1_val, 50)
        self.assertEqual([call.args[0].full_url for call in urlopen.call_args_list], list(provider.API_URLS))

    def test_primary_api_request_remains_compatible(self):
        class Response:
            status = 200
            def __enter__(self):
                return self
            def __exit__(self, *args):
                return False
            def read(self):
                return b'{"groups":[{"name":"Gemini","buckets":[{"id":"5h","remainingFraction":0.5}]}]}'

        provider = AgyProvider()
        with patch('core.providers.agy_provider.urllib.request.urlopen', return_value=Response()) as urlopen:
            result = provider._fetch_via_http('token', '12:00:00')
        request = urlopen.call_args.args[0]
        self.assertEqual(result.metric1_val, 50)
        self.assertEqual(request.full_url, provider.API_URL)
        self.assertEqual(request.get_method(), 'POST')
        self.assertEqual(request.data, b'{}')
        self.assertEqual(request.get_header('Authorization'), 'Bearer token')
