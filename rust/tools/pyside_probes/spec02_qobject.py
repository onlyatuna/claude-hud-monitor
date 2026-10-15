"""QTRS_SPEC.md chapter 02 (QObject model) PySide6 probes.

Purpose: observe Qt QObject behaviour cited by the [PYSIDE-RUN] evidence of
chapter 02 rows (R-00003 .. R-00036). Each probe prints `R-000xx: <result>`.

How to run (from any directory):
    QT_QPA_PLATFORM=offscreen python spec02_qobject.py

The PySide6 and Qt versions are printed first. Risky parent-reassignment cases
(self parent, descendant parent, cross-thread parent) run in child processes
so a crash or hang cannot stop the remaining probes.
"""

import os
import platform
import subprocess
import sys

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("QT_QPA_FONTDIR", "C:/Windows/Fonts")

import PySide6  # noqa: E402
import shiboken6  # noqa: E402
from PySide6.QtCore import (  # noqa: E402
    ClassInfo,
    Property,
    QCoreApplication,
    QEvent,
    QEventLoop,
    QObject,
    QThread,
    QTimer,
    Qt,
    Signal,
    Slot,
    qInstallMessageHandler,
    qVersion,
)
from PySide6.QtGui import QGuiApplication  # noqa: E402
from PySide6.QtWidgets import QApplication, QFrame, QPushButton, QWidget  # noqa: E402

app = QApplication.instance() or QApplication(sys.argv)

DEFERRED = QEvent.Type.DeferredDelete.value


class EventTap(QObject):
    """Event filter recording selected event types (ChildAdded/Removed carry the child name)."""

    def __init__(self, target, log, types):
        super().__init__()
        self.log = log
        self.types = types
        target.installEventFilter(self)

    def eventFilter(self, obj, ev):
        t = ev.type()
        if t in self.types:
            entry = t.name
            if t in (QEvent.Type.ChildAdded, QEvent.Type.ChildRemoved):
                try:
                    entry = (t.name, ev.child().objectName())
                except Exception:
                    entry = (t.name, "?")
            self.log.append(entry)
        return False


def delete_now(obj):
    obj.deleteLater()
    QCoreApplication.sendPostedEvents(None, DEFERRED)


def probe_R00003():
    log = []
    p = QObject()
    tap = EventTap(p, log, (QEvent.Type.ChildAdded, QEvent.Type.ChildRemoved))
    c = QObject(p)
    after_ctor = list(log)
    c.setParent(None)
    print(f"R-00003: after ctor(parent) {after_ctor}; after setParent(None) {log}")


def probe_R00001_49_50():
    """Tree state observed from inside ChildRemoved / ChildAdded handlers during setParent."""
    old = QObject(); old.setObjectName("old")
    new = QObject(); new.setObjectName("new")
    c = QObject(old); c.setObjectName("c")
    seen = []

    class Snap(QObject):
        def eventFilter(self, obj, ev):
            t = ev.type()
            if t in (QEvent.Type.ChildAdded, QEvent.Type.ChildRemoved):
                p = c.parent()
                seen.append((t.name, obj.objectName(),
                             "c.parent=" + (p.objectName() if p else "None"),
                             "c in old.children=" + str(c in old.children()),
                             "c in new.children=" + str(c in new.children())))
            return False

    snap = Snap()
    old.installEventFilter(snap)
    new.installEventFilter(snap)
    c.setParent(new)
    p = c.parent()
    print(f"R-00049/R-00050: during handlers {seen}")
    print(f"R-00001: after setParent returns c.parent={p.objectName() if p else None}, "
          f"c in old={c in old.children()}, c in new={c in new.children()}")


def probe_R00057():
    """A blocked object still emits destroyed() (~QObject clears blockSig first)."""
    o = QObject()
    got = []
    o.destroyed.connect(lambda *a: got.append("destroyed"))
    o.blockSignals(True)
    delete_now(o)
    print(f"R-00057: destroyed while blocked -> {got}")


