"""Shared menu icons.

The PNGs in assets/menu are the same files the Rust build embeds (rust/assets/menu), so the
context menu looks identical on both implementations. python/tests/test_menu_icons.py keeps the
two copies in sync.
"""
import os
import sys
from functools import lru_cache

from PySide6.QtGui import QIcon


def _menu_dir() -> str:
    if getattr(sys, "frozen", False) and hasattr(sys, "_MEIPASS"):
        return os.path.join(sys._MEIPASS, "assets", "menu")
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(base_dir, "assets", "menu")


@lru_cache(maxsize=None)
def menu_icon(name: str) -> QIcon:
    """Return the QIcon for `assets/menu/menu_<name>.png` (empty icon if the file is missing)."""
    path = os.path.join(_menu_dir(), f"menu_{name}.png")
    return QIcon(path) if os.path.exists(path) else QIcon()
