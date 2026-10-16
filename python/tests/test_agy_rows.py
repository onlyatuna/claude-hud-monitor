import unittest

from core.agy_rows import (AGY_ROW_TITLES, DEFAULT_ORDER, DEFAULT_PRIMARY, arrange_agy, move_row, normalize_order,
                           normalize_primary, toggle_primary)
from core.providers.base import MetricRow, UsageMetrics


def agy_metrics():
    return UsageMetrics(
        provider_id='agy', metric1_title='GEMINI 5H', metric1_val=0, metric1_text='0%',
        metric2_title='GEMINI 7D', metric2_val=1, metric2_text='1%',
        extra_metrics=[MetricRow('C/G 5H', 3, '3%'), MetricRow('C/G 7D', 15, '15%'), MetricRow('ARGON 7D', 21, '21%')])


class AgyRowsTests(unittest.TestCase):
    def test_default_pair_is_cg_and_argon(self):
        result = arrange_agy(agy_metrics(), None)
        self.assertEqual((result.metric1_title, result.metric1_val), ('C/G 5H', 3))
        self.assertEqual((result.metric2_title, result.metric2_val), ('ARGON 7D', 21))
        self.assertEqual([r.title for r in result.extra_metrics], ['GEMINI 5H', 'GEMINI 7D', 'C/G 7D'])
        self.assertEqual(result.provider_id, 'agy')

    def test_any_pair_keeps_all_five_rows(self):
        for first in AGY_ROW_TITLES:
            for second in AGY_ROW_TITLES:
                if first == second:
                    continue
                result = arrange_agy(agy_metrics(), [first, second])
                titles = [result.metric1_title, result.metric2_title] + [r.title for r in result.extra_metrics]
                self.assertEqual(sorted(titles), sorted(AGY_ROW_TITLES))
                self.assertEqual((result.metric1_title, result.metric2_title), (first, second))

    def test_incomplete_or_error_data_is_left_alone(self):
        data = UsageMetrics(provider_id='agy', error='offline')
        self.assertIs(arrange_agy(data, DEFAULT_PRIMARY), data)
        partial = agy_metrics()
        partial.extra_metrics = partial.extra_metrics[:2]
        self.assertIs(arrange_agy(partial, DEFAULT_PRIMARY), partial)

    def test_bad_settings_fall_back_to_default(self):
        for bad in (None, 'x', ['C/G 5H'], ['C/G 5H', 'C/G 5H'], ['nope', 'ARGON 7D'], 5):
            self.assertEqual(normalize_primary(bad), DEFAULT_PRIMARY)
        self.assertEqual(normalize_primary(['argon 7d', 'gemini 5h']), ('ARGON 7D', 'GEMINI 5H'))

    def test_toggle_replaces_oldest_or_moves_to_top(self):
        self.assertEqual(toggle_primary(['C/G 5H', 'ARGON 7D'], 'GEMINI 7D'), ('ARGON 7D', 'GEMINI 7D'))
        self.assertEqual(toggle_primary(['C/G 5H', 'ARGON 7D'], 'ARGON 7D'), ('ARGON 7D', 'C/G 5H'))
        self.assertEqual(toggle_primary(['C/G 5H', 'ARGON 7D'], 'C/G 5H'), ('C/G 5H', 'ARGON 7D'))
        self.assertEqual(toggle_primary(['C/G 5H', 'ARGON 7D'], 'bogus'), ('C/G 5H', 'ARGON 7D'))

class AgyOrderTests(unittest.TestCase):
    def test_default_order_starts_with_the_default_pair(self):
        self.assertEqual(DEFAULT_ORDER[:2], DEFAULT_PRIMARY)
        self.assertEqual(sorted(DEFAULT_ORDER), sorted(AGY_ROW_TITLES))

    def test_normalize_order_accepts_full_orders_and_legacy_pairs(self):
        full = ['GEMINI 7D', 'C/G 7D', 'ARGON 7D', 'C/G 5H', 'GEMINI 5H']
        self.assertEqual(normalize_order(full), tuple(full))
        self.assertEqual(normalize_order(['ARGON 7D', 'GEMINI 5H'])[:2], ('ARGON 7D', 'GEMINI 5H'))
        self.assertEqual(len(normalize_order(['ARGON 7D', 'GEMINI 5H'])), 5)
        for bad in (None, 'x', [], ['C/G 5H'], full[:4] + ['bogus']):
            self.assertEqual(normalize_order(bad), DEFAULT_ORDER)

    def test_move_row(self):
        order = list(DEFAULT_ORDER)
        self.assertEqual(move_row(order, 'GEMINI 7D', 0)[0], 'GEMINI 7D')
        self.assertEqual(move_row(order, 'C/G 5H', 4)[-1], 'C/G 5H')
        self.assertEqual(sorted(move_row(order, 'ARGON 7D', 2)), sorted(AGY_ROW_TITLES))
        self.assertEqual(move_row(order, 'nope', 0), tuple(order))

    def test_arrange_uses_the_full_order(self):
        order = ['GEMINI 7D', 'C/G 7D', 'ARGON 7D', 'C/G 5H', 'GEMINI 5H']
        result = arrange_agy(agy_metrics(), order)
        self.assertEqual((result.metric1_title, result.metric2_title), ('GEMINI 7D', 'C/G 7D'))
        self.assertEqual([r.title for r in result.extra_metrics], ['ARGON 7D', 'C/G 5H', 'GEMINI 5H'])


if __name__ == '__main__':
    unittest.main()
