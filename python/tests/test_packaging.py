import tempfile
import unittest
from pathlib import Path

from PyInstaller.building.icon import normalize_icon_type


class PackagingTests(unittest.TestCase):
    def test_macos_png_icon_can_be_converted_to_icns(self):
        icon = Path(__file__).resolve().parents[1] / 'assets' / 'app_icon.png'
        with tempfile.TemporaryDirectory() as directory:
            converted = Path(normalize_icon_type(str(icon), ('icns',), 'icns', directory))
            self.assertEqual(converted.suffix, '.icns')
            self.assertEqual(converted.read_bytes()[:4], b'icns')
            self.assertGreater(converted.stat().st_size, 8)
