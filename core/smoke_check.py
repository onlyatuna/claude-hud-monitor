"""Explicit offline packaged smoke check; never loads account credentials."""
import os
import sys
import tempfile
from pathlib import Path


def run():
    os.environ['QT_QPA_PLATFORM'] = 'offscreen'
    from PySide6.QtWidgets import QApplication
    from core.config_manager import ConfigManager, get_config_path
    from core.providers.base import UsageMetrics
    from ui.hud_window import HUDWindow
    app = QApplication.instance() or QApplication([])
    with tempfile.TemporaryDirectory(prefix='hud-smoke-') as directory:
        # Exercise frozen path selection with an isolated user-data directory.
        previous = os.environ.get('APPDATA')
        os.environ['APPDATA'] = directory
        try:
            if getattr(sys, 'frozen', False) and sys.platform == 'win32':
                path = get_config_path()
                if Path(directory).resolve() not in Path(path).resolve().parents:
                    raise RuntimeError('Frozen config did not use user-data directory')
            else:
                path = Path(directory) / 'config.json'
            config = ConfigManager(path)
            config.set('opacity', 0.55)
            if ConfigManager(path).get('opacity') != 0.55:
                raise RuntimeError('Settings did not survive reload')
            hud = HUDWindow(config, providers={})
            hud.cards['agy'].update_metrics(UsageMetrics(error='test'))
            hud.cards['agy'].update_metrics(UsageMetrics(metric1_val=20, metric1_text='20%'))
            if 'test' in hud.cards['agy'].m1_sub.text():
                raise RuntimeError('Error survived recovery')
            hud.toggle_layout_mode()
            app.processEvents()
            hud.refresh_controller.stop()
            hud.countdown_timer.stop()
            hud.geometry_timer.stop()
            hud.close()
        finally:
            if previous is None:
                os.environ.pop('APPDATA', None)
            else:
                os.environ['APPDATA'] = previous
    return 0
