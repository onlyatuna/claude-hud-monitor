"""Pure Win32 WS_EX_LAYERED frameless harness mimicking Qt 6.8 sequencing.

Usage: python w32_layered.py <variant A-F>
A Qt-like baseline: backing image = DIB, SWP_NOCOPYBITS on resize, WM_SIZE only
  snapshots geometry, ULW runs from a posted message, pptDst/psize from the same
  snapshot, hdcDst=NULL, ULW_ALPHA only.
B no SWP_NOCOPYBITS   C ULW synchronously inside WM_SIZE   D hdcDst=screen DC
E + ULW_EX_NORESIZE   F pptDst=live GetWindowRect, psize=image size
"""
import sys, ctypes
from ctypes import wintypes as W

VARIANT = sys.argv[1].upper() if len(sys.argv) > 1 else "A"
TITLE = "SLTEST"
u32, g32 = ctypes.windll.user32, ctypes.windll.gdi32
ctypes.windll.shcore.SetProcessDpiAwareness(2)

LRESULT = ctypes.c_ssize_t
WNDPROC = ctypes.WINFUNCTYPE(LRESULT, W.HWND, W.UINT, W.WPARAM, W.LPARAM)
u32.DefWindowProcW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
u32.DefWindowProcW.restype = LRESULT
u32.CreateWindowExW.restype = W.HWND
u32.CreateWindowExW.argtypes = [W.DWORD, W.LPCWSTR, W.LPCWSTR, W.DWORD, ctypes.c_int, ctypes.c_int,
                                ctypes.c_int, ctypes.c_int, W.HWND, W.HMENU, W.HINSTANCE, W.LPVOID]
u32.GetDC.restype = W.HDC
u32.GetDC.argtypes = [W.HWND]
g32.CreateCompatibleDC.argtypes = [W.HDC]
u32.GetWindowRect.argtypes = [W.HWND, ctypes.POINTER(W.RECT)]
u32.UpdateLayeredWindowIndirect.argtypes = [W.HWND, ctypes.c_void_p]
u32.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
u32.ShowWindow.argtypes = [W.HWND, ctypes.c_int]
u32.LoadCursorW.restype = W.HANDLE
u32.LoadCursorW.argtypes = [W.HINSTANCE, ctypes.c_void_p]
u32.GetMessageW.argtypes = [ctypes.POINTER(W.MSG), W.HWND, W.UINT, W.UINT]
u32.TranslateMessage.argtypes = [ctypes.POINTER(W.MSG)]
u32.DispatchMessageW.argtypes = [ctypes.POINTER(W.MSG)]
g32.CreateCompatibleDC.restype = W.HDC
g32.SelectObject.argtypes = [W.HDC, W.HGDIOBJ]
g32.SelectObject.restype = W.HGDIOBJ
g32.DeleteObject.argtypes = [W.HGDIOBJ]
g32.DeleteDC.argtypes = [W.HDC]
g32.CreateDIBSection.restype = W.HBITMAP
g32.CreateDIBSection.argtypes = [W.HDC, ctypes.c_void_p, W.UINT, ctypes.POINTER(ctypes.c_void_p), W.HANDLE, W.DWORD]

WM_SIZE, WM_MOVE, WM_DESTROY, WM_NCCALCSIZE, WM_NCHITTEST = 5, 3, 2, 0x83, 0x84
WM_WINDOWPOSCHANGING, WM_ERASEBKGND, WM_APP = 0x46, 0x14, 0x8001
SWP_NOSIZE, SWP_NOCOPYBITS = 1, 0x100
ULW_ALPHA, ULW_EX_NORESIZE = 2, 8


class WINDOWPOS(ctypes.Structure):
    _fields_ = [("hwnd", W.HWND), ("after", W.HWND), ("x", ctypes.c_int), ("y", ctypes.c_int),
                ("cx", ctypes.c_int), ("cy", ctypes.c_int), ("flags", W.UINT)]


class BLEND(ctypes.Structure):
    _fields_ = [("op", ctypes.c_ubyte), ("fl", ctypes.c_ubyte), ("sca", ctypes.c_ubyte), ("ab", ctypes.c_ubyte)]


class ULWI(ctypes.Structure):
    _fields_ = [("cbSize", W.DWORD), ("hdcDst", W.HDC), ("pptDst", ctypes.POINTER(W.POINT)),
                ("psize", ctypes.POINTER(W.SIZE)), ("hdcSrc", W.HDC), ("pptSrc", ctypes.POINTER(W.POINT)),
                ("crKey", W.COLORREF), ("pblend", ctypes.POINTER(BLEND)), ("dwFlags", W.DWORD),
                ("prcDirty", ctypes.POINTER(W.RECT))]


class BMI(ctypes.Structure):
    _fields_ = [("biSize", W.DWORD), ("w", ctypes.c_long), ("h", ctypes.c_long), ("planes", W.WORD),
                ("bpp", W.WORD), ("comp", W.DWORD), ("sz", W.DWORD), ("xp", ctypes.c_long),
                ("yp", ctypes.c_long), ("cu", W.DWORD), ("ci", W.DWORD)]


st = {"snap": (0, 0, 0, 0), "dib": None, "dc": None, "old": None, "w": 0, "h": 0, "bits": None, "fails": 0, "n": 0}
screen_dc = u32.GetDC(None)


