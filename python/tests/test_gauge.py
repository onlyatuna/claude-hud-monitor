import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import unittest

from PySide6.QtGui import QColor
from PySide6.QtWidgets import QApplication

from core.providers.base import MetricRow, UsageMetrics
from ui.gauge import GaugeBar, SURFACES, contrast, ramp_color, ramp_stops, readable
from ui.provider_card import ProviderCardWidget

APP = QApplication.instance() or QApplication([])


class GaugeColourTests(unittest.TestCase):
    def test_stops_follow_the_status_bands(self):
        for dark in (True, False):
            self.assertEqual([p for p, _c in ramp_stops(dark)], [0, 50, 75, 90])

    def test_every_gauge_colour_reads_on_its_surfaces(self):
        for dark in (True, False):
            for value in range(0, 101, 5):
                color = ramp_color(value, dark)
                for surface in SURFACES[dark]:
                    self.assertGreaterEqual(contrast(color, surface), 3.0, f'{value}% dark={dark}')

    def test_colour_warms_up_as_usage_rises(self):
        for dark in (True, False):
            low, high = QColor(ramp_color(5, dark)), QColor(ramp_color(95, dark))
            self.assertGreater(high.red() - high.green(), low.red() - low.green())

    def test_readable_keeps_good_colours_and_only_moves_lightness(self):
        self.assertEqual(readable('#1f1f24', False), '#1f1f24')
        adjusted = readable('#E3C66A', False)              # pale yellow on a light surface
        self.assertNotEqual(adjusted, '#E3C66A')
        original, moved = QColor('#E3C66A'), QColor(adjusted)
        self.assertLess(abs(original.hslHueF() - moved.hslHueF()), 0.04)
        self.assertLess(moved.lightnessF(), original.lightnessF())


class GaugeBarTests(unittest.TestCase):
    def test_api_matches_what_the_cards_use(self):
        bar = GaugeBar()
        bar.setRange(0, 100)
        bar.setTextVisible(False)
        bar.setValue(40)
        self.assertEqual(bar.value(), 40)
        self.assertAlmostEqual(bar.fraction(), 0.4)
        bar.setValue(250)
        self.assertEqual(bar.fraction(), 1.0)
        bar.setValue(-3)
        self.assertEqual(bar.fraction(), 0.0)

    def test_paints_in_both_themes_and_when_empty(self):
        bar = GaugeBar()
        bar.resize(120, 5)
        for dark in (True, False):
            bar.set_dark(dark)
            for value in (0, 1, 50, 100):
                bar.setValue(value)
                self.assertFalse(bar.grab().isNull())

    def test_fill_is_revealed_gradient_not_one_flat_colour(self):
        bar = GaugeBar()
        bar.resize(200, 6)
        bar.setValue(100)
        image = bar.grab().toImage()
        left, right = QColor(image.pixel(8, 3)), QColor(image.pixel(192, 3))
        self.assertNotEqual(left.name(), right.name())
        self.assertGreater(right.red(), left.red())          # warmer on the right


class CardUsesGaugeTests(unittest.TestCase):
    def test_card_bars_are_gauges_and_numbers_match_the_leading_colour(self):
        card = ProviderCardWidget('agy')
        card.set_appearance(True)
        card.update_metrics(UsageMetrics(provider_id='agy', metric1_title='C/G 5H', metric1_val=82, metric1_text='82%',
                                         metric2_title='ARGON 7D', metric2_val=12, metric2_text='12%',
                                         extra_metrics=[MetricRow('GEMINI 5H', 55, '55%')]))
        self.assertIsInstance(card.m1_bar, GaugeBar)
        self.assertEqual(card.m1_bar.value(), 82)
        self.assertIn(ramp_color(82, True), card.m1_val.styleSheet())
        self.assertIn(ramp_color(12, True), card.m2_val.styleSheet())
        extra_value, extra_bar = card.extra_rows[0][2], card.extra_rows[0][3]
        self.assertIsInstance(extra_bar, GaugeBar)
        self.assertIn(ramp_color(55, True), extra_value.styleSheet())
        card.set_appearance(False)
        self.assertIn(ramp_color(82, False), card.m1_val.styleSheet())
        self.assertFalse(card.m1_bar.dark)


if __name__ == '__main__':
    unittest.main()
