"""Differential test of the layout port against real Qt (PySide6).

Random box and grid layouts of widgets with fixed size hints, minimum and maximum sizes, size
policies, stretch factors and item alignments are laid out by `QHBoxLayout` / `QVBoxLayout` /
`QGridLayout` and by `BoxLayout` / `GridLayout` (through `examples/layout_probe.rs`); every visible
item's rectangle must be identical.

    python qt_layout_compare.py [cases] [seed]

Run from `rust/qtrs`; builds the probe once with `cargo build -j 1`.

    python qt_layout_compare.py --record PATH [cases] [seed]

Writes the oracle fixture replayed in-process by `crates/qtrs-widgets/tests/layout_qt_oracle.rs`:
the hand-written HUD and Qt cases (`hand_cases()`) followed by `cases` seeded rich cases (default 300,
seed 20260502). Each record is `probe_line(case)` TAB the Qt rectangles of every item in creation
order (`x,y,w,h`, `-` for a hidden item). Does not build the probe and does not run cargo.
"""
import os
import random
import subprocess
import sys

from PySide6.QtCore import QSize, Qt
from PySide6.QtWidgets import (QApplication, QGridLayout, QHBoxLayout, QSizePolicy, QVBoxLayout,
                               QWidget)

POLICIES = {
    "Fixed": QSizePolicy.Policy.Fixed,
    "Minimum": QSizePolicy.Policy.Minimum,
    "Maximum": QSizePolicy.Policy.Maximum,
    "Preferred": QSizePolicy.Policy.Preferred,
    "Expanding": QSizePolicy.Policy.Expanding,
    "MinimumExpanding": QSizePolicy.Policy.MinimumExpanding,
    "Ignored": QSizePolicy.Policy.Ignored,
}
NO_MAX = 16777215

# `Qt::Alignment` of an item, as the numeric flag values the probe reads (qnamespace.h:151-170):
# no alignment most of the time, otherwise any of the horizontal / vertical flags alone or together.
A = Qt.AlignmentFlag
ALIGN_H = [0, A.AlignLeft, A.AlignRight, A.AlignHCenter]
ALIGN_V = [0, A.AlignTop, A.AlignBottom, A.AlignVCenter]


def align_value(flags):
    value = 0
    for f in flags:
        value |= int(f.value if hasattr(f, "value") else f)
    return value


class Probe(QWidget):
    def __init__(self, parent, hint, minimum_hint=(0, 0)):
        super().__init__(parent)
        self._hint = QSize(*hint)
        self._minimum_hint = QSize(*minimum_hint)

    def sizeHint(self):
        return self._hint

    def minimumSizeHint(self):
        return self._minimum_hint


def random_item(rng):
    hint = (rng.randint(0, 120), rng.randint(0, 60))
    min_size = (rng.choice([0, 0, 0, rng.randint(1, 100)]), rng.choice([0, 0, 0, rng.randint(1, 50)]))
    max_size = (rng.choice([NO_MAX, NO_MAX, rng.randint(1, 200)]), rng.choice([NO_MAX, NO_MAX, rng.randint(1, 120)]))
    max_size = (max(max_size[0], min_size[0]), max(max_size[1], min_size[1]))
    return {
        "hint": hint,
        "min": min_size,
        "max": max_size,
        "pol": (rng.choice(list(POLICIES)), rng.choice(list(POLICIES))),
        "stretch": rng.choice([0, 0, 0, 1, 1, 2, 3]),
        "hidden": rng.random() < 0.1,
        "align": align_value((rng.choice(ALIGN_H), rng.choice(ALIGN_V))) if rng.random() < 0.6 else 0,
    }


