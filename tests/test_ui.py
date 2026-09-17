import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import unittest
from datetime import datetime, timezone
from PySide6.QtWidgets import QApplication
from core.providers.base import UsageMetrics
from ui.provider_card import ProviderCardWidget

APP = QApplication.instance() or QApplication([])

class CardTests(unittest.TestCase):
    def test_error_recovery_clears_text(self):
        card = ProviderCardWidget('agy')
        card.update_metrics(UsageMetrics(provider_id='agy', error='timeout'))
        card.update_metrics(UsageMetrics(provider_id='agy', metric1_val=20, metric1_text='20%'))
        self.assertNotIn('timeout', card.m1_sub.text())
        self.assertEqual(card.m2_val.text(), '--')
        self.assertEqual(card.toolTip(), '')
    def test_absent_timestamp_clears_previous_countdown(self):
        card = ProviderCardWidget('claude')
        card.update_metrics(UsageMetrics(metric2_reset=datetime.now(timezone.utc)))
        card.update_metrics(UsageMetrics())
        self.assertEqual(card.m2_sub.text(), '重設於: --')
    def test_stale_data_remains_visible_and_labelled(self):
        card = ProviderCardWidget('agy')
        card.update_metrics(UsageMetrics(metric1_val=25, metric1_text='25%', error='timeout', stale=True, last_success=datetime.now(timezone.utc)))
        self.assertEqual(card.m1_val.text(), '25%')
        self.assertEqual(card.badge.text(), 'STALE')
        self.assertEqual(card.toolTip(), 'timeout')

    def test_hud_preserves_independent_layout_sizes(self):
        import tempfile
        from pathlib import Path
        from unittest.mock import patch
        from core.config_manager import ConfigManager
        from ui.hud_window import HUDWindow
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config = ConfigManager(Path(directory) / 'config.json')
            config.update({'layout_mode': 'vertical', 'vertical_width': 350, 'vertical_height': 520, 'horizontal_width': 850, 'horizontal_height': 200})
            hud = HUDWindow(config)
            hud.toggle_layout_mode()
            self.assertEqual(hud.width(), 850)
            hud.toggle_layout_mode()
            self.assertEqual(hud.width(), 350)
            self.assertEqual(hud.height(), 520)
            hud.refresh_controller.stop()
            hud.countdown_timer.stop()
            hud.geometry_timer.stop()
            hud.close()
