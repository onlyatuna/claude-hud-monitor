import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import unittest
from unittest.mock import patch
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import QPoint, Qt
from PySide6.QtTest import QTest
from core.providers.base import UsageMetrics, MetricRow
from ui.provider_card import ProviderCardWidget
from ui.sidebar import SidebarWidget, SIDEBAR_TOKENS

APP = QApplication.instance() or QApplication([])

class SidebarTests(unittest.TestCase):
    def setUp(self):
        self.cards={key:ProviderCardWidget(key) for key in ('claude','agy','codex')}
        self.sidebar=SidebarWidget(self.cards)
        self.sidebar.resize(self.sidebar.preferred_size()); self.sidebar.show(); APP.processEvents()
    def tearDown(self):
        self.sidebar.close_flyout(); self.sidebar.close(); self.sidebar.deleteLater(); APP.processEvents()
    def test_states_and_signals(self):
        states=[]; settings=[]
        self.sidebar.stateChangeRequested.connect(states.append)
        self.sidebar.settingsRequested.connect(lambda:settings.append(True))
        self.sidebar.toggle_button.click(); self.sidebar.settings_button.click()
        self.assertEqual(states,['panel']); self.assertEqual(settings,[True])
        self.assertEqual(self.sidebar.state(),'strip')
        self.assertEqual(self.sidebar.preferred_size().width(),92)
        self.sidebar.set_state('panel'); self.sidebar.set_state('panel'); APP.processEvents()
        self.assertEqual(self.sidebar.preferred_size().width(),220)
        self.assertTrue(all(self.sidebar.isAncestorOf(c) and c.isVisible() for c in self.cards.values()))
        self.assertEqual(self.sidebar._panel_layout.count(),5)
        self.sidebar.set_state('strip'); self.assertEqual(self.sidebar.preferred_size().width(),92)
        self.assertTrue(all(c.isHidden() for c in self.cards.values()))
        self.sidebar.set_state('panel'); self.assertEqual(self.sidebar._panel_layout.count(),5)
        with self.assertRaises(ValueError): self.sidebar.set_state('bad')
    def test_metrics_and_external_ownership(self):
        data=UsageMetrics(provider_id='agy',metric1_val=0,metric2_val=1,extra_metrics=[MetricRow(val=v) for v in (15,15,21)])
        with patch.object(self.cards['agy'],'update_metrics') as update:
            self.sidebar.update_metrics(data); update.assert_not_called()
        button=self.sidebar.buttons['agy']; self.assertEqual(button.display_text,'21%')
        self.assertIn('21%',button.toolTip())
        self.sidebar.update_metrics(UsageMetrics(provider_id='agy')); self.assertEqual(button.display_text,'--')
        self.sidebar.update_metrics(UsageMetrics(provider_id='agy',error='offline')); self.assertEqual(button.display_text,'OFF')
        self.sidebar.update_metrics(UsageMetrics(provider_id='agy',error='timeout',stale=True,metric1_val=0))
        self.assertTrue(button.stale); self.assertFalse(button.offline); self.assertEqual(button.display_text,'0%')
    def test_flyout_reparent_pin_and_restore(self):
        self.sidebar.buttons['agy'].click(); APP.processEvents()
        card=self.cards['agy']
        self.assertIs(card.parentWidget(),self.sidebar._flyout)
        self.assertTrue(self.sidebar._pinned)
        with patch('ui.sidebar.QCursor.pos',return_value=QPoint(-1000,-1000)):
            self.sidebar._schedule_close(); self.sidebar._maybe_close()
        self.assertTrue(self.sidebar._flyout.isVisible())
        self.sidebar.close_flyout(); self.assertIs(card.parentWidget(),self.sidebar._storage)
        card.update_metrics(UsageMetrics(provider_id='agy',metric1_val=25,metric1_text='25%'))
        self.assertEqual(card.m1_val.text(),'25%')
        self.sidebar._open_flyout('claude'); self.sidebar.set_state('panel')
        self.assertFalse(self.sidebar._flyout.isVisible())
        self.assertTrue(all(c.isVisible() for c in self.cards.values()))
    def test_unpinned_leave_and_hover(self):
        self.sidebar._hover('claude'); self.assertEqual(self.sidebar._hover_timer.interval(),300)
        self.sidebar._hover_timer.timeout.emit(); self.assertEqual(self.sidebar._active,'claude')
        with patch('ui.sidebar.QCursor.pos',return_value=QPoint(-1000,-1000)):
            self.sidebar._schedule_close(); self.assertEqual(self.sidebar._leave_timer.interval(),600)
            self.sidebar._maybe_close()
        self.assertIsNone(self.sidebar._active)
    def test_keyboard_and_geometry_without_motion(self):
        button = self.sidebar.buttons['codex']
        button.setFocus()
        with patch.object(self.sidebar, '_animations_enabled', return_value=False):
            QTest.keyClick(button, Qt.Key_Return)
        APP.processEvents()
        self.assertTrue(self.sidebar._pinned)
        self.assertEqual(self.sidebar._flyout.width(), 260)
        self.assertTrue(button.screen().availableGeometry().contains(self.sidebar._flyout.geometry()))
        self.assertEqual(self.sidebar._animation.state(), self.sidebar._animation.State.Stopped)
        QTest.keyClick(button, Qt.Key_Escape)
        self.assertIsNone(self.sidebar._active)
        QTest.keyClick(button, Qt.Key_Space)
        self.assertEqual(self.sidebar._active, 'codex')
        self.sidebar.buttons['agy'].click()
        self.assertEqual(self.sidebar._active, 'agy')
        self.assertIs(self.cards['codex'].parentWidget(), self.sidebar._storage)

    def test_themes(self):
        self.assertEqual(set(SIDEBAR_TOKENS['dark']),set(SIDEBAR_TOKENS['light']))
        for dark in (False,True):
            with patch.object(self.cards['agy'],'set_appearance') as appearance:
                self.sidebar.set_appearance(dark); appearance.assert_not_called()
            self.sidebar.update_countdown(); self.sidebar.grab()

if __name__=='__main__': unittest.main()