def probe_R00005():
    log = []
    root = QObject()
    root.destroyed.connect(lambda *a: log.append("root"))
    for n in ("c1", "c2", "c3"):
        ch = QObject(root)
        ch.destroyed.connect(lambda *a, n=n: log.append(n))
    delete_now(root)
    print(f"R-00005: destruction order {log}")


def probe_R00012():
    cnt = []
    o = QObject()
    o.destroyed.connect(lambda *a: cnt.append(1))
    o.deleteLater()
    o.deleteLater()
    QCoreApplication.processEvents()
    QCoreApplication.sendPostedEvents(None, DEFERRED)
    print(f"R-00012: destroyed count after two deleteLater: {len(cnt)}")


def probe_R00014():
    calls = []

    def zero_arg():
        calls.append("0-arg")

    def one_arg(obj=None):
        calls.append(type(obj).__name__ if obj is not None else "None")

    o = QObject()
    o.destroyed.connect(zero_arg)
    o.destroyed.connect(one_arg)
    del o
    print(f"R-00014: destroyed slots received: {calls}")


def probe_R00015():
    o = QObject()
    print(f"R-00015: objectName() unset -> {o.objectName()!r}")


def probe_R00016():
    o = QObject()
    seen = []
    o.objectNameChanged.connect(lambda n: seen.append(n))
    for v in ("a", "a", "", ""):
        o.setObjectName(v)
    print(f"R-00016: objectNameChanged emissions for sets a,a,'','': {seen}")


def probe_R00017():
    class MyBtn(QPushButton):
        pass

    root = QWidget()
    QPushButton(root)
    QPushButton(root)
    MyBtn(root)
    print(
        "R-00017: findChild(QAbstractButton) type "
        f"{type(root.findChild(QPushButton)).__name__}; "
        f"findChildren(QAbstractButton) count {len(root.findChildren(__import__('PySide6.QtWidgets', fromlist=['QAbstractButton']).QAbstractButton))}; "
        f"findChildren(MyBtn) count {len(root.findChildren(MyBtn))}"
    )


def probe_R00018():
    root = QObject()
    a = QObject(root); a.setObjectName("A")
    a1 = QObject(a)
    deep = QObject(a1); deep.setObjectName("t")
    b = QObject(root); b.setObjectName("B")
    shallow = QObject(b); shallow.setObjectName("t")
    found = root.findChild(QObject, "t")
    where = "deep (under A)" if found is deep else ("shallow (under B)" if found is shallow else repr(found))
    print(f"R-00018: findChild('t') returns {where}; "
          f"FindDirectChildrenOnly -> {root.findChild(QObject, 't', __import__('PySide6.QtCore', fromlist=['Qt']).Qt.FindChildOption.FindDirectChildrenOnly) is not None}")


def probe_R00019():
    root = QObject()
    QObject(root).setObjectName("x")  # Python wrapper dropped; C++ child owned by root
    print(f"R-00019: findChild('x') on unowned-wrapper child -> {root.findChild(QObject, 'x') is not None}")


def probe_R00020():
    root = QObject()
    u = QObject(root)
    n = QObject(root); n.setObjectName("n")
    omitted = root.findChild(QObject)
    empty = root.findChild(QObject, "")
    none = root.findChild(QObject, None)
    print(
        f"R-00020: findChild(QObject) -> {'unnamed' if omitted is u else 'named' if omitted is n else omitted}; "
        f"findChild(QObject,'') -> {'unnamed' if empty is u else 'named' if empty is n else empty}; "
        f"findChild(QObject,None) -> {'unnamed' if none is u else 'named' if none is n else none}; "
        f"findChildren(QObject) count {len(root.findChildren(QObject))}; "
        f"findChildren(QObject,'') count {len(root.findChildren(QObject, ''))}"
    )


def probe_R00021():
    p = QObject()
    c = QObject(p)
    print(f"R-00021: c.parent() is p {c.parent() is p}; p.children() len {len(p.children())}; "
          f"c.thread() is main {c.thread() == app.thread()}")


