"""Full geometry oracle of the real PySide6 HUD (cards mode).
Usage: [VH=<vertical height>] python py_geom.py <horizontal|vertical> <out.json>
The Rust HUD cannot go below 463 px vertically (config.rs MIN_VERTICAL_HEIGHT), so the committed
results use VH=463; the Python default is 410. Run on a 125 % DPR Windows machine, as the Rust side is."""
import os, sys, tempfile, json
from pathlib import Path
os.environ.setdefault('QT_QPA_PLATFORM', 'windows')
PY_ROOT = str(Path(__file__).resolve().parents[3] / "python")
sys.path.insert(0, PY_ROOT); os.chdir(PY_ROOT)
from PySide6.QtWidgets import QApplication, QLabel, QProgressBar
from PySide6.QtCore import QPoint
from core.config_manager import ConfigManager
from ui.hud_window import HUDWindow

APP = QApplication.instance() or QApplication([])
mode = sys.argv[1]
tmp = Path(tempfile.mkdtemp())
cfg = ConfigManager(tmp / 'config.json')
cfg.update({'ui_mode': 'cards', 'layout_mode': mode, 'appearance': 'dark', 'window_x': 0, 'window_y': 0,
            'always_on_top': False, 'vertical_width': 280, 'vertical_height': int(os.environ.get('VH','410')), 'horizontal_width': 690, 'horizontal_height': 145, 'click_through': False})
hud = HUDWindow(cfg); hud.show()
for _ in range(8): APP.processEvents()

def sz(s): return [s.width(), s.height()]
def rect_in(w, hud): p = w.mapTo(hud, QPoint(0, 0)); return [p.x(), p.y(), w.width(), w.height()]
def lay(l):
    m = l.contentsMargins()
    return {"spacing": l.spacing(), "margins": [m.left(), m.top(), m.right(), m.bottom()],
            "sizeHint": sz(l.sizeHint()), "minimumSize": sz(l.minimumSize())}
def info(w):
    d = {"rect": rect_in(w, hud), "sizeHint": sz(w.sizeHint()), "minimumSizeHint": sz(w.minimumSizeHint()),
         "minimumSize": sz(w.minimumSize()), "maximumSize": sz(w.maximumSize())}
    sp = w.sizePolicy(); d["sizePolicy"] = [int(sp.horizontalPolicy().value), int(sp.verticalPolicy().value)]
    f = w.font(); fm = w.fontMetrics()
    d["font"] = {"family": f.family(), "px": f.pixelSize(), "pt": f.pointSizeF(), "weight": int(f.weight()),
                 "height": fm.height(), "ascent": fm.ascent(), "descent": fm.descent()}
    cm = w.contentsMargins(); d["contentsMargins"] = [cm.left(), cm.top(), cm.right(), cm.bottom()]
    if isinstance(w, QLabel):
        d["text"] = w.text(); d["alignment"] = int(w.alignment().value); d["margin"] = w.margin(); d["indent"] = w.indent()
        d["textWidth"] = fm.horizontalAdvance(w.text())
    if w.layout(): d["layout"] = lay(w.layout())
    return d

out = {"meta": {"mode": mode, "dpr": hud.devicePixelRatioF(), "hud": hud.geometry().getRect()}, "w": {}}
F = ["dot","title","badge","badge2","m1_label","m1_val","m1_bar","m1_sub","m2_label","m2_val","m2_bar","m2_sub"]
for pid, card in hud.cards.items():
    out["w"][f"{pid}.card"] = info(card)
    root = card.layout()
    out["w"][f"{pid}.card"]["layout"] = lay(root)
    # nested layouts: item 0 header, 1 m1_box, 2 m2_box
    names = ["header", "m1_box", "m2_box"]
    off = card.mapTo(hud, QPoint(0, 0))
    for i, n in enumerate(names):
        l = root.itemAt(i).layout(); g = l.geometry()
        d = lay(l); d["rect"] = [off.x()+g.x(), off.y()+g.y(), g.width(), g.height()]
        if n != "header": 
            hdr = l.itemAt(0).layout(); gh = hdr.geometry()
            d["hdr"] = lay(hdr); d["hdr"]["rect"] = [off.x()+gh.x(), off.y()+gh.y(), gh.width(), gh.height()]
        out["w"][f"{pid}.{n}"] = d
    for f in F:
        w = getattr(card, f)
        out["w"][f"{pid}.{f}"] = info(w) if w.isVisible() else {"visible": False}
for n in ("title_label","time_label","layout_toggle_btn","status_dot","ghost_label"):
    w = getattr(hud, n, None)
    if w is not None: out["w"][f"hud.{n}"] = info(w) if w.isVisible() else {"visible": False}
il = hud.inner_layout
hl = il.itemAt(0).layout(); g = hl.geometry()
out["w"]["hud.header"] = {**lay(hl), "rect": [g.x(), g.y(), g.width(), g.height()], "expanding": int(hl.expandingDirections().value)}
out["w"]["hud.cards_container"] = info(hud.cards_container)
out["w"]["hud.inner_layout"] = {**lay(il), "count": il.count()}
out["w"]["hud.root_layout"] = lay(hud.layout()) if hud.layout() else None
# header widget layouts
open(sys.argv[2], "w", encoding="utf-8").write(json.dumps(out, ensure_ascii=False, indent=1))
hud.refresh_controller.stop(); hud.close()
