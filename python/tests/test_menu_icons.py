import hashlib
import re
import unittest
from pathlib import Path

PY_ROOT = Path(__file__).resolve().parents[1]
PY_MENU = PY_ROOT / "assets" / "menu"
RUST_ASSETS = PY_ROOT.parent / "rust" / "assets"


def _digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class MenuIconTests(unittest.TestCase):
    def test_every_icon_used_by_python_menus_exists(self):
        used = set(re.findall(r'menu_icon\("(\w+)"\)', (PY_ROOT / "ui" / "hud_window.py").read_text(encoding="utf-8")))
        self.assertTrue(used)
        for icon in used:
            self.assertTrue((PY_MENU / f"menu_{icon}.png").exists(), f"missing menu_{icon}.png")

    @unittest.skipUnless((RUST_ASSETS / "menu").is_dir(), "rust/ not present")
    def test_python_and_rust_menu_icons_are_identical(self):
        rust_menu = RUST_ASSETS / "menu"
        rust_files = {p.name for p in rust_menu.glob("*.png")}
        py_files = {p.name for p in PY_MENU.glob("*.png")}
        # Rust keeps the ghost icon at assets/ghost.png (also used by the HUD itself).
        py_files.discard("menu_ghost.png")
        self.assertEqual(rust_files, py_files, "icon sets differ between python/ and rust/")
        for name in sorted(rust_files):
            self.assertEqual(_digest(rust_menu / name), _digest(PY_MENU / name), f"{name} differs")
        self.assertEqual(_digest(RUST_ASSETS / "ghost.png"), _digest(PY_MENU / "menu_ghost.png"))


if __name__ == "__main__":
    unittest.main()