def probe_R00022():
    btn = QPushButton()
    got = []
    btn.clicked.connect(lambda *a: got.append(1))
    btn.blockSignals(True)
    btn.clicked.emit()
    while_blocked = len(got)
    btn.blockSignals(False)
    btn.clicked.emit()
    print(f"R-00022: inherited clicked while blocked -> {while_blocked} emissions; after unblock -> {len(got) - while_blocked}")


def probe_R00023():
    o = QObject()
    seq = [o.blockSignals(True), o.blockSignals(True), o.blockSignals(False), o.blockSignals(False)]
    print(f"R-00023: blockSignals returns (prev state) for True,True,False,False: {seq}")


def probe_R00024():
    from PySide6.QtCore import QSignalBlocker

    o = QObject()
    with QSignalBlocker(o):
        outer = o.signalsBlocked()
        with QSignalBlocker(o):
            inner = o.signalsBlocked()
        after_inner = o.signalsBlocked()
    after_outer = o.signalsBlocked()
    p = QObject()
    p.blockSignals(True)
    with QSignalBlocker(p):
        pass
    print(f"R-00024: outer={outer} inner={inner} after_inner={after_inner} after_outer={after_outer}; "
          f"pre-blocked after blocker={p.signalsBlocked()}")


def probe_R00025_26():
    import PySide6.QtCore as qc

    print(f"R-00025: QPointer in PySide6.QtCore={hasattr(qc, 'QPointer')}, "
          f"QWeakPointer={hasattr(qc, 'QWeakPointer')} -> not PySide-observable; Qt-source evidence only")
    print("R-00026: same as R-00025 -> not PySide-observable; Qt-source evidence only")


def probe_R00027():
    class Decl(QObject):
        _rw = 0

        def get_rw(self):
            return self._rw

        def set_rw(self, v):
            self._rw = v

        rw = Property(int, get_rw, set_rw)
        ro = Property(int, get_rw)

    o = Decl()
    o.setProperty("rw", 5)
    r_rw = o.setProperty("ro", 9)
    print(f"R-00027: writable setProperty -> value {o.property('rw')}; read-only setProperty -> "
          f"{r_rw}, value {o.property('ro')}")


def probe_R00028():
    o = QObject()
    r1 = o.setProperty("dyn", 3)
    r2 = o.setProperty("dyn", 4)
    names = [bytes(x).decode() for x in o.dynamicPropertyNames()]
    print(f"R-00028: undeclared setProperty returns {r1}, {r2}; value {o.property('dyn')}; dynamicPropertyNames {names}")


def probe_R00029():
    o = QObject()
    ev = []
    tap = EventTap(o, ev, (QEvent.Type.DynamicPropertyChange,))
    o.setProperty("k", 1)
    n_new = len(ev)
    o.setProperty("k", 1)
    n_same = len(ev)
    o.setProperty("k", 2)
    n_change = len(ev)
    print(f"R-00029: DynamicPropertyChange counts after new/same/changed: {n_new}/{n_same}/{n_change}")


def probe_R00030():
    o = QObject()
    ev = []
    tap = EventTap(o, ev, (QEvent.Type.DynamicPropertyChange,))
    o.setProperty("k", 1)
    before = len(ev)
    r1 = o.setProperty("k", None)
    names_after = [bytes(x).decode() for x in o.dynamicPropertyNames()]
    after_existing = len(ev)
    r2 = o.setProperty("absent", None)
    print(f"R-00030: set None on existing -> {r1}, property {o.property('k')}, names {names_after}, "
          f"events +{after_existing - before}; set None on absent -> {r2}, events +{len(ev) - after_existing}")


def probe_R00032():
    class Sub(QPushButton):
        pass

    class Base(QObject):
        pass

    class Sub2(Base):
        pass

    print(f"R-00032: QPushButton superClass {QPushButton.staticMetaObject.superClass().className()}; "
          f"Sub(QPushButton) metaObject superClass {Sub.staticMetaObject.superClass().className()}; "
          f"Sub2(Base(QObject)) chain Sub2 -> Base -> "
          f"{'Base' if False else ''}"
          f"{Sub2.staticMetaObject.superClass().className()} -> "
          f"{Sub2.staticMetaObject.superClass().superClass().className()}")


