# -*- mode: python ; coding: utf-8 -*-
import sys
from pathlib import Path
from PyInstaller.building.datastruct import TOC

ROOT = Path(SPECPATH)
IS_MAC = sys.platform == "darwin"

# Hidden imports for platform-specific libraries
HIDDEN_IMPORTS = []
if IS_MAC:
    HIDDEN_IMPORTS.extend(["pynput.keyboard._darwin", "pynput.mouse._darwin"])

# Modules completely unused by ClaudeHUD (QWidget-only, standard urllib HTTP)
EXCLUDES = [
    "PySide6.QtNetwork",
    "PySide6.QtQml",
    "PySide6.QtQuick",
    "PySide6.QtPdf",
    "PySide6.QtOpenGL",
    "PySide6.QtSvg",
    "PySide6.QtVirtualKeyboard",
    "PySide6.QtTest",
    "PySide6.Qt3D",
    "PySide6.QtMultimedia",
    "PySide6.QtWebEngine",
    "PySide6.QtWebSockets",
]

EXCLUDED_MODULE_SUBSTRINGS = (
    "qtopengl", "qt6opengl",
    "qtpdf", "qt6pdf",
    "qtqml", "qt6qml",
    "qtquick", "qt6quick",
    "qtsvg", "qt6svg",
    "qtvirtualkeyboard", "qt6virtualkeyboard",
    "qtnetwork", "qt6network",
)

EXCLUDED_IMAGE_FORMATS = (
    "qgif", "qjpeg", "qpdf", "qsvg", "qtga", "qtiff", "qwbmp", "qwebp"
)


def keep_qt_file(entry):
    dest = entry[0].replace("\\", "/").lower()

    # Exclude translations (saves ~6.4MB on Windows, ~10MB on macOS)
    if "/translations/" in dest or dest.startswith("translations/") or dest.startswith("pyside6/translations/"):
        return False

    # Exclude software OpenGL fallback
    if "opengl32sw.dll" in dest:
        return False

    # Exclude unused Qt modules & frameworks
    for mod in EXCLUDED_MODULE_SUBSTRINGS:
        if mod in dest:
            return False

    # Exclude unneeded image formats (keep qico and qicns)
    if "/imageformats/" in dest:
        for fmt in EXCLUDED_IMAGE_FORMATS:
            if fmt in dest:
                return False

    # Platform plugins:
    # Windows requires qwindows; macOS requires qcocoa; headless smoke test requires qoffscreen.
    if "/platforms/" in dest:
        allowed = ("qwindows", "libqcocoa", "qcocoa", "qoffscreen", "libqoffscreen")
        if not any(a in dest for a in allowed):
            return False

    # Exclude other unused plugins
    if "qtvirtualkeyboardplugin" in dest or "qsvgicon" in dest:
        return False
    if "/tls/" in dest or "/networkinformation/" in dest:
        return False

    return True


a = Analysis(
    [str(ROOT / "main.py")],
    pathex=[str(ROOT)],
    binaries=[],
    datas=[(str(ROOT / "assets"), "assets")],
    hiddenimports=HIDDEN_IMPORTS,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=EXCLUDES,
    noarchive=False,
    optimize=0,
)
a.binaries = TOC(entry for entry in a.binaries if keep_qt_file(entry))
a.datas = TOC(entry for entry in a.datas if keep_qt_file(entry))

pyz = PYZ(a.pure)

if IS_MAC:
    from PyInstaller.building.osx import BUNDLE

    exe = EXE(
        pyz,
        a.scripts,
        [],
        exclude_binaries=True,
        name="ClaudeHUD",
        debug=False,
        bootloader_ignore_signals=False,
        strip=False,
        upx=True,
        console=False,
        disable_windowed_traceback=False,
        argv_emulation=False,
        target_arch=None,
        codesign_identity=None,
        entitlements_file=None,
        icon=[str(ROOT / "assets" / "app_icon.icns")],
    )
    coll = COLLECT(
        exe,
        a.binaries,
        a.datas,
        strip=False,
        upx=True,
        upx_exclude=[],
        name="ClaudeHUD",
    )
    app = BUNDLE(
        coll,
        name="ClaudeHUD.app",
        icon=str(ROOT / "assets" / "app_icon.icns"),
        bundle_identifier="com.claudehud.monitor",
        info_plist={
            "NSHighResolutionCapable": "True",
            "LSUIElement": "0",
        },
    )
else:
    exe = EXE(
        pyz,
        a.scripts,
        a.binaries,
        a.zipfiles,
        a.datas,
        [],
        name="ClaudeHUD",
        debug=False,
        bootloader_ignore_signals=False,
        strip=False,
        upx=True,
        upx_exclude=[],
        runtime_tmpdir=None,
        console=False,
        disable_windowed_traceback=False,
        argv_emulation=False,
        target_arch=None,
        codesign_identity=None,
        entitlements_file=None,
        icon=[str(ROOT / "assets" / "app_icon.ico")],
    )