def alloc(w, h):
    if st["dib"]:
        g32.SelectObject(st["dc"], st["old"]); g32.DeleteObject(st["dib"])
    else:
        st["dc"] = g32.CreateCompatibleDC(screen_dc)
    bmi = BMI(ctypes.sizeof(BMI), w, -h, 1, 32, 0, 0, 0, 0, 0, 0)
    p = ctypes.c_void_p()
    st["dib"] = g32.CreateDIBSection(screen_dc, ctypes.byref(bmi), 0, ctypes.byref(p), None, 0)
    st["old"] = g32.SelectObject(st["dc"], st["dib"])
    st["w"], st["h"], st["bits"] = w, h, p.value
    # premultiplied BGRA, opaque magenta with 24px rounded corners
    import numpy as np
    yy, xx = np.mgrid[0:h, 0:w]
    r = 24
    cx = np.clip(xx, r, w - 1 - r); cy = np.clip(yy, r, h - 1 - r)
    d = np.hypot(xx - cx, yy - cy)
    a = np.clip(r + 0.5 - d, 0, 1)
    px = np.zeros((h, w, 4), np.uint8)
    px[..., 0] = (255 * a).astype(np.uint8)  # B
    px[..., 2] = (255 * a).astype(np.uint8)  # R
    px[..., 3] = (255 * a).astype(np.uint8)
    ctypes.memmove(p.value, px.ctypes.data, px.nbytes)


def ulw(hwnd):
    x, y, w, h = st.get("prev", st["snap"]) if VARIANT == "Y" else st["snap"]
    if w <= 0 or h <= 0:
        return
    if (w, h) != (st["w"], st["h"]):
        alloc(w, h)
    if VARIANT == "F":
        rc = W.RECT(); u32.GetWindowRect(hwnd, ctypes.byref(rc))
        x, y = rc.left, rc.top
        w, h = st["w"], st["h"]
    pt, sz, src = W.POINT(x, y), W.SIZE(w, h), W.POINT(0, 0)
    bl = BLEND(0, 0, 255, 1)
    dirty = W.RECT(0, 0, w, h)
    flags = ULW_ALPHA | (ULW_EX_NORESIZE if VARIANT == "E" else 0)
    info = ULWI(ctypes.sizeof(ULWI), screen_dc if VARIANT == "D" else None, ctypes.pointer(pt),
                ctypes.pointer(sz), st["dc"], ctypes.pointer(src), 0, ctypes.pointer(bl), flags,
                ctypes.pointer(dirty))
    st["n"] += 1
    if not u32.UpdateLayeredWindowIndirect(hwnd, ctypes.byref(info)):
        st["fails"] += 1


def snapshot(hwnd):
    st.setdefault("hist", []).append(st["snap"]); st["prev"] = st["hist"][-40] if len(st["hist"]) >= 40 else st["hist"][0]
    rc = W.RECT(); u32.GetWindowRect(hwnd, ctypes.byref(rc))
    st["snap"] = (rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top)


@WNDPROC
def wndproc(hwnd, msg, wp, lp):
    if msg == WM_NCCALCSIZE:
        return 0
    if msg == WM_NCHITTEST:
        sx, sy = ctypes.c_short(lp & 0xFFFF).value, ctypes.c_short((lp >> 16) & 0xFFFF).value
        rc = W.RECT(); u32.GetWindowRect(hwnd, ctypes.byref(rc))
        m = 20
        l, t = sx - rc.left < m, sy - rc.top < m
        r, b = rc.right - sx < m, rc.bottom - sy < m
        if l and t: return 13
        if r and b: return 17
        if r and t: return 14
        if l and b: return 16
        return 1
    if msg == WM_ERASEBKGND:
        return 1
    if msg == WM_WINDOWPOSCHANGING and VARIANT != "B":
        wpos = WINDOWPOS.from_address(lp)
        if not wpos.flags & SWP_NOSIZE:
            rc = W.RECT(); u32.GetWindowRect(hwnd, ctypes.byref(rc))
            if wpos.cx != rc.right - rc.left or wpos.cy != rc.bottom - rc.top:
                wpos.flags |= SWP_NOCOPYBITS
        return 0
    if msg in (WM_SIZE, WM_MOVE):
        snapshot(hwnd)
        if VARIANT == "Z":  # positive control: never refresh
            return 0
        if VARIANT == "C":
            ulw(hwnd)
        elif msg == WM_SIZE:
            u32.PostMessageW(hwnd, WM_APP, 0, 0)
        else:
            u32.PostMessageW(hwnd, WM_APP, 0, 0)
        return 0
    if msg == WM_APP:
        ulw(hwnd)
        return 0
    if msg == WM_DESTROY:
        u32.PostQuitMessage(0)
        return 0
    return u32.DefWindowProcW(hwnd, msg, wp, lp)


class WNDCLASS(ctypes.Structure):
    _fields_ = [("style", W.UINT), ("proc", WNDPROC), ("cb", ctypes.c_int), ("cw", ctypes.c_int),
                ("inst", W.HINSTANCE), ("icon", W.HICON), ("cur", W.HANDLE), ("bg", W.HBRUSH),
                ("menu", W.LPCWSTR), ("name", W.LPCWSTR)]


wc = WNDCLASS(0, wndproc, 0, 0, None, None, u32.LoadCursorW(None, 32512), None, None, "SLW32")
u32.RegisterClassW(ctypes.byref(wc))
hwnd = u32.CreateWindowExW(0x80000 | 8, "SLW32", TITLE, 0x80000000 | 0x40000, 200, 150, 500, 400,
                           None, None, None, None)
snapshot(hwnd)
alloc(500, 400)
ulw(hwnd)
u32.ShowWindow(hwnd, 4)
msg = W.MSG()
while u32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
    u32.TranslateMessage(ctypes.byref(msg)); u32.DispatchMessageW(ctypes.byref(msg))
print(f"variant={VARIANT} ulw_calls={st['n']} ulw_fail={st['fails']}")
