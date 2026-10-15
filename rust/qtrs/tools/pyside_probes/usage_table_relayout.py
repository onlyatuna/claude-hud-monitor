"""UsageTable under dynamic relayout (G9.7.b, sub-scope 4).

The real `UsageTable` of `python/ui/usage_table.py` (scheme "scale", dark theme) with the table-mode
application sheet (`get_hud_stylesheet(THEMES["dark"], vibrant=True)`, the one the Rust oracle tests
install), 426 x 312 (the HUD's table page in a 450 x 350 window, as `test_grid_matches_qt_geometry`),
fed the `UsageMetrics` the HUD feeds it. The owner is shown with `WA_DontShowOnScreen` (visible and
laid out by the `LayoutRequest`s the event loop delivers, but no window appears); after each update the
events are processed and every column's cells are printed: the header, the dial and the cells relative
to the table; the icon, name and badge relative to the header. Reset times are left unset, so no text
depends on the clock. (An unshown owner with a hand-activated layout kept the grid's stale header
size after the stale step, which a shown widget does not.)

Steps (the Rust test in `rust/src/ui/usage_table_relayout_tests.rs` runs the same ones):
  initial       just constructed
  populated     badges "Code: 100%" / "Plan: Free" / "C/G: 55%", values 68 / 100 and so on
  long_pill     codex second window titled "SECONDARY" (pill "-- · SECONDARY"), agy "CREDITS BALANCE"
  offline       claude error, not stale (OFFLINE badge, "ERR" dial, muted cells)
  stale         agy error and stale ("⏱ STALE" badge)
  recovered     claude back to data, agy back to data

Run (Windows; PySide6 6.11.2): DPR 1.25 -> QT_SCREEN_SCALE_FACTORS=1.25; DPR 1.0 -> QT_ENABLE_HIGHDPI_SCALING=0
  python usage_table_relayout.py
"""

import sys
from datetime import datetime, timezone
from pathlib import Path

PY_ROOT = str(Path(__file__).resolve().parents[4] / "python")
sys.path.insert(0, PY_ROOT)

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication, QVBoxLayout, QWidget

from core.providers.base import UsageMetrics
from ui.styles import THEMES, get_hud_stylesheet
from ui.usage_table import UsageTable

TABLE_CELLS = ("header", "m1_reset", "m1_countdown", "dial", "m2_val", "m2_reset", "m2_countdown")
HEADER_CELLS = ("icon", "name", "badge")


def rect(w, parent):
    if w.isHidden():
        return "-"
    g = w.geometry()
    x, y = g.x(), g.y()
    p = w.parentWidget()
    while p is not None and p is not parent:
        x += p.geometry().x()
        y += p.geometry().y()
        p = p.parentWidget()
    return f"{x},{y},{g.width()},{g.height()}"


def main():
    app = QApplication(sys.argv)
    app.setStyleSheet(get_hud_stylesheet(THEMES["dark"], True))
    owner = QWidget()
    table = UsageTable(THEMES["dark"], "scale", parent=owner)
    outer = QVBoxLayout(owner)
    outer.setContentsMargins(0, 0, 0, 0)
    outer.addWidget(table)
    owner.setMinimumSize(100, 0)

    # `WA_DontShowOnScreen`: the widget is shown (visible, laid out by `LayoutRequest`s the event loop
    # delivers) without a window appearing.
    owner.setAttribute(Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    owner.resize(426, 312)
    owner.show()

    def show(step):
        for _ in range(3):
            app.processEvents()
        hint = table.sizeHint()
        print(f"{step} hint={hint.width()}x{hint.height()}")
        for pid, col in table.columns.items():
            cells = [f"{n}={rect(getattr(col, n), table)}" for n in TABLE_CELLS]
            cells += [f"{n}={rect(getattr(col, n), col.header)}" for n in HEADER_CELLS]
            print(f"  {pid} " + " ".join(cells))

    print(f"dpr={app.devicePixelRatio()}")
    show("initial")

    def data(pid, **kw):
        base = dict(provider_id=pid, metric1_val=68.0, metric1_text="68%",
                    metric2_val=100.0, metric2_text="100%")
        base.update(kw)
        return UsageMetrics(**base)

    good = {
        "claude": data("claude", badge1_text="Code: 100%"),
        "codex": data("codex", badge1_text="Plan: Free", metric1_val=55.0, metric1_text="55%",
                      metric2_val=20.0, metric2_text="20%"),
        "agy": data("agy", badge1_text="C/G: 55%", metric1_val=10.0, metric1_text="10%"),
    }

    def feed(step, updates):
        for m in updates.values():
            table.update_metrics(m)
        show(step)

    feed("populated", good)
    feed("long_pill", {
        "codex": data("codex", badge1_text="Plan: Free", metric2_title="SECONDARY",
                      metric2_val=None, metric2_text="--"),
        "agy": data("agy", badge1_text="C/G: 55%", metric2_title="CREDITS BALANCE"),
    })
    feed("offline", {"claude": data("claude", error="Network error")})
    feed("stale", {"agy": data("agy", error="Network error", stale=True,
                               last_success=datetime(2026, 1, 2, 3, 4, 5, tzinfo=timezone.utc))})
    feed("recovered", {"claude": good["claude"], "agy": good["agy"]})


main()
