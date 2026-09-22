"""
Native frosted-glass backdrop:
- macOS: NSVisualEffectView (pyobjc)
- Windows: DWM System Backdrop (Acrylic / Mica) via dwmapi.dll / user32.dll
"""
import os
import sys
from ctypes import c_void_p, c_int, Structure, byref, sizeof

from core.logger import logger

# macOS constants
_BLENDING_BEHIND_WINDOW = 0
_STATE_ACTIVE = 1
_MATERIAL_POPOVER = 6
_AUTORESIZE_WIDTH_HEIGHT = 2 | 16


def is_supported() -> bool:
    if os.environ.get("QT_QPA_PLATFORM") == "offscreen":
        return False
    if sys.platform == "darwin":
        try:
            import AppKit  # noqa: F401
            return True
        except Exception:
            return False
    elif sys.platform == "win32":
        try:
            import ctypes
            _ = ctypes.windll.dwmapi
            return True
        except Exception:
            return False
    return False


def _apply_macos(widget, dark: bool, corner_radius: float) -> bool:
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
        logger.warning(f"[Vibrancy macOS] Native blur unavailable: {e}")
        return False


def _apply_windows(widget, dark: bool) -> bool:
    try:
        import ctypes

        win_id = int(widget.winId())
        if win_id == 0:
            return False

        dwmapi = ctypes.windll.dwmapi

        class MARGINS(Structure):
            _fields_ = [
                ("cxLeftWidth", c_int),
                ("cxRightWidth", c_int),
                ("cyTopHeight", c_int),
                ("cyBottomHeight", c_int),
            ]

        # 1. Extend DWM frame into entire client area
        margins = MARGINS(-1, -1, -1, -1)
        dwmapi.DwmExtendFrameIntoClientArea(win_id, byref(margins))

        # 2. Immersive Dark Mode for DWM (Attribute 20)
        dark_val = c_int(1 if dark else 0)
        dwmapi.DwmSetWindowAttribute(win_id, 20, byref(dark_val), sizeof(dark_val))

        # 3. Windows 11 22H2+ (Build >= 22621): Official System Backdrop (Attribute 38)
        build = getattr(sys.getwindowsversion(), "build", 0)
        if build >= 22621:
            # 3 = DWMSBT_TRANSIENTWINDOW (Acrylic blur), 2 = DWMSBT_MAINWINDOW (Mica)
            backdrop_type = c_int(3)
            hr = dwmapi.DwmSetWindowAttribute(win_id, 38, byref(backdrop_type), sizeof(backdrop_type))
            if hr == 0:
                return True

        # Fallback for Windows 10 (1803+) and Windows 11 < 22621
        user32 = ctypes.windll.user32
        class ACCENT_POLICY(Structure):
            _fields_ = [("AccentState", c_int), ("AccentFlags", c_int),
                        ("GradientColor", c_int), ("AnimationId", c_int)]
        class WINCOMPATTRDATA(Structure):
            _fields_ = [("Attribute", c_int), ("Data", c_void_p), ("SizeOfData", c_int)]

        # ACCENT_ENABLE_ACRYLICBLURBEHIND = 4, GradientColor = 0xAABBGGRR
        grad_color = 0x66202026 if dark else 0x66F0F0F5
        accent = ACCENT_POLICY(4, 0, grad_color, 0)
        data = WINCOMPATTRDATA(19, ctypes.cast(ctypes.pointer(accent), c_void_p), sizeof(accent))
        res = user32.SetWindowCompositionAttribute(win_id, byref(data))
        return bool(res)
    except Exception as e:
        logger.warning(f"[Vibrancy Windows] Native blur unavailable: {e}")
        return False


def apply(widget, dark: bool, corner_radius: float = 12.0) -> bool:
    """Install (or retune) native frosted-glass / Acrylic blur behind widget."""
    if not is_supported():
        return False
    if not widget.isVisible():
        return False

    if sys.platform == "darwin":
        return _apply_macos(widget, dark, corner_radius)
    elif sys.platform == "win32":
        return _apply_windows(widget, dark)

    return False
