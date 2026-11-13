"""Extra Claude accounts next to the default tiles, applied live (fixture profile dirs only, no real home)."""
import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import tempfile
import time
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QEvent
from PySide6.QtWidgets import QApplication, QFrame

from core import accounts as acct
from core import tray_quota
from core.config_manager import ConfigManager
from core.providers.base import MetricRow, UsageMetrics
from core.providers.claude_provider import ClaudeAccountProvider, ClaudeProvider

APP = QApplication.instance() or QApplication([])
PID = 'claude@claude-a'


def wait_for(predicate):
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        APP.processEvents()
        if predicate():
            return
        time.sleep(0.005)
    raise AssertionError('condition not reached')


def flush():
    APP.processEvents()
    APP.sendPostedEvents(None, QEvent.Type.DeferredDelete)
    APP.processEvents()


class Stub:
    def __init__(self, pid):
        self.provider_id, self.config = pid, None

    def fetch_usage(self):
        return UsageMetrics(provider_id=self.provider_id, metric1_val=10, metric1_text='10%',
                            metric1_key=f'{self.provider_id}:5h', metric2_val=20, metric2_text='20%')


class StubAccount(Stub):
    def __init__(self, instance_id, profile, label):
        super().__init__(instance_id)
        self.profile, self.label = profile, label


