"""Explicit offline packaged smoke check; never loads account credentials."""
import os
import sys
import tempfile
from pathlib import Path


def run():
    os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
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
            hud._on_data_fetched(UsageMetrics(provider_id='agy', error='test'))
            hud._on_data_fetched(UsageMetrics(provider_id='agy', metric1_val=20, metric1_text='20%'))
            if 'test' in hud.cards['agy'].m1_sub.text():
                raise RuntimeError('Cards error survived recovery')
            if hud.table.columns['agy'].badge.text() == 'OFFLINE':
                raise RuntimeError('Table error survived recovery')
            hud.toggle_layout_mode()
            hud.toggle_ui_mode()
            hud.set_color_scheme('duo')
            hud.set_appearance('dark')
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
