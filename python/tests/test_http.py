import unittest
from urllib.error import HTTPError
from unittest.mock import patch
from core.providers import ClaudeProvider, CodexProvider
from core.providers.base import retry_delay

class HTTPTests(unittest.TestCase):
    def test_rate_limit_preserves_retry_after(self):
        cases = [(ClaudeProvider(), 'get_access_token', 'synthetic'), (CodexProvider(), 'get_auth_data', {'tokens': {'access_token': 'synthetic'}})]
        for provider, method, auth in cases:
            with self.subTest(provider=provider.provider_id), patch.object(provider, method, return_value=auth), patch('urllib.request.urlopen', side_effect=HTTPError('https://example.invalid', 429, 'limited', {'Retry-After': '120'}, None)):
                result = provider.fetch_usage()
                self.assertEqual(result.error_code, 'rate_limit')
                self.assertEqual(result.retry_after, 120)
    def test_bad_retry_header(self):
        for value in ['bad', '-1', 'NaN', 'Infinity', None]:
            self.assertIsNone(retry_delay(value))
