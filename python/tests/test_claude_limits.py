import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import copy
import unittest
from datetime import datetime, timezone
from unittest.mock import patch

from PySide6.QtWidgets import QApplication

from core.agy_rows import arrange_by_order
from core.providers import ClaudeProvider
from core.providers import claude_provider
from core.providers.base import MetricRow, UsageMetrics
from core.providers.claude_provider import parse_limits, slugify
from ui.provider_card import ProviderCardWidget
from ui.sidebar import SidebarWidget

APP = QApplication.instance() or QApplication([])

FIVE_RESET = '2026-10-16T14:00:00+00:00'
SEVEN_RESET = '2026-10-20T09:00:00+00:00'
FABLE_RESET = '2026-10-21T00:00:00+00:00'


def payload():
    """Redacted, public-capture-shaped OAuth usage response (no personal data)."""
    return {
        'five_hour': {'utilization': 12.0, 'resets_at': FIVE_RESET, 'limit_dollars': None,
                      'used_dollars': None, 'remaining_dollars': None, 'locked_reason': None},
        'seven_day': {'utilization': 34.0, 'resets_at': SEVEN_RESET, 'limit_dollars': None,
                      'used_dollars': None, 'remaining_dollars': None, 'locked_reason': None},
        'seven_day_opus': None, 'seven_day_sonnet': None, 'seven_day_omelette': None,
        'seven_day_cowork': None, 'seven_day_oauth_apps': None,
        'tangelo': None, 'iguana_necktie': None, 'nimbus_quill': None, 'cinder_cove': None,
        'amber_ladder': None, 'copper_kite': None, 'juniper_tide': None, 'omelette_promotional': None,
        'extra_usage': None, 'spend': None, 'member_dashboard_available': False,
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


def main_pair(result):
    return (result.metric1_title, result.metric1_val, result.metric1_reset,
            result.metric2_title, result.metric2_val, result.metric2_reset)


class ClaudeLimitsParseTests(unittest.TestCase):
    def setUp(self):
        claude_provider._logged_unknown.clear()

    def test_main_pair_unchanged_and_scoped_rows(self):
        result = parse(payload())
        self.assertIsNone(result.error)
        self.assertEqual(main_pair(result), ('SESSION 5H', 12.0, datetime.fromisoformat(FIVE_RESET),
                                             'WEEKLY 7D', 34.0, datetime.fromisoformat(SEVEN_RESET)))
        self.assertEqual((result.metric1_key, result.metric2_key), ('claude:5h', 'claude:7d'))
        # session / weekly_all duplicates are not rows; the malformed entry and the unreadable one are skipped
        self.assertEqual([(r.title, r.key, r.val) for r in result.extra_metrics],
                         [('FABLE 7D', 'claude:weekly_scoped:fable', 84.0),
                          ('SONNET 7D', 'claude:weekly_scoped:sonnet', 0.0)])
        fable = result.extra_metrics[0]
        self.assertEqual((fable.text, fable.reset, fable.subtext), ('84%', datetime.fromisoformat(FABLE_RESET), ''))

    def test_quiet_inactive_entry_still_renders(self):
        quiet = parse(payload()).extra_metrics[1]
        self.assertEqual((quiet.val, quiet.text, quiet.reset), (0.0, '0%', None))

    def test_is_active_is_ignored(self):
        data = payload()
        for entry in data['limits'][2:4]:
            entry['is_active'] = not entry['is_active']
        self.assertEqual([r.key for r in parse(data).extra_metrics],
                         ['claude:weekly_scoped:fable', 'claude:weekly_scoped:sonnet'])

    def test_main_pair_without_limits_matches_old_behaviour(self):
        data = payload()
        del data['limits']
        result = parse(data)
        self.assertEqual(result.extra_metrics, [])
        self.assertEqual(main_pair(result), main_pair(parse(payload())))

    def test_missing_top_level_pair_falls_back_to_limits(self):
        data = payload()
        data['five_hour'] = None
        del data['seven_day']
        result = parse(data)
        self.assertIsNone(result.error)
        self.assertEqual(main_pair(result), ('SESSION 5H', 12.0, datetime.fromisoformat(FIVE_RESET),
                                             'WEEKLY 7D', 34.0, datetime.fromisoformat(SEVEN_RESET)))
        self.assertEqual(len(result.extra_metrics), 2)

    def test_fallback_never_overrides_present_top_level(self):
        data = payload()
        data['five_hour'] = {'utilization': None, 'resets_at': None}
        data['limits'][0]['percent'] = 99
        result = parse(data)
        self.assertIsNone(result.metric1_val)
        self.assertEqual(result.metric2_val, 34.0)

    def test_fallback_skips_invalid_entries(self):
        data = payload()
        data['five_hour'] = None
        data['limits'][0]['percent'] = None
        data['limits'].insert(1, {'kind': 'session', 'group': 'session', 'percent': 7, 'resets_at': FIVE_RESET})
        self.assertEqual(parse(data).metric1_val, 7.0)

    def test_non_list_limits_ignored(self):
        for bad in (None, {}, 'x', 5, {'kind': 'weekly_scoped', 'percent': 10}):
            with self.subTest(limits=bad):
                data = payload()
                data['limits'] = bad
                result = parse(data)
                self.assertIsNone(result.error)
                self.assertEqual(result.extra_metrics, [])
                self.assertEqual(result.metric1_val, 12.0)
                self.assertEqual(result.metric2_val, 34.0)

    def test_broken_limits_never_discard_main_pair(self):
        entries = [None, 3, 'x', [], [{'kind': 'weekly_scoped'}], {}, {'kind': 5}, {'kind': ['a']},
                   {'kind': 'weekly_scoped', 'percent': 'nan'}, {'kind': 'weekly_scoped', 'percent': float('nan')},
                   {'kind': 'weekly_scoped', 'percent': True}, {'kind': 'weekly_scoped', 'percent': 101},
                   {'kind': 'weekly_scoped', 'percent': -1}, {'kind': 'weekly_scoped', 'percent': {}},
                   {'kind': 'weekly_scoped', 'group': 7, 'percent': 5, 'scope': 'oops'},
                   {'kind': 'weekly_scoped', 'percent': 5, 'resets_at': 12345, 'scope': {'model': 'Fable'}},
                   {'kind': 'weekly_scoped', 'percent': 5, 'resets_at': 'garbage', 'scope': {'model': {'display_name': 9}}}]
        data = payload()
        data['limits'] = entries
        result = parse(data)
        self.assertIsNone(result.error)
        self.assertEqual((result.metric1_val, result.metric2_val), (12.0, 34.0))
        # the entries with a finite percent collapse into one generic row (first wins); nothing else leaks in
        self.assertEqual([(r.title, r.key, r.reset) for r in result.extra_metrics],
                         [('WEEKLY_SCOPED', 'claude:weekly_scoped', None)])

    def test_unexpected_failure_in_limits_is_isolated(self):
        with patch.object(claude_provider, 'parse_limits', side_effect=TypeError('boom')):
            result = parse(payload())
        self.assertIsNone(result.error)
        self.assertEqual((result.metric1_val, result.metric2_val), (12.0, 34.0))
        self.assertEqual(result.extra_metrics, [])

    def test_wrong_typed_main_objects_still_report_schema(self):
        result = parse({'five_hour': ['x'], 'limits': payload()['limits']})
        self.assertEqual(result.error_code, 'schema')

    def test_empty_and_missing_everything_is_still_an_error(self):
        result = parse({'limits': payload()['limits'][2:4]})
        self.assertIsNotNone(result.error)
        self.assertEqual(result.error_code, 'schema')

    def test_unknown_kind_renders_generically_and_logs_once(self):
        data = payload()
        data['limits'] = [{'kind': 'Daily_Cap', 'group': 'daily', 'percent': 40, 'resets_at': None, 'scope': None}] * 2
        with patch.object(claude_provider.logger, 'info') as info:
            first = parse(data)
            second = parse(data)
        self.assertEqual([(r.title, r.key) for r in first.extra_metrics], [('DAILY_CAP', 'claude:daily_cap')])
        self.assertEqual(second.extra_metrics, first.extra_metrics)
        messages = [call.args[0] for call in info.call_args_list]
        self.assertEqual(len(messages), 2)  # one for the kind, one for the group, once per process
        self.assertTrue(any('kind' in m and 'daily_cap' in m for m in messages))
        self.assertTrue(any('group' in m and 'daily' in m for m in messages))

    def test_known_kinds_and_groups_are_not_logged(self):
        with patch.object(claude_provider.logger, 'info') as info:
            parse(dict(payload(), limits=payload()['limits'][:4]))
        info.assert_not_called()

    def test_duplicate_scope_first_wins_and_order_is_api_order(self):
        data = payload()
        data['limits'] = [
            {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 10, 'scope': {'model': {'display_name': 'Zed'}}},
            {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 20, 'scope': {'model': {'display_name': 'Alpha'}}},
            {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 99, 'scope': {'model': {'display_name': ' ZED '}}},
            {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 30,
             'scope': {'model': {'id': 'm-1', 'display_name': 'Zed'}}},
        ]
        rows = parse(data).extra_metrics
        self.assertEqual([(r.title, r.val) for r in rows], [('ZED 7D', 10.0), ('ALPHA 7D', 20.0)])

    def test_key_ignores_model_id_and_array_position(self):
        a = {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 5, 'scope': {'model': {'id': None, 'display_name': 'Fable'}}}
        b = {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 5,
             'scope': {'model': {'id': 'claude-fable-1', 'display_name': 'Fable'}}}
        filler = {'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 1, 'scope': {'model': {'display_name': 'Other'}}}
        self.assertEqual(parse_limits([a])[0][0].key, 'claude:weekly_scoped:fable')
        self.assertEqual(parse_limits([filler, b])[0][1].key, 'claude:weekly_scoped:fable')

    def test_title_rules(self):
        def row(entry):
            return parse_limits([dict({'kind': 'weekly_scoped', 'group': 'weekly', 'percent': 3}, **entry)])[0][0]
        cases = [
            ({'scope': {'model': {'display_name': 'Fable'}}}, 'FABLE 7D', 'claude:weekly_scoped:fable'),
            ({'scope': {'model': None, 'surface': 'Daily Routines'}}, 'DAILY ROUTINES 7D',
             'claude:weekly_scoped:daily-routines'),
            ({'scope': {'model': {'display_name': 'Fable'}, 'surface': 'cli'}}, 'FABLE 7D', 'claude:weekly_scoped:fable:cli'),
            ({'scope': {'model': {'display_name': '!!!'}, 'surface': 'cli'}}, 'CLI 7D', 'claude:weekly_scoped:cli'),
            ({'scope': None}, 'WEEKLY_SCOPED 7D', 'claude:weekly_scoped'),
            ({}, 'WEEKLY_SCOPED 7D', 'claude:weekly_scoped'),
            ({'group': 'session', 'scope': {'model': {'display_name': 'Fable'}}}, 'FABLE 5H', 'claude:weekly_scoped:fable'),
            ({'group': None, 'scope': {'model': {'display_name': 'Fable'}}}, 'FABLE', 'claude:weekly_scoped:fable'),
        ]
        for entry, title, key in cases:
            with self.subTest(entry=entry):
                r = row(entry)
                self.assertEqual((r.title, r.key), (title, key))

    def test_slugify(self):
        self.assertEqual(slugify('Fable'), 'fable')
        self.assertEqual(slugify('Claude Sonnet 4.5'), 'claude-sonnet-4-5')
        self.assertEqual(slugify('  Daily  Routines! '), 'daily-routines')
        self.assertEqual(slugify('---'), '')

    def test_metric_row_key_defaults_to_title(self):
        self.assertEqual(MetricRow('C/G 5H').key, 'C/G 5H')
        self.assertEqual(MetricRow('X', key='k').key, 'k')
        self.assertEqual(UsageMetrics(metric1_title='A', metric2_title='B').main_key(1), 'A')
        self.assertEqual(UsageMetrics(metric1_key='claude:5h').main_key(1), 'claude:5h')

    def test_arrange_by_order_keeps_keys_with_their_rows(self):
        result = parse(payload())
        arranged = arrange_by_order(result, ['FABLE 7D', 'SESSION 5H'])
        self.assertEqual((arranged.metric1_title, arranged.metric1_key), ('FABLE 7D', 'claude:weekly_scoped:fable'))
        self.assertEqual((arranged.metric2_title, arranged.metric2_key), ('SESSION 5H', 'claude:5h'))
        self.assertEqual([(r.title, r.key) for r in arranged.extra_metrics],
                         [('WEEKLY 7D', 'claude:7d'), ('SONNET 7D', 'claude:weekly_scoped:sonnet')])
        self.assertEqual(arrange_by_order(result, ['WEEKLY 7D']).metric1_key, 'claude:7d')

    def test_input_is_not_mutated(self):
        data = payload()
        before = copy.deepcopy(data)
        parse(data)
        self.assertEqual(data, before)


class ClaudeLimitsUiTests(unittest.TestCase):
    def setUp(self):
        claude_provider._logged_unknown.clear()
        self.result = parse(payload())
        self.result.provider_id = 'claude'

    def test_card_shows_scoped_rows_for_claude(self):
        card = ProviderCardWidget('claude')
        card.update_metrics(self.result)
        self.assertEqual([row[1].text() for row in card.extra_rows[:2]], ['FABLE 7D', 'SONNET 7D'])
        self.assertEqual([row[2].text() for row in card.extra_rows[:2]], ['84%', '0%'])
        self.assertTrue(all(not row[0].isHidden() for row in card.extra_rows[:2]))
        self.assertIn('--', card.extra_rows[1][4].text())  # no reset time -> the existing '--' handling
        card.update_metrics(UsageMetrics(provider_id='claude', metric1_val=20))
        self.assertTrue(all(row[0].isHidden() for row in card.extra_rows))

    def test_sidebar_tooltip_and_alert_read_scoped_rows(self):
        cards = {key: ProviderCardWidget(key) for key in ('claude', 'agy', 'codex')}
        sidebar = SidebarWidget(cards)
        try:
            sidebar.update_metrics(self.result)
            button = sidebar.buttons['claude']
            self.assertIn('FABLE 7D 已用 84%', button.toolTip())
            self.assertIn('SONNET 7D 已用 0%', button.toolTip())
            self.assertFalse(button.alert)  # 84% is below ALERT_PCT
            hot = copy.deepcopy(self.result)
            hot.extra_metrics[0].val = 95
            sidebar.update_metrics(hot)
            self.assertTrue(button.alert)  # a scoped row at or above ALERT_PCT marks the tile
        finally:
            sidebar.close()
            sidebar.deleteLater()
            APP.processEvents()


if __name__ == '__main__':
    unittest.main()
