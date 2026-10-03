"""Private/working-set bytes of the real HUD before and after the context menu loads the emoji font."""
import subprocess, time, ctypes, os
from ctypes import wintypes as W
u = ctypes.windll.user32; k = ctypes.windll.kernel32
u.FindWindowW.restype = W.HWND
ctypes.windll.shcore.SetProcessDpiAwareness(2)
class PMC(ctypes.Structure):
    _fields_ = [("cb", W.DWORD), ("PageFaultCount", W.DWORD), ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t), ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t), ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t), ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t)]
def mem(p):
    h = k.OpenProcess(0x1000 | 0x0400, False, p.pid)
    m = PMC(); m.cb = ctypes.sizeof(m)
    ctypes.windll.psapi.GetProcessMemoryInfo(h, ctypes.byref(m), m.cb)
    k.CloseHandle(h)
    return m.WorkingSetSize / 1048576, m.PagefileUsage / 1048576  # working set, private commit
exe = os.environ.get("HUD_EXE") or os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "target", "release", "ClaudeHUD.exe"))
p = subprocess.Popen([exe])
t0 = time.time(); h = 0
while time.time() - t0 < 30:
    h = u.FindWindowW(None, "Claude HUD Monitor")
    if h and u.IsWindowVisible(h): break
    time.sleep(0.05)
time.sleep(5)
print("idle, before menu : ws %.1f MB  private %.1f MB" % mem(p))
r = W.RECT(); u.GetWindowRect(h, ctypes.byref(r))
u.SetCursorPos((r.left + r.right) // 2, (r.top + r.bottom) // 2); time.sleep(0.2)
u.mouse_event(0x0008, 0, 0, 0, 0); time.sleep(0.03); u.mouse_event(0x0010, 0, 0, 0, 0)
time.sleep(2)
print("menu open         : ws %.1f MB  private %.1f MB" % mem(p))
u.mouse_event(0x0008, 0, 0, 0, 0); time.sleep(0.03); u.mouse_event(0x0010, 0, 0, 0, 0)
time.sleep(1.5)
print("menu closed       : ws %.1f MB  private %.1f MB" % mem(p))
p.kill()
