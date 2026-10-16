import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest.mock import patch

from PySide6.QtWidgets import QApplication
from core.config_manager import ConfigManager
from core.providers.base import UsageMetrics, MetricRow
from ui.provider_card import ProviderCardWidget
from ui.styles import THEMES
from ui.usage_table import UsageTable

APP = QApplication.instance() or QApplication([])


def make_table(scheme='scale', appearance='dark'):
    return UsageTable(THEMES[appearance], scheme)


class CardTests(unittest.TestCase):
    def test_extra_rows_show_hide_reuse_and_countdown(self):
        card = ProviderCardWidget('agy')
        reset = datetime.now(timezone.utc) + timedelta(hours=1)
        metrics = [MetricRow("C/G 5H", 30, "30%", reset),
                   MetricRow("C/G 7D", 15, "15%"),
                   MetricRow("ARGON 7D", subtext="需啟用 agy statusLine")]
        card.update_metrics(UsageMetrics(extra_metrics=metrics))
        self.assertEqual(len(card.extra_rows), 3)
        self.assertTrue(all(not row[0].isHidden() for row in card.extra_rows))
        self.assertEqual([row[1].text() for row in card.extra_rows], [m.title for m in metrics])
        self.assertEqual([row[2].text() for row in card.extra_rows], ['30%', '15%', '--'])
        with patch('ui.provider_card.BaseProvider.format_countdown', return_value='updated'):
            card.update_countdown()
        self.assertEqual(card.extra_rows[0][4].text(), '重設於: updated')
        self.assertEqual(card.extra_rows[2][4].text(), '需啟用 agy statusLine')
        original = card.extra_rows[0][0]
        card.update_metrics(UsageMetrics())
        self.assertTrue(all(row[0].isHidden() for row in card.extra_rows))
        card.update_metrics(UsageMetrics(extra_metrics=metrics[:1]))
        self.assertIs(card.extra_rows[0][0], original)
        self.assertFalse(original.isHidden())
        self.assertTrue(card.extra_rows[1][0].isHidden())
        card.update_metrics(UsageMetrics(error='offline', extra_metrics=metrics))
        self.assertTrue(all(row[0].isHidden() for row in card.extra_rows))
        claude = ProviderCardWidget('claude')
        claude.update_metrics(UsageMetrics(metric1_val=20))
        self.assertFalse(any(not row[0].isHidden() for row in claude.extra_rows))


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