def place_grid(rng, items, columns):
    """Assigns each grid item a non-overlapping cell and a row stretch (rng order is fixed)."""
    occupied = set()
    for item in items:
        col_span = rng.choice([1, 1, 1, min(2, columns)])
        row_span = rng.choice([1, 1, 1, 2])
        item["rstretch"] = rng.choice([0, 0, 0, 1, 2])
        row = col = 0
        while True:
            cells = {(row + r, col + c) for r in range(row_span) for c in range(col_span)}
            if col + col_span <= columns and not cells & occupied:
                break
            col += 1
            if col >= columns:
                col, row = 0, row + 1
        occupied |= cells
        item["cell"] = (row, col, row_span, col_span)


def make_case(rng):
    kind = rng.choice(["H", "V", "G2", "G3"])
    items = [random_item(rng) for _ in range(rng.randint(1, 6 if kind in "HV" else 7))]
    if kind[0] == "G":
        place_grid(rng, items, int(kind[1:]))
    return {
        "kind": kind,
        "spacing": rng.randint(0, 12),
        "margin": rng.randint(0, 10),
        "size": (rng.randint(20, 500), rng.randint(20, 400)),
        "items": items,
    }


# ---- Recorder: hand-written HUD and Qt cases, and seeded rich cases -----------------------------

def W(hint, **kw):
    """A widget item with the defaults of `random_item` (`Fixed`/`Fixed`, no stretch, visible)."""
    item = {"hint": hint, "min": (0, 0), "max": (NO_MAX, NO_MAX), "pol": ("Fixed", "Fixed"),
            "stretch": 0, "hidden": False, "align": 0}
    item.update(kw)
    return item


def L(kind, items, size=None, **kw):
    """A layout spec; `size` only for the top-level layout (the container)."""
    spec = {"kind": kind, "spacing": 0, "margin": 0, "items": items}
    spec.update(kw)
    if size is not None:
        spec["size"] = size
    return spec


def nested(layout, stretch=0):
    return {"nested": layout, "stretch": stretch}


def spacer(stretch):
    return {"spacer": stretch}


