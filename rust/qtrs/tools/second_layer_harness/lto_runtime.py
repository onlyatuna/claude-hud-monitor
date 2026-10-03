import os, re, subprocess, statistics as st, sys
here = os.path.dirname(os.path.abspath(__file__)); tmp = os.environ["TEMP"]
names = ["fat", "thin", "off"]
res = {n: {"paint": [], "total": [], "first": []} for n in names}
for rnd in range(3):
    for n in names:
        env = dict(os.environ, HUD_EXE=os.path.abspath(f"target_b_{n}/release/ClaudeHUD.exe"))
        log = os.path.join(tmp, f"lto_{n}_{rnd}.log")
        subprocess.run([sys.executable, os.path.join(here, "hud_drag.py"), log], env=env, capture_output=True)
        t = open(log, encoding="utf-8", errors="replace").read()
        for l in t.splitlines():
            if l.startswith("  #") and "render=Y" in l:
                m = re.search(r"total=([\d.]+)", l); p = re.search(r"paint=([\d.]+)", l)
                if m: res[n]["total"].append(float(m[1]))
                if p: res[n]["paint"].append(float(p[1]))
        tr = os.path.join(tmp, f"lto_{n}_{rnd}.trace")
        out = subprocess.run([sys.executable, os.path.join(here, "hud_startup.py"), tr], env=env, capture_output=True, text=True).stdout
        l0 = int(re.search(r"launch_epoch_ms (\d+)", out)[1])
        rows = [(int(x.split()[0]) - l0, x) for x in open(tr, encoding="utf-8")]
        first = [a for a, x in rows if "] paint (render_widget_recursive)" in x or "paint (render_widget_recursive)" in x][0]
        res[n]["first"].append(first)
def s(v): v = sorted(v); return f"p50 {st.median(v):.1f} p90 {v[int(len(v)*.9)]:.1f} (n={len(v)})"
for n in names:
    print(n, "| paint", s(res[n]["paint"]), "| frame", s(res[n]["total"]), "| first painted frame ms after launch", sorted(res[n]["first"]))
