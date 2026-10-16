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
                self.assertIn('#0369a1', hud.cards['claude'].title.styleSheet())
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
            config, hud = self._hud(directory, ui_mode='sidebar', sidebar_bottom_offset=40)
            try:
                work = QGuiApplication.primaryScreen().availableGeometry()
                self.assertEqual(hud.width(), 92)
                self.assertEqual(hud.geometry().right() + 1, work.x() + work.width())
                self.assertEqual(hud.geometry().bottom() + 1, work.y() + work.height() - 40)
                hud._on_data_fetched(UsageMetrics(provider_id='agy', metric1_val=3, metric1_text='3%', metric2_val=1,
                                                  metric2_text='1%'))
                self.assertEqual(hud.sidebar.buttons['agy'].display_text, '3%')
                self.assertEqual(hud.sidebar_cards['agy'].m1_val.text(), '3%')
                hud.set_sidebar_state('panel')
                self.assertEqual(hud.width(), 220)
                self.assertEqual(hud.geometry().right() + 1, work.x() + work.width())
                self.assertEqual(ConfigManager(config.path).get('sidebar_state'), 'panel')
                hud.set_sidebar_state('bogus')
                self.assertEqual(hud.width(), 220)
            finally:
                self._close(hud)

    def test_sidebar_does_not_overwrite_card_geometry_and_can_switch_back(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='cards', window_x=120, window_y=130)
            try:
                hud._apply_ui_mode('sidebar')
                hud._persist_geometry()
                self.assertEqual((config.get('window_x'), config.get('window_y')), (120, 130))
                self.assertIsNotNone(config.get('sidebar_bottom_offset'))
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

        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}),                 patch('ui.hud_window.QMenu', RecordingMenu):
            _, hud = self._hud(directory, ui_mode='cards')
            try:
                from PySide6.QtGui import QContextMenuEvent
                from PySide6.QtCore import QPoint
                hud.contextMenuEvent(QContextMenuEvent(QContextMenuEvent.Reason.Mouse, QPoint(1, 1), QPoint(1, 1)))
                style = next(a.menu() for a in captured[0].actions() if a.menu() and 'UI Style' in a.text())
                self.assertTrue(any('Sidebar' in a.text() for a in style.actions()))
            finally:
                self._close(hud)
