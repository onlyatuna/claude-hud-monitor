"""Soak gate: run ClaudeHUD.exe for hours and fail if memory or handle counts keep growing.

Windows only (ctypes, no third-party packages).

    python tools/soak.py --exe target/release/ClaudeHUD.exe --hours 24

Samples private bytes, process handles, GDI objects and USER objects every --interval seconds
into a CSV. Verdict: the median of the last hour must not exceed the median of the first hour
after the 10-minute warm-up by more than the limits below, and the process must not have died
or restarted (a panic-guard restart changes the PID and counts as a failure).
"""

import argparse
import csv
import ctypes
import statistics
import subprocess
import sys
import time
from ctypes import wintypes

CREATE_NO_WINDOW = 0x08000000
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
GR_GDIOBJECTS, GR_USEROBJECTS = 0, 1
WARMUP_S = 600
WINDOW_S = 3600
LIMITS = {"private_mb": 20.0, "handles": 10, "gdi": 20, "user": 20}


class PMC(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD),
        ("PageFaultCount", wintypes.DWORD),
        ("PeakWorkingSetSize", ctypes.c_size_t),
        ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t),
        ("PeakPagefileUsage", ctypes.c_size_t),
        ("PrivateUsage", ctypes.c_size_t),
    ]


k32 = ctypes.WinDLL("kernel32", use_last_error=True)
u32 = ctypes.WinDLL("user32", use_last_error=True)
psapi = ctypes.WinDLL("psapi", use_last_error=True)
k32.OpenProcess.restype = wintypes.HANDLE
k32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
k32.CloseHandle.argtypes = [wintypes.HANDLE]
k32.GetProcessHandleCount.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
u32.GetGuiResources.restype = wintypes.DWORD
u32.GetGuiResources.argtypes = [wintypes.HANDLE, wintypes.DWORD]
psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(PMC), wintypes.DWORD]


def sample(pid):
    h = k32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not h:
        return None
    try:
        pmc = PMC()
        pmc.cb = ctypes.sizeof(pmc)
        if not psapi.GetProcessMemoryInfo(h, ctypes.byref(pmc), pmc.cb):
            return None
        handles = wintypes.DWORD()
        k32.GetProcessHandleCount(h, ctypes.byref(handles))
        return {
            "private_mb": pmc.PrivateUsage / 1048576,
            "handles": handles.value,
            "gdi": u32.GetGuiResources(h, GR_GDIOBJECTS),
            "user": u32.GetGuiResources(h, GR_USEROBJECTS),
        }
    finally:
        k32.CloseHandle(h)


def verdict(rows):
    """rows: list of (elapsed_s, metrics). Returns (ok, lines)."""
    base = [m for t, m in rows if WARMUP_S <= t < WARMUP_S + WINDOW_S]
    end_t = rows[-1][0]
    tail = [m for t, m in rows if t >= end_t - WINDOW_S]
    if not base or not tail or end_t < WARMUP_S + 2 * WINDOW_S:
        return False, ["run too short: need at least 2h10m for a verdict"]
    ok, lines = True, []
    for key, limit in LIMITS.items():
        a = statistics.median(m[key] for m in base)
        b = statistics.median(m[key] for m in tail)
        bad = b - a > limit
        ok &= not bad
        lines.append(f"{'FAIL' if bad else 'ok  '} {key}: {a:.1f} -> {b:.1f} (limit +{limit})")
    return ok, lines


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", required=True)
    ap.add_argument("--hours", type=float, default=24)
    ap.add_argument("--interval", type=float, default=30)
    ap.add_argument("--csv", default="soak.csv")
    args = ap.parse_args()

    proc = subprocess.Popen([args.exe], creationflags=CREATE_NO_WINDOW)
    start = time.monotonic()
    rows = []
    died = None
    with open(args.csv, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["elapsed_s", "private_mb", "handles", "gdi", "user"])
        try:
            while time.monotonic() - start < args.hours * 3600:
                if proc.poll() is not None:
                    died = proc.returncode
                    break
                m = sample(proc.pid)
                if m:
                    t = time.monotonic() - start
                    rows.append((t, m))
                    w.writerow([f"{t:.0f}", f"{m['private_mb']:.1f}", m["handles"], m["gdi"], m["user"]])
                    f.flush()
                time.sleep(args.interval)
        finally:
            if proc.poll() is None:
                proc.terminate()
    if died is not None:
        print(f"FAIL process exited with code {died} after {time.monotonic() - start:.0f}s")
        return 1
    ok, lines = verdict(rows)
    print("\n".join(lines))
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