class TableTests(unittest.TestCase):
    def test_error_recovery_clears_offline_state(self):
        table = make_table()
        col = table.columns['agy']
        table.update_metrics(UsageMetrics(provider_id='agy', error='timeout'))
        self.assertEqual(col.badge.text(), 'OFFLINE')
        self.assertTrue(col.dial.muted)
        table.update_metrics(UsageMetrics(provider_id='agy', metric1_val=20, metric1_text='20%'))
        self.assertNotEqual(col.badge.text(), 'OFFLINE')
        self.assertFalse(col.dial.muted)
        self.assertEqual(col.m2_val.text(), '--')
        self.assertEqual(col.header.toolTip(), '')

    def test_absent_timestamp_clears_previous_countdown(self):
        table = make_table()
        col = table.columns['claude']
        table.update_metrics(UsageMetrics(provider_id='claude', metric2_reset=datetime.now(timezone.utc) + timedelta(days=2)))
        self.assertNotEqual(col.m2_countdown.text(), '--:--:--')
        table.update_metrics(UsageMetrics(provider_id='claude'))
        self.assertEqual(col.m2_countdown.text(), '--:--:--')
        self.assertEqual(col.m1_reset.text(), '--:--')

    def test_stale_data_remains_visible_and_labelled(self):
        table = make_table()
        col = table.columns['agy']
        table.update_metrics(UsageMetrics(provider_id='agy', metric1_val=25, metric1_text='25%', metric2_val=10,
                                          metric2_text='10%', error='timeout', stale=True,
                                          last_success=datetime.now(timezone.utc)))
        self.assertEqual(col.dial.inner_text, '25%')
        self.assertEqual(col.m2_val.text(), '10 %')
        self.assertIn('STALE', col.badge.text())
        self.assertIn('timeout', col.header.toolTip())

    def test_ahead_of_pace_shows_overage_on_weekly_pill(self):
        table = make_table()
        col = table.columns['codex']
        table.update_metrics(UsageMetrics(provider_id='codex', metric2_val=62, metric2_text='62%',
                                          metric2_reset=datetime.now(timezone.utc) + timedelta(days=6)))
        self.assertIn('▲48', col.m2_val.text())
        self.assertIn('用完', col.m2_val.toolTip())

    def test_non_default_window_gets_caption(self):
        table = make_table()
        col = table.columns['codex']
        table.update_metrics(UsageMetrics(provider_id='codex', metric1_title='WINDOW 1D', metric1_val=5,
                                          metric1_text='5%', metric2_title='WINDOW 30D', metric2_val=10,
                                          metric2_text='10%'))
        self.assertEqual(col.dial.caption, '1D')
        self.assertIn('30D', col.m2_val.text())

    def test_scale_and_duo_colour_the_dial_differently(self):
        data = UsageMetrics(provider_id='claude', metric1_val=95, metric1_text='95%', metric2_val=10, metric2_text='10%')
        scale, duo = make_table('scale'), make_table('duo')
        scale.update_metrics(data)
        duo.update_metrics(data)
        dark = THEMES['dark']
        self.assertEqual(scale.columns['claude'].dial.inner[2], dark['scale']['red'])
        self.assertEqual(scale.columns['claude'].dial.outer[2], dark['scale']['green'])
        self.assertEqual(duo.columns['claude'].dial.inner[2], dark['duo']['inner'])
        self.assertEqual(duo.columns['claude'].dial.outer[2], dark['duo']['outer'])