def probe_R00034():
    class Named(QObject):
        sig = Signal(str)
        sig2 = Signal(int, list)

    mo = Named.staticMetaObject
    sigs = [mo.method(i).methodSignature().data().decode() for i in range(mo.methodOffset(), mo.methodCount())]
    print(f"R-00034: signals in meta data {sigs}")


def probe_R00035():
    class Notif(QObject):
        sig = Signal(str)
        _v = ""

        def get_v(self):
            return self._v

        def set_v(self, v):
            self._v = v

        v = Property(str, get_v, set_v, notify=sig)

    mo = Notif.staticMetaObject
    idx = mo.indexOfProperty("v")
    prop = mo.property(idx)
    notify_idx = prop.notifySignalIndex()
    print(f"R-00035: notifySignalIndex {notify_idx} -> {mo.method(notify_idx).name().data().decode()}; "
          f"indexOfSignal('sig(QString)') {mo.indexOfSignal('sig(QString)')}")


def probe_R00036():
    class SubFrame(QFrame):
        pass

    print(f"R-00036: QFrame enumeratorCount {QFrame.staticMetaObject.enumeratorCount()}; "
          f"SubFrame(QFrame) enumeratorCount {SubFrame.staticMetaObject.enumeratorCount()} (expect sum over superdata); "
          f"QObject classInfoCount {QObject.staticMetaObject.classInfoCount()}")


def probe_a_b_c_d_e_f():
    # (a) see R-00020 (omitted vs '' vs None)
    # (b) deleteLater inside a slot of the same object
    class Emitter(QObject):
        sig = Signal()

    e = Emitter()
    seen = []

    def slot():
        e.deleteLater()
        seen.append(("in slot", shiboken6.isValid(e)))

    e.sig.connect(slot)
    e.sig.emit()
    seen.append(("after emit", shiboken6.isValid(e)))
    QCoreApplication.processEvents()
    seen.append(("after processEvents", shiboken6.isValid(e)))
    print(f"(b) deleteLater in own slot: {seen}")

    # (c) sendPostedEvents(None, DeferredDelete) at same level
    o = QObject()
    o.deleteLater()
    before = shiboken6.isValid(o)
    QCoreApplication.sendPostedEvents(None, DEFERRED)
    print(f"(c) sendPostedEvents(None, DeferredDelete): valid before {before}, after {shiboken6.isValid(o)}")

    # (d) setParent(current parent) sends no ChildAdded/ChildRemoved
    p = QObject()
    c = QObject(p)
    log = []
    tap = EventTap(p, log, (QEvent.Type.ChildAdded, QEvent.Type.ChildRemoved))
    c.setParent(p)
    print(f"(d) setParent(current parent) events: {log}")

    # (e) property() round-trip, declared and dynamic
    class Props(QObject):
        def __init__(self):
            super().__init__()
            self._v = 1

        def getv(self):
            return self._v

        def setv(self, v):
            self._v = v

        v = Property(int, getv, setv)

    q = Props()
    q.setProperty("v", 7)
    q.setProperty("dyn", "hi")
    print(f"(e) declared v round-trip {q.property('v')}; dynamic dyn round-trip {q.property('dyn')!r}; "
          f"missing {q.property('nope')!r}")

    # (f) ~QObject order: destroyed(parent), children deleted, then ChildRemoved at grandparent
    gp = QObject()
    p2 = QObject(gp)
    p2.setObjectName("p")
    c2 = QObject(p2)
    c2.setObjectName("c")
    flog = []
    p2.destroyed.connect(lambda *a: flog.append("p.destroyed"))
    c2.destroyed.connect(lambda *a: flog.append("c.destroyed"))
    gtap = EventTap(gp, flog, (QEvent.Type.ChildRemoved,))
    delete_now(p2)
    print(f"(f) ~QObject order at grandparent: {flog}")