def hand_cases():
    usage_items = [
        W((30, 14), cell=(0, 0, 1, 1), stretch=1), W((40, 14), cell=(0, 1, 1, 1)),
        W((20, 14), cell=(0, 2, 1, 1)), W((50, 12), cell=(1, 0, 1, 1)),
        W((60, 18), cell=(2, 0, 1, 1), rstretch=5), W((25, 16), cell=(2, 1, 1, 1)),
    ]
    return [
        # Qt case A: asymmetric margins L1 T2 R3 B4 (hud_window.py:144 style).
        L("H", [W((30, 20)) for _ in range(3)], spacing=5, margins=(1, 2, 3, 4), size=(200, 60)),
        # Qt case B: H spacing 3 != V spacing 9 (usage_table.py:386-387).
        L("G2", [W((40, 20), cell=(r, c, 1, 1)) for r in range(2) for c in range(2)],
          hspacing=3, vspacing=9, size=(200, 100)),
        # Qt case C: column minimum width 80 (setColumnMinimumWidth).
        L("G2", [W((20, 20), cell=(0, 0, 1, 1)), W((20, 20), cell=(0, 1, 1, 1))], spacing=4,
          colmin=[(0, 80)], size=(200, 40)),
        # Qt case D: addStretch between two items.
        L("H", [W((30, 20)), spacer(1), W((30, 20))], size=(200, 40)),
        # Qt case E: container smaller than the minimum sum (shrink branch).
        L("H", [W((50, 20), min=(50, 0)) for _ in range(3)], size=(100, 40)),
        # Qt case F: row minimum height 40 on row 1 (usage_table.py:435 style).
        L("G1", [W((20, 20), cell=(0, 0, 1, 1)), W((20, 20), cell=(1, 0, 1, 1))], spacing=4,
          rowmin=[(1, 40)], size=(60, 100)),
        # HUD container margins 12/8/12/10 spacing 6 with two items (hud_window.py:144-145).
        L("V", [W((120, 30), pol=("Preferred", "Fixed")), W((80, 24), pol=("Expanding", "Fixed"), stretch=1)],
          spacing=6, margins=(12, 8, 12, 10), size=(300, 200)),
        # HUD header: label, addStretch(0), button (hud_window.py:286).
        L("H", [W((60, 20)), spacer(0), W((24, 24))], spacing=5, size=(240, 30)),
        # Usage table grid: H spacing 10, V spacing 4, row 2 stretch 5 and minimum 18.
        L("G3", usage_items, hspacing=10, vspacing=4, rowmin=[(2, 18)], size=(450, 350)),
        # Nested layout inside a box (hud_window.py:288 addLayout).
        L("H", [W((50, 20)), nested(L("V", [W((40, 20)), W((30, 10))], spacing=2), stretch=1),
                W((50, 20))], spacing=6, size=(300, 60)),
        # Nested box with a spacer and non-zero minimumSizeHint, Minimum policies.
        L("H", [nested(L("H", [W((30, 20), msh=(40, 10)), spacer(1),
                               W((20, 20), pol=("Minimum", "Fixed"), min=(10, 0))])),
                W((60, 30), pol=("Minimum", "Minimum"), msh=(25, 12)), spacer(2),
                W((40, 20), pol=("Preferred", "Preferred"), msh=(15, 5))],
          spacing=4, size=(160, 50)),
        # Alignment in a box and in a grid (AlignHCenter|AlignVCenter, Left|Top, Right|Bottom).
        L("H", [W((30, 20), align=0x84), W((30, 20), align=0x21), W((30, 20), align=0x42)],
          spacing=5, size=(200, 60)),
        L("G2", [W((30, 20), cell=(0, 0, 1, 1), align=0x84), W((30, 20), cell=(0, 1, 1, 1), align=0x42),
                 W((30, 20), cell=(1, 0, 1, 2), align=0x21)], spacing=4, size=(150, 90)),
        # Hidden item in the middle is skipped by the layout.
        L("H", [W((30, 20)), W((30, 20), hidden=True), W((30, 20))], spacing=4, size=(100, 30)),
        # Vertical box with stretch factors and asymmetric margins.
        L("V", [W((20, 20), stretch=1, pol=("Fixed", "Expanding")),
                W((20, 30), stretch=2, pol=("Fixed", "Expanding")), W((20, 10))],
          spacing=3, margins=(2, 5, 7, 11), size=(80, 200)),
        # Container narrower than the minimum of two fixed-minimum items (narrow HUD).
        L("H", [W((100, 20), min=(100, 20)), W((100, 20), min=(100, 20))], spacing=6,
          margins=(12, 8, 12, 10), size=(120, 40)),
        # Grid with an item spanning two rows, H != V spacing and asymmetric grid margins.
        L("G2", [W((30, 20), cell=(0, 0, 2, 1)), W((30, 20), cell=(0, 1, 1, 1)),
                 W((30, 20), cell=(1, 1, 1, 1))], margins=(3, 1, 5, 7), hspacing=2, vspacing=8,
          size=(150, 90)),
    ]


def rich_widget(rng):
    item = random_item(rng)
    item["msh"] = rng.choice([(0, 0), (0, 0), (rng.randint(0, 60), rng.randint(0, 30))])
    return item


def rich_box(rng, depth):
    items = []
    for _ in range(rng.randint(1, 5)):
        r = rng.random()
        if r < 0.15:
            items.append(spacer(rng.choice([0, 1, 2])))
        elif r < 0.3 and depth == 0:
            child = L(rng.choice(["H", "V"]), rich_box(rng, 1), spacing=rng.randint(0, 12),
                      margin=rng.randint(0, 10))
            items.append(nested(child, rng.choice([0, 0, 1])))
        else:
            items.append(rich_widget(rng))
    return items


