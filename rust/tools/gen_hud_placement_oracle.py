"""Generates rust/src/ui/placement_oracle.txt by running the REAL Python HUD functions
`HUDWindow._ensure_within_screen`, `HUDWindow._restore_or_default_position` and 
`HUDWindow._reset_geometry` (python/ui/hud_window.py) against fake screens.

Only the screen list is faked (QGuiApplication.screens / primaryScreen / the window's screen());
the placement logic under test is the unmodified Python code, so the file is an independent oracle
for the Rust port in rust/src/ui/placement.rs.

    python rust/tools/gen_hud_placement_oracle.py   (from the repo root; needs PySide6)

Line formats (all rects are x,y,w,h; screens are available geometries, ';'-separated):
    E screens=<rects> primary=<idx|-1> winscreen=<idx|-1> geom=<rect> => <x>,<y>
    R screens=<rects> primary=<idx|-1> saved=<x>,<y>|none size=<w>,<h> => <x>,<y>
    X primary=<rect> mode=<table|horizontal|vertical> => <w>,<h>,<x>,<y>   (_reset_geometry)
"""
import itertools
import os
import random
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "python"))

from PySide6.QtCore import QRect  # noqa: E402

import ui.hud_window as hw  # noqa: E402


class FakeScreen:
    def __init__(self, rect):
        self._r = QRect(*rect)

    def availableGeometry(self):
        return QRect(self._r)


class FakeQGuiApplication:
    _screens = []
    _primary = None

    @staticmethod
    def screens():
        return list(FakeQGuiApplication._screens)

    @staticmethod
    def primaryScreen():
        return FakeQGuiApplication._primary


hw.QGuiApplication = FakeQGuiApplication


class FakeWindow:
    def __init__(self, geom, win_screen):
        self._g = QRect(*geom)
        self._ws = win_screen
        self.moved = None

    def geometry(self):
        return QRect(self._g)

    def x(self):
        return self._g.x()

    def y(self):
        return self._g.y()

    def screen(self):
        return self._ws

    def move(self, x, y):
        self.moved = (x, y)


def rects_str(rs):
    return ";".join(",".join(map(str, r)) for r in rs)


def setup(layout, primary_idx):
    FakeQGuiApplication._screens = [FakeScreen(r) for r in layout]
    FakeQGuiApplication._primary = FakeScreen(layout[primary_idx]) if primary_idx >= 0 else None
    return FakeQGuiApplication._screens


LAYOUTS = {
    "single": [(0, 0, 1920, 1040)],
    "dual_right": [(0, 0, 1920, 1040), (1920, 0, 1920, 1080)],
    "dual_left": [(0, 0, 1920, 1040), (-1920, 0, 1920, 1080)],
    "stacked_above": [(0, 0, 1920, 1040), (0, -1080, 1920, 1080)],
    "small": [(0, 0, 800, 560)],
    "offset_work_area": [(0, 40, 1920, 1000)],
}
SIZES = [(400, 300), (2500, 1500), (50, 30)]


