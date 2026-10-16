import sys
import unittest

from ui.dock_geometry import (
    bottom_offset_from_rect,
    constrain_to_edge,
    dock_rect,
    pick_work_area,
)


class TestDockGeometry(unittest.TestCase):
    def test_no_qt_imported(self):
        import ui.dock_geometry as mod

        with open(mod.__file__, encoding="utf-8") as f:
            source = f.read()
        self.assertNotIn("PySide6", source)
        self.assertNotIn("PyQt", source)

    def test_dock_rect_standard_1920x1040(self):
        wa = (0, 0, 1920, 1040)
        self.assertEqual(dock_rect(wa, 92, 270, 12), (1828, 758, 92, 270))
        self.assertEqual(dock_rect(wa, 92, 270), (1828, 758, 92, 270))

    def test_dock_rect_secondary_monitor_offset(self):
        wa = (1920, 0, 1920, 1080)
        rect = dock_rect(wa, 92, 270, 12)
        self.assertEqual(rect[0], 1920 + 1920 - 92)
        self.assertEqual(rect, (3748, 798, 92, 270))

    def test_dock_rect_clamps_oversized_dimensions_and_offset(self):
        wa = (100, 50, 800, 600)
        # height > work_area.h -> clamped to 600, y == work_area.y
        r_tall = dock_rect(wa, 92, 900, 12)
        self.assertEqual(r_tall, (100 + 800 - 92, 50, 92, 600))

        # width > work_area.w -> clamped to 800, x == work_area.x
        r_wide = dock_rect(wa, 1200, 200, 12)
        self.assertEqual(r_wide, (100, 50 + 600 - 12 - 200, 800, 200))

        # offset too large -> clamped so y does not go above top edge
        r_offset = dock_rect(wa, 92, 200, 9999)
        self.assertEqual(r_offset, (100 + 800 - 92, 50, 92, 200))

        # non-positive width/height and negative offset clamped to minimum valid
        r_min = dock_rect(wa, 0, -10, -5)
        self.assertEqual(r_min, (100 + 800 - 1, 50 + 600 - 1, 1, 1))

    def test_dock_rect_dpi_scaling_scenarios(self):
        # 125% scaling logical work area
        wa_125 = (0, 0, 1536, 816)
        r_125 = dock_rect(wa_125, 92, 270, 12)
        self.assertEqual(r_125, (1444, 534, 92, 270))
        self.assertEqual(r_125[0] + r_125[2], wa_125[0] + wa_125[2])
        self.assertGreaterEqual(r_125[0], wa_125[0])
        self.assertGreaterEqual(r_125[1], wa_125[1])
        self.assertLessEqual(r_125[1] + r_125[3], wa_125[1] + wa_125[3])

        # 150% scaling logical work area
        wa_150 = (0, 0, 1280, 680)
        r_150 = dock_rect(wa_150, 92, 270, 12)
        self.assertEqual(r_150, (1188, 398, 92, 270))
        self.assertEqual(r_150[0] + r_150[2], wa_150[0] + wa_150[2])
        self.assertGreaterEqual(r_150[0], wa_150[0])
        self.assertGreaterEqual(r_150[1], wa_150[1])
        self.assertLessEqual(r_150[1] + r_150[3], wa_150[1] + wa_150[3])

    def test_constrain_to_edge(self):
        wa = (0, 40, 1920, 1000)  # bottom edge = 1040
        # x in middle, y inside work area
        self.assertEqual(
            constrain_to_edge((500, 300, 92, 270), wa),
            (1828, 300, 92, 270),
        )
        # y above top edge
        self.assertEqual(
            constrain_to_edge((500, -20, 92, 270), wa),
            (1828, 40, 92, 270),
        )
        # y below bottom edge
        self.assertEqual(
            constrain_to_edge((500, 900, 92, 270), wa),
            (1828, 1040 - 270, 92, 270),
        )
        # w, h larger than work_area -> clamped to work_area size
        self.assertEqual(
            constrain_to_edge((10, 10, 3000, 2000), wa),
            (0, 40, 1920, 1000),
        )

    def test_bottom_offset_from_rect(self):
        wa = (0, 0, 1920, 1040)
        # normal offset
        self.assertEqual(bottom_offset_from_rect((1828, 758, 92, 270), wa), 12)
        # flush with bottom edge (0)
        self.assertEqual(bottom_offset_from_rect((1828, 770, 92, 270), wa), 0)
        # beyond bottom edge -> 0
        self.assertEqual(bottom_offset_from_rect((1828, 850, 92, 270), wa), 0)

    def test_pick_work_area(self):
        primary = (0, 0, 1920, 1040)
        secondary_right = (1920, 0, 1920, 1080)
        secondary_left = (-1920, 0, 1920, 1080)
        areas = [primary, secondary_right, secondary_left]

        # Point inside second screen
        self.assertEqual(pick_work_area((2500, 500), areas), secondary_right)

        # Point on negative-x left screen
        self.assertEqual(pick_work_area((-500, 500), areas), secondary_left)

        # Half-open boundary: right edge of primary (x=1920) belongs to secondary_right
        self.assertEqual(pick_work_area((1920, 500), areas), secondary_right)

        # Point in gap between two screens -> picks closest center
        gapped_areas = [(0, 0, 1920, 1040), (2100, 0, 1920, 1040)]
        # Center 0 is (960, 520), Center 1 is (3060, 520); x=1950 is closer to Center 0 (dist 990 vs 1110)
        self.assertEqual(pick_work_area((1950, 520), gapped_areas), gapped_areas[0])
        # x=2050 is closer to Center 1 (dist 1010 vs 1090)
        self.assertEqual(pick_work_area((2050, 520), gapped_areas), gapped_areas[1])

        # Empty list raises ValueError
        with self.assertRaises(ValueError):
            pick_work_area((100, 100), [])


if __name__ == "__main__":
    unittest.main()
