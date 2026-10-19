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


class CompactTableTests(unittest.TestCase):
    def _shown(self, scheme='scale', appearance='dark'):
        table = make_table(scheme, appearance)
        table.resize(450, 300)
        table.show()
        APP.processEvents()
        return table

    def test_compact_keeps_only_the_dials_in_provider_order(self):
        table = UsageTable(THEMES['dark'], 'scale', provider_ids=['codex', 'claude', 'agy'])
        table.resize(450, 300)
        table.show()
        APP.processEvents()
        self.assertFalse(table.is_compact())
        table.set_compact(True)
        APP.processEvents()
        self.assertTrue(table.is_compact())
        self.assertFalse(table.full_view.isVisibleTo(table))
        self.assertTrue(table.compact_view.isVisibleTo(table))
        dials = table.compact_dials()
        self.assertEqual(len(dials), 3)
        self.assertEqual([d.provider_id for d in dials], ['codex', 'claude', 'agy'])
        table.set_compact(False)
        APP.processEvents()
        self.assertTrue(table.full_view.isVisibleTo(table))
        self.assertFalse(table.compact_view.isVisibleTo(table))

    def test_compact_dials_follow_the_data(self):
        table = self._shown()
        reset = datetime.now(timezone.utc) + timedelta(hours=2)
        table.update_metrics(UsageMetrics(provider_id='claude', metric1_val=42, metric1_text='42%', metric1_reset=reset,
                                          metric2_val=71, metric2_text='71%',
                                          metric2_reset=datetime.now(timezone.utc) + timedelta(days=3)))
        full = table.columns['claude'].dial
        small = next(d for d in table.compact_dials() if d.provider_id == 'claude')
        self.assertEqual(small.inner_text, '42%')
        self.assertEqual(small.inner[0], full.inner[0])
        self.assertEqual(small.outer[0], full.outer[0])
        self.assertEqual(small.inner[2], full.inner[2])

    def test_compact_dials_have_no_caption_and_a_smaller_minimum(self):
        table = self._shown()
        table.update_metrics(UsageMetrics(provider_id='codex', metric1_title='WINDOW 1D', metric1_val=5, metric1_text='5%',
                                          metric2_title='WINDOW 30D', metric2_val=10, metric2_text='10%'))
        full = table.columns['codex'].dial
        small = next(d for d in table.compact_dials() if d.provider_id == 'codex')
        self.assertEqual(full.caption, '1D')
        self.assertEqual(small.caption, '')
        self.assertLess(small.minimumWidth(), full.minimumWidth())
        self.assertEqual(small.minimumWidth(), 48)

    def test_compact_tooltip_carries_what_the_screen_no_longer_shows(self):
        table = self._shown()
        reset = datetime.now(timezone.utc) + timedelta(hours=2)
        table.update_metrics(UsageMetrics(provider_id='agy', metric1_val=30, metric1_text='30%', metric1_reset=reset,
                                          metric2_val=60, metric2_text='60%',
                                          metric2_reset=datetime.now(timezone.utc) + timedelta(days=2)))
        small = next(d for d in table.compact_dials() if d.provider_id == 'agy')
        tip = small.toolTip()
        self.assertIn('Antigravity', tip)
        self.assertIn('30%', tip)
        self.assertIn('60%', tip)
        self.assertIn('5 小時', tip)
        self.assertIn('1 週', tip)

    def test_offline_and_stale_states_reach_the_compact_dials(self):
        table = self._shown()
        table.update_metrics(UsageMetrics(provider_id='agy', error='timeout'))
        small = next(d for d in table.compact_dials() if d.provider_id == 'agy')
        self.assertTrue(small.muted)
        self.assertEqual(small.inner_text, 'ERR')
        self.assertIn('timeout', small.toolTip())
        table.update_metrics(UsageMetrics(provider_id='agy', metric1_val=25, metric1_text='25%', metric2_val=10,
                                          metric2_text='10%', error='timeout', stale=True,
                                          last_success=datetime.now(timezone.utc)))
        self.assertFalse(small.muted)
        self.assertEqual(small.inner_text, '25%')
        self.assertIn('STALE', small.toolTip())

    def test_compact_row_gives_each_dial_a_square_to_draw_in(self):
        table = UsageTable(THEMES['dark'], 'scale')
        table.set_compact(True)
        table.resize(240, 90)
        table.show()
        APP.processEvents()
        widths = [d.width() for d in table.compact_dials()]
        self.assertTrue(all(w >= 48 for w in widths))
        self.assertLessEqual(max(widths) - min(widths), 1)


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

    def test_panel_appearance_persists_and_keeps_data(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel', appearance='dark')
            try:
                hud._on_data_fetched(UsageMetrics(provider_id='claude', metric1_val=42, metric1_text='42%'))
                hud.set_appearance('light')
                self.assertIn('rgba(248, 250, 252, 0.94)', hud.styleSheet())
                self.assertIn('#B4532F', hud.sidebar_cards['claude'].title.styleSheet())
                self.assertEqual(hud.sidebar_cards['claude'].m1_val.text(), '42%')
                self.assertEqual(ConfigManager(config.path).get('appearance'), 'light')
                hud.sidebar_cards['claude'].update_metrics(UsageMetrics(provider_id='claude', error='offline'))
                hud.set_appearance('dark')
                self.assertEqual(hud.sidebar_cards['claude'].badge.text(), 'OFFLINE')
                self.assertIn('#ef4444', hud.sidebar_cards['claude'].m1_val.styleSheet())
                self.assertIn('rgba(14, 17, 23, 0.94)', hud.styleSheet())
            finally:
                self._close(hud)


    def test_panel_follow_system_only_in_auto_mode(self):
        from PySide6.QtCore import Qt
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar', appearance='auto')
            try:
                with patch.object(hud, '_system_is_dark', return_value=False):
                    APP.styleHints().colorSchemeChanged.emit(Qt.ColorScheme.Light)
                    APP.processEvents()
                    self.assertFalse(hud.sidebar_cards['claude'].dark)
                with patch.object(hud, '_system_is_dark', return_value=True):
                    APP.styleHints().colorSchemeChanged.emit(Qt.ColorScheme.Dark)
                    APP.processEvents()
                    self.assertTrue(hud.sidebar_cards['claude'].dark)
                self.assertEqual(config.get('appearance'), 'auto')
                hud.set_appearance('light')
                with patch.object(hud, '_system_is_dark', return_value=True):
                    APP.styleHints().colorSchemeChanged.emit(Qt.ColorScheme.Dark)
                    APP.processEvents()
                    self.assertFalse(hud.sidebar_cards['claude'].dark)
            finally:
                self._close(hud)

    def test_hud_dual_mode_toggle(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar')
            self.assertEqual(hud.config.get('ui_mode'), 'sidebar')
            hud.toggle_ui_mode()
            self.assertEqual(hud.config.get('ui_mode'), 'table')
            hud.toggle_ui_mode()
            self.assertEqual(hud.config.get('ui_mode'), 'sidebar')
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
            config, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='strip', sidebar_edge='right', sidebar_offset=40)
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
            config, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='strip')
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

    def test_sidebar_does_not_overwrite_table_geometry_and_can_switch_back(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='table', window_x=120, window_y=130)
            try:
                hud._apply_ui_mode('sidebar')
                hud._persist_geometry()
                self.assertEqual((config.get('window_x'), config.get('window_y')), (120, 130))
                hud._apply_ui_mode('table')
                self.assertTrue(hud.table.isVisibleTo(hud))
                self.assertGreaterEqual(hud.width(), 380)
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

    def test_context_menu_lists_only_table_and_sidebar_styles(self):
        from PySide6.QtWidgets import QMenu
        captured = []

        class RecordingMenu(QMenu):
            def exec(self, *args):
                captured.append(self)

        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}),                 patch('ui.hud_menu.QMenu', RecordingMenu):
            _, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel')
            try:
                from PySide6.QtGui import QContextMenuEvent
                from PySide6.QtCore import QPoint
                hud.contextMenuEvent(QContextMenuEvent(QContextMenuEvent.Reason.Mouse, QPoint(1, 1), QPoint(1, 1)))
                titles = [a.text() for a in captured[0].actions() if a.menu()]
                self.assertEqual(len(titles), 4)   # 介面 / 外觀 / 行為 / 更多
                style = next(a.menu() for a in captured[0].actions() if a.menu() and 'Interface' in a.text())
                styles = [a.text() for a in style.actions() if a.isCheckable() and 'Row' not in a.text()][:2]
                self.assertEqual(len(styles), 2)
                self.assertTrue(any('Table' in t for t in styles))
                self.assertTrue(any('Sidebar' in t for t in styles))
                self.assertFalse(any('Classic' in a.text() or 'Cards' in a.text() for a in style.actions()))
                self.assertFalse(any('Vertical' in a.text() or 'Horizontal' in a.text() for a in style.actions()))
            finally:
                self._close(hud)


    def _shown_table(self, directory, **settings):
        config, hud = self._hud(directory, ui_mode='table', **settings)
        hud.show()
        APP.processEvents()
        return config, hud

    def test_table_collapses_to_dials_when_shrunk_and_expands_when_enlarged(self):
        from ui import table_layout as tl
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._shown_table(directory, table_width=450, table_height=350)
            try:
                self.assertFalse(hud.table.is_compact())
                self.assertEqual(hud.minimumSize().width(), tl.compact_min_size(3)[0])   # free to shrink past the old 380x280
                hud.resize(300, 200)
                APP.processEvents()
                self.assertTrue(hud.table.is_compact())
                self.assertTrue(hud.title_label.isHidden())
                self.assertTrue(hud.time_label.isHidden())
                self.assertTrue(hud.status_dot.isHidden())
                hud.resize(380 + tl.HYSTERESIS - 1, 300)        # inside the hysteresis band: stays compact
                APP.processEvents()
                self.assertTrue(hud.table.is_compact())
                hud.resize(420, 320)
                APP.processEvents()
                self.assertFalse(hud.table.is_compact())
                self.assertFalse(hud.title_label.isHidden())
                self.assertFalse(hud.time_label.isHidden())
            finally:
                self._close(hud)

    def test_table_can_be_dragged_down_to_the_compact_minimum(self):
        from ui import table_layout as tl
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._shown_table(directory)
            try:
                hud.resize(10, 10)
                APP.processEvents()
                self.assertTrue(hud.table.is_compact())
                self.assertEqual((hud.width(), hud.height()), tl.compact_min_size(3))
                self.assertEqual(hud.minimumSize().width(), tl.compact_min_size(3)[0])
                for dial in hud.table.compact_dials():
                    self.assertGreaterEqual(dial.width(), tl.COMPACT_DIAL_MIN)
                    self.assertGreaterEqual(dial.height(), tl.COMPACT_DIAL_MIN)
            finally:
                self._close(hud)

    def test_compact_and_full_sizes_are_remembered_separately(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._shown_table(directory, table_width=520, table_height=410)
            try:
                hud.resize(260, 100)
                APP.processEvents()
                hud._persist_geometry()
                saved = ConfigManager(config.path)
                self.assertTrue(saved.get('table_compact'))
                self.assertEqual((saved.get('table_compact_width'), saved.get('table_compact_height')), (260, 100))
                self.assertEqual((saved.get('table_width'), saved.get('table_height')), (520, 410))
                hud.resize(600, 420)
                APP.processEvents()
                hud._persist_geometry()
                saved = ConfigManager(config.path)
                self.assertFalse(saved.get('table_compact'))
                self.assertEqual((saved.get('table_width'), saved.get('table_height')), (600, 420))
                self.assertEqual((saved.get('table_compact_width'), saved.get('table_compact_height')), (260, 100))
            finally:
                self._close(hud)

    def test_compact_table_is_restored_after_restart(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='table', table_compact=True,
                               table_compact_width=260, table_compact_height=100)
            try:
                self.assertTrue(hud.table.is_compact())
                self.assertEqual((hud.width(), hud.height()), (260, 100))
                self.assertTrue(hud.title_label.isHidden())
            finally:
                self._close(hud)

    def test_compact_table_survives_a_style_round_trip_and_theme_change(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='table', table_compact=True,
                               table_compact_width=260, table_compact_height=100)
            try:
                hud._on_data_fetched(UsageMetrics(provider_id='claude', metric1_val=42, metric1_text='42%'))
                hud.set_appearance('light')           # rebuilds the table
                self.assertTrue(hud.table.is_compact())
                small = next(d for d in hud.table.compact_dials() if d.provider_id == 'claude')
                self.assertEqual(small.inner_text, '42%')
                hud._apply_ui_mode('sidebar')
                hud._apply_ui_mode('table')
                self.assertTrue(hud.table.is_compact())
                self.assertEqual((hud.width(), hud.height()), (260, 100))
            finally:
                self._close(hud)

    def test_reset_geometry_returns_the_table_to_its_full_default_size(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='table', table_compact=True,
                                    table_compact_width=260, table_compact_height=100)
            try:
                hud._reset_geometry()
                APP.processEvents()
                self.assertFalse(hud.table.is_compact())
                self.assertEqual((hud.width(), hud.height()), (450, 350))
            finally:
                self._close(hud)

    def test_classic_cards_style_is_gone(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='table')
            try:
                for name in ('cards', 'cards_container', 'layout_toggle_btn', 'toggle_layout_mode'):
                    self.assertFalse(hasattr(hud, name), name)
                hud._apply_ui_mode('cards')               # a stale caller must not resurrect the style
                self.assertIn(hud.config.get('ui_mode'), ('table', 'sidebar'))
            finally:
                self._close(hud)

    def _agy_five_bars(self):
        from datetime import timedelta
        from core.providers.base import MetricRow
        reset = datetime.now(timezone.utc) + timedelta(hours=3)
        return UsageMetrics(
            provider_id='agy', metric1_val=1, metric1_text='1%', metric2_val=2, metric2_text='2%',
            extra_metrics=[MetricRow('C/G 5H', 3, '3%', reset), MetricRow('C/G 7D', 4, '4%', reset),
                           MetricRow('ARGON 7D', 5, '5%', reset)])

    def _assert_panel_fits(self, hud, work):
        """Every card is fully inside the window, nothing is stretched, and the window is on screen."""
        sb = hud.sidebar
        self.assertEqual(hud.height(), sb.preferred_size().height())
        self.assertEqual(hud.width(), sb.preferred_size().width())
        for pid, card in sb.cards.items():
            self.assertTrue(card.isVisibleTo(hud), pid)
            bottom = card.mapTo(hud, card.rect().bottomLeft()).y()
            self.assertLess(bottom, hud.height(), pid)
            self.assertGreaterEqual(card.height(), card.minimumSizeHint().height(), pid)
        g = hud.geometry()
        self.assertGreaterEqual(g.y(), work.y())
        self.assertLessEqual(g.bottom() + 1, work.y() + work.height())
        self.assertGreaterEqual(g.x(), work.x())
        self.assertLessEqual(g.right() + 1, work.x() + work.width())

    def test_panel_fits_agy_five_bars_as_soon_as_the_data_arrives(self):
        from PySide6.QtGui import QGuiApplication
        work = QGuiApplication.primaryScreen().availableGeometry()
        cases = (dict(sidebar_edge='right', sidebar_offset=None, sidebar_orientation='vertical'),
                 dict(sidebar_edge='left', sidebar_offset=40, sidebar_orientation='vertical'),
                 dict(sidebar_edge='right', sidebar_offset='end', sidebar_orientation='vertical'),
                 dict(sidebar_edge='bottom', sidebar_offset=None, sidebar_orientation='horizontal'))
        for case in cases:
            with self.subTest(**case), tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
                _, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel', **case)
                try:
                    hud.show()
                    APP.processEvents()
                    before = hud.height()
                    hud._on_data_fetched(self._agy_five_bars())
                    # no event-loop turn yet: the window must already be big enough, not clipped
                    if case['sidebar_orientation'] == 'vertical':
                        self.assertGreater(hud.height(), before)
                    self._assert_panel_fits(hud, work)
                    APP.processEvents()      # polish settles the new rows' real height (they measure 2 px taller unstyled)
                    self._assert_panel_fits(hud, work)
                    settled = hud.geometry()
                    hud._on_data_fetched(self._agy_five_bars())
                    APP.processEvents()
                    self.assertEqual(hud.geometry(), settled)   # republishing the same data does not move or resize
                    if case['sidebar_edge'] == 'left':
                        self.assertEqual(hud.geometry().y(), work.y() + 40)
                    if case['sidebar_offset'] == 'end':
                        self.assertEqual(hud.geometry().bottom() + 1, work.y() + work.height())
                finally:
                    self._close(hud)

    def test_panel_shrinks_back_when_the_agy_extra_bars_disappear(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel')
            try:
                hud.show()
                APP.processEvents()
                base = hud.height()
                hud._on_data_fetched(self._agy_five_bars())
                APP.processEvents()
                self.assertGreater(hud.height(), base)
                hud._on_data_fetched(UsageMetrics(provider_id='agy', metric1_val=1, metric1_text='1%'))
                APP.processEvents()
                self.assertEqual(hud.height(), base)
                self.assertEqual(hud.height(), hud.sidebar.preferred_size().height())
            finally:
                self._close(hud)

    def test_agy_main_rows_follow_the_setting_and_republish(self):
        from datetime import timedelta
        from core.providers.base import MetricRow
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel')
            try:
                reset = datetime.now(timezone.utc) + timedelta(hours=3)
                hud._on_data_fetched(UsageMetrics(
                    provider_id='agy', metric1_title='GEMINI 5H', metric1_val=0, metric1_text='0%',
                    metric2_title='GEMINI 7D', metric2_val=1, metric2_text='1%',
                    extra_metrics=[MetricRow('C/G 5H', 3, '3%', reset), MetricRow('C/G 7D', 15, '15%', reset),
                                   MetricRow('ARGON 7D', 21, '21%', reset)]))
                card = hud.sidebar_cards['agy']
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
                hud._apply_ui_mode('sidebar')
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
            _, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel')
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
            config, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel')
            try:
                hud.show()
                reset = datetime.now(timezone.utc) + timedelta(hours=3)
                hud._on_data_fetched(UsageMetrics(
                    provider_id='agy', metric1_title='GEMINI 5H', metric1_val=0, metric1_text='0%',
                    metric2_title='GEMINI 7D', metric2_val=1, metric2_text='1%',
                    extra_metrics=[MetricRow('C/G 5H', 3, '3%', reset), MetricRow('C/G 7D', 15, '15%', reset),
                                   MetricRow('ARGON 7D', 21, '21%', reset)]))
                APP.processEvents()
                card = hud.sidebar_cards['agy']
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
            config, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel')
            try:
                reset = datetime.now(timezone.utc) + timedelta(hours=3)
                hud._on_data_fetched(UsageMetrics(
                    provider_id='claude', metric1_title='SESSION 5H', metric1_val=27, metric1_text='27%', metric1_reset=reset,
                    metric2_title='WEEKLY 7D', metric2_val=9, metric2_text='9%', metric2_reset=reset))
                card = hud.sidebar_cards['claude']
                self.assertEqual(card.m1_label.text(), 'SESSION 5H')
                hud.set_row_order('claude', ['WEEKLY 7D', 'SESSION 5H'])
                self.assertEqual((card.m1_label.text(), card.m1_val.text()), ('WEEKLY 7D', '9%'))
                self.assertEqual(ConfigManager(config.path).get('row_order'), {'claude': ['WEEKLY 7D', 'SESSION 5H']})
                self.assertEqual(hud.sidebar_cards['claude'].m1_label.text(), 'WEEKLY 7D')
                hud._apply_ui_mode('table')             # the dial keeps 5 h inside / week outside
                self.assertEqual(hud._latest_metrics['claude'].metric1_title, 'SESSION 5H')
                hud._apply_ui_mode('sidebar')
                self.assertEqual(hud._latest_metrics['claude'].metric1_title, 'WEEKLY 7D')
            finally:
                self._close(hud)

    def test_rows_are_hoverable_draggable_cells(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel')
            try:
                card = hud.sidebar_cards['claude']
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
            _, hud = self._hud(directory, ui_mode='sidebar', sidebar_state='panel')
            try:
                five = datetime.now(timezone.utc) + timedelta(hours=3)
                week = datetime.now(timezone.utc) + timedelta(days=6, hours=5)
                hud._on_data_fetched(UsageMetrics(provider_id='claude', metric1_title='SESSION 5H', metric1_val=1,
                                                  metric1_reset=five, metric2_title='WEEKLY 7D', metric2_val=2,
                                                  metric2_reset=week))
                card = hud.sidebar_cards['claude']
                self.assertNotIn('週', card.m1_sub.text())          # short windows: countdown only
                self.assertIn('·', card.m2_sub.text())              # weekly: "週X HH:MM · countdown"
                local = week.astimezone()
                self.assertIn(f"{local.strftime('%H:%M')}", card.m2_sub.text())
                self.assertIn('週', card.m2_sub.text())
            finally:
                self._close(hud)

    def test_sidebar_dims_when_idle_and_recovers_on_hover(self):
        from unittest.mock import PropertyMock
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar', opacity=0.9)
            try:
                hud._fade_timer.stop()
                with patch.object(type(hud), 'underMouse', lambda self: False):
                    hud._apply_opacity()
                    self.assertAlmostEqual(hud.windowOpacity(), 0.9 * 0.6, delta=0.02)
                with patch.object(type(hud), 'underMouse', lambda self: True):
                    hud._apply_opacity()
                    self.assertAlmostEqual(hud.windowOpacity(), 0.9, delta=0.02)
                with patch.object(type(hud), 'underMouse', lambda self: False):
                    hud.toggle_idle_fade()                       # setting off: never dims
                    self.assertAlmostEqual(hud.windowOpacity(), 0.9, delta=0.02)
                    hud.toggle_idle_fade()
                    config.set('click_through', True)            # click-through cannot be hovered: no dimming
                    hud._apply_opacity()
                    self.assertAlmostEqual(hud.windowOpacity(), 0.9, delta=0.02)
                    config.set('click_through', False)
                    hud.sidebar._active = 'claude'               # a detail page is open: stay opaque
                    hud._apply_opacity()
                    self.assertAlmostEqual(hud.windowOpacity(), 0.9, delta=0.02)
                    hud.sidebar._active = None
            finally:
                self._close(hud)

    def test_table_mode_never_dims(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            _, hud = self._hud(directory, ui_mode='table', opacity=0.9)
            try:
                with patch.object(type(hud), 'underMouse', lambda self: False):
                    hud._apply_opacity()
                    self.assertAlmostEqual(hud.windowOpacity(), 0.9, delta=0.02)
            finally:
                self._close(hud)

    def test_menu_labels_are_plain_text(self):
        import re
        from PySide6.QtWidgets import QMenu
        captured = []

        class RecordingMenu(QMenu):
            def exec(self, *args):
                captured.append(self)

        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}), \
                patch('ui.hud_menu.QMenu', RecordingMenu):
            _, hud = self._hud(directory, ui_mode='sidebar')
            try:
                from PySide6.QtGui import QContextMenuEvent
                from PySide6.QtCore import QPoint
                hud.contextMenuEvent(QContextMenuEvent(QContextMenuEvent.Reason.Mouse, QPoint(1, 1), QPoint(1, 1)))
                emoji = re.compile('[\U0001F300-\U0001FAFF\u2190-\u21FF\u2600-\u27BF\u2B00-\u2BFF]')

                def texts(menu):
                    for action in menu.actions():
                        yield action.text()
                        if action.menu():
                            yield from texts(action.menu())
                labels = list(texts(captured[0]))
                self.assertTrue(labels)
                self.assertFalse([t for t in labels if emoji.search(t)])
                behaviour = next(a.menu() for a in captured[0].actions() if a.menu() and 'Behaviour' in a.text())
                self.assertTrue(any('Idle fade' in a.text() for a in behaviour.actions()))
            finally:
                self._close(hud)

    def test_idle_fade_level_and_delay_are_customisable(self):
        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}):
            config, hud = self._hud(directory, ui_mode='sidebar', opacity=1.0)
            try:
                hud._fade_timer.stop()
                with patch.object(type(hud), 'underMouse', lambda self: False):
                    hud.set_idle_fade_level(40)
                    self.assertAlmostEqual(hud.windowOpacity(), 0.4, delta=0.02)
                    hud.set_idle_fade_level(5)                       # clamped to 20 %
                    self.assertAlmostEqual(hud.idle_fade_level(), 0.2)
                    hud.set_idle_fade_level(99)                      # clamped to 95 %
                    self.assertAlmostEqual(hud.idle_fade_level(), 0.95)
                hud.set_idle_fade_delay(2000)
                self.assertEqual(hud.idle_fade_delay(), 2000)
                self.assertEqual(ConfigManager(config.path).get('sidebar_idle_delay_ms'), 2000)
                hud.set_idle_fade_delay(5)                           # clamped to 200 ms
                self.assertEqual(hud.idle_fade_delay(), 200)
                config.set('sidebar_idle_level', 'junk')
                self.assertAlmostEqual(hud.idle_fade_level(), 0.6)   # bad values fall back
                from PySide6.QtCore import QEvent
                hud.leaveEvent(QEvent(QEvent.Type.Leave))                                 # uses the configured delay
                self.assertEqual(hud._fade_timer.interval(), 200)
            finally:
                self._close(hud)

    def test_idle_fade_submenu_lists_levels_and_delays(self):
        from PySide6.QtWidgets import QMenu
        captured = []

        class RecordingMenu(QMenu):
            def exec(self, *args):
                captured.append(self)

        with tempfile.TemporaryDirectory() as directory, patch('ui.hud_window.PROVIDERS', {}), \
                patch('ui.hud_menu.QMenu', RecordingMenu):
            _, hud = self._hud(directory, ui_mode='sidebar', sidebar_idle_level=0.5)
            try:
                from PySide6.QtGui import QContextMenuEvent
                from PySide6.QtCore import QPoint
                hud.contextMenuEvent(QContextMenuEvent(QContextMenuEvent.Reason.Mouse, QPoint(1, 1), QPoint(1, 1)))
                behaviour = next(a.menu() for a in captured[0].actions() if a.menu() and 'Behaviour' in a.text())
                fade = next(a.menu() for a in behaviour.actions() if a.menu() and 'Idle fade' in a.text())
                level = next(a.menu() for a in fade.actions() if a.menu() and 'Dim to' in a.text())
                delay = next(a.menu() for a in fade.actions() if a.menu() and 'Delay' in a.text())
                self.assertEqual([a.text() for a in level.actions()], ['80%', '60%', '50%', '40%', '25%'])
                self.assertEqual([a.text() for a in level.actions() if a.isChecked()], ['50%'])   # custom value shows up
                self.assertEqual(len(delay.actions()), 4)
                level.actions()[3].trigger()
                self.assertAlmostEqual(hud.idle_fade_level(), 0.4)
            finally:
                self._close(hud)
