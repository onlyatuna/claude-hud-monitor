import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import unittest
from unittest.mock import patch
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import QPoint, QPointF, Qt, QRect, QSize, QEvent
from PySide6.QtGui import QWheelEvent, QContextMenuEvent, QMouseEvent, QColor
from datetime import datetime, timedelta, timezone
from ui.styles import THEMES
from PySide6.QtTest import QTest
from core.providers.base import UsageMetrics, MetricRow
from ui.provider_card import ProviderCardWidget
from ui.sidebar import SidebarWidget, SIDEBAR_TOKENS, FLASH_MS

APP = QApplication.instance() or QApplication([])

class SidebarTests(unittest.TestCase):
    def setUp(self):
        self.cards={key:ProviderCardWidget(key) for key in ('claude','agy','codex')}
        self.sidebar=SidebarWidget(self.cards)
        self.sidebar.resize(self.sidebar.preferred_size()); self.sidebar.show(); APP.processEvents()
    def tearDown(self):
        self.sidebar.close_flyout(); self.sidebar.close(); self.sidebar.deleteLater(); APP.processEvents()
    def fit(self):
        self.sidebar.resize(self.sidebar.preferred_size())
        APP.processEvents()

    def test_states_and_signals(self):
        states=[]
        self.sidebar.stateChangeRequested.connect(states.append)
        QTest.mouseClick(self.sidebar.toggle_button, Qt.LeftButton)
        self.assertEqual(states,['panel'])
        self.assertEqual(self.sidebar.state(),'strip')
        for state in ('panel','strip','panel','strip'):
            self.sidebar.set_state(state)
            self.sidebar.set_state(state)
            self.fit()
            self.assertEqual(self.sidebar.state(),state)
            self.assertTrue(all(c.isHidden() == (state=='strip') for c in self.cards.values()))
        with self.assertRaises(ValueError): self.sidebar.set_state('bad')
        with self.assertRaises(ValueError): self.sidebar.set_orientation('bad')
        with self.assertRaises(ValueError): self.sidebar.set_dock_edge('bad')
        with self.assertRaises(ValueError): self.sidebar.set_appearance(True,'bad')

    def test_geometry_scaling_and_layout(self):
        initial=self.sidebar.preferred_size()
        self.assertLessEqual(initial.width(),80)
        self.assertEqual(initial.width(),72)
        self.sidebar.set_scale(1.5)
        scaled=self.sidebar.preferred_size()
        self.assertAlmostEqual(scaled.width(),initial.width()*1.5,delta=3)
        self.assertAlmostEqual(scaled.height(),initial.height()*1.5,delta=3)
        self.sidebar.set_scale(0.5)
        self.assertEqual(self.sidebar.scale(),0.6)
        self.sidebar.set_scale(1)
        self.sidebar.set_orientation('horizontal')
        self.assertGreater(self.sidebar.preferred_size().width(),self.sidebar.preferred_size().height())
        self.fit()
        self.assertEqual(self.sidebar.size(),self.sidebar.preferred_size())
        self.sidebar.set_state('panel')
        self.fit()
        self.assertLess(self.cards['claude'].x(),self.cards['agy'].x())
        self.assertLess(self.cards['agy'].x(),self.cards['codex'].x())
        self.sidebar.set_orientation('vertical')
        self.fit()
        self.assertLess(self.cards['claude'].y(),self.cards['agy'].y())
        self.assertLess(self.cards['agy'].y(),self.cards['codex'].y())
        self.assertEqual(self.sidebar.preferred_size().width(),220)
        self.sidebar.set_state('strip')
        self.assertTrue(all(c.isHidden() for c in self.cards.values()))

    def test_metrics_and_external_ownership(self):
        reset=datetime.now(timezone.utc)+timedelta(hours=2)
        data=UsageMetrics(provider_id='claude',metric1_val=20,metric1_text='20%',metric2_val=8,metric1_reset=reset)
        with patch.object(self.cards['claude'],'update_metrics') as update:
            self.sidebar.update_metrics(data)
            update.assert_not_called()
        button=self.sidebar.buttons['claude']
        self.assertEqual(button.rows, [('5H',20,self.sidebar._color(20)),('7D',8,self.sidebar._color(8,'outer'))])
        self.assertEqual(button.display,['20%','8%'])
        self.assertFalse(hasattr(button, 'dial'))
        self.sidebar.set_scale(0.6)
        self.assertFalse(button.show_labels)
        self.sidebar.set_scale(1)
        self.assertTrue(button.show_labels)
        self.sidebar.update_metrics(UsageMetrics(provider_id='claude'))
        self.assertEqual(button.display,['--','--'])
        self.assertTrue(all(row[1] is None for row in button.rows))
        self.sidebar.update_metrics(UsageMetrics(provider_id='claude',error='offline',metric1_val=20))
        self.assertTrue(button.offline)
        self.assertEqual(button.display,['OFF','--'])
        self.assertTrue(all(row[1] is None for row in button.rows))
        self.sidebar.update_metrics(UsageMetrics(provider_id='claude',error='timeout',stale=True,metric1_val=0,metric1_text='0%'))
        self.assertTrue(button.stale)
        self.assertFalse(button.offline)
        self.assertEqual(button.display,['0%','--'])
        self.assertEqual(self.sidebar.tokens['stale'], '#f59e0b')
        self.assertIn('STALE',button.accessibleDescription())

    def test_agy_dots(self):
        self.sidebar.update_metrics(UsageMetrics(provider_id='agy',extra_metrics=[MetricRow('C/G 7D',15),MetricRow('ARGON 7D',21)]))
        button=self.sidebar.buttons['agy']
        self.assertEqual(button.dot_colors,[THEMES['dark']['scale']['green']]*2)
        self.assertIn('C/G 7D 已用 15%',button.toolTip())
        self.sidebar.set_appearance(False,'duo')
        self.assertEqual(button.dot_colors,[THEMES['light']['scale']['green']]*2)
        self.sidebar.update_metrics(UsageMetrics(provider_id='agy'))
        self.assertEqual(button.dot_colors,[])
        self.sidebar.update_metrics(UsageMetrics(provider_id='agy',extra_metrics=[MetricRow('C/G 7D',None)]))
        self.assertEqual(button.dot_colors,[THEMES['light']['neutral']])

    def test_tile_colors_and_contrast(self):
        for dark in (True,False):
            for scheme in ('scale','duo'):
                self.sidebar.set_appearance(dark,scheme)
                self.sidebar.update_metrics(UsageMetrics(provider_id='claude',metric1_val=95,metric2_val=10))
                button = self.sidebar.buttons['claude']
                self.assertEqual([r[2] for r in button.rows], [self.sidebar._color(95),self.sidebar._color(10,'outer')])
                self.assertNotEqual(button.rows[0][2],button.rows[1][2])
                for row in button.rows:
                    for state in ('background','hover','pressed'):
                        self.assertGreaterEqual(button.contrast(button.text_color(row[2]),self.sidebar.tokens[state]),3)
        self.sidebar.set_appearance(True,'scale')
        scale_colors = [r[2] for r in self.sidebar.buttons['claude'].rows]
        self.sidebar.set_appearance(True,'duo')
        self.assertNotEqual(scale_colors,[r[2] for r in self.sidebar.buttons['claude'].rows])

    def test_tooltip_countdown_only_and_no_dial_import(self):
        self.sidebar.update_metrics(UsageMetrics(provider_id='agy',metric1_val=21,metric2_val=1,
            extra_metrics=[MetricRow('C/G 7D',15),MetricRow('ARGON 7D',21)]))
        button = self.sidebar.buttons['agy']
        for text in ('已用','重設於 --','C/G 7D','ARGON 7D'):
            self.assertIn(text,button.toolTip())
        self.assertNotIn('?',button.toolTip())
        with patch('ui.sidebar.BaseProvider.format_countdown',return_value='countdown') as countdown:
            before = list(button.rows)
            self.sidebar.update_countdown()
            self.assertEqual(button.rows,before)
            self.assertIn('countdown',button.toolTip())
            self.assertEqual(countdown.call_count,4)
        import inspect
        import ui.sidebar
        self.assertNotIn('UsageDial',inspect.getsource(ui.sidebar))
        self.assertFalse(hasattr(self.sidebar,'settingsRequested'))

    def test_context_wheel_drag(self):
        contexts=[]; scales=[]; drags=[]
        self.sidebar.contextMenuRequested.connect(contexts.append)
        self.sidebar.scaleStepRequested.connect(scales.append)
        self.sidebar.dragRequested.connect(lambda:drags.append(True))
        button=self.sidebar.buttons['claude']
        pos=button.mapToGlobal(QPoint(10,10))
        APP.sendEvent(button,QContextMenuEvent(QContextMenuEvent.Mouse,QPoint(10,10),pos))
        self.assertEqual(contexts,[pos])
        def wheel(delta, modifiers):
            APP.sendEvent(button,QWheelEvent(QPointF(10,10),QPointF(pos),QPoint(),QPoint(0,delta),Qt.NoButton,modifiers,Qt.NoScrollPhase,False))
        wheel(240,Qt.ControlModifier)
        wheel(-120,Qt.ControlModifier)
        wheel(120,Qt.NoModifier)
        self.assertEqual(scales,[1,1,-1])
        self.assertEqual(self.sidebar.scale(),1)
        QTest.mousePress(button,Qt.LeftButton,pos=QPoint(10,10))
        QTest.mouseMove(button,QPoint(25,10))
        QTest.mouseMove(button,QPoint(30,10))
        QTest.mouseRelease(button,Qt.LeftButton,pos=QPoint(30,10))
        self.assertEqual(drags,[True])
        self.assertIsNone(self.sidebar._active)

    def test_independent_setter_order_and_card_font_restore(self):
        other=SidebarWidget({k:ProviderCardWidget(k) for k in self.cards})
        other.show(); APP.processEvents()    # polished like the shown fixture so font metrics match
        try:
            self.sidebar.set_scale(1.5)
            self.sidebar.set_orientation('horizontal')
            self.sidebar.set_state('panel')
            other.set_state('panel')
            other.set_orientation('horizontal')
            other.set_scale(1.5)
            size_a, size_b = self.sidebar.preferred_size(), other.preferred_size()
            self.assertEqual(size_a.width(), size_b.width())
            self.assertLessEqual(abs(size_a.height() - size_b.height()), 4)
            card=self.cards['claude']
            self.assertIn('21px',card.m1_val.styleSheet())
            self.sidebar.set_scale(1.5)
            self.assertIn('21px',card.m1_val.styleSheet())
            self.sidebar.set_state('strip')
            self.assertIn('14px',card.m1_val.styleSheet())
        finally:
            other.close()
            other.deleteLater()

    def test_flyout_directions(self):
        self.sidebar.move(500,400)
        button=self.sidebar.buttons['claude']
        origin=button.mapToGlobal(QPoint())
        size=QSize(260,200)
        area=QRect(0,0,2000,1500)
        for edge in ('right','left','top','bottom',None):
            self.sidebar.set_dock_edge(edge)
            pos=self.sidebar._flyout_position(button,size,area)
            self.assertTrue(area.contains(QRect(pos,size)))
            if edge in ('right',None): self.assertEqual(pos.x()+size.width(),origin.x())
            if edge=='left': self.assertEqual(pos.x(),origin.x()+button.width())
            if edge=='top': self.assertEqual(pos.y(),origin.y()+button.height())
            if edge=='bottom': self.assertEqual(pos.y()+size.height(),origin.y())
        self.assertTrue(QRect(490,390,300,250).contains(QRect(self.sidebar._flyout_position(button,size,QRect(490,390,300,250)),size)))

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


    def test_row_labels_follow_the_chosen_rows(self):
        self.sidebar.update_metrics(UsageMetrics(provider_id='agy', metric1_title='C/G 5H', metric1_val=3,
                                                 metric2_title='ARGON 7D', metric2_val=21,
                                                 extra_metrics=[MetricRow('GEMINI 5H', 0), MetricRow('GEMINI 7D', 1),
                                                                MetricRow('C/G 7D', 15)]))
        button = self.sidebar.buttons['agy']
        self.assertEqual([r[0] for r in button.rows], ['CG 5H', 'Ar 7D'])
        self.assertEqual(len(button.dot_colors), 3)
        self.sidebar.update_metrics(UsageMetrics(provider_id='claude', metric1_title='SESSION 5H', metric2_title='WEEKLY 7D'))
        self.assertEqual([r[0] for r in self.sidebar.buttons['claude'].rows], ['5H', '7D'])

    def test_order_applies_to_strip_and_panel_and_rejects_bad_values(self):
        self.sidebar.set_order(['codex', 'claude', 'agy'])
        ys = [self.sidebar.buttons[k].y() for k in ('codex', 'claude', 'agy')]
        self.assertEqual(ys, sorted(ys))
        with self.assertRaises(ValueError):
            self.sidebar.set_order(['claude', 'agy'])
        self.sidebar.set_state('panel')
        cards = [self.sidebar.cards[k] for k in ('codex', 'claude', 'agy')]
        self.assertEqual([c.y() for c in cards], sorted(c.y() for c in cards))

    def test_press_hold_drag_reorders_and_emits_once(self):
        seen = []
        self.sidebar.orderChanged.connect(seen.append)
        button = self.sidebar.buttons['claude']
        button._press_pos = QPoint(5, 5)
        button._begin_reorder()
        self.assertTrue(button._reordering)
        target = self.sidebar.buttons['codex'].mapToGlobal(QPoint(5, self.sidebar.buttons['codex'].height() - 2))
        self.sidebar._move_reorder('claude', target)
        self.assertEqual(self.sidebar.order()[-1], 'claude')
        self.sidebar._finish_reorder()
        self.assertEqual(seen, [self.sidebar.order()])

    def test_click_pins_then_second_click_on_same_tile_unpins(self):
        self.sidebar._click_provider('claude')
        self.assertEqual(self.sidebar._active, 'claude')
        self.assertTrue(self.sidebar._pinned)
        self.sidebar._hover('codex')                      # hover while pinned switches the content
        self.assertEqual(self.sidebar._active, 'codex')
        self.assertTrue(self.sidebar._pinned)
        self.sidebar._click_provider('codex')             # clicking what is already showing must not close it
        self.assertEqual(self.sidebar._active, 'codex')
        self.assertTrue(self.sidebar._flyout.isVisible())
        self.assertEqual(self.sidebar._pinned_key, 'codex')
        self.sidebar._click_provider('codex')             # second click on the pinned tile unpins and closes
        self.assertIsNone(self.sidebar._active)
        self.assertFalse(self.sidebar._flyout.isVisible())

    def test_tile_shows_an_alert_mark_from_ninety_percent(self):
        button = self.sidebar.buttons['claude']
        self.sidebar.update_metrics(UsageMetrics(provider_id='claude', metric1_title='SESSION 5H', metric1_val=89,
                                                 metric2_title='WEEKLY 7D', metric2_val=10))
        self.assertFalse(button.alert)
        self.sidebar.update_metrics(UsageMetrics(provider_id='claude', metric1_title='SESSION 5H', metric1_val=90,
                                                 metric2_title='WEEKLY 7D', metric2_val=10))
        self.assertTrue(button.alert)
        self.assertIn('90%', button.toolTip())
        self.sidebar.update_metrics(UsageMetrics(provider_id='claude', metric1_title='SESSION 5H', metric1_val=95,
                                                 metric2_title='WEEKLY 7D', metric2_val=10, error='offline'))
        self.assertFalse(button.alert)                       # offline tiles do not claim a level
        self.sidebar.update_metrics(UsageMetrics(provider_id='agy', metric1_val=1, metric2_val=2,
                                                 extra_metrics=[MetricRow('ARGON 7D', 93)]))
        self.assertTrue(self.sidebar.buttons['agy'].alert)   # extra rows count too
        pixmap = button.indicators.grab()                    # painting the mark must not raise
        self.assertFalse(pixmap.isNull())

    def test_tile_flashes_when_a_window_resets(self):
        now = datetime.now(timezone.utc)
        button = self.sidebar.buttons['codex']
        def data(val, reset):
            return UsageMetrics(provider_id='codex', metric1_title='WINDOW 5H', metric1_val=val, metric1_reset=reset,
                                metric2_title='WINDOW 7D', metric2_val=5, metric2_reset=now + timedelta(days=5))
        self.sidebar.update_metrics(data(80, now + timedelta(hours=1)))
        self.assertEqual(button.flash, 0.0)
        self.sidebar.update_metrics(data(82, now + timedelta(hours=1)))          # normal growth
        self.assertEqual(button.flash, 0.0)
        self.sidebar.update_metrics(data(3, now + timedelta(hours=5)))           # new window: lower use, later reset
        self.assertEqual(button.flash, 1.0)
        QTest.qWait(FLASH_MS + 400)
        self.assertEqual(button.flash, 0.0)
        self.sidebar.update_metrics(data(40, now + timedelta(hours=5, minutes=2)))   # tiny drift is not a reset
        self.assertEqual(button.flash, 0.0)

    def test_bar_and_number_always_share_one_colour(self):
        for dark in (True, False):
            for scheme in ('scale', 'duo'):
                self.sidebar.set_appearance(dark, scheme)
                for value in (0, 30, 60, 85, 95):
                    self.sidebar.update_metrics(UsageMetrics(provider_id='codex', metric1_val=value, metric2_val=value))
                    button = self.sidebar.buttons['codex']
                    for index in (0, 1):
                        shown = button.row_color(index)
                        for state in ('background', 'hover', 'pressed'):
                            self.assertGreaterEqual(button.contrast(shown, self.sidebar.tokens[state]), 3)
                        raw = QColor(button.rows[index][2])
                        adjusted = QColor(shown)
                        hue_gap = abs(raw.hslHueF() - adjusted.hslHueF())
                        if raw.name() != adjusted.name() and raw.hslSaturationF() > 0.1:
                            self.assertLess(min(hue_gap, 1 - hue_gap), 0.04)   # same hue, only lightness moved


if __name__=='__main__': unittest.main()
