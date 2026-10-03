"""Drives the real HUD: drags its bottom-right then top-left corner, QTRS_RESIZE_DEBUG logs the result."""
import subprocess, time, ctypes, os, sys
from ctypes import wintypes as W
u = ctypes.windll.user32
ctypes.windll.shcore.SetProcessDpiAwareness(2)
u.FindWindowW.restype = W.HWND
exe = os.environ.get("HUD_EXE") or os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "target", "release", "ClaudeHUD.exe"))
log = sys.argv[1]
if os.path.exists(log): os.remove(log)
env = dict(os.environ, QTRS_RESIZE_DEBUG=log)
p = subprocess.Popen([exe], env=env)
h = 0
for _ in range(100):
    h = u.FindWindowW(None, "Claude HUD Monitor")
    if h: break
    time.sleep(0.2)
print("hwnd", h)
time.sleep(3)
# Same starting rectangle for every run (the HUD persists its last geometry between runs).
u.SetWindowPos(h, 0, 300, 200, 900, 560, 0x0004)  # SWP_NOZORDER
time.sleep(1.0)
r = W.RECT(); u.GetWindowRect(h, ctypes.byref(r)); print("rect", r.left, r.top, r.right, r.bottom)
def drag(corner, steps, delay):
    u.GetWindowRect(h, ctypes.byref(r))
    x, y = (r.right - 5, r.bottom - 5) if corner == "br" else (r.left + 5, r.top + 5)
    d = 1 if corner == "br" else -1
    u.SetCursorPos(x, y); time.sleep(0.4); u.mouse_event(2, 0, 0, 0, 0)
    for i in range(1, steps + 1):
        u.SetCursorPos(x + d * i * 6, y + d * i * 4); time.sleep(delay)
    time.sleep(0.2); u.mouse_event(4, 0, 0, 0, 0); time.sleep(0.8)
for c in ("br", "tl", "br", "tl"):
    drag(c, 50, 0.012)
u.GetWindowRect(h, ctypes.byref(r)); print("final", r.left, r.top, r.right, r.bottom)
p.kill()
