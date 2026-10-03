"""Times ShowWindow(SW_SHOW) + the WM_ACTIVATE it triggers for a bare Win32 window (no qtrs code)."""
import ctypes, time, sys
from ctypes import wintypes as W
u = ctypes.windll.user32; k = ctypes.windll.kernel32
LRESULT = ctypes.c_ssize_t
WNDPROC = ctypes.WINFUNCTYPE(LRESULT, W.HWND, W.UINT, W.WPARAM, W.LPARAM)
u.DefWindowProcW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]; u.DefWindowProcW.restype = LRESULT
class WC(ctypes.Structure):
    _fields_ = [("cbSize", W.UINT), ("style", W.UINT), ("lpfnWndProc", WNDPROC), ("cbClsExtra", ctypes.c_int),
                ("cbWndExtra", ctypes.c_int), ("hInstance", W.HINSTANCE), ("hIcon", W.HANDLE), ("hCursor", W.HANDLE),
                ("hbrBackground", W.HANDLE), ("lpszMenuName", W.LPCWSTR), ("lpszClassName", W.LPCWSTR), ("hIconSm", W.HANDLE)]
spent = {}
def proc(h, m, w, l):
    t = time.perf_counter(); r = u.DefWindowProcW(h, m, w, l); d = (time.perf_counter() - t) * 1000
    if d > 3: spent[m] = d
    return r
cb = WNDPROC(proc)
layered = len(sys.argv) > 1 and sys.argv[1] == "layered"
wc = WC(); wc.cbSize = ctypes.sizeof(WC); wc.lpfnWndProc = cb; wc.lpszClassName = "sw_cost"
u.RegisterClassExW(ctypes.byref(wc))
ex = 0x80000 if layered else 0
u.CreateWindowExW.restype = W.HWND
h = u.CreateWindowExW(ex, "sw_cost", "x", 0x80000000, 100, 100, 600, 400, None, None, None, None)
t = time.perf_counter(); u.ShowWindow(h, 5); print("layered" if layered else "plain", "ShowWindow ms:", round((time.perf_counter() - t) * 1000, 1), {hex(k): round(v) for k, v in spent.items()})
