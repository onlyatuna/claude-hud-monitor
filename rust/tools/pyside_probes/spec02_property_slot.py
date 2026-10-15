"""QTRS_SPEC.md R-00031 (property notify) and R-00033 (slot in meta-object) PySide6 probes.

Purpose: observable Qt behaviour, so these rows do not rest on reading moc's generator.
Limit: PySide6's Property/Slot are registered by PySide's own dynamic meta-object builder, not by
moc. The R-00031 cases below therefore observe PySide's and Qt's behaviour, not moc's generated
`MEMBER` + `NOTIFY` code path.

Run: QT_QPA_PLATFORM=offscreen python spec02_property_slot.py
"""

import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import PySide6  # noqa: E402
from PySide6.QtCore import (  # noqa: E402
    Property,
    QCoreApplication,
    QMetaObject,
    QObject,
    Qt,
    Q_ARG,
    Signal,
    Slot,
    qVersion,
)


def probe_R00031():
    class Notif(QObject):
        changed = Signal(int)

        def __init__(self):
            super().__init__()
            self._v = 0

        def get_v(self):
            return self._v

        def set_v(self, v):
            self._v = v  # the setter itself never emits

        v = Property(int, get_v, set_v, notify=changed)

    n = Notif()
    got = []
    n.changed.connect(got.append)
    n.setProperty("v", 5)
    a = list(got)
    n.setProperty("v", 5)  # same value
    b = list(got)
    n.setProperty("v", 6)
    c = list(got)
    print(f"R-00031 (a) Python Property + notify, setProperty via QObject: first set 5 -> {a}; "
          f"same value 5 again -> {b}; 6 -> {c}")

    # Built-in class: objectName has a notify signal (objectNameChanged) in Qt 6.
    o = QObject()
    names = []
    o.objectNameChanged.connect(names.append)
    o.setObjectName("a")
    o.setObjectName("a")
    o.setObjectName("b")
    print(f"R-00031 (b) built-in QObject.objectName notify: set a, a, b -> {names}")


def probe_R00033():
    class S(QObject):
        def __init__(self):
            super().__init__()
            self.hits = []

        @Slot(int)
        def on_int(self, v):
            self.hits.append(("on_int", v))

        @Slot()
        def on_void(self):
            self.hits.append(("on_void",))

        def not_a_slot(self, v):
            self.hits.append(("not_a_slot", v))

    s = S()
    mo = s.metaObject()
    own = [mo.method(i).methodSignature().data().decode() for i in range(mo.methodOffset(), mo.methodCount())]
    print(f"R-00033 (a) own methods in meta-object: {own}")
    print(f"R-00033 (b) indexOfSlot on_int(int)={mo.indexOfSlot('on_int(int)')}, on_void()={mo.indexOfSlot('on_void()')}, "
          f"not_a_slot(int)={mo.indexOfSlot('not_a_slot(int)')}")
    r1 = QMetaObject.invokeMethod(s, "on_int", Qt.DirectConnection, Q_ARG(int, 7))
    r2 = QMetaObject.invokeMethod(s, "on_void", Qt.DirectConnection)
    r3 = QMetaObject.invokeMethod(s, "not_a_slot", Qt.DirectConnection, Q_ARG(int, 9))
    print(f"R-00033 (c) invokeMethod by name: on_int -> {r1}, on_void -> {r2}, not_a_slot -> {r3}; hits {s.hits}")

    class Sg(QObject):
        sig = Signal(int)

    g = Sg()
    seen = []
    g.sig.connect(seen.append)
    r4 = QMetaObject.invokeMethod(g, "sig", Qt.DirectConnection, Q_ARG(int, 3))
    print(f"R-00033 (d) invokeMethod on a signal by name (direct): returned {r4}; receiver got {seen}")
    r5 = QMetaObject.invokeMethod(g, "sig", Qt.QueuedConnection, Q_ARG(int, 4))
    before = list(seen)
    QCoreApplication.processEvents()
    print(f"R-00033 (e) invokeMethod on a signal by name (queued): returned {r5}; before processEvents {before}; after {seen}")


def main():
    app = QCoreApplication([])  # noqa: F841
    print(f"PySide6 {PySide6.__version__}; Qt {qVersion()}")
    for fn in (probe_R00031, probe_R00033):
        try:
            fn()
        except Exception as e:
            print(f"{fn.__name__}: ERROR {type(e).__name__}: {e}")


if __name__ == "__main__":
    main()
    os._exit(0)