def positions(layout):
    xs, ys = set(), set()
    for (x, y, w, h) in layout:
        for dx in (-500, -399, -1, 0, 10, w // 2 - 200, w - 400, w - 399, w - 60, w - 1, w):
            xs.add(x + dx)
        for dy in (-400, -299, -1, 0, 10, h // 2 - 150, h - 300, h - 299, h - 40, h - 1, h):
            ys.add(y + dy)
    return sorted(xs), sorted(ys)


# The full grid is ~23k cases (2 MB); a fixed-seed sample keeps the boundary-heavy grid but not the size.
KEEP = 0.04


def centre_edge_cases(layout, w, h):
    """Window positions whose centre is within 1 px of a screen edge (not sampled away), where
    QRect's inclusive centre/contains differ from an x + w/2, half-open reading."""
    xs, ys = set(), set()
    for (x, y, sw, sh) in layout:
        for edge in (x, x + sw):
            xs.update(edge - w // 2 + k for k in range(-1, 2))
            xs.update(edge - (w - 1) // 2 + k for k in range(-1, 2))
        for edge in (y, y + sh):
            ys.update(edge - h // 2 + k for k in range(-1, 2))
            ys.update(edge - (h - 1) // 2 + k for k in range(-1, 2))
    mid_x = [layout[0][0] + 100, layout[-1][0] + 100]
    mid_y = [layout[0][1] + 100, layout[-1][1] + 100]
    return [(x, y) for x in sorted(xs) for y in mid_y] + [(x, y) for x in mid_x for y in sorted(ys)]


def main():
    rng = random.Random(20240607)
    out = []
    for name, layout in LAYOUTS.items():
        xs, ys = positions(layout)
        primaries = sorted({0, len(layout) - 1})
        for primary_idx in primaries:
            for (w, h) in SIZES:
                edge_cases = set(centre_edge_cases(layout, w, h))
                for x, y in itertools.chain(itertools.product(xs, ys), sorted(edge_cases)):
                    if (x, y) not in edge_cases and rng.random() > KEEP:
                        continue
                    # E: window screen is None, the primary, or the last screen.
                    for ws_idx in sorted({-1, 0, len(layout) - 1}):
                        screens = setup(layout, primary_idx)
                        win = FakeWindow((x, y, w, h), screens[ws_idx] if ws_idx >= 0 else None)
                        hw.HUDWindow._ensure_within_screen(win, w, h)
                        rx, ry = win.moved if win.moved else (x, y)
                        out.append(
                            f"E screens={rects_str(layout)} primary={primary_idx} winscreen={ws_idx} "
                            f"geom={x},{y},{w},{h} => {rx},{ry}"
                        )
                    # R: restore.
                    screens = setup(layout, primary_idx)
                    win = FakeWindow((0, 0, w, h), None)
                    hw.HUDWindow._restore_or_default_position(win, x, y, w, h)
                    out.append(
                        f"R screens={rects_str(layout)} primary={primary_idx} saved={x},{y} "
                        f"size={w},{h} => {win.moved[0]},{win.moved[1]}"
                    )
        # R with nothing saved, and with no primary screen.
        for (w, h) in SIZES:
            for primary_idx in sorted({0, len(layout) - 1}):
                screens = setup(layout, primary_idx)
                win = FakeWindow((0, 0, w, h), None)
                hw.HUDWindow._restore_or_default_position(win, None, None, w, h)
                out.append(
                    f"R screens={rects_str(layout)} primary={primary_idx} saved=none size={w},{h} "
                    f"=> {win.moved[0]},{win.moved[1]}"
                )
            screens = setup(layout, -1)
            win = FakeWindow((0, 0, w, h), None)
            hw.HUDWindow._restore_or_default_position(win, None, None, w, h)
            out.append(
                f"R screens={rects_str(layout)} primary=-1 saved=none size={w},{h} => {win.moved[0]},{win.moved[1]}"
            )
    # X: the real `_reset_geometry`, with a fake `self` that records resize/move.
    class FakeResetWindow:
        def __init__(self, mode):
            ui, layout = ("table", "horizontal") if mode == "table" else ("cards", mode)
            self.config = {"ui_mode": ui, "layout_mode": layout}
            self.size = None
            self.moved = None

        def resize(self, w, h):
            self.size = (w, h)

        def move(self, x, y):
            self.moved = (x, y)

        def _persist_geometry(self):
            pass

    class Cfg(dict):
        def get(self, k, d=None):
            return dict.get(self, k, d)

    for layout in LAYOUTS.values():
        for primary in layout:
            for mode in ("table", "horizontal", "vertical"):
                FakeQGuiApplication._primary = FakeScreen(primary)
                fw = FakeResetWindow(mode)
                fw.config = Cfg(fw.config)
                hw.HUDWindow._reset_geometry(fw)
                out.append(
                    f"X primary={','.join(map(str, primary))} mode={mode} => "
                    f"{fw.size[0]},{fw.size[1]},{fw.moved[0]},{fw.moved[1]}"
                )
    path = os.path.join(ROOT, "rust", "src", "ui", "placement_oracle.txt")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(out) + "\n")
    print(len(out), "cases ->", path)


if __name__ == "__main__":
    main()