def valid(o):
    return shiboken6.isValid(o)


def run_loop_ms(ms):
    loop = QEventLoop()
    QTimer.singleShot(ms, loop.quit)
    loop.exec()


class Emitter(QObject):
    sig = Signal()


class SelfDel(QObject):
    sig = Signal()


class Receiver(QObject):
    def __init__(self, sink):
        super().__init__()
        self.sink = sink

    @Slot()
    def on_sig(self):
        self.sink.append("called")


class LabelTap(QObject):
    """Event filter logging ChildAdded/ChildRemoved as (label, event, child name) into a shared log."""

    def __init__(self, target, log, label):
        super().__init__()
        self.log = log
        self.label = label
        target.installEventFilter(self)

    def eventFilter(self, obj, ev):
        if ev.type() in (QEvent.Type.ChildAdded, QEvent.Type.ChildRemoved):
            self.log.append((self.label, ev.type().name, ev.child().objectName()))
        return False


class TypeTap(QObject):
    def __init__(self, target, sink, etype):
        super().__init__()
        self.sink = sink
        self.etype = etype
        target.installEventFilter(self)

    def eventFilter(self, obj, ev):
        if ev.type() == self.etype:
            self.sink.append(1)
        return False


class Ticker(QObject):
    def __init__(self, sink):
        super().__init__()
        self.sink = sink
        self.startTimer(10)

    def timerEvent(self, ev):
        self.sink.append(1)


def probe_R00002():
    log = []
    old, new = QObject(), QObject()
    old.setObjectName("old")
    new.setObjectName("new")
    c = QObject(old)
    c.setObjectName("c")
    taps = [LabelTap(old, log, "old"), LabelTap(new, log, "new")]
    c.setParent(new)
    print(f"R-00002: setParent(old->new) event order {log}")


def probe_R00004():
    p = QObject()
    c = QObject(p)
    c.setParent(None)
    print(f"R-00004: after setParent(None) valid {valid(c)}; parent() is None {c.parent() is None}")


def probe_R00009():
    o = QObject()
    o.deleteLater()
    before = valid(o)
    QCoreApplication.sendPostedEvents(None, DEFERRED)
    after = valid(o)
    o2 = QObject()
    o2.deleteLater()
    loop_before = valid(o2)
    run_loop_ms(20)
    loop_after = valid(o2)
    print(f"R-00009: deleteLater valid before processing {before}; after sendPostedEvents(None, DeferredDelete) {after}; "
          f"event-loop path valid before loop {loop_before}, after loop return {loop_after}")


def probe_R00010():
    log = []
    holder = []

    def handler():
        o = QObject()
        holder.append(o)
        o.destroyed.connect(lambda *a: log.append("destroyed"))
        o.deleteLater()
        QCoreApplication.processEvents()
        log.append(("inside handler after processEvents, valid", valid(o)))

    def finish():
        log.append(("after handler returned to loop, valid", valid(holder[0])))
        loop.quit()

    loop = QEventLoop()
    QTimer.singleShot(0, handler)
    QTimer.singleShot(20, finish)
    loop.exec()
    log.append(("after loop exec, valid", valid(holder[0])))
    print(f"R-00010: {log}")


def probe_R00011():
    o = QObject()

    def last_handler():
        o.deleteLater()
        QCoreApplication.exit(0)

    QTimer.singleShot(0, last_handler)
    rc = app.exec()
    print(f"R-00011: after app.exec() returned rc={rc}, valid {valid(o)}")


def probe_R00013():
    etype = QEvent.Type(QEvent.Type.User.value + 1)
    obj = QObject()
    obj_hits, app_hits = [], []
    taps = [TypeTap(obj, obj_hits, etype), TypeTap(app, app_hits, etype)]
    QCoreApplication.postEvent(obj, QEvent(etype))
    shiboken6.delete(obj)
    QCoreApplication.sendPostedEvents(None, 0)
    app.removeEventFilter(taps[1])
    print(f"R-00013: custom event posted then obj deleted; obj-filter hits {len(obj_hits)}; app-filter hits {len(app_hits)}")


