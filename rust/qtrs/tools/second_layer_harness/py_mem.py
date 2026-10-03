"""Commit charge of a HUD process tree: idle, and after opening its context menu (right-click on its window).
Usage: python py_mem.py <cwd> <command...>   e.g.  python py_mem.py ../../python python main.py
"""
import subprocess, time, ctypes, sys
from ctypes import wintypes as W
u = ctypes.windll.user32; k = ctypes.windll.kernel32
ctypes.windll.shcore.SetProcessDpiAwareness(2)


class PMC(ctypes.Structure):
    _fields_ = [("cb", W.DWORD), ("PageFaultCount", W.DWORD), ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t), ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t), ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t), ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t)]


def mem(pid):
    h = k.OpenProcess(0x1000 | 0x0400, False, pid)
    if not h:
        return (0.0, 0.0)
    m = PMC(); m.cb = ctypes.sizeof(m)
    ctypes.windll.psapi.GetProcessMemoryInfo(h, ctypes.byref(m), m.cb); k.CloseHandle(h)
    return m.WorkingSetSize / 1048576, m.PagefileUsage / 1048576


cwd = sys.argv[1]
cmd = sys.argv[2:]
p = subprocess.Popen(cmd, cwd=cwd)
found = []


@ctypes.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
def cb(h, _):
    pid = W.DWORD(); u.GetWindowThreadProcessId(h, ctypes.byref(pid))
    if pid.value == p.pid and u.IsWindowVisible(h):
        r = W.RECT(); u.GetWindowRect(h, ctypes.byref(r))
        if r.right - r.left > 100:
            found.append((h, r.left, r.top, r.right, r.bottom))
    return True


t0 = time.time()
while time.time() - t0 < 40 and not found:
    u.EnumWindows(cb, 0); time.sleep(0.2)
print("window:", found[:1], "after %.1fs" % (time.time() - t0))
time.sleep(6)
print("idle : ws %.1f MB  commit %.1f MB" % mem(p.pid))
if found:
    _, l, t, r_, b = found[0]
    u.SetCursorPos((l + r_) // 2, (t + b) // 2); time.sleep(0.2)
    u.mouse_event(0x0008, 0, 0, 0, 0); time.sleep(0.03); u.mouse_event(0x0010, 0, 0, 0, 0)
    time.sleep(2)
    print("menu : ws %.1f MB  commit %.1f MB" % mem(p.pid))
    u.mouse_event(0x0008, 0, 0, 0, 0); time.sleep(0.03); u.mouse_event(0x0010, 0, 0, 0, 0)
    time.sleep(1)
p.kill()
