import unittest
from datetime import datetime, timedelta, timezone

from core import pace


def in_(**delta):
    return datetime.now(timezone.utc) + timedelta(**delta)


class PaceTests(unittest.TestCase):
    def test_countdown_formats(self):
        self.assertEqual(pace.format_countdown_hm(in_(hours=2, minutes=14, seconds=30)), '02:14')
        self.assertEqual(pace.format_countdown_dhm(in_(days=3, hours=5, minutes=41, seconds=30)), '03:05:41')
        self.assertEqual(pace.format_countdown_hm(None), '--:--')
        self.assertEqual(pace.format_countdown_dhm(None), '--:--:--')
        self.assertEqual(pace.format_countdown_hm(in_(minutes=-5)), '00:00')

    def test_reset_time_includes_weekday_only_when_asked(self):
        target = in_(days=2)
        self.assertRegex(pace.format_reset_time(target, with_day=False), r'^\d\d:\d\d$')
        self.assertRegex(pace.format_reset_time(target, with_day=True), r'^週[一二三四五六日] \d\d:\d\d$')

    def test_window_parsing(self):
        self.assertEqual(pace.window_seconds('SESSION 5H', 0), 5 * 3600)
        self.assertEqual(pace.window_seconds('WINDOW 30D', 0), 30 * 86400)
        self.assertEqual(pace.window_seconds('WINDOW 90M', 0), 90 * 60)
        self.assertEqual(pace.window_seconds('PRIMARY', 123), 123)
        self.assertEqual(pace.window_caption('WEEKLY 7D', '7D'), '')
        self.assertEqual(pace.window_caption('WINDOW 7D', '7D'), '')
        self.assertEqual(pace.window_caption('WINDOW 1D', '5H'), '1D')

    def test_pace_mark_needs_a_meaningful_elapsed_share(self):
        self.assertIsNone(pace.pace_mark(None))
        self.assertIsNone(pace.pace_mark(0.02))
        self.assertAlmostEqual(pace.pace_mark(0.5), 50)
        elapsed = pace.elapsed_fraction(in_(hours=2, minutes=30), pace.FIVE_HOURS)
        self.assertAlmostEqual(elapsed, 0.5, places=2)
        self.assertIsNone(pace.elapsed_fraction(None, pace.FIVE_HOURS))

    def test_scale_levels(self):
        self.assertIsNone(pace.scale_level(None))
        self.assertEqual([pace.scale_level(v) for v in (0, 49.9, 50, 74.9, 75, 89.9, 90, 100)],
                         ['green', 'green', 'yellow', 'yellow', 'orange', 'orange', 'red', 'red'])

    def test_runout_text(self):
        self.assertEqual(pace.runout_text(30, 0.5, pace.ONE_WEEK), '照目前速度，重設前不會用完')
        self.assertIn('用完', pace.runout_text(80, 0.5, pace.ONE_WEEK))
        self.assertEqual(pace.runout_text(80, 0.01, pace.ONE_WEEK), '')
        self.assertEqual(pace.runout_text(None, 0.5, pace.ONE_WEEK), '')
