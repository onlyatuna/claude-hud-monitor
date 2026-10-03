"""Automated drag + screen-capture detector for the 'second layer' ghost.

python driver.py <qt|w32:X> [scenario ...]
Drags the real window corner with SendInput-equivalent mouse_event (enters the real
Windows modal sizing loop) while a thread grabs the composed desktop. A frame is
'ghost' when, with the HWND rect unchanged across the grab, magenta window pixels lie
outside that HWND rect (old frame/content/position still visible).
second_layer = reproduced if any ghost frame exceeds GHOST_PX pixels.
"""
import sys, time, random, subprocess, threading, ctypes, os
from ctypes import wintypes as W
import numpy as np
from PIL import ImageGrab

u32 = ctypes.windll.user32
ctypes.windll.shcore.SetProcessDpiAwareness(2)
HERE = os.path.dirname(os.path.abspath(__file__))
GHOST_PX = 1500
RAD = int(os.environ.get('RAD', '24'))
INSET = int(os.environ.get('INSET', '14'))
PAD = 4


def rect(h):
    r = W.RECT(); u32.GetWindowRect(h, ctypes.byref(r)); return (r.left, r.top, r.right, r.bottom)


def mouse(flag):
    u32.mouse_event(flag, 0, 0, 0, 0)


def seq(name, corner):
    sx = 1 if corner == "br" else -1
    pts = []
    def lin(a, b, n):
        return [(a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n) for i in range(1, n + 1)]
    if name == "slow":
        pts = lin((0, 0), (sx * 300, sx * 200), 60); d = 0.012
    elif name == "fast":
        pts = lin((0, 0), (sx * 400, sx * 300), 40); d = 0.0
    elif name == "gsg":
        pts = lin((0, 0), (sx * 300, sx * 200), 25) + lin((sx * 300, sx * 200), (sx * -150, sx * -100), 40) \
            + lin((sx * -150, sx * -100), (sx * 250, sx * 150), 40); d = 0.002
    elif name == "rapid":
        random.seed(1); x = y = 0; pts = []
        for _ in range(400):
            x = max(-250, min(350, x + random.randint(-60, 60))); y = max(-200, min(250, y + random.randint(-50, 50)))
            pts.append((sx * x, sx * y))
        d = 0.0
    if corner == "tl":
        pts = [(x * 0.45, y * 0.45) for x, y in pts]
    return pts, d


def run(target, scenario, corner):
    if target.startswith("qtrs"):
        prof = target.split(":")[1] if ":" in target else "debug"
        cmd = [os.path.join(HERE, "..", "..", "target", prof, "examples", "second_layer_probe.exe")]
    else:
        cmd = [sys.executable, os.path.join(HERE, "qt_layered.py" if target == "qt" else "w32_layered.py")] \
            + ([] if target == "qt" else [target.split(":")[1]])
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, text=True)
    h = 0
    for _ in range(100):
        h = u32.FindWindowW(None, "SLTEST")
        if h: break
        time.sleep(0.1)
    time.sleep(1.0)
    l, t, r, b = rect(h); init = (l, t, r, b)
    u32.SetForegroundWindow(h)
    sx, sy = (r - INSET, b - INSET) if corner == "br" else (l + INSET, t + INSET)
    u32.SetCursorPos(sx, sy); time.sleep(0.3)
    frames, stop = [], threading.Event()
    box = (max(0, l - 450), max(0, t - 350), l + 1100, t + 900)

    def cap():
        while not stop.is_set():
            r0 = rect(h); im = ImageGrab.grab(bbox=box, all_screens=False); r1 = rect(h)
            frames.append((r0, np.asarray(im), r1))
    th = threading.Thread(target=cap, daemon=True); th.start()
    mouse(2)  # left down
    pts, d = seq(scenario, corner)
    for (dx, dy) in pts:
        u32.SetCursorPos(int(sx + dx), int(sy + dy))
        if d: time.sleep(d)
    time.sleep(0.2); mouse(4)
    time.sleep(0.5); stop.set(); th.join()
    final_rect = rect(h)
    u32.PostMessageW(h, 0x10, 0, 0)  # WM_CLOSE
    p.terminate()
    ghosts, worst, stable_n = 0, 0, 0
    res = []  # (rect, mismatch) for stable frames
    for r0, a, r1 in frames:
        if r0 != r1: continue
        stable_n += 1
        m = (a[..., 0] > 200) & (a[..., 2] > 200) & (a[..., 1] < 60)
        ox, oy = box[0], box[1]
        l, t, r, b = r0
        def rr(l_, t_, r_, b_, grow):
            # rounded-rect (radius 24) mask in box coordinates, grown/shrunk by `grow`
            l_, t_, r_, b_ = l_ - ox - grow, t_ - oy - grow, r_ - ox + grow, b_ - oy + grow
            yy, xx = np.mgrid[0:m.shape[0], 0:m.shape[1]]
            rad = RAD + grow
            cx = np.clip(xx, l_ + rad, r_ - 1 - rad); cy = np.clip(yy, t_ + rad, b_ - 1 - rad)
            return (np.hypot(xx - cx, yy - cy) <= rad) & (xx >= l_) & (xx < r_) & (yy >= t_) & (yy < b_)
        if not (r - l > 60 and b - t > 60): continue
        out = int((m & ~rr(l, t, r, b, 4)).sum()) + int((~m & rr(l, t, r, b, -4)).sum())
        worst = max(worst, out)
        res.append((r0, out))
    for (ra, oa), (rb, ob) in zip(res, res[1:]):
        if ra == rb and oa > GHOST_PX and ob > GHOST_PX: ghosts += 1
    return dict(target=target, scenario=scenario, corner=corner, frames=len(frames), stable=stable_n,
                persistent_ghost_pairs=ghosts, final_mismatch=(res[-1][1] if res else None), worst_mismatch_px=worst,
                second_layer="reproduced" if ghosts else "not_reproduced",
                final_rect=final_rect, init_rect=init)


if __name__ == "__main__":
    tgt = sys.argv[1]
    scs = sys.argv[2:] or ["slow", "fast", "gsg", "rapid"]
    for corner in ("br", "tl"):
        for s in scs:
            print(run(tgt, s, corner), flush=True)
