import unittest
from core.providers import ClaudeProvider, CodexProvider, AgyProvider
from core.providers.base import percentage

class ProviderTests(unittest.TestCase):
    def test_missing_data_is_not_zero(self):
        for provider, method in [(ClaudeProvider(), '_parse_response'), (CodexProvider(), '_parse_response'), (AgyProvider(), '_parse_agy_json')]:
            with self.subTest(provider=provider.provider_id):
                result = getattr(provider, method)({}, 'test')
                self.assertIsNotNone(result.error)
                self.assertIsNone(result.metric1_val)
                self.assertEqual(result.metric1_text, '--')

    def test_zero_and_partial_data(self):
        for provider, data in [(ClaudeProvider(), {'five_hour': {'utilization': 0}}), (CodexProvider(), {'rate_limit': {'primary_window': {'used_percent': 0}}})]:
            result = provider._parse_response(data, 'test')
            self.assertIsNone(result.error)
            self.assertEqual(result.metric1_val, 0)
            self.assertIsNone(result.metric2_val)

    def test_invalid_percentage(self):
        for value in [None, True, 'bad', float('nan'), float('inf'), -1, 101]:
            self.assertIsNone(percentage(value))

    def test_agy_zero(self):
        result = AgyProvider()._parse_agy_json({'command': {'data': {'groups': [{'name': 'Gemini', 'buckets': [{'id': '5h', 'remaining_fraction': 1}]}]}}}, 'test')
        self.assertEqual(result.metric1_val, 0)
        self.assertIsNone(result.metric2_val)
        self.assertIn('--', result.badge1_text)

    def test_codex_does_not_invent_model_or_plan(self):
        result = CodexProvider()._parse_response({'rate_limit': {'primary_window': {'used_percent': 3, 'limit_window_seconds': 3600}}}, 'test')
        self.assertEqual(result.badge1_text, '')
        self.assertEqual(result.badge2_text, '')
        self.assertEqual(result.metric1_title, 'WINDOW 1H')

    def test_malformed_schema_is_reported(self):
        for provider, method, payload in [(ClaudeProvider(), '_parse_response', {'five_hour': []}), (CodexProvider(), '_parse_response', {'rate_limit': 'bad'}), (AgyProvider(), '_parse_agy_json', {'command': None})]:
            result = getattr(provider, method)(payload, 'test')
            self.assertIsNotNone(result.error)
            self.assertEqual(result.error_code, 'schema')

    def test_agy_uses_most_constrained_bucket(self):
        result = AgyProvider()._parse_agy_json({'command': {'data': {'groups': [{'name': 'Gemini', 'buckets': [{'id': '5h', 'remaining_fraction': 0.2}, {'id': '5h', 'remaining_fraction': 0.9}]}]}}}, 'test')
        self.assertAlmostEqual(result.metric1_val, 80)