class HudTests(unittest.TestCase):
    def _hud(self, directory, **settings):
        from ui.hud_window import HUDWindow
        config = ConfigManager(Path(directory) / 'config.json')
        config.update(settings)
        return config, HUDWindow(config)

    def _close(self, hud):
        hud.refresh_controller.stop()
        hud.countdown_timer.stop()
        hud.geometry_timer.stop()
        hud.close()

    def test_cards_appearance_menu_persists_and_keeps_data(self):
        from PySide6.QtWidgets import QMenu
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='cards', appearance='dark')
            try:
                hud._on_data_fetched(UsageMetrics(provider_id='claude', metric1_val=42, metric1_text='42%'))
                menu = QMenu(hud)
                hud.add_theme_menus(menu)
                colors, appearance = [action.menu() for action in menu.actions()]
                self.assertFalse(colors.menuAction().isVisible())
                light = next(action for action in appearance.actions() if action.data() == 'light')
                light.trigger()
                appearance.aboutToShow.emit()
                self.assertTrue(light.isChecked())
                self.assertIn('rgba(248, 250, 252, 0.94)', hud.styleSheet())
                self.assertIn('#B4532F', hud.cards['claude'].title.styleSheet())
                self.assertEqual(hud.cards['claude'].m1_val.text(), '42%')
                self.assertEqual(ConfigManager(config.path).get('appearance'), 'light')
                hud.cards['claude'].update_metrics(UsageMetrics(provider_id='claude', error='offline'))
                hud.set_appearance('dark')
                self.assertEqual(hud.cards['claude'].badge.text(), 'OFFLINE')
                self.assertIn('#ef4444', hud.cards['claude'].m1_val.styleSheet())
                self.assertIn('rgba(14, 17, 23, 0.94)', hud.styleSheet())
            finally:
                self._close(hud)

    def test_cards_follow_system_only_in_auto_mode(self):
        from PySide6.QtCore import Qt
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='cards', appearance='auto')
            try:
                with patch.object(hud, '_system_is_dark', return_value=False):
                    APP.styleHints().colorSchemeChanged.emit(Qt.ColorScheme.Light)
                    APP.processEvents()
                    self.assertFalse(hud.cards['claude'].dark)
                with patch.object(hud, '_system_is_dark', return_value=True):
                    APP.styleHints().colorSchemeChanged.emit(Qt.ColorScheme.Dark)
                    APP.processEvents()
                    self.assertTrue(hud.cards['claude'].dark)
                self.assertEqual(config.get('appearance'), 'auto')
                hud.set_appearance('light')
                with patch.object(hud, '_system_is_dark', return_value=True):
                    APP.styleHints().colorSchemeChanged.emit(Qt.ColorScheme.Dark)
                    APP.processEvents()
                    self.assertFalse(hud.cards['claude'].dark)
            finally:
                self._close(hud)

    def test_hud_preserves_independent_cards_layout_sizes(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='cards', layout_mode='vertical',
                                    vertical_width=350, vertical_height=520,
                                    horizontal_width=850, horizontal_height=200)
            hud.toggle_layout_mode()
            self.assertEqual(hud.width(), 850)
            hud.toggle_layout_mode()
            self.assertEqual(hud.width(), 350)
            self.assertEqual(hud.height(), 520)
            self._close(hud)

    def test_hud_dual_mode_toggle(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='cards')
            self.assertEqual(hud.config.get('ui_mode'), 'cards')
            hud.toggle_ui_mode()
            self.assertEqual(hud.config.get('ui_mode'), 'table')
            hud.toggle_ui_mode()
            self.assertEqual(hud.config.get('ui_mode'), 'cards')
            self._close(hud)

    def test_theme_switch_keeps_latest_data_and_persists(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='table')
            hud._on_data_fetched(UsageMetrics(provider_id='claude', metric1_val=42, metric1_text='42%'))
            hud.set_color_scheme('duo')
            hud.set_appearance('light')
            self.assertEqual(hud.table.columns['claude'].dial.inner_text, '42%')
            self.assertEqual(hud.table.columns['claude'].dial.t, THEMES['light'])
            self.assertEqual(ConfigManager(config.path).get('color_scheme'), 'duo')
            self.assertEqual(ConfigManager(config.path).get('appearance'), 'light')
            self._close(hud)

    def test_invalid_theme_values_are_ignored(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, color_scheme='neon')
            self.assertEqual(hud.color_scheme(), 'scale')
            hud.set_appearance('sepia')
            self.assertEqual(config.get('appearance'), 'auto')
            self._close(hud)

    def test_saved_size_is_restored_and_undersized_values_reset(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='table', table_width=520, table_height=410)
            self.assertEqual((hud.width(), hud.height()), (520, 410))
            self._close(hud)
            _, hud = self._hud(directory, ui_mode='table', table_width=50, table_height=50)
            self.assertEqual((hud.width(), hud.height()), (450, 350))
            self._close(hud)

    def test_sidebar_mode_docks_right_edge_and_switches_state(self):
        from PySide6.QtGui import QGuiApplication
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar', sidebar_edge='right', sidebar_offset=40)
            try:
                work = QGuiApplication.primaryScreen().availableGeometry()
                self.assertEqual(hud.width(), hud.sidebar.preferred_size().width())
                self.assertEqual(hud.geometry().right() + 1, work.x() + work.width())
                self.assertEqual(hud.geometry().y(), work.y() + 40)
                hud._on_data_fetched(UsageMetrics(provider_id='agy', metric1_val=3, metric1_text='3%', metric2_val=1,
                                                  metric2_text='1%'))
                self.assertEqual(hud.sidebar_cards['agy'].m1_val.text(), '3%')
                strip_w = hud.width()
                hud.set_sidebar_state('panel')
                self.assertGreater(hud.width(), strip_w)
                self.assertEqual(hud.geometry().right() + 1, work.x() + work.width())
                self.assertEqual(ConfigManager(config.path).get('sidebar_state'), 'panel')
                hud.set_sidebar_state('bogus')
                self.assertEqual(ConfigManager(config.path).get('sidebar_state'), 'panel')
            finally:
                self._close(hud)

    def test_sidebar_drag_snaps_to_any_edge_and_switches_orientation(self):
        from PySide6.QtGui import QGuiApplication
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar')
            try:
                work = QGuiApplication.primaryScreen().availableGeometry()
                vertical = hud.size()
                hud._docking = True
                hud.move(work.x() + 5, work.y() + 100)
                hud._docking = False
                hud._finish_sidebar_move()
                self.assertEqual(config.get('sidebar_edge'), 'left')
                self.assertEqual(hud.x(), work.x())
                self.assertEqual(hud.sidebar.orientation(), 'vertical')
                hud._docking = True
                hud.move(work.x() + 200, work.y() + 4)
                hud._docking = False
                hud._finish_sidebar_move()
                self.assertEqual(config.get('sidebar_edge'), 'top')
                self.assertEqual(config.get('sidebar_orientation'), 'horizontal')
                self.assertEqual(hud.y(), work.y())
                self.assertGreater(hud.width(), hud.height())
                hud._docking = True
                hud.move(work.x() + work.width() // 2, work.y() + work.height() // 2)
                hud._docking = False
                hud._finish_sidebar_move()
                self.assertEqual(config.get('sidebar_edge'), 'none')
                self.assertIsNotNone(config.get('sidebar_x'))
            finally:
                self._close(hud)

    def test_sidebar_scale_steps_and_persists(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar')
            try:
                base = hud.width()
                hud._on_sidebar_scale_step(5)
                self.assertGreater(hud.width(), base)
                self.assertEqual(ConfigManager(config.path).get('sidebar_scale'), 1.5)
                hud.set_sidebar_scale(9)
                self.assertEqual(ConfigManager(config.path).get('sidebar_scale'), 2.0)
            finally:
                self._close(hud)

    def test_sidebar_does_not_overwrite_card_geometry_and_can_switch_back(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='cards', window_x=120, window_y=130)
            try:
                hud._apply_ui_mode('sidebar')
                hud._persist_geometry()
                self.assertEqual((config.get('window_x'), config.get('window_y')), (120, 130))
                hud._apply_ui_mode('cards')
                self.assertTrue(hud.cards['claude'].isVisibleTo(hud))
                self.assertGreaterEqual(hud.width(), 250)
                self.assertFalse(hud.sidebar.isVisible())
            finally:
                self._close(hud)

    def test_sidebar_ignores_edge_resize(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='sidebar')
            try:
                from PySide6.QtCore import QPoint
                self.assertIsNone(hud._calc_edge(QPoint(0, 0)))
            finally:
                self._close(hud)

    def test_cards_window_grows_for_agy_extra_bars(self):
        from datetime import timedelta
        from core.providers.base import MetricRow
        for layout in ('horizontal', 'vertical'):
            with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
                _, hud = self._hud(directory, ui_mode='cards', layout_mode=layout)
                try:
                    hud.show()
                    before = hud.height()
                    reset = datetime.now(timezone.utc) + timedelta(hours=3)
                    hud._on_data_fetched(UsageMetrics(
                        provider_id='agy', metric1_val=1, metric1_text='1%', metric2_val=2, metric2_text='2%',
                        extra_metrics=[MetricRow('C/G 5H', 3, '3%', reset), MetricRow('C/G 7D', 4, '4%', reset),
                                       MetricRow('ARGON 7D', 5, '5%', reset)]))
                    QApplication.processEvents()
                    card = hud.cards['agy']
                    self.assertGreaterEqual(hud.height(), before)
                    self.assertGreaterEqual(card.height(), card.minimumSizeHint().height())
                    self.assertGreaterEqual(hud.minimumHeight(), card.minimumSizeHint().height())
                finally:
                    self._close(hud)

    def test_context_menu_lists_sidebar_mode(self):
        from PySide6.QtWidgets import QMenu
        captured = []

        class RecordingMenu(QMenu):
            def exec(self, *args):
                captured.append(self)

        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}),                 patch('ui.hud_menu.QMenu', RecordingMenu):
            _, hud = self._hud(directory, ui_mode='cards')
            try:
                from PySide6.QtGui import QContextMenuEvent
                from PySide6.QtCore import QPoint
                hud.contextMenuEvent(QContextMenuEvent(QContextMenuEvent.Reason.Mouse, QPoint(1, 1), QPoint(1, 1)))
                titles = [a.text() for a in captured[0].actions() if a.menu()]
                self.assertEqual(len(titles), 4)   # 介面 / 外觀 / 行為 / 更多
                style = next(a.menu() for a in captured[0].actions() if a.menu() and 'Interface' in a.text())
                self.assertTrue(any('Sidebar' in a.text() for a in style.actions()))
            finally:
                self._close(hud)


    def test_agy_main_rows_follow_the_setting_and_republish(self):
        from datetime import timedelta
        from core.providers.base import MetricRow
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='cards')
            try:
                reset = datetime.now(timezone.utc) + timedelta(hours=3)
                hud._on_data_fetched(UsageMetrics(
                    provider_id='agy', metric1_title='GEMINI 5H', metric1_val=0, metric1_text='0%',
                    metric2_title='GEMINI 7D', metric2_val=1, metric2_text='1%',
                    extra_metrics=[MetricRow('C/G 5H', 3, '3%', reset), MetricRow('C/G 7D', 15, '15%', reset),
                                   MetricRow('ARGON 7D', 21, '21%', reset)]))
                card = hud.cards['agy']
                self.assertEqual((card.m1_label.text(), card.m1_val.text()), ('C/G 5H', '3%'))
                self.assertEqual((card.m2_label.text(), card.m2_val.text()), ('ARGON 7D', '21%'))
                hud.set_agy_order(['GEMINI 5H', 'ARGON 7D', 'C/G 5H', 'C/G 7D', 'GEMINI 7D'])
                self.assertEqual(card.m1_label.text(), 'GEMINI 5H')
                self.assertEqual(card.m2_label.text(), 'ARGON 7D')
                self.assertEqual(ConfigManager(config.path).get('agy_order'),
                                 ['GEMINI 5H', 'ARGON 7D', 'C/G 5H', 'C/G 7D', 'GEMINI 7D'])
                hud.reset_agy_order()
                self.assertEqual(card.m1_label.text(), 'C/G 5H')
            finally:
                self._close(hud)

    def test_provider_order_applies_to_cards_table_and_sidebar(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='table')
            try:
                hud.set_provider_order(['codex', 'claude', 'agy'])
                self.assertEqual(list(hud.table.columns), ['codex', 'claude', 'agy'])
                self.assertEqual(hud.sidebar.order(), ['codex', 'claude', 'agy'])
                hud.set_provider_order(['codex', 'claude'])
                self.assertEqual(hud.provider_order(), ['codex', 'claude', 'agy'])
                hud._apply_ui_mode('cards')
                self.assertEqual(ConfigManager(config.path).get('provider_order'), ['codex', 'claude', 'agy'])
            finally:
                self._close(hud)

    def test_sidebar_edge_and_corner_drag_scales_proportionally(self):
        from PySide6.QtCore import QPoint
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar')
            try:
                g = hud.geometry()
                base = hud.size()
                hud._begin_resize(QPoint(g.left() + 2, g.top() + 2), 'lt')
                hud._resize_to(QPoint(g.left() - 40, g.top() - 40))
                self.assertGreater(hud.width(), base.width())
                self.assertAlmostEqual(hud.width() / hud.height(), base.width() / base.height(), delta=0.08)
                self.assertEqual(hud.geometry().bottom(), g.bottom())
                hud._end_resize()
                scale = ConfigManager(config.path).get('sidebar_scale')
                self.assertGreater(scale, 1.0)
                self.assertLessEqual(scale, 2.0)
                hud._begin_resize(QPoint(0, 0), 'r')
                hud._resize_to(QPoint(-10000, 0))
                hud._end_resize()
                self.assertEqual(ConfigManager(config.path).get('sidebar_scale'), 0.6)
            finally:
                self._close(hud)

    def test_menu_offers_agy_rows_inside_interface(self):
        from PySide6.QtWidgets import QMenu
        captured = []

        class RecordingMenu(QMenu):
            def exec(self, *args):
                captured.append(self)

        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}), \
                patch('ui.hud_menu.QMenu', RecordingMenu):
            _, hud = self._hud(directory, ui_mode='cards')
            try:
                from PySide6.QtGui import QContextMenuEvent
                from PySide6.QtCore import QPoint
                hud.contextMenuEvent(QContextMenuEvent(QContextMenuEvent.Reason.Mouse, QPoint(1, 1), QPoint(1, 1)))
                interface = next(a.menu() for a in captured[0].actions() if a.menu() and 'Interface' in a.text())
                rows = next(a.menu() for a in interface.actions() if a.menu() and 'Row order' in a.text())
                texts = [a.text() for a in rows.actions() if not a.isSeparator()]
                self.assertTrue(any(t.startswith('1. C/G 5H') for t in texts))
                self.assertTrue(any(t.startswith('5. ') for t in texts))
            finally:
                self._close(hud)

    def test_sidebar_sticks_to_the_corner_without_turning_sideways(self):
        from PySide6.QtGui import QGuiApplication
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar')
            try:
                work = QGuiApplication.primaryScreen().availableGeometry()
                hud._docking = True
                hud.move(work.right() - hud.width() - 3, work.bottom() - hud.height() - 8)
                hud._docking = False
                hud._finish_sidebar_move()
                self.assertEqual(config.get('sidebar_edge'), 'right')
                self.assertEqual(config.get('sidebar_orientation'), 'vertical')
                self.assertEqual(config.get('sidebar_offset'), 'end')
                self.assertEqual(hud.geometry().right() + 1, work.x() + work.width())
                self.assertEqual(hud.geometry().bottom() + 1, work.y() + work.height())
                # the same corner for a horizontal bar
                config.set_many({'sidebar_edge': 'bottom', 'sidebar_orientation': 'horizontal', 'sidebar_offset': None})
                hud.sidebar.set_orientation('horizontal')
                hud._dock_sidebar()
                hud._docking = True
                hud.move(work.right() - hud.width() - 5, work.bottom() - hud.height() - 4)
                hud._docking = False
                hud._finish_sidebar_move()
                self.assertEqual(config.get('sidebar_edge'), 'bottom')
                self.assertEqual(config.get('sidebar_orientation'), 'horizontal')
                self.assertEqual(config.get('sidebar_offset'), 'end')
                self.assertEqual(hud.geometry().right() + 1, work.x() + work.width())
            finally:
                self._close(hud)

    def test_dragging_a_row_title_in_the_agy_card_reorders_and_republishes(self):
        from datetime import timedelta
        from PySide6.QtCore import QEvent, QPoint, QPointF, Qt
        from PySide6.QtGui import QMouseEvent
        from core.providers.base import MetricRow
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='cards', layout_mode='vertical')
            try:
                hud.show()
                reset = datetime.now(timezone.utc) + timedelta(hours=3)
                hud._on_data_fetched(UsageMetrics(
                    provider_id='agy', metric1_title='GEMINI 5H', metric1_val=0, metric1_text='0%',
                    metric2_title='GEMINI 7D', metric2_val=1, metric2_text='1%',
                    extra_metrics=[MetricRow('C/G 5H', 3, '3%', reset), MetricRow('C/G 7D', 15, '15%', reset),
                                   MetricRow('ARGON 7D', 21, '21%', reset)]))
                APP.processEvents()
                card = hud.cards['agy']
                rows = card._drag_rows()
                self.assertEqual([t for t, _r, _l in rows], ['C/G 5H', 'ARGON 7D', 'GEMINI 5H', 'GEMINI 7D', 'C/G 7D'])
                label = rows[4][2]                                   # drag the last row (C/G 7D) ...
                start = label.mapToGlobal(QPoint(3, 3))
                top_y = card.mapToGlobal(QPoint(0, rows[0][1].top() + 1)).y()   # ... above the first row
                press = QMouseEvent(QEvent.Type.MouseButtonPress, QPointF(3, 3), QPointF(start), Qt.MouseButton.LeftButton,
                                    Qt.MouseButton.LeftButton, Qt.KeyboardModifier.NoModifier)
                self.assertTrue(card.eventFilter(label, press))
                move = QMouseEvent(QEvent.Type.MouseMove, QPointF(3, 3), QPointF(start.x(), top_y), Qt.MouseButton.NoButton,
                                   Qt.MouseButton.LeftButton, Qt.KeyboardModifier.NoModifier)
                card.eventFilter(label, move)
                release = QMouseEvent(QEvent.Type.MouseButtonRelease, QPointF(3, 3), QPointF(start.x(), top_y),
                                      Qt.MouseButton.LeftButton, Qt.MouseButton.NoButton, Qt.KeyboardModifier.NoModifier)
                card.eventFilter(label, release)
                self.assertEqual(ConfigManager(config.path).get('agy_order')[:2], ['C/G 7D', 'C/G 5H'])
                self.assertEqual(card.m1_label.text(), 'C/G 7D')
            finally:
                self._close(hud)

    def test_any_provider_rows_can_be_reordered_except_in_table_mode(self):
        from datetime import timedelta
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='cards', layout_mode='vertical')
            try:
                reset = datetime.now(timezone.utc) + timedelta(hours=3)
                hud._on_data_fetched(UsageMetrics(
                    provider_id='claude', metric1_title='SESSION 5H', metric1_val=27, metric1_text='27%', metric1_reset=reset,
                    metric2_title='WEEKLY 7D', metric2_val=9, metric2_text='9%', metric2_reset=reset))
                card = hud.cards['claude']
                self.assertEqual(card.m1_label.text(), 'SESSION 5H')
                hud.set_row_order('claude', ['WEEKLY 7D', 'SESSION 5H'])
                self.assertEqual((card.m1_label.text(), card.m1_val.text()), ('WEEKLY 7D', '9%'))
                self.assertEqual(ConfigManager(config.path).get('row_order'), {'claude': ['WEEKLY 7D', 'SESSION 5H']})
                self.assertEqual(hud.sidebar_cards['claude'].m1_label.text(), 'WEEKLY 7D')
                hud._apply_ui_mode('table')             # the dial keeps 5 h inside / week outside
                self.assertEqual(hud._latest_metrics['claude'].metric1_title, 'SESSION 5H')
                hud._apply_ui_mode('cards')
                self.assertEqual(hud._latest_metrics['claude'].metric1_title, 'WEEKLY 7D')
            finally:
                self._close(hud)

    def test_rows_are_hoverable_draggable_cells(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='cards')
            try:
                card = hud.cards['claude']
                hud._on_data_fetched(UsageMetrics(provider_id='claude', metric1_title='SESSION 5H', metric1_val=1,
                                                  metric2_title='WEEKLY 7D', metric2_val=2))
                for cell in (card.m1_cell, card.m2_cell):
                    self.assertEqual(cell.objectName(), 'RowCell')
                    self.assertIn(':hover', cell.styleSheet())
                self.assertEqual([t for t, _r, _c in card._drag_rows()], ['SESSION 5H', 'WEEKLY 7D'])
                self.assertEqual(card._cell_of(card.m1_val), card.m1_cell)
            finally:
                self._close(hud)

    def test_weekly_rows_show_the_wall_clock_reset_time(self):
        from datetime import timedelta
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='cards')
            try:
                five = datetime.now(timezone.utc) + timedelta(hours=3)
                week = datetime.now(timezone.utc) + timedelta(days=6, hours=5)
                hud._on_data_fetched(UsageMetrics(provider_id='claude', metric1_title='SESSION 5H', metric1_val=1,
                                                  metric1_reset=five, metric2_title='WEEKLY 7D', metric2_val=2,
                                                  metric2_reset=week))
                card = hud.cards['claude']
                self.assertNotIn('週', card.m1_sub.text())          # short windows: countdown only
                self.assertIn('·', card.m2_sub.text())              # weekly: "週X HH:MM · countdown"
                local = week.astimezone()
                self.assertIn(f"{local.strftime('%H:%M')}", card.m2_sub.text())
                self.assertIn('週', card.m2_sub.text())
            finally:
                self._close(hud)