def probe_R00040():
    e, sink = Emitter(), []
    r = Receiver(sink)
    e.sig.connect(r.on_sig)
    e.sig.emit()
    before = len(sink)
    shiboken6.delete(e)
    QCoreApplication.processEvents()
    print(f"R-00040: slot calls before sender deleted {before}; after sender deleted {len(sink)}; "
          f"sender valid {valid(e)}; receiver valid {valid(r)}")


def probe_R00041():
    class Sg(QObject):
        sig = Signal()

    s = Sg()
    got = []
    s.sig.connect(lambda: got.append(1))
    s.blockSignals(True)
    s.sig.emit()
    blocked = len(got)
    s.blockSignals(False)
    s.sig.emit()
    print(f"R-00041: user Signal emissions while blockSignals(True) {blocked}; after unblock {len(got) - blocked}")


def probe_R00043():
    e, log = SelfDel(), []
    e.destroyed.connect(lambda *a: log.append("destroyed"))

    def slot():
        e.deleteLater()
        log.append(("in slot", valid(e)))

    def after_slot():
        log.append(("after slot returned (same emit)", valid(e)))

    e.sig.connect(slot)
    e.sig.connect(after_slot)
    e.sig.emit()
    log.append(("after emit returned", valid(e)))
    QCoreApplication.sendPostedEvents(None, DEFERRED)
    log.append(("after sendPostedEvents(DeferredDelete)", valid(e)))
    print(f"R-00043: {log}")


def probe_R00051():
    e, sink = Emitter(), []
    r = Receiver(sink)
    e.sig.connect(r.on_sig, Qt.ConnectionType.QueuedConnection)
    e.sig.emit()
    shiboken6.delete(r)
    QCoreApplication.processEvents()
    print(f"R-00051: queued slot after receiver deleted before processing -> sink {sink}")


def probe_R00053():
    ticks = []
    t = Ticker(ticks)
    run_loop_ms(50)
    before = len(ticks)
    shiboken6.delete(t)
    run_loop_ms(100)
    print(f"R-00053: timer events before delete {before}; after delete + 100 ms loop {len(ticks)} "
          f"(unchanged {len(ticks) == before}); valid {valid(t)}")


def probe_R00059():
    root = QObject()
    unnamed = [QObject(root), QObject(root)]
    named = QObject(root)
    named.setObjectName("n")
    n_empty = len(root.findChildren(QObject, ""))
    n_all = len(root.findChildren(QObject))
    print(f"R-00059: findChildren(QObject, '') count {n_empty} (2 unnamed children, 1 named); "
          f"findChildren(QObject) count {n_all}; findChild(QObject, '') is None {root.findChild(QObject, '') is None}")


def probe_R00060():
    class RO(QObject):
        def getv(self):
            return 3

        ro = Property(int, getv)

    o = RO()
    ok = o.setProperty("ro", 5)
    names = [bytes(n) for n in o.dynamicPropertyNames()]
    print(f"R-00060: setProperty on read-only declared 'ro' returned {ok}; value {o.property('ro')}; "
          f"dynamicPropertyNames contains 'ro' {b'ro' in names}")


def probe_R00066():
    parent = QWidget()
    parent.setObjectName("parent")
    child = QWidget(parent)
    child.setObjectName("child")
    log = []
    parent.destroyed.connect(lambda *a: log.append("parent"))
    child.destroyed.connect(lambda *a: log.append("child"))
    shiboken6.delete(parent)
    print(f"R-00066: QWidget destroyed order (expect parent first) {log}")


def probe_R00067():
    @ClassInfo(author="base")
    class CiBase(QObject):
        pass

    @ClassInfo(version="2")
    class CiSub(CiBase):
        pass

    print(f"R-00067: classInfoCount base {CiBase.staticMetaObject.classInfoCount()}; "
          f"sub (adds one) {CiSub.staticMetaObject.classInfoCount()} (expect base+1 = 2); "
          f"QObject {QObject.staticMetaObject.classInfoCount()}")


