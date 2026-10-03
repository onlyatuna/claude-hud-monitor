"""Launches the real HUD with the startup trace and reports steady-state paint times (idle repaints).
Usage: python hud_paint_steady.py [seconds=20]   (HUD_EXE overrides the exe)
"""
import subprocess, time, os, re, sys, statistics as st
exe = os.environ.get("HUD_EXE") or os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "target", "release", "ClaudeHUD.exe"))
tr = os.path.join(os.environ["TEMP"], "paint_steady.log")
if os.path.exists(tr):
    os.remove(tr)
secs = float(sys.argv[1]) if len(sys.argv) > 1 else 20
p = subprocess.Popen([exe], env=dict(os.environ, QTRS_STARTUP_TRACE=tr))
time.sleep(secs)
p.kill()
frames = []
for l in open(tr, encoding="utf-8", errors="replace"):
    m = re.search(r"paint \(render_widget_recursive\)\s+\(([\d.]+) ms\)", l)
    if m:
        frames.append(float(m[1]))
steady = frames[4:]  # skip the cold frames (font loading)
print(f"frames {len(frames)}, steady {len(steady)}: paint ms p50 {st.median(steady):.2f}  mean {st.mean(steady):.2f}  min {min(steady):.2f}  max {max(steady):.2f}")
