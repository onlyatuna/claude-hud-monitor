import unittest

from ui import vibrancy


class MaskPolicyTests(unittest.TestCase):
    def test_only_windows_with_native_blur_needs_the_rounded_mask(self):
        self.assertTrue(vibrancy.mask_needed(True, 'win32'))
        self.assertFalse(vibrancy.mask_needed(False, 'win32'))     # no blur: Qt already paints the rounded panel
        self.assertFalse(vibrancy.mask_needed(True, 'darwin'))     # macOS clips the effect view itself
        self.assertFalse(vibrancy.mask_needed(True, 'linux'))

    def test_suspend_is_a_noop_where_there_is_no_acrylic(self):
        self.assertFalse(vibrancy.suspend(object(), True))


if __name__ == '__main__':
    unittest.main()