def make_rich_case(rng):
    kind = rng.choice(["H", "V", "G2", "G3"])
    spec = {"kind": kind, "spacing": rng.randint(0, 12), "margin": rng.randint(0, 10),
            "size": (rng.randint(20, 500), rng.randint(20, 400))}
    if rng.random() < 0.3:
        spec["margins"] = tuple(rng.randint(0, 12) for _ in range(4))
    if kind[0] == "G":
        columns = int(kind[1:])
        items = [rich_widget(rng) for _ in range(rng.randint(1, 7))]
        place_grid(rng, items, columns)
        spec["items"] = items
        if rng.random() < 0.5:
            spec["hspacing"] = rng.randint(0, 12)
            spec["vspacing"] = rng.randint(0, 12)
        if rng.random() < 0.4:
            spec["rowmin"] = [(rng.randint(0, 2), rng.randint(1, 60))]
        if rng.random() < 0.4:
            spec["colmin"] = [(rng.randint(0, columns - 1), rng.randint(1, 80))]
    else:
        spec["items"] = rich_box(rng, 0)
    return spec


def head_text(spec, width, height):
    text = "{} {} {} {} {}".format(spec["kind"], spec["spacing"], spec["margin"], width, height)
    if "margins" in spec:
        text += " m=" + ",".join(str(v) for v in spec["margins"])
    if "hspacing" in spec:
        text += " hs={}".format(spec["hspacing"])
    if "vspacing" in spec:
        text += " vs={}".format(spec["vspacing"])
    for row, h in spec.get("rowmin", []):
        text += " rmin={}:{}".format(row, h)
    for col, w in spec.get("colmin", []):
        text += " cmin={}:{}".format(col, w)
    return text


def item_text(item):
    if "spacer" in item:
        return "S{}".format(item["spacer"])
    if "nested" in item:
        return "{" + layout_text(item["nested"], 0, 0) + "}:" + str(item["stretch"])
    fields = [*item["hint"], *item["min"], *item["max"], *item["pol"], item["stretch"],
              int(item["hidden"]), *item.get("cell", (0, 0, 1, 1)), item.get("rstretch", 0),
              item["align"]]
    if "msh" in item:
        fields += list(item["msh"])
    return ",".join(str(f) for f in fields)


def layout_text(spec, width, height):
    items = ";".join(item_text(i) for i in spec["items"])
    return "{} | {}".format(head_text(spec, width, height), items)


def probe_line(case):
    return layout_text(case, *case["size"])


def widget_items(spec):
    """Widget items in creation order (depth-first, item order), as `qt_layout` returns them."""
    for item in spec["items"]:
        if "spacer" in item:
            continue
        if "nested" in item:
            yield from widget_items(item["nested"])
        else:
            yield item


def new_layout(kind, container):
    if kind == "H":
        return QHBoxLayout(container) if container else QHBoxLayout()
    if kind == "V":
        return QVBoxLayout(container) if container else QVBoxLayout()
    return QGridLayout(container) if container else QGridLayout()


def build_layout(container, spec, widgets, top):
    kind = spec["kind"]
    layout = new_layout(kind, container if top else None)
    if "margins" in spec:
        layout.setContentsMargins(*spec["margins"])
    else:
        m = spec["margin"]
        layout.setContentsMargins(m, m, m, m)
    layout.setSpacing(spec["spacing"])
    grid = kind[0] == "G"
    if grid:
        if "hspacing" in spec:
            layout.setHorizontalSpacing(spec["hspacing"])
        if "vspacing" in spec:
            layout.setVerticalSpacing(spec["vspacing"])
        for row, h in spec.get("rowmin", []):
            layout.setRowMinimumHeight(row, h)
        for col, w in spec.get("colmin", []):
            layout.setColumnMinimumWidth(col, w)
    for item in spec["items"]:
        if "spacer" in item:
            layout.addStretch(item["spacer"])
            continue
        if "nested" in item:
            child = build_layout(container, item["nested"], widgets, False)
            layout.addLayout(child, item["stretch"])
            continue
        w = Probe(container, item["hint"], item.get("msh", (0, 0)))
        w.setMinimumSize(*item["min"])
        w.setMaximumSize(*item["max"])
        w.setSizePolicy(QSizePolicy(POLICIES[item["pol"][0]], POLICIES[item["pol"][1]]))
        align = Qt.AlignmentFlag(item["align"])
        if grid:
            row, col, row_span, col_span = item["cell"]
            layout.addWidget(w, row, col, row_span, col_span, align)
            if row == 0 and item["stretch"]:
                layout.setColumnStretch(col, item["stretch"])
            if item.get("rstretch", 0):
                layout.setRowStretch(row, item["rstretch"])
        else:
            layout.addWidget(w, item["stretch"], align)
        w.setVisible(not item["hidden"])
        widgets.append(w)
    return layout


