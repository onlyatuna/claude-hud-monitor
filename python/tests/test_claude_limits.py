import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import unittest
from datetime import datetime
from unittest.mock import patch

from PySide6.QtWidgets import QApplication

from core.agy_rows import arrange_by_order
from core.providers import ClaudeProvider
from core.providers import claude_provider
from core.providers.base import UsageMetrics
from core.providers.claude_provider import parse_limits
from ui.provider_card import ProviderCardWidget

APP = QApplication.instance() or QApplication([])

FIVE_RESET = '2026-10-16T14:00:00+00:00'
SEVEN_RESET = '2026-10-20T09:00:00+00:00'
FABLE_RESET = '2026-10-21T00:00:00+00:00'


def payload():
    """Redacted, public-capture-shaped OAuth usage response (no personal data)."""
    return {
        'five_hour': {'utilization': 12.0, 'resets_at': FIVE_RESET, 'locked_reason': None},
        'seven_day': {'utilization': 34.0, 'resets_at': SEVEN_RESET, 'locked_reason': None},
        'seven_day_opus': None, 'seven_day_sonnet': None, 'tangelo': None, 'iguana_necktie': None,
        'omelette_promotional': None, 'extra_usage': None, 'spend': None,
        'limits': [
            {'kind': 'session', 'group': 'session', 'percent': 12, 'severity': 'normal',
             'resets_at': FIVE_RESET, 'scope': {'model': None, 'surface': None}, 'is_active': False},
            {'kind': 'weekly_all', 'group': 'weekly', 'percent': 34, 'severity': 'normal',
             'resets_at': SEVEN_RESET, 'scope': {'model': None, 'surface': None}, 'is_active': False},
            {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 84, 'severity': 'warning',
             'resets_at': FABLE_RESET, 'scope': {'model': {'id': None, 'display_name': 'Fable'}, 'surface': None},
             'is_active': True},
            {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 0, 'severity': 'normal', 'resets_at': None,
             'scope': {'model': {'id': None, 'display_name': 'Sonnet'}, 'surface': None}, 'is_active': False},
            'not an object',
            {'kind': 'something_new', 'group': 'monthly', 'percent': 'lots', 'scope': None},
        ],
    }


def parse(data):
    return ClaudeProvider()._parse_response(data, 'test')


