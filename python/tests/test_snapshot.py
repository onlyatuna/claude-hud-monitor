import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtGui import QImage
from PySide6.QtWidgets import QApplication

from core import snapshot

APP = QApplication.instance() or QApplication([])
ROOT = Path(__file__).resolve().parents[1]
EXPECTED = ['menu_dark', 'sidebar_panel_horizontal_dark', 'sidebar_panel_vertical_dark', 'sidebar_panel_vertical_light',
            'sidebar_strip_horizontal_dark', 'sidebar_strip_vertical_dark', 'table_compact_dark', 'table_compact_min_light',
            'table_full_dark', 'table_full_light', 'table_states_dark']


def image(path):
    img = QImage(str(path))
    assert not img.isNull(), path
    return img


def distinct_colours(img, step=3):
    return len({img.pixel(x, y) for x in range(0, img.width(), step) for y in range(0, img.height(), step)})


class ArgumentTests(unittest.TestCase):
    def test_directory_is_optional(self):
        self.assertEqual(snapshot.parse_args(['main.py', '--snapshot']), Path('snapshots'))
        self.assertEqual(snapshot.parse_args(['main.py', '--snapshot', 'out']), Path('out'))
        self.assertEqual(snapshot.parse_args(['--snapshot', 'a b']), Path('a b'))

    def test_a_following_flag_is_not_a_directory(self):
        self.assertEqual(snapshot.parse_args(['main.py', '--snapshot', '--smoke-test']), Path('snapshots'))


class SnapshotRunTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory()
        cls.home = tempfile.TemporaryDirectory()
        cls.out = Path(cls.directory.name) / 'nested' / 'shots'
        with patch.dict(os.environ, {'APPDATA': cls.home.name, 'HOME': cls.home.name,
                                     'XDG_CONFIG_HOME': cls.home.name}):
            cls.paths = snapshot.run(cls.out)

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()
        cls.home.cleanup()

    def test_one_png_per_scenario_in_the_requested_directory(self):
        self.assertEqual(sorted(p.stem for p in self.paths), EXPECTED)
        for path in self.paths:
            self.assertEqual(path.parent, self.out)
            self.assertTrue(path.exists(), path)
            self.assertEqual(path.suffix, '.png')

    def test_images_are_real_renderings_not_blank(self):
        for path in self.paths:
            img = image(path)
            self.assertGreater(img.width(), 40, path.name)
            self.assertGreater(img.height(), 20, path.name)
            self.assertGreater(distinct_colours(img), 8, path.name)

    def test_sizes_match_the_style_and_state(self):
        by = {p.stem: image(p) for p in self.paths}
        self.assertEqual((by['table_full_dark'].width(), by['table_full_dark'].height()), (450, 350))
        self.assertEqual((by['table_compact_dark'].width(), by['table_compact_dark'].height()), (240, 96))
        self.assertLess(by['table_compact_min_light'].width(), 200)
        self.assertLess(by['table_compact_min_light'].height(), 80)
        self.assertLess(by['sidebar_strip_vertical_dark'].width(), by['sidebar_panel_vertical_dark'].width())
        self.assertGreater(by['sidebar_panel_horizontal_dark'].width(), by['sidebar_panel_horizontal_dark'].height())
        self.assertGreater(by['sidebar_strip_horizontal_dark'].width(), by['sidebar_strip_horizontal_dark'].height())

    def test_light_and_dark_really_differ(self):
        by = {p.stem: image(p) for p in self.paths}
        a, b = by['table_full_dark'], by['table_full_light']
        self.assertEqual((a.width(), a.height()), (b.width(), b.height()))
        self.assertNotEqual(a.pixel(a.width() // 2, 12), b.pixel(b.width() // 2, 12))

    def test_the_users_own_settings_are_never_touched(self):
        self.assertEqual(list(Path(self.home.name).rglob('config.json')), [])
        self.assertEqual(list(Path(self.home.name).rglob('*.log')), [])

    def test_running_again_overwrites_without_error(self):
        with tempfile.TemporaryDirectory() as out:
            first = snapshot.run(Path(out))
            second = snapshot.run(Path(out))
            self.assertEqual([p.name for p in first], [p.name for p in second])


class CommandLineTests(unittest.TestCase):
    def test_main_py_snapshot_flag_writes_the_files_and_exits_cleanly(self):
        with tempfile.TemporaryDirectory() as home, tempfile.TemporaryDirectory() as out:
            env = dict(os.environ, QT_QPA_PLATFORM='offscreen', HOME=home, XDG_CONFIG_HOME=home, APPDATA=home)
            result = subprocess.run([sys.executable, '-B', str(ROOT / 'main.py'), '--snapshot', out], cwd=str(ROOT),
                                    env=env, capture_output=True, text=True, timeout=120)
            self.assertEqual(result.returncode, 0, result.stderr[-800:])
            self.assertEqual(sorted(p.stem for p in Path(out).glob('*.png')), EXPECTED)
            self.assertIn('table_full_dark.png', result.stdout)


if __name__ == '__main__':
    unittest.main()