def probe_R00071():
    e, sink = Emitter(), []
    r = Receiver(sink)
    e.sig.connect(r.on_sig)
    e.sig.emit()
    before = len(sink)
    shiboken6.delete(r)
    e.sig.emit()
    print(f"R-00071: slot calls before receiver deleted {before}; after receiver deleted + sender emit {len(sink)}; "
          f"receiver valid {valid(r)}; sender valid {valid(e)}")


PROBES = [
    probe_R00001_49_50, probe_R00057, probe_R00003, probe_R00005, probe_R00012, probe_R00014, probe_R00015, probe_R00016,
    probe_R00017, probe_R00018, probe_R00019, probe_R00020, probe_R00021, probe_R00022,
    probe_R00023, probe_R00024, probe_R00025_26, probe_R00027, probe_R00028, probe_R00029,
    probe_R00030, probe_R00032, probe_R00034, probe_R00035, probe_R00036, probe_a_b_c_d_e_f,
    probe_R00002, probe_R00004, probe_R00009, probe_R00010, probe_R00013, probe_R00040, probe_R00041,
    probe_R00043, probe_R00051, probe_R00053, probe_R00059, probe_R00060, probe_R00066, probe_R00067,
    probe_R00071, probe_R00011,
]


# ---- R-00007: reparenting to self / descendant / other thread (child-process cases)

def case_self():
    o = QObject()
    p = QObject()
    o.setParent(p)
    o.setParent(o)
    print(f"R-00048 self: o.parent() is o {o.parent() is o}; o in o.children() {o in o.children()}", flush=True)


def case_desc():
    a = QObject()
    b = QObject(a)
    a.setParent(b)
    print(f"R-00048 descendant: a.parent() is b {a.parent() is b}; b.parent() is a {b.parent() is a}", flush=True)


def case_thread():
    t = QThread()
    t.start()
    w = QObject()
    w.moveToThread(t)
    p = QObject()
    c = QObject(p)
    c.setParent(w)
    print(f"R-00007 other-thread: c.parent() is None {c.parent() is None}; p.children() len {len(p.children())}", flush=True)
    t.quit()
    t.wait()


CASES = {"self": case_self, "desc": case_desc, "thread": case_thread}


def run_r00007_cases():
    for name in CASES:
        try:
            r = subprocess.run(
                [sys.executable, os.path.abspath(__file__), f"--case={name}"],
                capture_output=True, text=True, timeout=60,
            )
            for line in r.stdout.splitlines():
                if line.startswith(("R-00007", "R-00048", "R-00064")):
                    print(line)
            warns = [ln for ln in r.stderr.splitlines() if "QObject::setParent" in ln]
            for w in warns:
                print(f"R-00007 {name} stderr: {w}")
            if r.returncode != 0:
                print(f"R-00007 {name}: exit code {r.returncode}")
        except subprocess.TimeoutExpired:
            print(f"R-00007 {name}: timeout (hang)")


def main():
    if len(sys.argv) > 1 and sys.argv[1].startswith("--case="):
        name = sys.argv[1].split("=", 1)[1]
        msgs = []

        def handler(mode, context, message):
            msgs.append(message)
            sys.stderr.write(message + "\n")

        qInstallMessageHandler(handler)
        CASES[name]()
        hits = [m for m in msgs if "setParent" in m]
        print(f"R-00064: warning captured={bool(hits)} text={hits[0] if hits else ''!r} case={name}", flush=True)
        os._exit(0)
    print(f"PySide6 {PySide6.__version__}; Qt {qVersion()}")
    print(f"platform={QGuiApplication.platformName()} dpr={app.primaryScreen().devicePixelRatio()} os={platform.platform()}")
    for fn in PROBES:
        try:
            fn()
        except Exception as e:  # keep running remaining probes
            print(f"{fn.__name__}: ERROR {type(e).__name__}: {e}")
    run_r00007_cases()


if __name__ == "__main__":
    main()
    os._exit(0)
