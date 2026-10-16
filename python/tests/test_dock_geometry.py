import sys
import unittest

from ui.dock_geometry import (
    EDGES,
    SCALE_MAX,
    SCALE_MIN,
    SCALE_STEP,
    bottom_offset_from_rect,
    clamp_scale,
    constrain_to_edge,
    dock_rect,
    dock_to_edge,
    offset_along_edge,
    orientation_for_edge,
    pick_work_area,
    scale_size,
    snap_rect,
    step_scale,
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

    def test_snap_rect(self):
        wa = (0, 0, 1920, 1040)
        # right edge exceeded (-44) -> snap right, x=1856, y=300
        self.assertEqual(
            snap_rect((1900, 300, 64, 200), wa),
            ((1856, 300, 64, 200), "right"),
        )
        # left edge within threshold -> snap left, x=0
        self.assertEqual(
            snap_rect((10, 500, 64, 200), wa),
            ((0, 500, 64, 200), "left"),
        )
        # top edge within threshold -> snap top, y=0
        self.assertEqual(
            snap_rect((500, 8, 200, 64), wa),
            ((500, 0, 200, 64), "top"),
        )
        # bottom edge exceeded (-4) -> snap bottom, y=976
        self.assertEqual(
            snap_rect((500, 980, 200, 64), wa),
            ((500, 976, 200, 64), "bottom"),
        )
        # far from all edges -> (rect, None)
        self.assertEqual(
            snap_rect((800, 400, 64, 200), wa),
            ((800, 400, 64, 200), None),
        )
        # corner tie (left=5, top=5) -> priority left
        self.assertEqual(
            snap_rect((5, 5, 64, 200), wa),
            ((0, 5, 64, 200), "left"),
        )
        # far outside work area (x=-500, y=-100) -> snap left and clamp y inside
        self.assertEqual(
            snap_rect((-500, -100, 64, 200), wa),
            ((0, 0, 64, 200), "left"),
        )
        # secondary monitor offset work_area (1920, 0, 1920, 1080)
        wa_sec = (1920, 0, 1920, 1080)
        self.assertEqual(
            snap_rect((1930, 200, 64, 200), wa_sec),
            ((1920, 200, 64, 200), "left"),
        )
        self.assertEqual(
            snap_rect((3800, 200, 64, 200), wa_sec),
            ((3776, 200, 64, 200), "right"),
        )

    def test_orientation_for_edge(self):
        self.assertEqual(orientation_for_edge("left"), "vertical")
        self.assertEqual(orientation_for_edge("right", "horizontal"), "vertical")
        self.assertEqual(orientation_for_edge("top"), "horizontal")
        self.assertEqual(orientation_for_edge("bottom", "vertical"), "horizontal")
        self.assertEqual(orientation_for_edge(None), "vertical")
        self.assertEqual(orientation_for_edge(None, "horizontal"), "horizontal")
        self.assertEqual(orientation_for_edge("unknown", "horizontal"), "horizontal")
        self.assertEqual(orientation_for_edge("invalid"), "vertical")

    def test_dock_to_edge(self):
        wa = (0, 0, 1920, 1040)
        # right default (bottom-aligned with margin 12) matches dock_rect
        self.assertEqual(
            dock_to_edge(wa, "right", 92, 270),
            dock_rect(wa, 92, 270, 12),
        )
        # left with offset=100 -> (wa.x, wa.y+100, w, h)
        wa_offset = (100, 50, 1920, 1040)
        self.assertEqual(
            dock_to_edge(wa_offset, "left", 92, 270, offset=100),
            (100, 150, 92, 270),
        )
        # top default (right-aligned with margin 12)
        self.assertEqual(
            dock_to_edge(wa, "top", 270, 64),
            (1920 - 270 - 12, 0, 270, 64),
        )
        # bottom with oversized offset -> clamped to [0, ww - w]
        self.assertEqual(
            dock_to_edge(wa, "bottom", 270, 64, offset=99999),
            (1920 - 270, 1040 - 64, 270, 64),
        )
        # width/height exceeding work_area -> clamped to work_area size
        self.assertEqual(
            dock_to_edge(wa, "left", 5000, 5000),
            (0, 0, 1920, 1040),
        )
        # invalid edge raises ValueError
        with self.assertRaises(ValueError):
            dock_to_edge(wa, "center", 92, 270)

        # DPI scaling scenarios: (0,0,1536,816) and (0,0,1280,680) completely inside work_area
        for wa_scaled in ((0, 0, 1536, 816), (0, 0, 1280, 680)):
            for edge in EDGES:
                r = dock_to_edge(wa_scaled, edge, 92, 270)
                self.assertGreaterEqual(r[0], wa_scaled[0])
                self.assertGreaterEqual(r[1], wa_scaled[1])
                self.assertLessEqual(r[0] + r[2], wa_scaled[0] + wa_scaled[2])
                self.assertLessEqual(r[1] + r[3], wa_scaled[1] + wa_scaled[3])

    def test_offset_along_edge_round_trip(self):
        wa = (100, 50, 1920, 1040)
        for edge in EDGES:
            w, h = (92, 270) if edge in ("left", "right") else (270, 64)
            rect = dock_to_edge(wa, edge, w, h, offset=180)
            self.assertEqual(offset_along_edge(rect, wa, edge), 180)

        with self.assertRaises(ValueError):
            offset_along_edge((0, 0, 92, 270), wa, "invalid")

    def test_scale_helpers(self):
        # clamp_scale
        self.assertEqual(clamp_scale(0.3), 0.6)
        self.assertEqual(clamp_scale(5), 2.0)
        self.assertEqual(clamp_scale(1.04), 1.0)
        self.assertEqual(clamp_scale(1.06), 1.1)
        self.assertEqual(clamp_scale(float("nan")), 1.0)
        self.assertEqual(clamp_scale("x"), 1.0)  # type: ignore[arg-type]

        # step_scale
        self.assertEqual(step_scale(1.0, 3), 1.3)
        self.assertEqual(step_scale(0.6, -1), 0.6)
        self.assertEqual(step_scale(2.0, 1), 2.0)

        # scale_size
        self.assertEqual(scale_size(64, 68, 1.5), (96, 102))
        self.assertEqual(scale_size(64, 68, 0.1), (38, 41))


if __name__ == "__main__":
    unittest.main()

