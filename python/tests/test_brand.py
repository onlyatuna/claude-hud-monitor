import unittest

from ui.brand import BRAND, brand_color
from ui.provider_card import PROVIDER_THEMES
from ui.sidebar import SIDEBAR_TOKENS, _Button


class BrandTests(unittest.TestCase):
    def test_every_service_has_a_colour_per_theme(self):
        self.assertEqual(set(BRAND), {'claude', 'agy', 'codex'})
        self.assertEqual(brand_color('claude', True), '#D97757')
        self.assertEqual(brand_color('claude', False), '#B4532F')
        self.assertEqual(PROVIDER_THEMES['codex']['color'], BRAND['codex']['dark'])
        self.assertEqual(SIDEBAR_TOKENS['light']['providers']['agy'], BRAND['agy']['light'])
        self.assertEqual(brand_color('unknown', True), '#38bdf8')

    def test_names_read_on_the_tile_background(self):
        for theme, dark in (('dark', True), ('light', False)):
            background = SIDEBAR_TOKENS[theme]['background']
            for provider in BRAND:
                ratio = _Button.contrast(brand_color(provider, dark), background)
                self.assertGreaterEqual(ratio, 4.5, f'{provider}/{theme}: {ratio:.2f}')

    def test_brand_colours_differ_from_each_other(self):
        for dark in (True, False):
            colors = {brand_color(p, dark) for p in BRAND}
            self.assertEqual(len(colors), 3)


if __name__ == '__main__':
    unittest.main()
