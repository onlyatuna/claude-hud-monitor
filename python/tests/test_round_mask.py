import math
import unittest

from ui.round_mask import rounded_rects


def covered(rects, x, y):
    return any(rx <= x < rx + rw and ry <= y < ry + rh for rx, ry, rw, rh in rects)


class RoundedRectTests(unittest.TestCase):
    def test_corners_are_cut_and_edges_and_centre_are_kept(self):
        w, h = 450, 350
        rects = rounded_rects(w, h, 12)
        for corner in ((0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)):
            self.assertFalse(covered(rects, *corner), corner)
        for kept in ((w // 2, 0), (w // 2, h - 1), (0, h // 2), (w - 1, h // 2), (w // 2, h // 2), (12, 12), (4, 4)):
            self.assertTrue(covered(rects, *kept), kept)
        self.assertFalse(covered(rects, 2, 2))

    def test_shape_is_symmetric(self):
        w, h = 200, 120
        rects = rounded_rects(w, h, 12)
        for x in range(0, 24):
            for y in range(0, 24):
                self.assertEqual(covered(rects, x, y), covered(rects, w - 1 - x, y))
                self.assertEqual(covered(rects, x, y), covered(rects, x, h - 1 - y))

    def test_rects_stay_inside_and_do_not_overlap(self):
        for w, h, r in ((450, 350, 12), (174, 62, 12), (100, 40, 40), (30, 30, 12)):
            rects = rounded_rects(w, h, r)
            cells = set()
            for rx, ry, rw, rh in rects:
                self.assertGreaterEqual(rx, 0)
                self.assertGreaterEqual(ry, 0)
                self.assertLessEqual(rx + rw, w)
                self.assertLessEqual(ry + rh, h)
                self.assertGreater(rw, 0)
                self.assertGreater(rh, 0)
                for y in range(ry, ry + rh):
                    for x in range(rx, rx + rw):
                        self.assertNotIn((x, y), cells)
                        cells.add((x, y))

    def test_area_matches_a_rounded_rectangle(self):
        w, h, r = 450, 350, 12
        area = sum(rw * rh for _x, _y, rw, rh in rounded_rects(w, h, r))
        expected = w * h - (4 - math.pi) * r * r
        self.assertAlmostEqual(area / expected, 1.0, delta=0.002)

    def test_radius_is_clamped_to_half_the_short_side(self):
        rects = rounded_rects(100, 40, 400)
        self.assertTrue(covered(rects, 50, 20))
        self.assertFalse(covered(rects, 0, 0))
        self.assertTrue(covered(rects, 50, 0))

    def test_zero_radius_is_the_plain_rectangle(self):
        self.assertEqual(rounded_rects(80, 50, 0), [(0, 0, 80, 50)])

    def test_degenerate_sizes_give_no_rects(self):
        self.assertEqual(rounded_rects(0, 50, 12), [])
        self.assertEqual(rounded_rects(50, 0, 12), [])


if __name__ == '__main__':
    unittest.main()