class AccountHudTests(unittest.TestCase):
    def setUp(self):
        stack = ExitStack()
        self.addCleanup(stack.close)
        root = Path(stack.enter_context(tempfile.TemporaryDirectory()))
        home = root / 'home'
        for name in ('.claude', '.claude-a', '.claude-b'):
            (home / name).mkdir(parents=True)
            (home / name / '.credentials.json').write_text('{}', encoding='utf-8')
        env = {'USERPROFILE': str(home), 'HOME': str(home)}
        stack.enter_context(patch.dict(os.environ, env))
        os.environ.pop('CLAUDE_CONFIG_DIR', None)
        providers = {k: Stub(k) for k in ('claude', 'agy', 'codex')}
        stack.enter_context(patch('ui.hud_window.PROVIDERS', providers))
        stack.enter_context(patch('ui.hud_window.ClaudeAccountProvider', StubAccount))
        self.config_path = root / 'config.json'

    def hud(self, **settings):
        from ui.hud_window import HUDWindow
        config = ConfigManager(self.config_path)
        config.update({'ui_mode': 'sidebar', 'claude_profile': 'default', 'sidebar_idle_fade': False, **settings})
        hud = HUDWindow(config)
        self.addCleanup(self.close, hud)
        return hud

    @staticmethod
    def close(hud):
        hud.refresh_controller.stop()
        hud.countdown_timer.stop()
        hud.geometry_timer.stop()
        hud.close()

    def add(self, hud, profile='claude-a'):
        self.assertTrue(hud.add_account(profile))
        hud.apply_accounts()

    def test_add_and_remove_live_and_drop_late_results(self):
        hud = self.hud(sidebar_state='strip')
        self.add(hud)
        controller = hud.refresh_controller
        self.assertTrue(all(PID in d for d in (hud.sidebar.buttons, hud.sidebar_cards, controller.states, hud.providers)))
        wait_for(lambda: PID in hud._raw_metrics)                    # fetched at once, not at the next tick
        self.assertEqual(hud.sidebar.buttons[PID].display[0], '10%')
        self.assertEqual(hud.sidebar_cards[PID].title.text(), 'claude-a')
        self.assertEqual(hud.sidebar.buttons[PID].spec.text, 'claude-a')
        self.assertEqual(hud.sidebar.buttons['claude'].spec.text, 'Claude')
        self.assertEqual(ConfigManager(self.config_path).get('accounts')[0]['profile'], 'claude-a')

        self.assertTrue(hud.remove_account(PID))
        hud.apply_accounts()
        flush()
        self.assertFalse(any(PID in d for d in (hud.sidebar.buttons, hud.sidebar_cards, controller.states, hud.providers,
                                                hud._raw_metrics, hud._latest_metrics, controller._born)))
        self.assertEqual(ConfigManager(self.config_path).get('accounts'), [])
        seen = []
        controller.updated.connect(seen.append)
        controller._complete(PID, 99, UsageMetrics(provider_id=PID, metric1_val=5))    # worker finished after removal
        self.assertEqual(seen, [])

        self.add(hud)                                                  # same profile again: same id, new incarnation
        controller._complete(PID, 1, UsageMetrics(provider_id=PID, metric1_val=77))   # the old worker's result
        self.assertNotIn(77, [m.metric1_val for m in seen])

    def test_window_follows_the_tile_count_in_strip_and_panel(self):
        cases = [('strip', 'right'), ('strip', 'bottom'), ('panel', 'right'), ('panel', 'bottom')]
        for state, edge in cases:
            with self.subTest(state=state, edge=edge):
                self.config_path.unlink(missing_ok=True)
                hud = self.hud(sidebar_state=state, sidebar_edge=edge, sidebar_scale=0.6)
                # Compare settled cards on both sides: the first fetch hides
                # empty badges and changes the panel's initial placeholder size.
                wait_for(lambda: {'claude', 'agy', 'codex'} <= set(hud._raw_metrics))
                flush()
                tiles = lambda: len(hud.sidebar.order())
                self.assertEqual(hud.size(), hud.sidebar.preferred_size())
                three = hud.size()
                self.add(hud)
                flush()
                self.assertEqual(tiles(), 4)
                self.assertEqual(hud.size(), hud.sidebar.preferred_size())
                self.assertGreater(hud.size().width() * hud.size().height(), three.width() * three.height())
                hud.remove_account(PID)
                hud.apply_accounts()
                flush()
                self.assertEqual(hud.size(), three)
                self.close(hud)

    def test_removing_a_tile_with_open_flyout_or_in_panel_leaves_nothing_behind(self):
        hud = self.hud(sidebar_state='strip')
        self.add(hud)
        hud.sidebar._click_provider(PID)
        self.assertEqual(hud.sidebar._active, PID)
        card = hud.sidebar_cards[PID]
        hud.remove_account(PID)
        hud.apply_accounts()
        flush()
        sidebar = hud.sidebar
        self.assertIsNone(sidebar._active)
        self.assertIsNone(sidebar._pinned_key)
        self.assertEqual(sidebar._flyout_layout.count(), 0)
        self.assertFalse(sidebar._flyout.isVisible())
        self.assertNotIn(card, sidebar._storage.findChildren(type(card)))

        hud.set_sidebar_state('panel')
        self.add(hud)
        flush()
        dividers = lambda: sidebar._panel.findChildren(QFrame, 'SidebarDivider')
        self.assertEqual(len(dividers()), 3)
        hud.remove_account(PID)
        hud.apply_accounts()
        flush()
        self.assertEqual(len(dividers()), 2)
        self.assertEqual(sidebar._panel_layout.count(), 5)             # 3 cards + 2 dividers

    def test_order_is_persisted_and_old_config_gives_todays_tiles(self):
        hud = self.hud()
        self.assertEqual(hud.sidebar.order(), ['claude', 'agy', 'codex'])
        self.assertNotIn('sidebar_tiles', ConfigManager(self.config_path).data)
        self.add(hud)
        self.assertEqual(hud.sidebar.order(), ['claude', PID, 'agy', 'codex'])
        hud.sidebar.set_order(['agy', 'claude', PID, 'codex'])
        hud.sidebar.orderChanged.emit(hud.sidebar.order())
        saved = ConfigManager(self.config_path)
        self.assertEqual(saved.get('sidebar_tiles'), ['agy', 'claude', PID, 'codex'])
        self.assertEqual(saved.get('provider_order'), ['agy', 'claude', 'codex'])      # base ids only: older exes keep working
        self.close(hud)
        again = self.hud()
        self.assertEqual(again.sidebar.order(), ['agy', 'claude', PID, 'codex'])

    def test_hidden_tile_keeps_fetching_and_the_tray_still_resolves(self):
        hud = self.hud(sidebar_state='strip')
        wait_for(lambda: {'claude', 'agy', 'codex'} <= set(hud._raw_metrics))
        self.assertTrue(hud.set_tile_visible('claude', False))
        self.assertTrue(hud.set_tile_visible('codex', False))
        self.assertFalse(hud.set_tile_visible('agy', False))            # the last visible tile stays
        hud.apply_accounts()
        self.assertEqual(hud.sidebar.order(), ['agy'])
        self.assertTrue(hud.sidebar.buttons['claude'].isHidden())
        self.assertEqual(hud.sidebar.preferred_size().height(), 22 + 64 + 8)
        self.assertEqual({'claude', 'codex'} & set(hud.refresh_controller.states), {'claude', 'codex'})
        hud.refresh_controller.refresh()
        hud._on_data_fetched(UsageMetrics(provider_id='claude', metric1_val=91, metric1_text='91%', metric1_key='claude:5h'))
        rows = hud.tray_quota_readings()
        self.assertEqual(tray_quota.select('claude:5h', rows).used, 91)
        self.assertEqual(tray_quota.select('auto', rows).key, 'claude:5h')
        self.assertTrue(hud.set_tile_visible('claude', True))
        hud.apply_accounts()
        self.assertEqual(hud.sidebar.order(), ['agy', 'claude'])        # shown again at the end

    def test_apply_is_coalesced_and_an_invalid_value_keeps_the_tiles(self):
        hud = self.hud()
        self.add(hud)
        with patch.object(hud, 'apply_accounts', wraps=hud.apply_accounts) as apply:
            for _ in range(3):
                hud.request_apply_accounts()
            wait_for(lambda: apply.call_count >= 1)
            APP.processEvents()
            self.assertEqual(apply.call_count, 1)
        hud.config.set('accounts', 'garbage')
        hud.apply_accounts()
        self.assertIn(PID, hud.sidebar.buttons)
        self.assertEqual(hud.sidebar.order(), ['claude', PID, 'agy', 'codex'])

    def test_picker_rules(self):
        hud = self.hud(claude_profile='claude-b')
        self.assertFalse(hud.add_account('claude-b'))                   # the main tile's own profile
        self.assertFalse(hud.add_account('default'))
        self.assertTrue(hud.add_account('claude-a'))
        self.assertFalse(hud.add_account('.Claude-A'))                  # duplicate, case-folded
        self.assertEqual(ConfigManager(self.config_path).get('accounts')[0]['id'], PID)

    def test_same_id_profile_replacement_clears_old_quota_before_fetch(self):
        hud = self.hud(sidebar_state='strip')
        self.add(hud)
        wait_for(lambda: PID in hud._raw_metrics)
        controller = hud.refresh_controller
        generation = controller.states[PID].generation
        hud.sidebar._click_provider(PID)
        order = hud.sidebar.order()
        hud.config.set('accounts', [{'id': PID, 'provider': 'claude', 'profile': 'claude-b'}])
        with patch.object(controller, '_launch'):
            hud.apply_accounts()
            controller._complete(PID, generation, UsageMetrics(provider_id=PID, metric1_val=88))
            self.assertEqual(hud.sidebar_cards[PID].title.text(), 'claude-b')
            self.assertIsNone(hud.sidebar_cards[PID].current_metrics.metric1_val)
            self.assertEqual(hud.sidebar.buttons[PID].display, ['--', '--'])
            self.assertNotIn(PID, hud.sidebar._metrics)
            self.assertNotIn(PID, hud._raw_metrics)
            self.assertIsNone(hud.sidebar._active)
            self.assertFalse(hud.sidebar._flyout.isVisible())
            self.assertEqual(hud.sidebar.order(), order)

    def test_nickname_change_updates_card_without_new_fetch(self):
        hud = self.hud()
        self.add(hud)
        wait_for(lambda: PID in hud._raw_metrics)
        hud._on_data_fetched(UsageMetrics(provider_id=PID, provider_name='claude-a', metric1_val=10))
        hud.config.set('accounts', [{'id': PID, 'provider': 'claude', 'profile': 'claude-a', 'nickname': 'Work'}])
        with patch.object(hud.refresh_controller, '_launch'):
            hud.apply_accounts()
            self.assertEqual(hud.sidebar_cards[PID].title.text(), 'Work')
            self.assertEqual(hud.sidebar.buttons[PID].spec.text, 'Work')
            self.assertEqual(hud.sidebar_cards[PID].current_metrics.metric1_val, 10)
            hud._republish_all()       # cached metrics may still carry the old provider label
            hud.sidebar_cards[PID].set_appearance(False)
            self.assertEqual(hud.sidebar_cards[PID].title.text(), 'Work')


