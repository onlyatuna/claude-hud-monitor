"""
Native frosted-glass backdrop (macOS NSVisualEffectView).

Qt can't blur what is behind a window, so on macOS the window's content view is
wrapped in an NSVisualEffectView and Qt keeps drawing (translucently) on top.
Everywhere else this is a no-op and the caller falls back to a more opaque panel.
"""
import os
import sys
from ctypes import c_void_p

from core.logger import logger

_BLENDING_BEHIND_WINDOW = 0
_STATE_ACTIVE = 1
_MATERIAL_POPOVER = 6
_AUTORESIZE_WIDTH_HEIGHT = 2 | 16


def is_supported() -> bool:
    if sys.platform != "darwin":
        return False
    if os.environ.get("QT_QPA_PLATFORM") == "offscreen":
        return False
    try:
        import AppKit  # noqa: F401  (pyobjc, pulled in by pynput on macOS)
        return True
    except Exception:
        return False


def apply(widget, dark: bool, corner_radius: float) -> bool:
    """Install (or retune) the blur behind a top-level widget. Returns True when active."""
    if not is_supported():
        return False
    if not widget.isVisible():
        return False
    try:
        import AppKit
        import objc

        win_id = int(widget.winId())
        if win_id == 0:
            return False

        view = objc.objc_object(c_void_p=win_id)
        window = view.window()
        if window is None:
            return False

        effect = window.contentView()
        if not isinstance(effect, AppKit.NSVisualEffectView):
            # Qt recreates the native window when flags change, so this runs again after that.
            qt_view = effect
            effect = AppKit.NSVisualEffectView.alloc().initWithFrame_(qt_view.frame())
            effect.setBlendingMode_(_BLENDING_BEHIND_WINDOW)
            effect.setState_(_STATE_ACTIVE)
            effect.setMaterial_(_MATERIAL_POPOVER)
            effect.setWantsLayer_(True)
            window.setContentView_(effect)
            effect.addSubview_(qt_view)
            qt_view.setFrame_(effect.bounds())
            qt_view.setAutoresizingMask_(_AUTORESIZE_WIDTH_HEIGHT)

        effect.layer().setCornerRadius_(corner_radius)
        effect.layer().setMasksToBounds_(True)
        name = AppKit.NSAppearanceNameDarkAqua if dark else AppKit.NSAppearanceNameAqua
        effect.setAppearance_(AppKit.NSAppearance.appearanceNamed_(name))
        return True
    except Exception as e:
        logger.warning(f"[Vibrancy] Native blur unavailable: {e}")
        return False