def qt_layout(app, parent, case):
    container = QWidget(parent)
    widgets = []
    layout = build_layout(container, case, widgets, True)
    container.setGeometry(0, 0, *case["size"])
    container.show()
    layout.activate()
    layout.setGeometry(container.rect())
    app.processEvents()
    return [(w.x(), w.y(), w.width(), w.height()) for w in widgets]


def pyside_version():
    import PySide6
    return PySide6.__version__


def qt_version():
    from PySide6.QtCore import qVersion
    return qVersion()


def record(path, cases, seed):
    app = QApplication([])
    parent = QWidget()
    parent.resize(800, 700)
    parent.show()
    hand = hand_cases()
    rng = random.Random(seed)
    batch = hand + [make_rich_case(rng) for _ in range(cases)]
    lines = [
        "# Qt layout oracle for crates/qtrs-widgets/tests/layout_qt_oracle.rs.",
        "# Recorded by tools/second_layer_harness/qt_layout_compare.py --record (PySide6 {}, Qt {}).".format(
            pyside_version(), qt_version()),
        "# Hand-written cases: {}; rich seeded cases: {} (seed {}).".format(len(hand), cases, seed),
        "# Record: probe_line(case) TAB Qt rectangles of every item, x,y,w,h; `-` = hidden, not compared.",
    ]
    for case in batch:
        rects = qt_layout(app, parent, case)
        expect = []
        for item, rect in zip(widget_items(case), rects):
            expect.append("-" if item["hidden"] else "{},{},{},{}".format(*rect))
        lines.append("{}\t{}".format(probe_line(case), ";".join(expect)))
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print(f"recorded {len(batch)} cases to {path}")


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--record":
        path = sys.argv[2]
        cases = int(sys.argv[3]) if len(sys.argv) > 3 else 300
        seed = int(sys.argv[4]) if len(sys.argv) > 4 else 20260502
        record(path, cases, seed)
        return

    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 400
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    rng = random.Random(seed)
    app = QApplication([])
    parent = QWidget()
    parent.resize(800, 700)
    parent.show()

    subprocess.run(["cargo", "build", "-j", "1", "-p", "qtrs-widgets", "--example", "layout_probe"],
                   check=True)
    exe = os.path.join("target", "debug", "examples", "layout_probe.exe")
    batch = [make_case(rng) for _ in range(cases)]
    out = subprocess.run([exe], input="\n".join(probe_line(c) for c in batch) + "\n", text=True,
                         capture_output=True, check=True).stdout.strip().split("\n")

    bad = 0
    for number, (case, rust_line) in enumerate(zip(batch, out)):
        qt = qt_layout(app, parent, case)
        rust = [tuple(int(v) for v in r.split(",")) for r in rust_line.split(";")]
        diffs = [(i, q, r) for i, (q, r, item) in enumerate(zip(qt, rust, case["items"]))
                 if not item["hidden"] and q != r]
        if diffs:
            bad += 1
            if bad <= 8:
                print(f"case {number}: {probe_line(case)}")
                for i, q, r in diffs:
                    print(f"  item {i}: qt {q} rust {r}")
    print(f"{cases} cases, {bad} differ")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