class WrapperAndRulesTests(unittest.TestCase):
    def test_wrapper_rewrites_keys_and_errors_carry_the_instance_id(self):
        ok = UsageMetrics(provider_id='claude', provider_name='CLAUDE CODE', metric1_key='claude:5h', metric2_key='claude:7d',
                          extra_metrics=[MetricRow('FABLE 7D', 3, '3%', key='claude:weekly_scoped:fable')])
        offline = UsageMetrics(provider_id='claude', error='x', error_code='auth')
        wrapper = ClaudeAccountProvider(PID, 'claude-a', 'claude-a')
        with patch.object(ClaudeProvider, 'fetch_usage', side_effect=[ok, offline]):
            result, failed = wrapper.fetch_usage(), wrapper.fetch_usage()
        self.assertEqual((result.provider_id, result.provider_name), (PID, 'claude-a'))
        self.assertEqual([result.metric1_key, result.metric2_key, result.extra_metrics[0].key],
                         [f'{PID}:5h', f'{PID}:7d', f'{PID}:weekly_scoped:fable'])
        self.assertEqual(tray_quota.qualified_key(PID, result.metric1_key), f'{PID}:5h')
        self.assertEqual((failed.provider_id, failed.error), (PID, 'x'))
        self.assertEqual(wrapper.inner.profile_preference, 'claude-a')   # never follows claude_profile

    def test_account_validation_ids_and_labels(self):
        raw = [{'id': 'claude@a', 'provider': 'claude', 'profile': 'a'},
               {'id': 'claude@dup', 'provider': 'claude', 'profile': '.A'},          # same profile
               {'id': 'claude@d', 'provider': 'claude', 'profile': 'default'},        # the main tile's profile
               {'id': 'claude@x', 'provider': 'claude', 'profile': '../x'},
               {'id': 'bad id', 'provider': 'claude', 'profile': 'b'},
               {'id': 'claude@c', 'provider': 'codex', 'profile': 'c'}] + \
              [{'id': f'claude@p{i}', 'provider': 'claude', 'profile': f'p{i}'} for i in range(8)]
        specs, ok = acct.parse_accounts(raw)
        self.assertTrue(ok)
        self.assertEqual([s.id for s in specs], ['claude@a'] + [f'claude@p{i}' for i in range(5)])   # cap 6
        self.assertEqual(acct.parse_accounts('x'), ([], False))
        self.assertEqual(acct.new_account_id('Claude A!', {'claude@claude-a'}), 'claude@claude-a-2')
        self.assertEqual(acct.account_label(None, '/h/.claude-a', ['/h/.claude']), 'claude-a')
        self.assertEqual(acct.account_label(None, '/h/.claude-a', ['/w/.claude-a']), 'h/claude-a')
        self.assertEqual(acct.account_label(' Work ', '/h/.claude-a'), 'Work')
        self.assertEqual(acct.elide('claude-a', 8), 'claude-a')
        self.assertEqual(acct.elide('claude-work', 8), 'claude-\u2026')


if __name__ == '__main__':
    unittest.main()
