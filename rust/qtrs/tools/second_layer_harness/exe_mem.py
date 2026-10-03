"""Commit charge of a packaged HUD exe (all processes with that image name, e.g. a PyInstaller bootloader
+ child): idle, and after right-clicking its window to open the context menu.
Usage: python exe_mem.py <path-to-exe>      (no other process with the same image name may be running)
"""
import subprocess, time, ctypes, sys, os, csv, io
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


def pids(image):
    out = subprocess.run(["tasklist", "/fi", f"imagename eq {image}", "/fo", "csv", "/nh"],
                         capture_output=True, text=True, encoding="mbcs").stdout
    return [int(r[1]) for r in csv.reader(io.StringIO(out)) if len(r) > 1 and r[0].lower() == image.lower()]


def report(label, image):
    ps = pids(image)
    parts = [(pid, *mem(pid)) for pid in ps]
    print(f"{label}: " + "  ".join(f"pid{pid}: ws {ws:.1f} commit {c:.1f}" for pid, ws, c in parts))
    print(f"{label} TOTAL: ws {sum(x[1] for x in parts):.1f} MB  commit {sum(x[2] for x in parts):.1f} MB")


exe = os.path.abspath(sys.argv[1])
image = os.path.basename(exe)
p = subprocess.Popen([exe], cwd=os.path.dirname(exe))
found = []


@ctypes.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
def cb(h, _):
    pid = W.DWORD(); u.GetWindowThreadProcessId(h, ctypes.byref(pid))
    if pid.value in mine and u.IsWindowVisible(h):
        r = W.RECT(); u.GetWindowRect(h, ctypes.byref(r))
        if r.right - r.left > 100:
            found.append((h, r.left, r.top, r.right, r.bottom))
    return True


t0 = time.time()
while time.time() - t0 < 60 and not found:
    mine = set(pids(image))
    u.EnumWindows(cb, 0); time.sleep(0.3)
print("window:", found[:1], "after %.1fs" % (time.time() - t0))
time.sleep(6)
report("idle", image)
if found:
    _, l, t, r_, b = found[0]
    u.SetCursorPos((l + r_) // 2, (t + b) // 2); time.sleep(0.2)
    u.mouse_event(0x0008, 0, 0, 0, 0); time.sleep(0.03); u.mouse_event(0x0010, 0, 0, 0, 0)
    time.sleep(2)
    report("menu", image)
    u.mouse_event(0x0008, 0, 0, 0, 0); time.sleep(0.03); u.mouse_event(0x0010, 0, 0, 0, 0)
    time.sleep(1)
subprocess.run(["taskkill", "/f", "/t", "/im", image], capture_output=True)
