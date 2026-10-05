"""Differential test of the layout port against real Qt (PySide6).

Random box and grid layouts of widgets with fixed size hints, minimum and maximum sizes, size
policies and stretch factors are laid out by `QHBoxLayout` / `QVBoxLayout` / `QGridLayout` and by
`BoxLayout` / `GridLayout` (through `examples/layout_probe.rs`); every visible item's rectangle
must be identical.

    python qt_layout_compare.py [cases] [seed]

Run from `rust/qtrs`; builds the probe once with `cargo build -j 1`.
"""
import os
import random
import subprocess
import sys

from PySide6.QtCore import QSize
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


class Probe(QWidget):
    def __init__(self, parent, hint):
        super().__init__(parent)
        self._hint = QSize(*hint)

    def sizeHint(self):
        return self._hint

    def minimumSizeHint(self):
        return QSize(0, 0)


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
    }


def make_case(rng):
    kind = rng.choice(["H", "V", "G2", "G3"])
    items = [random_item(rng) for _ in range(rng.randint(1, 6 if kind in "HV" else 7))]
    if kind[0] == "G":
        columns = int(kind[1:])
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
    return {
        "kind": kind,
        "spacing": rng.randint(0, 12),
        "margin": rng.randint(0, 10),
        "size": (rng.randint(20, 500), rng.randint(20, 400)),
        "items": items,
    }


def probe_line(case):
    items = ";".join(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}".format(
            *i["hint"], *i["min"], *i["max"], *i["pol"], i["stretch"], int(i["hidden"]),
            *i.get("cell", (0, 0, 1, 1)), i.get("rstretch", 0))
        for i in case["items"])
    return "{} {} {} {} {} | {}".format(
        case["kind"], case["spacing"], case["margin"], *case["size"], items)


def qt_layout(app, parent, case):
    container = QWidget(parent)
    kind = case["kind"]
    if kind == "H":
        layout = QHBoxLayout(container)
    elif kind == "V":
        layout = QVBoxLayout(container)
    else:
        layout = QGridLayout(container)
    layout.setSpacing(case["spacing"])
    m = case["margin"]
    layout.setContentsMargins(m, m, m, m)
    widgets = []
    columns = int(kind[1:]) if kind[0] == "G" else 0
    for index, item in enumerate(case["items"]):
        w = Probe(container, item["hint"])
        w.setMinimumSize(*item["min"])
        w.setMaximumSize(*item["max"])
        w.setSizePolicy(QSizePolicy(POLICIES[item["pol"][0]], POLICIES[item["pol"][1]]))
        if kind[0] == "G":
            row, col, row_span, col_span = item["cell"]
            layout.addWidget(w, row, col, row_span, col_span)
            if row == 0 and item["stretch"]:
                layout.setColumnStretch(col, item["stretch"])
            if item["rstretch"]:
                layout.setRowStretch(row, item["rstretch"])
        else:
            layout.addWidget(w, item["stretch"])
        w.setVisible(not item["hidden"])
        widgets.append(w)
    container.setGeometry(0, 0, *case["size"])
    container.show()
    layout.activate()
    layout.setGeometry(container.rect())
    app.processEvents()
    return [(w.x(), w.y(), w.width(), w.height()) for w in widgets]


def main():
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
