"""Launches the real HUD with QTRS_STARTUP_TRACE and reports launch -> window-visible times."""
import subprocess, time, ctypes, os, sys
from ctypes import wintypes as W
u = ctypes.windll.user32
u.FindWindowW.restype = W.HWND
exe = os.environ.get("HUD_EXE") or os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "target", "release", "ClaudeHUD.exe"))
trace = sys.argv[1]
env = dict(os.environ, QTRS_STARTUP_TRACE=trace)
launch = time.time()
p = subprocess.Popen([exe], env=env)
seen = None
while time.time() - launch < 30:
    h = u.FindWindowW(None, "Claude HUD Monitor")
    if h and u.IsWindowVisible(h):
        seen = time.time(); break
    time.sleep(0.005)
print("launch_epoch_ms", int(launch * 1000))
print("window_visible_after_ms", None if seen is None else round((seen - launch) * 1000))
time.sleep(2)
p.kill()
