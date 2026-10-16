"""PySide6 measurements for the liveness of a signal's owner.

Run with `QT_QPA_PLATFORM=offscreen python signal_owner_liveness.py`. Each probe prints one line.

What it measures (Qt 6, PySide6):
  A1  emitting through a signal handle after its owner was deleted
  A2  a slot deletes the owner during the emission: do the remaining slots run?
  A3  a queued call posted before the owner was deleted: is it still delivered?
  A4  a direct slot of a live signal that emits a deleted owner's signal: does the outer emission go on?
  B1  an owner that no registry knows (Qt has none) emits normally
"""
import sys

import shiboken6
from PySide6.QtCore import QCoreApplication, QObject, Qt, Signal, Slot

app = QCoreApplication.instance() or QCoreApplication(sys.argv)


class Owner(QObject):
    sig = Signal()


class Sink(QObject):
    def __init__(self):
        super().__init__()
        self.hits = 0

    @Slot()
    def hit(self):
        self.hits += 1


def probe_a1():
    owner = Owner()
    sink = Sink()
    owner.sig.connect(sink.hit)
    handle = owner.sig  # a bound signal: what a Rust `Signal` clone corresponds to
    shiboken6.delete(owner)
    try:
        handle.emit()
        outcome = "no error"
    except RuntimeError as e:
        outcome = f"RuntimeError: {e}"
    print(f"A1 emit after the owner was deleted -> {outcome}; slot hits={sink.hits}")


def probe_a2():
    owner = Owner()
    log = []

    def first():
        log.append("first")
        shiboken6.delete(owner)

    owner.sig.connect(first)
    owner.sig.connect(lambda: log.append("second"))
    owner.sig.emit()
    print(f"A2 slot deletes the owner during the emission -> calls={log}")


def probe_a3():
    owner = Owner()
    sink = Sink()
    owner.sig.connect(sink.hit, Qt.ConnectionType.QueuedConnection)
    owner.sig.emit()
    shiboken6.delete(owner)
    app.processEvents()
    print(f"A3 queued call posted, owner deleted, loop run -> slot hits={sink.hits}")


def probe_a4():
    dead = Owner()
    live = Owner()
    log = []
    handle = dead.sig
    shiboken6.delete(dead)

    def first():
        log.append("first")
        try:
            handle.emit()
        except RuntimeError:
            log.append("emit on the deleted owner raised")

    live.sig.connect(first)
    live.sig.connect(lambda: log.append("second"))
    live.sig.emit()
    print(f"A4 slot of a live signal emits a deleted owner's signal -> calls={log}")


def probe_b1():
    owner = Owner()  # no parent, no registry: Qt has nothing to unregister from
    sink = Sink()
    owner.sig.connect(sink.hit)
    owner.sig.emit()
    print(f"B1 an owner with no registry emits normally -> slot hits={sink.hits}")


for p in (probe_a1, probe_a2, probe_a3, probe_a4, probe_b1):
    p()
print("PySide6", __import__("PySide6").__version__)
