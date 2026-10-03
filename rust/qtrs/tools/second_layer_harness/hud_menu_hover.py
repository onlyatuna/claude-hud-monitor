"""Opens the real HUD context menu (right-click on the HUD body) and sweeps the mouse over its items.

Per step it records how long the popup took to react (time until a pixel in the hovered row changes)
so a stalled / dropped hover shows up as a long or missing reaction. Trace goes to argv[1].
Usage: python hud_menu_hover.py <trace> [shots_prefix]
"""
import subprocess, time, ctypes, os, sys
from ctypes import wintypes as W
from PIL import ImageGrab

u = ctypes.windll.user32
u.FindWindowW.restype = W.HWND
ctypes.windll.shcore.SetProcessDpiAwareness(2)
exe = os.environ.get("HUD_EXE") or os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "target", "release", "ClaudeHUD.exe"))
trace = sys.argv[1]
shots = sys.argv[2] if len(sys.argv) > 2 else None
env = dict(os.environ, QTRS_STARTUP_TRACE=trace)
p = subprocess.Popen([exe], env=env)

h = 0
t0 = time.time()
while time.time() - t0 < 30:
    h = u.FindWindowW(None, "Claude HUD Monitor")
    if h and u.IsWindowVisible(h):
        break
    time.sleep(0.05)
time.sleep(3)  # let first frame + data settle

r = W.RECT()
u.GetWindowRect(h, ctypes.byref(r))
cx, cy = (r.left + r.right) // 2, (r.top + r.bottom) // 2


def move(x, y):
    u.SetCursorPos(int(x), int(y))


def click(flag_down, flag_up):
    u.mouse_event(flag_down, 0, 0, 0, 0)
    time.sleep(0.03)
    u.mouse_event(flag_up, 0, 0, 0, 0)


def grab(box):
    return ImageGrab.grab(bbox=box, all_screens=True)


move(cx, cy)
time.sleep(0.2)
t_open = time.time()
click(0x0008, 0x0010)  # RIGHTDOWN / RIGHTUP
# find the popup: a new visible top-level window near the cursor
time.sleep(1.0)
print("hud rect", r.left, r.top, r.right, r.bottom, "cursor", cx, cy)

# sweep straight down from the click point in 6 px steps, 40 ms apart, snapshotting each step
steps = []
base = grab((cx - 20, cy, cx + 420, cy + 900))
if shots:
    base.save(f"{shots}_open.png")
prev = base
for i in range(0, 60):
    y = cy + 8 + i * 12
    move(cx + 60, y)
    t = time.time()
    # wait up to 400 ms for the screen under the popup to change
    changed = None
    while time.time() - t < 0.4:
        cur = grab((cx - 20, cy, cx + 420, cy + 900))
        if cur.tobytes() != prev.tobytes():
            changed = (time.time() - t) * 1000
            prev = cur
            break
        time.sleep(0.005)
    steps.append((y - cy, changed))
    if shots and i % int(os.environ.get("SHOT_EVERY", "6")) == 0:
        grab((cx - 20, cy, cx + 420, cy + 900)).save(f"{shots}_{i:02d}.png")

slow = [(dy, c) for dy, c in steps if c is not None and c > 60]
nochg = [dy for dy, c in steps if c is None]
print("steps", len(steps), "slow(>60ms)", slow)
print("no visual change within 400ms at dy:", nochg)
click(0x0008, 0x0010)
time.sleep(0.3)
p.kill()