class ClaudeLimitsTests(unittest.TestCase):
    def setUp(self):
        claude_provider._logged_unknown.clear()

    def test_main_pair_untouched_and_scoped_rows_added(self):
        result = parse(payload())
        self.assertIsNone(result.error)
        self.assertEqual((result.metric1_title, result.metric1_val, result.metric1_reset, result.metric1_key),
                         ('SESSION 5H', 12.0, datetime.fromisoformat(FIVE_RESET), 'claude:5h'))
        self.assertEqual((result.metric2_title, result.metric2_val, result.metric2_reset, result.metric2_key),
                         ('WEEKLY 7D', 34.0, datetime.fromisoformat(SEVEN_RESET), 'claude:7d'))
        # session / weekly_all duplicates, the non-object entry and the unreadable one are not rows
        self.assertEqual([(r.title, r.key, r.val, r.text) for r in result.extra_metrics],
                         [('FABLE 7D', 'claude:weekly_scoped:fable', 84.0, '84%'),
                          ('SONNET 7D', 'claude:weekly_scoped:sonnet', 0.0, '0%')])
        self.assertEqual(result.extra_metrics[0].reset, datetime.fromisoformat(FABLE_RESET))

    def test_quiet_entry_renders_and_is_active_is_ignored(self):
        data = payload()
        for entry in data['limits'][2:4]:
            entry['is_active'] = not entry['is_active']
        rows = parse(data).extra_metrics
        self.assertEqual([r.key for r in rows], ['claude:weekly_scoped:fable', 'claude:weekly_scoped:sonnet'])
        self.assertEqual((rows[1].val, rows[1].reset), (0.0, None))

    def test_broken_limits_never_discard_main_pair(self):
        junk = [None, 3, 'x', [], {}, {'kind': 5}, {'kind': 'weekly_scoped', 'percent': 'nan'},
                {'kind': 'weekly_scoped', 'percent': True}, {'kind': 'weekly_scoped', 'percent': 101},
                {'kind': 'weekly_scoped', 'percent': 5, 'resets_at': 12345, 'scope': {'model': 'Fable'}}]
        for limits in (None, {}, 'x', 5, junk):
            with self.subTest(limits=limits):
                data = payload()
                data['limits'] = limits
                result = parse(data)
                self.assertIsNone(result.error)
                self.assertEqual((result.metric1_val, result.metric2_val), (12.0, 34.0))
        with patch.object(claude_provider, 'parse_limits', side_effect=TypeError('boom')):
            result = parse(payload())
        self.assertIsNone(result.error)
        self.assertEqual((result.metric1_val, result.metric2_val, result.extra_metrics), (12.0, 34.0, []))

    def test_limits_are_only_a_fallback_for_a_missing_main_pair(self):
        data = payload()
        data['five_hour'] = None
        del data['seven_day']
        result = parse(data)
        self.assertEqual((result.metric1_val, result.metric1_reset, result.metric2_val, result.metric2_reset),
                         (12.0, datetime.fromisoformat(FIVE_RESET), 34.0, datetime.fromisoformat(SEVEN_RESET)))
        data = payload()
        data['five_hour'] = {'utilization': None, 'resets_at': None}  # present but empty: not replaced
        data['limits'][0]['percent'] = 99
        self.assertIsNone(parse(data).metric1_val)

    def test_unknown_kind_is_shown_without_error_and_logged_once(self):
        data = payload()
        data['limits'] = [{'kind': 'Daily_Cap', 'group': 'daily', 'percent': 40, 'scope': None}]
        with patch.object(claude_provider.logger, 'info') as info:
            first, second = parse(data), parse(data)
        self.assertEqual([(r.title, r.key) for r in first.extra_metrics], [('DAILY_CAP', 'claude:daily_cap')])
        self.assertEqual(first.extra_metrics, second.extra_metrics)
        self.assertEqual(info.call_count, 2)  # kind + group, once per process

    def test_key_ignores_model_id_and_position_and_first_duplicate_wins(self):
        def entry(percent, name='Fable', model_id=None):
            return {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': percent,
                    'scope': {'model': {'id': model_id, 'display_name': name}}}
        alone = parse_limits([entry(5)])[0]
        shifted = parse_limits([entry(1, 'Other'), entry(5, model_id='claude-fable-1'), entry(9, ' FABLE ')])[0]
        self.assertEqual(alone[0].key, 'claude:weekly_scoped:fable')
        self.assertEqual([(r.key, r.val) for r in shifted],
                         [('claude:weekly_scoped:other', 1), ('claude:weekly_scoped:fable', 5)])

    def test_reordering_keeps_keys_with_their_rows(self):
        arranged = arrange_by_order(parse(payload()), ['FABLE 7D', 'SESSION 5H'])
        self.assertEqual((arranged.metric1_key, arranged.metric2_key), ('claude:weekly_scoped:fable', 'claude:5h'))
        self.assertEqual([r.key for r in arranged.extra_metrics], ['claude:7d', 'claude:weekly_scoped:sonnet'])

    def test_card_shows_scoped_rows_for_claude(self):
        card = ProviderCardWidget('claude')
        card.update_metrics(parse(payload()))
        self.assertEqual([row[1].text() for row in card.extra_rows[:2]], ['FABLE 7D', 'SONNET 7D'])
        self.assertEqual([row[2].text() for row in card.extra_rows[:2]], ['84%', '0%'])
        self.assertTrue(all(not row[0].isHidden() for row in card.extra_rows[:2]))
        self.assertIn('--', card.extra_rows[1][4].text())  # no reset time
        card.update_metrics(UsageMetrics(provider_id='claude', metric1_val=20))
        self.assertTrue(all(row[0].isHidden() for row in card.extra_rows))


if __name__ == '__main__':
    unittest.main()
