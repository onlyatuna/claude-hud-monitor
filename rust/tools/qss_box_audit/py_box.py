"""PySide6 oracle: minimumSize/maximumSize/sizeHint/minimumSizeHint of widgets under QSS min/max-width/height
with padding/border. Usage: [QT_ENABLE_HIGHDPI_SCALING=0] python py_box.py <out.json>"""
import os, sys, json
os.environ.setdefault('QT_QPA_PLATFORM', 'windows')
from pathlib import Path
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication, QPushButton, QLabel, QFrame, QProgressBar
APP = QApplication([])
spec = json.load(open(Path(__file__).with_name('cases.json'), encoding='utf-8'))
APP.setStyleSheet(spec['sheet'])
T = {'QPushButton': QPushButton, 'QLabel': QLabel, 'QFrame': QFrame, 'QProgressBar': QProgressBar}
sz = lambda s: [s.width(), s.height()]
out = {'meta': {'dpr': APP.primaryScreen().devicePixelRatio()}}
for name, typ, text, *opts in spec['cases']:
    w = T[typ](text) if text else T[typ]()
    w.setObjectName(name)
    if typ == 'QProgressBar':
        w.setTextVisible(True)
        if 'vertical' in opts:
            w.setOrientation(Qt.Vertical)
    w.ensurePolished()
    out[name] = {'min': sz(w.minimumSize()), 'max': sz(w.maximumSize()), 'hint': sz(w.sizeHint()), 'minHint': sz(w.minimumSizeHint())}
open(sys.argv[1], 'w', encoding='utf-8').write(json.dumps(out, indent=1))
