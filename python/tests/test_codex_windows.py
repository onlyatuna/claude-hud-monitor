import unittest
from datetime import datetime, timezone

from core import tray_quota as tq
from core.providers import CodexProvider

# Shape captured from a free-plan account: one 30-day window, secondary_window null.
FREE = {
    'plan_type': 'free',
    'rate_limit': {
        'allowed': True,
        'limit_reached': False,
        'primary_window': {'used_percent': 16, 'limit_window_seconds': 2592000,
                           'reset_after_seconds': 793030, 'reset_at': 1795248533},
        'secondary_window': None,
    },
}


def parse(data):
    return CodexProvider()._parse_response(data, 'test')


class CodexWindowTests(unittest.TestCase):
    def test_single_window_fills_one_row_only(self):
        result = parse(FREE)
        self.assertIsNone(result.error)
        self.assertEqual((result.metric1_title, result.metric1_val, result.metric1_text), ('WINDOW 30D', 16, '16%'))
        self.assertEqual(result.metric1_reset, datetime.fromtimestamp(1795248533, tz=timezone.utc))
        self.assertEqual((result.metric2_title, result.metric2_val), ('', None))
        keys = [r.key for r in tq.readings({'codex': result}, ['codex'])]
        self.assertEqual(keys, ['codex:WINDOW 30D'])
        stored = tq.select('codex:WINDOW 7D', tq.readings({'codex': result}, ['codex']))
        self.assertEqual(stored.state, tq.UNAVAILABLE)

    def test_two_windows_are_unchanged(self):
        result = parse({'rate_limit': {'primary_window': {'used_percent': 10, 'limit_window_seconds': 18000},
                                       'secondary_window': {'used_percent': 20, 'limit_window_seconds': 604800}}})
        self.assertEqual([(result.metric1_title, result.metric1_val), (result.metric2_title, result.metric2_val)],
                         [('WINDOW 5H', 10), ('WINDOW 7D', 20)])
        keys = [r.key for r in tq.readings({'codex': result}, ['codex'])]
        self.assertEqual(keys, ['codex:WINDOW 5H', 'codex:WINDOW 7D'])

    def test_unknown_length_gets_a_derived_title(self):
        result = parse({'rate_limit': {'primary_window': {'used_percent': 1, 'limit_window_seconds': 259200}}})
        self.assertEqual(result.metric1_title, 'WINDOW 3D')

    def test_no_usable_window_is_still_an_error(self):
        for rate_limit in ({'primary_window': None, 'secondary_window': None}, {}):
            self.assertIsNotNone(parse({'rate_limit': rate_limit}).error)


if __name__ == '__main__':
    unittest.main()
