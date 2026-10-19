import unittest

from ui import table_layout as tl


class TableStateTests(unittest.TestCase):
    def test_full_size_threshold_is_the_old_minimum(self):
        self.assertEqual((tl.FULL_MIN_W, tl.FULL_MIN_H), (380, 280))

    def test_collapses_below_either_dimension(self):
        self.assertFalse(tl.table_state(380, 280, False))
        self.assertTrue(tl.table_state(379, 400, False))
        self.assertTrue(tl.table_state(600, 279, False))
        self.assertTrue(tl.table_state(200, 90, False))

    def test_full_window_stays_full_above_the_threshold(self):
        self.assertFalse(tl.table_state(450, 350, False))
        self.assertFalse(tl.table_state(380, 280, False))

    def test_compact_window_needs_the_hysteresis_margin_to_expand(self):
        margin = tl.HYSTERESIS
        self.assertEqual(margin, 16)
        self.assertTrue(tl.table_state(380, 280, True))
        self.assertTrue(tl.table_state(380 + margin - 1, 280 + margin, True))
        self.assertTrue(tl.table_state(380 + margin, 280 + margin - 1, True))
        self.assertFalse(tl.table_state(380 + margin, 280 + margin, True))
        self.assertFalse(tl.table_state(800, 600, True))

    def test_no_flicker_while_resizing_across_the_threshold(self):
        state = False
        seen = []
        for width in list(range(420, 360, -4)) + list(range(360, 420, 4)):
            state = tl.table_state(width, 400, state)
            seen.append(state)
        flips = sum(1 for a, b in zip(seen, seen[1:]) if a != b)
        self.assertEqual(flips, 2)

    def test_compact_minimum_size_is_small_but_holds_three_dials(self):
        w, h = tl.compact_min_size(3)
        self.assertLess(w, tl.FULL_MIN_W)
        self.assertLess(h, tl.FULL_MIN_H)
        self.assertGreaterEqual(w, 3 * tl.COMPACT_DIAL_MIN)
        self.assertGreaterEqual(h, tl.COMPACT_DIAL_MIN)
        self.assertGreater(tl.compact_min_size(4)[0], w)

    def test_default_compact_size_is_not_below_the_minimum(self):
        w, h = tl.compact_min_size(3)
        self.assertGreaterEqual(tl.DEF_COMPACT_W, w)
        self.assertGreaterEqual(tl.DEF_COMPACT_H, h)


if __name__ == '__main__':
    unittest.main()
