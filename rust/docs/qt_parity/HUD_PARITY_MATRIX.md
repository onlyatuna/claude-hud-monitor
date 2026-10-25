# Python HUD vs Rust HUD behaviour parity matrix

Reference: python/ at 8e2f5fc. Static code reading only: nothing here was run on a desktop. Status values: same, intentional-diff, diff, missing, unverified.

## Totals

| Slice | same | intentional-diff | diff | missing | unverified |
|---|---|---|---|---|---|
| core | 65 | 15 | 14 | 4 | 4 |
| providers | 62 | 2 | 16 | 0 | 4 |
| window, menu, tray | 93 | 10 | 4 | 1 | 4 |
| widgets | 215 | 18 | 11 | 3 | 23 |
| **all** | **435** | **45** | **45** | **8** | **35** |

No P0. P1: usage table agy badge text; smoke test is not offline and checks less than Python's.

---

# Parity matrix: core slice (config / refresh / pace / autostart / logger / diagnostics / smoke / main / hotkey / memory)

Worktree: C:/Users/sss19/AppData/Local/Temp/wt_retire. Read-only static comparison, nothing run.
Python: core/{config_manager,refresh_controller,pace,autostart,logger,diagnostics,smoke_check}.py, main.py, system/{hotkey,memory}.py.
Rust: config.rs, refresh_controller.rs, pace.rs, autostart.rs, logger.rs, main.rs, hotkey.rs, memory.rs (no diagnostics.rs / smoke file; smoke lives in main.rs + config.rs).

## diff / missing, by severity

| Sev | Item | Fix hint |
|---|---|---|
| P1 | `--smoke-test` is not offline: `run_smoke_test` calls `HUDWindow::new` -> `default_providers` (hud_window.rs:495-514) which builds Claude/Agy/Codex providers and `with_providers` launches a live fetch for each (hud_window.rs:879-883). Python passes `providers={}` (smoke_check.py:30) and the docstring promises it never loads credentials. | Use `HUDWindow::with_providers(cfg, ctrl, HashMap::new())` in `run_smoke_test`. |
| P1 | Smoke does not assert error-recovery: Python checks the agy card sub-text and table badge are cleared after the recovery fetch (smoke_check.py:33-36) and raises. Rust calls `on_data_fetched` twice and asserts nothing (main.rs:111-121). `set_color_scheme('duo')` is also never exercised (main.rs:122-125 has no equivalent). | Read card/badge state back after the second `on_data_fetched`, return `ExitCode::FAILURE` on mismatch; call the colour-scheme setter. |
| P2 | `pace::window_seconds` parses loosely: collects every digit/`.` and the last alphabetic char anywhere in the string (pace.rs:100-127). Python needs `^…(\d+(?:\.\d+)?)\s*([DHM])$` at the end of the string, case-sensitive (pace.py:76-78). So Rust accepts `H5`, `5h`, `5H ` and Python returns the default. Today's titles (`SESSION 5H`, `WEEKLY 7D`, `C/G 7D`, `WINDOW 30D`) agree. | Port the regex (or an end-anchored manual parse): number, optional spaces, one uppercase D/H/M, end of string. |
| P2 | `ConfigManager::sanitize` has no Python counterpart in the manager: opacity <=0/NaN -> 0.88, else clamp 0.1..1.0 (config.rs:477-483); `refresh_interval_sec < 20` -> **60** (config.rs:485-487); blank `claude_profile` -> `auto`. Python keeps the stored values and `RefreshController` clamps `max(20, interval)`, so 5 becomes **20** (refresh_controller.py:30; hud_window.py:104-105). | Change the Rust fallback to `.max(20)` (clamp, not reset to 60) or document the intentional reset. |
| P2 | Config save has no fsync and uses a fixed temp name `config.tmp` (config.rs:570-590). Python writes a unique `.config-*.tmp`, `flush()` + `os.fsync()`, `os.replace` (config_manager.py:129-135). The file header comment (config.rs:8) claims fsync. | Use `File::create` + `write_all` + `sync_all` before rename. |
| P2 | No diagnostics log: Python `configure_logging` makes a `core` logger writing `diagnostics.log` beside config.json (256 KB x 2 backups, INFO) (diagnostics.py:7-19, main.py:59). Rust has no module; provider warnings go into `hud_monitor.log` only. | Add a second file sink, or record as intentional-diff and update the docs. |
| P2 | `RefreshController::stop()` and the `closed` guard do not exist in Rust (refresh_controller.py:60-62,100-101,128-133). Late results after quit are not discarded; main only drops HUD/tray (main.rs:640-645). `main.py:99-100` stops the controller in `aboutToQuit`. | Add a `closed` flag checked in `poll`/`refresh`/`complete`, set it before shutdown. |
| P2 | Hotkey start failure is all-or-nothing: if the worker does not report within 1.5 s, `start` returns `Err` (hotkey.rs:228-231), main logs only and sets no manager, so no tray balloon and click-through startup is blocked. Python waits 1.5 s, continues, and still shows any failure balloons (hotkey.py:228). | Return a degraded `HotkeyRegistration` instead of `Err` on timeout. |
| P2 | Config `hotkey` string is honoured by Rust (`parse_hotkey`, hotkey.rs:166-200, with `compute_ct_mods`) but Python ignores it: `start(key_char="C")` is hard-coded (main.py:93) and the toggle is always Alt+C / click-through Alt+Shift+C. Identical at the default `Alt+C`; differs for any other stored value. | Decide the contract; if Python is the reference, drop `parse_hotkey` and fix to Alt+C. |
| P2 | Log layout: Python rotates 3 backups at 2 MB, file level DEBUG, line format `asctime [LEVEL] [name:file:line] msg`, portable `exe_dir/logs` when a portable config.json exists (logger.py:10-31,36-61). Rust keeps 1 backup (`.log.1`), INFO max, format `time [LEVEL] [target] msg`, no portable dir (logger.rs:41-53,74-105,170-193). macOS/Linux dirs differ (Python `<config dir>/logs`; Rust `~/Library/Logs/...`, `~/.local/share/...`). Windows dir matches. | Optional: add `.2/.3` backups, DEBUG for file, portable branch. |
| P2 | macOS autostart plist is hand-formatted with no XML escaping and only `[exe]` (autostart.rs:98-112); Python uses `plistlib` (escapes `&`) and appends the script path when not frozen (autostart.py:53-61; tests/test_platform.py::test_mac_source_autostart_has_script_and_escapes_xml). A path containing `&`/`<` corrupts the plist. | Escape XML (or use `plist` crate). |
| P2 | `Config::set`/`set_many` skip-if-equal semantics are not mirrored: `update_and_save` always writes (config.rs:547-556), Python returns early when nothing changed (config_manager.py:151,161-165). | Compare before save, or accept extra disk writes. |
| P2 | Missing Python-only `qt_message_handler` routing Qt messages to the logger (main.py:19-29,47), `SetHighDpiScaleFactorRoundingPolicy` (main.py:50) and `app.setWindowIcon` (main.py:54-56). | Probably N/A for ferrokit; confirm icon at the tray/window level. |

## Parity rows

### config_manager.py vs config.rs

| Python behaviour (file:line) | Rust location (file:line) | Status | Test coverage | Notes |
|---|---|---|---|---|
| DEFAULT_CONFIG values: window_x/y None, ui_mode "sidebar", sidebar_state "panel", edge "right", orientation "vertical", offset/x/y None, idle_fade True, idle_level 0.6, idle_delay 900, scale 1.0, row_order {}, agy_order [C/G 5H, ARGON 7D, GEMINI 5H, GEMINI 7D, C/G 7D], provider_order [claude, agy, codex], table 450x350, blur True, compact False, compact 240x96, color_scheme "scale", appearance "auto", always_on_top True, opacity 0.88, click_through False, interval 60, hotkey_enabled True, hotkey "Alt+C", locked False, autostart False, claude_profile "auto" (config_manager.py:9-43) | config.rs:157-306 (defaults), 237-246, 340-345 | same | Rust: config.rs::tests::defaults_match_python_default_config (config.rs:998); Python: none | Compact 240x96 comes from `ui::table_layout::DEF_COMPACT_W/H`, not read here; checked only by the named Rust test. All other constants read and equal. |
| UI_MODES = ("table","sidebar") (config_manager.py:45) | config.rs:598 | same | sidebar_config_tests::unknown_mode_falls_back_to_default_style | |
| `migrate_style`: mode None + (layout_mode key or window_x set) -> "cards"; table/sidebar untouched; else ui_mode=sidebar, state=panel; cards -> orientation from layout_mode (horizontal/vertical, default vertical); int x,y (non-bool) -> edge "none" + sidebar_x/y; returns changed (config_manager.py:48-68) | config.rs:602-641 | same | Rust: config.rs::sidebar_config_tests::{fresh_install_defaults_to_sidebar_panel, cards_become_sidebar_panel_keeping_position_and_orientation, vertical_cards_become_vertical_panel, cards_without_saved_position_keep_default_edge, legacy_file_without_ui_mode_counts_as_cards, table_and_sidebar_settings_are_untouched, unknown_mode_falls_back_to_default_style, migration_is_saved_and_idempotent_and_keeps_the_legacy_keys}; Python: tests/test_config.py::StyleMigrationTests::* (same names) | Rust x/y limited to i32; Python any int. Immaterial. |
| Migration persisted on load when changed (config_manager.py:117-118) | config.rs:508-513 | same | sidebar_config_tests::migration_is_saved_idempotent… | |
| Config dir: win `%APPDATA%\ClaudeHUDMonitor` (fallback ~), mac `~/Library/Application Support/ClaudeHUDMonitor`, else `~/.config/ClaudeHUDMonitor` (config_manager.py:71-81) | config.rs:425-444 | same | none / none | Rust creates dir lazily at save instead of at lookup. |
| Config path: frozen -> portable `exe_dir/config.json` if present+writable else user dir (config_manager.py:86-92); dev source mode -> project-dir config.json (94-100) | config.rs:451-476 | intentional-diff | none / tests/test_config.py::ConfigTests::test_frozen_path_is_persistent | Rust is always a binary: portable beside exe if writable, else user dir. No source-tree mode. Writability = `!readonly` metadata vs `os.access(W_OK)`. |
| Load: JSON object merged over defaults; unknown keys kept; non-dict ignored; any exception logged, defaults kept (config_manager.py:109-120) | config.rs:497-523 (+ `extra` flatten, config.rs:150) | intentional-diff | Rust: config.rs::sidebar_config_tests::{a_python_written_file_loads, a_bad_value_keeps_that_one_default_and_nothing_else, a_wrong_typed_older_key_keeps_its_default_and_nothing_else, a_file_without_the_keys_gets_the_defaults}; Python: none | Rust validates each known key (`lenient_*`): a wrong type falls back to that key's default; Python stores any type raw. Unknown keys survive via `extra`. A truncated/invalid JSON file gives all defaults in both. |
| sidebar_offset is None, number or "end" (hud_window.py:478-480; dock_geometry.py:101) | config.rs:310-340 `SidebarOffset::{At,End}` | same | config.rs::sidebar_config_tests::offset_round_trips_as_a_number_or_the_word_end | Python rejects other types at use site; Rust drops them at load. |
| Value sanitising: none in manager (opacity, interval, profile used raw) | config.rs:477-495 | diff | config.rs::tests::test_config_nan_and_bounds_sanitization (Rust) / none | See P2 above (interval reset to 60 vs clamp to 20). |
| Atomic save: unique temp in same dir, write, flush, fsync, `os.replace`, temp cleanup, errors logged (config_manager.py:122-143) | config.rs:570-590 | diff | Rust: config.rs::tests::test_concurrent_save_serialization_no_corruption; Python: tests/test_config.py::ConfigTests::test_failed_replace_preserves_settings | No fsync, fixed `config.tmp`. Rename failure removes temp: same. |
| JSON format: indent 2, ensure_ascii False (config_manager.py:132) | config.rs:574 `to_string_pretty` | same | config.rs::tests::test_config_serde_roundtrip | Key order differs (struct order, extras last). |
| RLock around get/set/save (config_manager.py:104,110,123) | config.rs:517-521 `SAVE_LOCK`, `Mutex<Config>` | intentional-diff | Rust: config.rs::tests::{test_config_lock_not_held_during_disk_io, test_stale_snapshot_cannot_overwrite_newer_config, test_save_lock_is_not_reentrant}; Python: none | Rust takes a snapshot, releases the data lock, then does IO. |
| `set` / `set_many` / `update`, equality short-circuit, auto_save flag (config_manager.py:149-168) | config.rs:547-556 `update_and_save` | diff | none / none | Always saves; no `auto_save=False` path (caller mutates `Config` directly). |
| Debounced geometry save (not in config_manager.py; handled in hud_window.py geometry_timer) | config.rs:690-980 `ResizeDebouncer`, `RESIZE_DEBOUNCE_MS=250` | unverified | Rust: config.rs::tests::test_resize_debounce_*; Python: none | Belongs to window slice (ParityWindow); delay compared there. |

### refresh_controller.py vs refresh_controller.rs

| Python behaviour | Rust location | Status | Test coverage | Notes |
|---|---|---|---|---|
| `interval = max(20, interval)` (refresh_controller.py:30,48) | refresh_controller.rs:63-67,123-125 | same | refresh_controller.rs::test_set_interval_clamps_min / none | Rust takes u64. |
| Per-provider state: generation, running, pending, failures, due(0.0), cached (13-20) | refresh_controller.rs:22-31 | same | – | `due: None` means due now (matches 0.0 on a monotonic clock). |
| Poll tick 1000 ms QTimer + immediate `poll()` in `start` (39-45); result drain 25 ms QTimer (35-38) | main.rs:612-623 (100 ms poll timer), refresh_controller.rs:92 (drain inside `poll`), 205-207 (notify callback -> `drain_refresh_results`, main.rs:587-591) | intentional-diff | none / none | Rust launches within 100 ms of due and drains on a worker wake-up; observable timing is finer-grained, scheduling logic unchanged. Initial launch happens in `HUDWindow::with_providers` (hud_window.rs:879-883) rather than `start()`. |
| `poll`: launch every non-running provider whose due <= now (53-58) | refresh_controller.rs:88-105 | same | refresh_controller.rs::test_refresh_controller_pipeline_and_late_notify / tests/test_refresh.py::test_failure_retains_data_and_backs_off | |
| `refresh`: running -> generation+1 & pending; else launch (60-69) | refresh_controller.rs:108-119 | same | refresh_controller.rs::test_refresh_controller_pipeline_and_late_notify / tests/test_refresh.py::test_wake_discards_old_result_without_overlap | |
| `_launch`: running, clear pending, generation+1, thread named `quota-<pid>`, busy_changed(True) (71-88) | refresh_controller.rs:134-210 | same | same as above | Busy is derived (`is_busy`, hud_window.rs:1050) rather than a signal. |
| Worker exception -> `UsageMetrics(error="查詢失敗，請查看診斷紀錄", error_code="unexpected")`, logs only exception type (80-85) | refresh_controller.rs:150-173 | same | refresh_controller.rs::test_worker_panic_catch / none | Same string and code. Rust logs the panic payload text, Python only the type name (minor leak risk). |
| Fetch timing logs (none) | refresh_controller.rs:174-198 | intentional-diff | none / none | Rust adds `fetch_ms` / `slow_fetch` (>2000 ms) lines. |
| `_complete`: stale generation or pending -> relaunch, drop result (98-106) | refresh_controller.rs:230-244 | same | refresh_controller.rs::test_refresh_controller_pipeline_and_late_notify / tests/test_refresh.py::test_wake_discards_old_result_without_overlap | |
| Error backoff: `min(900, interval * 2**min(failures-1,4))`; due = now+delay (109-116) | refresh_controller.rs:249-272 | same | tests/test_refresh.py::test_failure_retains_data_and_backs_off / refresh_controller.rs::test_retry_after_clamp_and_safety | Both truncate to whole seconds in tests (60/120/240). |
| `retry_after` used only if finite and >= 0: `min(86400, max(delay, retry_after))` (111-114) | refresh_controller.rs:256-265 | same | refresh_controller.rs::test_retry_after_clamp_and_safety / tests/test_refresh.py::test_retry_after_is_finite_and_capped | Rust adds `.clamp(1.0, 86400.0)`; delay is >= 20 so the lower bound never binds. |
| Failure keeps cached data: copy of cached with error, error_code, retry_after, stale=True (117-119) | refresh_controller.rs:274-282 | same | tests/test_refresh.py::test_failure_retains_data_and_backs_off / none in Rust | |
| Success: failures=0, due=now+interval, last_success=now(UTC), stale=False, cache (121-124) | refresh_controller.rs:283-289 | same | refresh_controller.rs::test_refresh_controller_pipeline_and_late_notify / tests/test_refresh.py | |
| `updated` per result; `busy_changed(any running)` after each completion (125-126) | refresh_controller.rs:213-228 (`updated.emit`, `idle_completions`), hud_window.rs:1056-1064 | same | none / none | Idle trim fired per completion that leaves all idle, matching `_on_busy_changed(False)` (hud_window.py:820). |
| `set_interval`: due = now + interval for providers with no failures (47-51) | refresh_controller.rs:123-131 | same | refresh_controller.rs::test_set_interval_clamps_min / none | |
| `stop()`: closed, stop timers, generation+1 (128-133); `closed` guards (54-55,61-62,92,100-101) | none | missing | none / tests/test_refresh.py::test_stop_ignores_late_results | P2. |
| One slow provider must not block another (separate threads) | refresh_controller.rs:144 thread per launch | same | none / tests/test_refresh.py::test_one_slow_provider_does_not_block_another | Same per-provider threading model; no Rust test. |

### pace.py vs pace.rs

| Python behaviour | Rust location | Status | Test coverage | Notes |
|---|---|---|---|---|
| WEEKDAYS 一二三四五六日 (pace.py:12) | pace.rs:9 | same | pace.rs::test_reset_time_weekday / tests/test_pace.py::test_reset_time_includes_weekday_only_when_asked | |
| SCALE_THRESHOLDS 90 red / 75 orange / 50 yellow / 0 green; `scale_level` (15,96-99) | pace.rs:11-16,146-154 | same | pace.rs::test_scale_levels / tests/test_pace.py::test_scale_levels | Negative percent: Python raises `StopIteration`, Rust returns None (intentional, safer). |
| MIN_ELAPSED_FOR_PACE 0.05; `pace_mark` = elapsed*100 or None (18,89-93) | pace.rs:18,138-144 | same | pace.rs::test_pace_mark / tests/test_pace.py::test_pace_mark_needs_a_meaningful_elapsed_share | |
| FIVE_HOURS, ONE_WEEK constants (20-21) | pace.rs:19-20 | same | none / none | |
| `remaining_seconds`: max(0, int(delta)), None for no target (29-32) | pace.rs:22-27 | same | pace.rs::test_countdown_formats / tests/test_pace.py::test_countdown_formats | Rust is UTC-only; Python also handles naive datetimes. |
| `format_countdown_hm` `HH:MM` / `--:--` (35-39) | pace.rs:29-35 | same | same | |
| `format_countdown_dhm` `DD:HH:MM` / `--:--:--` (42-46) | pace.rs:37-48 | same | same | |
| `format_reset_time` local `%H:%M`, optional `週X ` prefix, `--:--` for None (49-54) | pace.rs:50-65 | same | pace.rs::test_reset_time_weekday / tests/test_pace.py::test_reset_time_includes_weekday_only_when_asked | |
| `reset_stamp`: long window (>=86400 s or title contains WEEK) -> with weekday, else "" (57-62) | pace.rs:69-79 | same | pace.rs::test_reset_stamp_only_for_day_or_longer_windows / none | |
| `short_window`: strips WINDOW/SESSION/WEEKLY/GEMINI/C/G/ARGON prefixes, same order (65-67) | pace.rs:81-89 | same | pace.rs::test_window_parsing / tests/test_pace.py::test_window_parsing | Python treats None title as ""; Rust takes `&str`. |
| `window_caption` (70-73) | pace.rs:91-98 | same | none / none | |
| `window_seconds`: regex `(\d+(?:\.\d+)?)\s*([DHM])$` on short title, else default (76-78) | pace.rs:100-127 | diff | pace.rs::test_window_parsing / tests/test_pace.py::test_window_parsing | P2: looser parse (see top). Standard titles identical; truncation `int()` vs `as i64` identical. |
| `elapsed_fraction`: clamp 0..1; None for no reset or window<=0 (81-86) | pace.rs:129-136 | same | none / none | |
| `runout_text`: "" without percent/mark; `percent <= mark` -> "照目前速度，重設前不會用完"; else extrapolate to "照目前速度，預計 週X HH:MM 用完" (102-110) | pace.rs:156-189 | same | pace.rs::test_runout_text / tests/test_pace.py::test_runout_text | Rust truncates remaining seconds to whole before adding; Python adds a float timestamp: can differ by at most the rounding of seconds, minute label rarely differs. Rust adds a `rate <= 0` guard (unreachable after mark check). Strings identical. |

### autostart.py vs autostart.rs

| Python behaviour | Rust location | Status | Test coverage | Notes |
|---|---|---|---|---|
| APP_NAME `ClaudeHUDMonitor`, key `Software\Microsoft\Windows\CurrentVersion\Run` (autostart.py:7-8) | autostart.rs:8,46,58 | same | none / none | |
| Win `is_autostart_enabled`: value exists and truthy (10-18) | autostart.rs:42-50 | same | none / none | Rust treats an empty string value as enabled; Python as disabled. Negligible. |
| Win enable writes `"<exe>"` when frozen; from source `"<pythonw>" "<script>"` (28-37) | autostart.rs:63-68 | intentional-diff | none / none | Rust is always the single exe: `"<current_exe>"`. |
| Win disable: delete value, tolerate missing (38-43) | autostart.rs:69-72 | same | none / none | Rust also ignores other delete errors and returns true; Python would return False on other OSError. |
| Errors logged, returns False (44-46) | autostart.rs:67,75 | same | none / none | |
| macOS plist `~/Library/LaunchAgents/com.claudehud.plist`, Label com.claudehud, RunAtLoad true, ProgramArguments [exe(+script)] (47-61) | autostart.rs:83-118 | diff | none / tests/test_platform.py::test_mac_source_autostart_has_script_and_escapes_xml | No XML escaping (P2 above); no script arg (not needed for a binary). |
| macOS disable: `launchctl unload`, remove file (62-70) | autostart.rs:119-129 | same | none / none | |
| Linux: unsupported (returns False) (22,74) | autostart.rs:134-175 | intentional-diff | none / none | Rust adds `~/.config/autostart/claude-hud-monitor.desktop`. |

### logger.py / diagnostics.py vs logger.rs

| Python behaviour | Rust location | Status | Test coverage | Notes |
|---|---|---|---|---|
| Log dir by platform + portable `logs/` (logger.py:10-31) | logger.rs:170-193 | diff | none / none | P2: win equal (`%APPDATA%\ClaudeHUDMonitor\logs`); mac/linux differ; no portable branch. |
| Log file `hud_monitor.log` (34) | logger.rs:77 | same | none / none | |
| Rotating 2 MB, 3 backups (42-47) | logger.rs:41-53,78,80-85 | diff | none / none | Rust keeps 1 backup. |
| Levels: logger DEBUG, file DEBUG, console INFO, dev-mode console only (38,52,56-61) | logger.rs:19-21,35,102 | diff | none / none | Rust max level INFO everywhere; stderr always written (ignored errors). |
| Line format (48-50) | logger.rs:25-32 | intentional-diff | none / none | Rust has no filename:lineno. |
| Uncaught exception hooks: main thread "Unhandled exception in main thread:", background "… in background thread '<name>':" (64-82) | logger.rs:103-104,126-167 | same | logger.rs::tests::{main_thread_header_matches_python_record, background_header_uses_thread_name_or_unknown} / none | Payload and location appended in one line instead of a traceback. Provider-thread panics are logged by the controller instead (logger.rs:115-124). |
| `open_log_dir` via startfile/open/xdg-open, error logged (86-96) | logger.rs:196-201 | same | none / none | Rust also creates the dir first. |
| `configure_logging(config.path)`: `core` logger -> `diagnostics.log` next to config, 256 KB x 2, INFO, no propagate, NullHandler on OSError (diagnostics.py:7-19; main.py:59) | none | missing | none / none | P2. |

### smoke_check.py vs main.rs

| Python behaviour | Rust location | Status | Test coverage | Notes |
|---|---|---|---|---|
| `--smoke-test` flag runs `smoke_check.run` then exits with its code (main.py:109-111) | main.rs:158-168 | same | none / none | Rust enters isolation before logging so the log dir is isolated too. |
| Isolated APPDATA temp dir; restored afterwards (smoke_check.py:15-19,45-49) | config.rs:645-682 `SmokeIsolation` | same | config.rs::tests::test_smoke_writes_only_into_isolated_directory / none | Rust also overrides the config path and deletes the dir on drop. |
| Frozen path selection asserted to use the user-data dir (20-23) | config.rs:684-688 | same | config.rs::tests::test_smoke_writes_only_into_isolated_directory / none | Rust asserts path under the isolation root. |
| Config round trip: set opacity 0.55, reload must match (26-29) | config.rs:689-701, 670 `SMOKE_OPACITY` | same | config.rs::tests::test_smoke_round_trip_detects_persistence_failure / none | |
| HUD built with `providers={}` (offline, no credentials) (30) | main.rs:110 -> hud_window.rs:495-514,879-883 | diff | none / none | P1: live providers created and fetched. |
| Error then recovery on agy; asserts card sub-text and table badge cleared (31-36) | main.rs:111-121 | diff | none / none | P1: no assertions. |
| `toggle_ui_mode`, `set_color_scheme('duo')`, `set_appearance('dark')`, `processEvents` (37-40) | main.rs:122-126 | diff | none / none | Rust does `apply_ui_mode` table+sidebar, `set_theme(true)`, `update_clock`; no colour-scheme call. |
| Stop controller/timers, close window (41-44) | main.rs:128 (return, Drop) | intentional-diff | none / none | Cleanup via destructors. |
| `QT_QPA_PLATFORM=offscreen` (9) | none | intentional-diff | none / none | Not applicable to ferrokit; whether the Rust smoke needs a desktop is `unverified` at runtime. |

### main.py vs main.rs

| Python behaviour | Rust location | Status | Test coverage | Notes |
|---|---|---|---|---|
| `agy_statusline.cli(argv)` handled first, exit with its code (main.py:32-35) | main.rs:149-155 | same | none / tests/test_agy_statusline.py (not read) | Rust also attaches the parent console on Windows so output shows (main.rs:134-141). |
| `setup_logging()` then start banner lines (36-38) | main.rs:163-164 | same | none / none | Rust omits the Python-version line. |
| Windows AppUserModelID `ClaudeHUD.Monitor.App` (40-45) | main.rs:339-345,652-658 | same | none / none | |
| `qInstallMessageHandler` Qt->logger (19-29,47) | none | missing | none / none | P2; N/A for ferrokit. |
| High-DPI PassThrough, QApplication, `setQuitOnLastWindowClosed(False)` (50-52) | main.rs:361-362, 91-93 | same | main.rs::tests::hud_disables_quit_on_last_window_closed_like_python_main / none | DPI policy: not applicable / unverified. |
| App/window icon (54-56,63) | none seen in main.rs | unverified | none / none | Check `HUDTrayIcon`/`HUDWindow` for icon setup (outside this slice). |
| Config load + log path (57-59) | main.rs:348-352 | same | none / none | |
| HUD create + `show()` (61-65) | main.rs:370-379 | same | none / none | Rust fails with ExitCode::FAILURE if HUD creation errors. |
| Tray created and shown (67-69) | main.rs:388-390 | same | none / none | |
| Tray click toggles HUD visibility | main.rs:397-408 | unverified | none / none | Behaviour lives in `HUDTrayIcon` (tray_icon.py not in this slice). |
| Hotkeys: only when `hotkey_enabled`; Alt+C toggle, Alt+Shift+C click-through (71-94) | main.rs:505-540 | same | hotkey.rs::tests::test_compute_ct_mods / tests/test_platform.py (hotkey signal test) | Wiring via notify callback + main-thread post instead of Qt signals. |
| Hotkey failure balloon: title `⚠️ 全域快捷鍵通知`, text `{msg}\n您仍可透過系統匣圖示完整操作所有功能。`, 5000 ms, one per message (81-90) | main.rs:544-555 + hotkey.rs:56-79 | same | none / none | Order and strings checked: failed (with Win32 error) then unavailable, for toggle then click-through. Python logs "Hotkey registration issue"; Rust same log text (main.rs:547). |
| Click-through startup safety (HUD side; hud_window.py:426-437,725) | main.rs:557-572, hotkey.rs:81-92 | same | hotkey.rs::tests::test_lockout_warning_message_matches_python / none | Message text equal. Python decides inside HUD; Rust in main (intentional placement). |
| `aboutToQuit`: hotkey.stop, refresh_controller.stop (96-102) | main.rs:626-648 | diff | none / none | Hotkey stopped by Drop; `std::process::exit` skips locals' destructors (hotkey Arc, instance guard), the OS releases them. No controller stop. |
| Exit code propagated (104-106) | main.rs:626,647 | same | main.rs::tests::test_exit_code_preserves_32bit_values / none | |
| Single-instance guard / wake-up (none in Python main.py) | main.rs:308-333,61-87 | intentional-diff | none / none | Rust extra feature: second launch exits and wakes first. |
| `--snapshot` PNG generation (none in Python) | main.rs:170-306 | intentional-diff | none / none | Rust-only dev/preview mode. |
| System theme follow (Python HUD side) | main.rs:577-584 | unverified | none / none | HUD behaviour; owned by window slice. |

### hotkey.py vs hotkey.rs

| Python behaviour | Rust location | Status | Test coverage | Notes |
|---|---|---|---|---|
| IDs 9527 toggle / 9528 click-through; MOD_ALT 1, MOD_SHIFT 4, MOD_NOREPEAT 0x4000; WM_HOTKEY 0x0312 (hotkey.py:149-154) | hotkey.rs:329-334 | same | hotkey.rs::tests::test_parse_hotkey / none | |
| Message queue created with PeekMessage(PM_NOREMOVE) before registering (178-180) | hotkey.rs:337-341 | same | none / none | Rust also publishes thread id after that (race-free quit). |
| Toggle = Alt+C; click-through = Alt+Shift+C (182,191) | hotkey.rs:166-200 `parse_hotkey`, `compute_ct_mods` (304-317) | diff | hotkey.rs::tests::{test_parse_hotkey, test_compute_ct_mods} / none | P2: Rust derives keys from `config.hotkey` (default "Alt+C" -> Alt+C / Alt+Shift+C, equal). |
| Failure messages: `Alt+C 全域快捷鍵註冊失敗 (Win32 Error: N)，可能已被其他程式佔用` and `…已被占用，請使用系統匣操作`, same for click-through `穿透模式快捷鍵註冊失敗` (186-198) | hotkey.rs:56-79 | same | none / none | Labels from `format_hotkey` ("Alt+C", "Alt+Shift+C"). Error read immediately after `RegisterHotKey`. |
| `toggle_registered` / `clickthrough_registered` flags (133-134,184,192) | hotkey.rs:35-54 `HotkeyStatus`, `clickthrough_registered()` | same | none / none | |
| Wait for ready, timeout 1.5 s (228) | hotkey.rs:208-209,231 | diff | hotkey.rs::tests::test_hotkey_start_timeout_constant / none | Timeout constant equal; behaviour on timeout differs (P2). |
| Message loop: WM_HOTKEY -> signal for each id; exit on `GetMessageW <= 0` (204-218) | hotkey.rs:371-389 | same | none / none | Rust sets an atomic flag + calls notify callback; main posts to the UI thread. Python logs `GetMessageW == -1` (209); Rust silent. |
| Unregister on exit (219-223) | hotkey.rs:391-393 | same | none / none | Python only unregisters registered ones; Rust calls both. |
| `stop()`: `_running=False`, WM_QUIT with up to 5 retries, join 1.0 s (263-283) | hotkey.rs:104-129 `Drop` | intentional-diff | none / none | Single `PostThreadMessageW`, unbounded join. |
| macOS via pynput, Alt+C / Alt+Shift+C, accessibility check (230-261) | hotkey.rs:241-254 | missing | none / none | Non-Windows: `Unsupported`, warn log only; no balloon. File header has a stale `TODO`. Platform-scope decision. |
| `hotkey_failed` / `unavailable` signals (121-124) | hotkey.rs:56-79 `failure_notices` | intentional-diff | none / none | Signals replaced by a returned notice list. |

### memory.py vs memory.rs

| Python behaviour | Rust location | Status | Test coverage | Notes |
|---|---|---|---|---|
| `trim_memory`: `gc.collect()` + Win32 `EmptyWorkingSet(GetCurrentProcess())`, exceptions swallowed (memory.py:32-39) | memory.rs:7-48 | same | none / none | Windows: same API pair. `gc.collect` has no Rust analogue. |
| Non-Windows: no-op (Python) | memory.rs:35-47 | intentional-diff | none / none | Rust adds `malloc_zone_pressure_relief` (macOS), `malloc_trim` (Linux). |
| Schedule: 2500 ms after init, 1000 ms after busy->idle, 150 ms after hide (hud_window.py:150,820,989) | hud_window.rs:329-345,2560,2646 | same | hud_window.rs tests with counting trim (e.g. near 2593) / none | Named for completeness; owned by the window slice. |

## Counts per status

| Status | Count |
|---|---|
| same | 65 |
| intentional-diff | 15 |
| diff | 14 |
| missing | 4 |
| unverified | 4 |

Total rows: 102 (machine-counted from the Parity rows tables).

---

# Parity: providers slice (Python core/providers + agy_statusline + agy_rows vs Rust providers/*.rs, agy_statusline.rs, agy_rows.rs)

Paths: PY = `python/core/...`, RS = `rust/src/...` (worktree wt_retire). Read-only audit, nothing run.
Note: `read rust/src/providers/agy.rs` by suffix resolves to a *different* checkout (`claude-hud-monitor-main`, CLI-subprocess design); the worktree file (1004 lines, HTTP+cache) was used here.

## Discrepancies (diff / missing), most severe first

| Sev | Item | Fix hint |
|---|---|---|
| P2 | Non-2xx HTTP (e.g. 500) text: Rust `API 回應異常: HTTP n` (claude.rs:638-645, codex.rs:173-180); Python `配額查詢 HTTP n` (claude_provider.py:386, codex_provider.py:82). Code `http` both. | Use `配額查詢 HTTP {status}` for every non-401/429 error status. |
| P2 | Codex 401/429 results carry titles `PRIMARY`/`SECONDARY` (codex.rs:146-149,162-165); Python leaves defaults `SESSION 5H`/`WEEKLY 7D` (codex_provider.py:83). | Drop the title overrides in those two struct literals. |
| P2 | `retry_delay`: Python accepts non-negative finite seconds or an HTTP date, rejects `-1`/NaN/inf (base.py:55-66; test_http). Rust `parse::<f64>` (claude.rs:738, codex.rs:259) accepts negative/NaN/inf, no HTTP-date. | Share one `retry_delay()` in base.rs: reject non-finite/negative, parse RFC 2822 date. |
| P2 | `percentage()` Rust adds 1e-6 epsilon + clamp, accepts only JSON numbers (base.rs:13-20); Python strict `0<=v<=max`, converts numeric strings (base.py:10-18). -1e-7 -> Python None, Rust 0.0; "50" -> Python 50, Rust None. | Decide on one rule (Rust epsilon is deliberate per its comment; port the string/`float()` coercion or document). |
| P2 | Claude/Codex parse not wrapped by `safe_parse`: non-object body or odd types give Python `配額資料格式不相容` schema (base.py:25-38); Rust gives `未取得有效配額資料` (claude.rs:673, codex.rs:204). Test `test_malformed_schema_is_reported` has no Rust twin. | Validate object/field types and return `配額資料格式不相容`/`schema`. |
| P2 | Claude `resolve_active_profile` fallback keeps leading dot in `id` (`.claude-02`), Python strips it (`claude-02`) (claude.rs:473 vs claude_provider.py:276-277). | `id: pref_trimmed.trim_start_matches('.')`-style strip. |
| P2 | `resolve_active_profile` auto tie-break: Python `max` keeps first on equal `last_activity` (claude_provider.py:260); Rust `max_by_key` keeps last (claude.rs:423). | Iterate keeping first max. |
| P2 | `.Claude` alias: Python lowercases before `== ".claude"` (claude_provider.py:272); Rust compares raw (claude.rs:451). | Compare lowercased. |
| P2 | Codex `plan.title()`: Python title-cases every word (`plus_team` -> `Plus_Team`); Rust uppercases first char only (codex.rs:218-230 vs codex_provider.py:113). | Capitalise after each non-letter. |
| P2 | Codex `reset_at` of 0/negative: Python returns 1970 datetime (countdown "即將重設"); Rust None -> "--" (codex.rs:51 vs codex_provider.py:15-18). | Align or accept as intentional. |
| P2 | Codex auth path: Rust Windows prefers `HOME` over `USERPROFILE` (codex.rs:32-35); Python `expanduser` uses `USERPROFILE` (codex_provider.py:27). Claude/agy use correct order. | Reuse `agy_statusline::home_dir()` / `dirs_home()`. |
| P2 | Codex empty `{}` auth.json: Python -> "未找到 Codex 授權檔" (falsy); Rust -> "未找到 access_token" (codex.rs:101-121). Empty-string token / non-string `account_id` also treated differently (Python truthiness; Rust Some("")). | Treat empty object/empty token as missing. |
| P2 | Claude keychain fallback (macOS only): Python only for `default` profile and 5 s timeout (claude_provider.py:306,324); Rust any profile, no timeout (claude.rs:524-529,494-502). | Gate on `id == "default"`, add timeout. |
| P2 | `resets_at` parsing: Python `fromisoformat` (accepts naive, 3.11+ forms); Rust RFC3339 only (claude.rs:730). Naive strings -> Rust None. | Accept naive as local/UTC if API can emit it. |
| P2 | `agy` `command` not an object (`{"command":null}`): Python -> `配額資料格式不相容` (test_malformed_schema_is_reported); Rust pointer miss falls through to "no groups" `未取得有效配額資料` (agy.rs:521-543). | Return `format_error` when `command`/`data` present but not objects. |
| P2 | Dedup key in profile scan: Python `normpath` (lexical); Rust `canonicalize` (resolves symlinks) (claude.rs:320 vs claude_provider.py:192). | Intentional-ish; keep or use lexical. |
| P2 | `install_statusline` hook path: Python `abspath` normalises `..`; Rust `std::path::absolute` does not (agy_statusline.rs:257). | Lexically normalise. |
| P2 | `AgyProvider::os_cred` macOS: Python `timeout=3`; Rust none (agy.rs:88-98 vs agy_provider.py:63-66). | Add timeout. |
| P2 | Provider `display_name`: Claude "Claude" (py) vs "Claude Code" (rs); only used in Python safe_parse fallback name (claude.rs:556). | Cosmetic. |

No P0/P1 discrepancies found in this slice. `missing`: Claude `CREDENTIALS_PATH` override and `profile_preference`/`token` hooks (test seams only), Python `PROVIDERS` registry (Rust uses `PROVIDER_IDS` + factory elsewhere).

## Matrix

### base
|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|`percentage(value,max)` finite, 0..=max else None, bool->None (base.py:10-18)|providers/base.rs:13-20|diff|Rust base::test_percentage_valid_and_invalid; Py test_providers::test_invalid_percentage|epsilon 1e-6 + clamp; no string coercion. Listed above.|
|`percent_text` `"--"` or `{:.0f}%` (base.py:21-22)|base.rs:23-28|same|Rust base::test_percent_text; Py test_providers (indirect)|Both round-half-even on exact value.|
|`safe_parse` decorator: non-dict/Type/Value errors -> `配額資料格式不相容`, code `schema`; error w/o code -> schema (base.py:25-38)|claude.rs:716-725, codex.rs:245-254 (code set inline); agy.rs:415 `format_error`|diff|Py test_providers::test_malformed_schema_is_reported; Rust none (agy: agy::a_body_of_the_wrong_shape_is_a_format_error_not_a_panic)|agy is faithful; claude/codex lack type validation.|
|`ssl_context()` certifi CA bundle, verify on (base.py:41-52)|claude.rs:547, codex.rs:27 (ureq default TLS)|intentional-diff|Py test_credentials::SSLContextTests (both); Rust none|Rust uses ureq's TLS roots; no certifi concept.|
|`retry_delay` seconds or HTTP-date, no inf/neg (base.py:55-66)|claude.rs:738-740, codex.rs:259-261|diff|Py test_http::test_bad_retry_header; Rust none|Listed above.|
|`MetricRow` fields/defaults (`--`, "") (base.py:69-75)|base.rs:30-56|same|Rust agy_rows tests use it; Py test_agy_rows|`reset` is `DateTime<Utc>` (Python may hold naive).|
|`UsageMetrics` fields + defaults (SESSION 5H / WEEKLY 7D, `--`, error_code "", stale False) (base.py:78-104)|base.rs:62-116|same|none (indirect)|Rust adds `error_result()` helper (base.rs:119-137).|
|`format_countdown`: `--`, `即將重設` when <=0, `{d}天 {h}時 {m}分`, `{h}h {m:02}m {s:02}s`, `{m}m {s:02}s` (base.py:114-136)|base.rs:146-170|same|Rust base::test_format_countdown; Py none|Python also handles naive datetimes (local now); Rust UTC only (intentional, types).|
|`now_str` `%H:%M:%S` local (each provider)|base.rs:140-142|same|none||
|`PROVIDERS` registry claude/agy/codex (providers/__init__.py:6-10)|providers/mod.rs:13 `PROVIDER_IDS` = claude, agy, codex|same|none|Order identical. Rust `stub.rs` is test-only (no Python counterpart).|

### claude
|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|USAGE_URL / UA `claude-code/0.2.29` / beta `oauth-2025-04-20` (claude_provider.py:286-288)|claude.rs:250-252|same|none||
|Timeout 10 s (claude_provider.py:290)|claude.rs:546|same|none|ureq total timeout vs urllib per-op.|
|Scan `$HOME` dirs starting `.claude` (case-insens.), default `.claude` always listed, others need `.credentials.json`; id/short/display strings (claude_provider.py:181-211)|claude.rs:306-348|same|Rust claude::test_resolve_active_profile (indirect); Py test_credentials::test_resolve_active_profile|Strings `預設`, `預設帳號 (~/.claude)`, `{raw} (~/{entry})` equal.|
|Dedup via `normpath` (claude_provider.py:192)|claude.rs:320|intentional-diff|none|canonicalize.|
|`CLAUDE_CONFIG_DIR`: dir + creds required, id = basename minus dot, display `{raw} ({env_dir})` (claude_provider.py:216-232)|claude.rs:352-376|same|none|Trailing-slash dir: Python basename "" vs Rust "env" fallback.|
|Ensure default included; sort default first then `id.lower()` (claude_provider.py:235-250)|claude.rs:379-405|same|none||
|`get_last_activity` max mtime of creds/history (claude_provider.py:159-173)|claude.rs:280-293|same|none||
|Auto resolve: with-creds candidates, newest activity; else first; else synthetic default (claude_provider.py:257-265)|claude.rs:416-444|diff|Py test_credentials::test_resolve_active_profile (auto); Rust claude::test_resolve_active_profile|Tie-break first vs last.|
|Explicit match: id / short_name / `.`+id / `.claude`->default, case-insens. (claude_provider.py:267-273)|claude.rs:447-456|diff|same tests|`.Claude` case.|
|Unknown pref fallback profile (claude_provider.py:275-277)|claude.rs:459-481|diff|same tests|id keeps dot in Rust. short_name/display equal.|
|Header title: `CLAUDE CODE` default; auto `CLAUDE [short]`; explicit `CLAUDE (short)` (claude_provider.py:343-348)|claude.rs:569-579|same|none|Python also `Claude Code` for CREDENTIALS_PATH override (test seam, missing in Rust).|
|Token: profile creds file, macOS keychain only for default profile (claude_provider.py:295-308)|claude.rs:513-532|diff|Py test_credentials::test_macos_falls_back_to_keychain / test_file_takes_precedence_over_keychain / test_keychain_not_used_off_macos; Rust none|Keychain gate differs (macOS only). Python overridable `CREDENTIALS_PATH` is `missing`.|
|`_extract_token`: `claudeAiOauth.accessToken`, non-dict -> None (claude_provider.py:332-337)|claude.rs:484-489|same|Rust claude::test_extract_token; Py test_credentials::test_keychain_failures_return_none|Python returns non-string tokens as-is; Rust requires string.|
|Keychain cmd `/usr/bin/security find-generic-password -s "Claude Code-credentials" -w`, failure -> None (claude_provider.py:320-330)|claude.rs:491-511|unverified|Py test_credentials::test_keychain_failures_return_none; Rust none|macOS-only; no 5 s timeout in Rust.|
|No token message `未找到 Claude 登入憑證\n請於終端機執行 claude 登入` / `未找到帳號 [x] 登入憑證\n請使用 x 登入或檢查目錄`, code "" (claude_provider.py:352-360)|claude.rs:581-591|same|none||
|Request headers Authorization/User-Agent/anthropic-beta/Accept (claude_provider.py:362-367)|claude.rs:593-601|same|none||
|401 -> `登入憑證已失效，請使用原 CLI 重新登入`, code `auth` (claude_provider.py:385-386)|claude.rs:606-621|same|none|retry_after parsing differs (see base).|
|429 -> `配額查詢 HTTP 429`, `rate_limit`, `retry_after` (claude_provider.py:385-389)|claude.rs:622-637|same|Py test_http::test_rate_limit_preserves_retry_after; Rust none|Retry value parsing per `retry_delay` diff.|
|Other error status -> `配額查詢 HTTP n`, code `http` (claude_provider.py:386)|claude.rs:638-645|diff|none|Message `API 回應異常: HTTP n`.|
|Body not JSON -> `未取得有效配額資料`, `schema` (claude_provider.py:390-392)|claude.rs:652-657|same|none|Rust also maps body I/O errors here; Python -> network.|
|Connection failure -> `配額連線失敗，將自動重試`, `network` (claude_provider.py:393-396)|claude.rs:660-668|same|Py test_credentials::test_providers_pass_context_to_urlopen; Rust none||
|Parse: `five_hour`/`seven_day` utilization -> metric1/2, titles SESSION 5H / WEEKLY 7D (claude_provider.py:400-440)|claude.rs:673-715|same|Rust claude::test_parse_claude_response; Py test_providers::test_zero_and_partial_data|Zero preserved in both.|
|Badges `Code: x%` / `Chat: x%` from `seven_day_breakdown.rows` keys `claude_code`/`chat` (claude_provider.py:403-411,441-442)|claude.rs:681-694,713-714|same|Rust claude::test_parse_claude_response|Non-dict row: Python schema error, Rust skips.|
|Both metrics None -> `未取得有效配額資料` + `schema` (claude_provider.py:444; base.py:30-31)|claude.rs:716-725|same|Py test_providers::test_missing_data_is_not_zero; Rust none||
|`resets_at` via `fromisoformat` (claude_provider.py:413-425)|claude.rs:696-697,730-736|diff|Rust claude::test_parse_claude_response (Z form)|RFC3339 only.|

### codex
|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|URL `chatgpt.com/backend-api/wham/usage`, UA `codex-cli/0.154.0`, timeout 8 (codex_provider.py:28,30,68)|codex.rs:16-17,26|same|none||
|Auth file `~/.codex/auth.json` read as UTF-8 JSON (codex_provider.py:27,33-40)|codex.rs:31-45|diff|none|Windows HOME-before-USERPROFILE.|
|Missing auth message `未找到 Codex 授權檔 (~/.codex/auth.json)\n請執行 codex 登入` (codex_provider.py:46-52)|codex.rs:101-108|diff|none|Text equal; `{}` handling differs.|
|Missing token `未找到 access_token\n請於終端機執行 codex 登入` (codex_provider.py:58-64)|codex.rs:111-121|same|none|Empty string handling differs.|
|Headers + `ChatGPT-Account-Id` when present (codex_provider.py:66-72)|codex.rs:127-136|same|none||
|401 `auth` / 429 `rate_limit`+retry_after / other `http` (codex_provider.py:79-85)|codex.rs:141-180|diff|Py test_http::test_rate_limit_preserves_retry_after; Rust none|Titles PRIMARY/SECONDARY on 401/429; non-2xx message.|
|Non-JSON -> schema; transport error -> `network` (codex_provider.py:86-92)|codex.rs:181-199|same|Py test_credentials::test_providers_pass_context_to_urlopen; Rust none||
|Parse `rate_limit.primary_window/secondary_window.used_percent` -> metrics (codex_provider.py:97-129)|codex.rs:204-257|same|Rust codex::test_parse_codex_response; Py test_providers::test_zero_and_partial_data|Provider name `OpenAI Codex` both.|
|Plan badge `Plan: {plan.title()}`, badge2 empty, no model invented (codex_provider.py:112-127)|codex.rs:218-230,243|diff|Rust codex::test_parse_codex_response; Py test_providers::test_codex_does_not_invent_model_or_plan|Single-word plans equal; multi-word differs.|
|`_window_title`: `WINDOW nD/nH/nM` from `limit_window_seconds`, else fallback PRIMARY/SECONDARY (codex_provider.py:132-141)|codex.rs:76-89|same|Rust codex::test_window_title; Py test_providers::test_codex_does_not_invent_model_or_plan|`:g` vs Display: same for integer-valued results; fractional values with >6 digits could differ.|
|`_parse_timestamp`: numeric, ms if >1e11, UTC (codex_provider.py:11-21)|codex.rs:48-74|diff|Rust codex::test_parse_timestamp; Py none|Rust also accepts ISO strings, rejects <=0. Superset/edge.|
|Both None -> `未取得有效配額資料`+`schema` (codex_provider.py:129)|codex.rs:245-254|same|Py test_providers::test_missing_data_is_not_zero; Rust none||

### agy provider
|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|Endpoints SUMMARY/QUOTA `daily-cloudcode-pa...retrieveUserQuotaSummary` / `retrieveUserQuota`, UA `antigravity/1.2.4`, timeout 8 (agy_provider.py:19-21,92)|agy.rs:15-19|same|Rust agy::both_endpoints_are_posted_in_order_with_the_token; Py test_agy_transport::test_both_endpoints_succeed_and_request_spec||
|No CLI-subprocess path in either (Python never had one here)|agy.rs:5|same|none|Rust has no `agy` binary lookup in this worktree.|
|Token: Windows CredReadW `gemini:antigravity` -> `token.access_token` (agy_provider.py:28-58)|agy.rs:26-80|same|Rust agy::os_cred_token_lookup_does_not_panic; Py none|Platform-specific; runtime behaviour `unverified` against live Credential Manager.|
|Token: macOS `security find-generic-password -s gemini -a antigravity -w` (agy_provider.py:61-72)|agy.rs:82-105|unverified|none|3 s timeout missing in Rust.|
|Token: file `~/.gemini/antigravity-cli/antigravity-oauth-token` fallback; empty token ignored (agy_provider.py:75-86)|agy.rs:107-145|same|none|Linux has no keyring lookup in either.|
|`_fetch_via_http`: summary POST then quota POST, `{}` body; 401 on either -> auth failure, other failure leaves part empty (agy_provider.py:88-135)|agy.rs:190-277|same|Rust agy::a_401_from_the_first_endpoint_stops_at_once / a_failing_quota_endpoint_leaves_a_partial_result; Py test_agy_transport::test_endpoint_a_401_returns_auth_error / test_endpoint_b_failure_returns_partial_result||
|HTTP result assembly: GEMINI 5H from quota, GEMINI 7D None + `Gemini 週額度來源未知`, badge1 from summary else `C/G 剩餘: --`, badge2 `Gemini Models`, error schema when no m1 and no C/G rows (agy_provider.py:137-162)|agy.rs:221-254|same|Rust agy::both_endpoints_failing_is_a_schema_error / both_endpoints_are_posted_in_order_with_the_token; Py test_agy_transport::test_both_endpoints_fail_returns_error||
|`fetch_usage`: read cache, token, http; no cache -> http result or `no_token` (`未找到 Antigravity 登入憑證\n請先登入 Antigravity`), badge2 `Argon: --` (agy_provider.py:164-177)|agy.rs:366-387|same|Rust agy::without_a_cache_or_token_the_failure_is_no_token / without_a_cache_the_http_auth_error_is_kept; Py test_agy_transport::test_no_token_returns_no_token_without_subprocess / test_no_cache_preserves_http_auth_error|Python wraps cache read in try/except; Rust `Option`.|
|`_merge_cache`: fill empty rows from `3p-5h`/`3p-weekly`/`gemini-argon-wtu-weekly`, m1 from `gemini-5h`, m2 from `gemini-weekly` (+ hint `需啟用 agy statusLine` when absent), badge1 recomputed if empty/`--`, badge2 `Argon 剩餘: x%`, titles, error from has_value (agy_provider.py:179-217)|agy.rs:280-349|same|Rust agy::the_cache_complements_the_endpoints_and_the_endpoints_win / the_cache_alone_gives_complete_metrics_without_a_token / the_cache_recovers_when_every_request_fails / a_missing_weekly_bucket_asks_for_the_status_line / a_cache_without_buckets_is_a_schema_error; Py test_agy_transport (matching names)|Used% = clamp((1-f)*100) both.|
|`_empty_rows`: `C/G 5H`, `C/G 7D`, `ARGON 7D` (subtext `需啟用 agy statusLine`) (agy_provider.py:219-222)|agy.rs:391-400|same|Rust agy::both_endpoints_are_posted_in_order_with_the_token||
|`_failure(code,msg)` (agy_provider.py:224-231)|agy.rs:402-412|same|Rust agy::without_a_cache_the_http_auth_error_is_kept||
|`_parse_model_buckets`: max used Gemini model bucket -> GEMINI 5H, schema when none, rows only if found (agy_provider.py:233-283)|agy.rs:460-513|same|Rust agy::the_most_constrained_gemini_bucket_is_the_five_hour_row; Py test_agy_transport::test_gemini_picks_most_constrained_bucket|`modelId`/`model_id`, `remaining_fraction`/`remainingFraction`, `reset_time`/`resetTime` equal.|
|`_parse_agy_json`: `command.data.groups` or `groups`; empty -> schema; Gemini (non-argon) 5h/week buckets, claude/gpt rows, unknown group logged (agy_provider.py:285-363)|agy.rs:517-634|same|Rust agy::groups_under_command_data_are_read_too / gemini_week_buckets_fill_the_weekly_row / an_unknown_group_is_ignored; Py test_providers::test_agy_zero / test_agy_uses_most_constrained_bucket, test_agy_transport::test_unknown_group_logged_and_ignored||
|`_parse_agy_json` result: badge1 `C/G 剩餘: x%` (100 - 7D used), badge2 `Gemini Models`, m2 subtext, error when all empty (agy_provider.py:365-389)|agy.rs:636-660|same|Rust agy::the_cache_complements...; Py test_agy_transport::test_both_endpoints_succeed_and_request_spec||
|Malformed shapes -> `配額資料格式不相容`/`schema` (via safe_parse) (agy_provider.py:233-239,285-292)|agy.rs:415-417,462-468,518-520,554-559|diff|Rust agy::a_body_of_the_wrong_shape_is_a_format_error_not_a_panic; Py test_providers::test_malformed_schema_is_reported|`{"command":null}` case differs.|
|Metric titles for agy HTTP parse `GEMINI 5H`/`GEMINI 7D` (agy_provider.py:374,378)|agy.rs:640,644|same|Rust agy tests above|Old `SESSION 5H` titles no longer in Rust.|

### agy_statusline
|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|HOOK_FILENAME / QUOTA_CACHE_FILENAME (agy_statusline.py:12-13)|agy_statusline.rs:17-18|same|none||
|HOOK_SOURCE script text (agy_statusline.py:15-59)|agy_statusline.rs:21-65|same|Rust agy_statusline::install_then_uninstall_round_trips_and_keeps_a_backup (writes it); Py test_agy_statusline::test_hook_privacy_and_throttle / test_hook_invalid_input|Compared programmatically: byte-identical. Hook behaviour itself is only tested on the Python side.|
|`agy_settings_path` `~/.gemini/antigravity-cli/settings.json` (agy_statusline.py:62-63)|agy_statusline.rs:83-88|same|none||
|Home dir: `expanduser` (USERPROFILE on Windows) (agy_statusline.py:63)|agy_statusline.rs:91-102|same|none|HOME first elsewhere.|
|`quota_cache_path` = `get_user_config_dir()/agy-quota.json` (agy_statusline.py:66-67)|agy_statusline.rs:104-106|unverified|none|Depends on `ConfigManager::config_dir()` equality with Python (config_manager.py:71-81 vs config.rs:425-440): APPDATA/ClaudeHUDMonitor and macOS path equal; Linux branch not read. Python also `makedirs`.|
|`_utc`: ISO + offset -> UTC, naive -> None (agy_statusline.py:70-79)|agy_statusline.rs:109-115|unverified|Rust agy_statusline::reader_drops_bad_fractions_and_expired_buckets_and_ignores_a_broken_reset; Py test_reader_filters_and_invalid_reset|RFC3339 + one fallback format vs full `fromisoformat`; exotic forms may differ.|
|`read_quota_cache`: `quota` dict, `written_at` valid, fraction numeric (no bool) in 0..=1, drop reset<=now, None when empty (agy_statusline.py:82-105)|agy_statusline.rs:124-157|same|Rust agy_statusline::reader_returns_the_five_buckets_as_floats / reader_drops_bad_fractions_and_expired_buckets... / reader_returns_none_for_a_missing_or_unusable_file; Py test_reader_five_buckets / test_reader_filters_and_invalid_reset / test_reader_bad_files||
|`_load_settings` must be object (agy_statusline.py:108-113)|agy_statusline.rs:159-165|same|Rust agy_statusline::unsupported_paths_and_unreadable_settings_change_nothing; Py test_unsupported_and_missing||
|`_owned`: statusLine dict with string command containing hook filename (agy_statusline.py:116-119)|agy_statusline.rs:167-172|same|Rust a_status_line_that_is_not_ours_is_left_alone; Py test_conflict||
|`_backup` -> `<path>.bak-hud-%Y%m%d-%H%M%S[-n]` exclusive create (agy_statusline.py:122-135)|agy_statusline.rs:176-200|same|Rust install_then_uninstall_round_trips_and_keeps_a_backup; Py test_install_and_uninstall||
|`_write_settings`: temp+replace, indent 2, non-ASCII kept (agy_statusline.py:138-148)|agy_statusline.rs:203-220|same|Rust install_keeps_the_order_of_the_users_keys / uninstall_keeps_the_order_of_the_users_keys; Py test_install_and_uninstall|Temp name differs (irrelevant). Requires serde_json `preserve_order` for key-order tests (asserted by those tests).|
|`install_statusline`: unsupported_path on whitespace/quotes, conflict, already, installed, error; desired `{type:command, command:"python <hook>", enabled:true}`; hook written after backup (agy_statusline.py:151-174)|agy_statusline.rs:263-305|diff|Rust install_then_uninstall..., unsupported_paths_and_unreadable_settings_change_nothing, a_status_line_that_is_not_ours_is_left_alone; Py test_install_and_uninstall, test_conflict, test_unsupported_and_missing|Only difference: `abspath` normalisation (listed above). `python`/`python3` default equal.|
|`uninstall_statusline`: not_installed / conflict / removed (+backup) (agy_statusline.py:177-190)|agy_statusline.rs:308-329|same|Rust install_then_uninstall..., uninstall_keeps_the_order_of_the_users_keys; Py test_install_and_uninstall||
|`cli` flags `--install-agy-statusline` / `--uninstall-agy-statusline`, messages, exit 0/1 (agy_statusline.py:193-210)|agy_statusline.rs:222-255,333-344|same|Rust agy_statusline::cli_exit_codes_follow_the_outcomes; Py test_cli|All 7 message strings equal.|

### agy_rows
|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|`AGY_ROW_TITLES`, `DEFAULT_PRIMARY` (`C/G 5H`,`ARGON 7D`), `DEFAULT_ORDER` (agy_rows.py:18-20)|agy_rows.rs:7-9|same|Rust agy_rows::default_order_starts_with_the_default_pair; Py test_default_order_starts_with_the_default_pair|Rust DEFAULT_ORDER literal equals computed Python value.|
|`normalize_primary`: exactly two distinct known titles (upper-cased) else default (agy_rows.py:223-233)|agy_rows.rs:16-39|same|Rust agy_rows::bad_settings_fall_back_to_default; Py test_bad_settings_fall_back_to_default|Non-list input is type-excluded in Rust.|
|`normalize_order`: 5 titles / 2 titles (+rest canonical) / default (agy_rows.py:236-249)|agy_rows.rs:42-56|same|Rust agy_rows::normalize_order_accepts_full_orders_and_legacy_pairs; Py test_normalize_order_accepts_full_orders_and_legacy_pairs||
|`move_row` (agy_rows.py:252-260)|agy_rows.rs:60-70|same|Rust agy_rows::move_row_cases; Py test_move_row|Index clamped after removal in both.|
|`toggle_primary` (agy_rows.py:263-274)|agy_rows.rs:73-83|same|Rust agy_rows::toggle_replaces_oldest_or_moves_to_top; Py test_toggle_replaces_oldest_or_moves_to_top||
|`arrange_agy`: needs all five titles else unchanged; first two -> metric1/2, others extras; later duplicate title overwrites (agy_rows.py:277-299)|agy_rows.rs:126-154|same|Rust agy_rows::any_pair_keeps_all_five_rows / incomplete_or_error_data_is_left_alone / arrange_uses_the_full_order; Py test_any_pair_keeps_all_five_rows / test_incomplete_or_error_data_is_left_alone / test_arrange_uses_the_full_order|Python returns same object (`assertIs`); Rust returns clone (test uses `same()` stand-in).|
|`arrange_by_order`: stable sort by index in `order`, unlisted keep natural position after; unchanged if titles identical (agy_rows.py:302-325)|agy_rows.rs:159-184|same|Rust agy_rows::arrange_by_order_for_two_row_providers; Py test_arrange_by_order_for_two_row_providers|Non-list `order` early return is type-excluded.|

### Tests present on one side only
|Python test|Rust|Note|
|---|---|---|
|test_encoding::test_source_and_documentation_have_no_replacement_runs|none|Source-hygiene check on Python files; no Rust equivalent needed (or add a repo-wide U+FFFD / `???` scan).|
|test_agy_statusline::test_hook_privacy_and_throttle / test_hook_invalid_input|none|Hook is executed by Python; Rust only embeds identical text.|
|test_credentials::SSLContextTests|none|Intentional-diff (TLS stack).|
|test_http::test_bad_retry_header|none|Related to `retry_delay` diff.|

## Counts (matrix rows)
- same: 62
- intentional-diff: 2 (ssl_context, normpath vs canonicalize; Python-only seams noted as missing in prose)
- diff: 16
- missing: 0 rows (seam-only items in the prose list above: `CREDENTIALS_PATH` override, `profile_preference`)
- unverified: 4 (macOS keychain Claude, macOS keychain agy, `_utc` exotic ISO forms, `quota_cache_path`/config dir equality)

---

# Parity matrix: HUD window / menu / tray / vibrancy / round mask

Python: `python/ui/{hud_window,hud_menu,tray_icon,vibrancy,round_mask}.py`, tests `python/tests/{test_ui,test_vibrancy,test_round_mask}.py`.
Rust: `rust/src/ui/{hud_window,tray_icon,round_mask,placement}.rs` (menu = `tray_icon.rs::build_hud_menu`; vibrancy/Acrylic/suspend = ferrokit `Window::set_backdrop/suspend_backdrop/resume_backdrop` called from `hud_window.rs`, implemented in ferrokit rev 8422301 `ferrokit-platform/src/{backdrop,window}.rs`, read there). Right-click / tray glue is in `rust/src/main.rs`.
Static read only; nothing built or run. `Python test` = `tests/test_ui.py::HudTests::<fn>` unless noted.

## Discrepancies (diff / missing), by severity

| Sev | Item | Fix hint |
|---|---|---|
| P2 | `mouseDoubleClickEvent` -> `trigger_async_refresh` (py hud_window.py:991-994) has no Rust counterpart: grep for double-click in `rust/src` finds nothing | Add a double-click handler on the window (ferrokit reports native double clicks) that calls `trigger_refresh()` |
| P2 | Tray/app icon: py picks `app_icon.ico` on win32, falls back to a painted rounded "C" icon (tray_icon.py:11-42); Rust always embeds `app_icon.png`, fallback is a flat 16x16 cyan square (tray_icon.rs:89-101) | Prefer .ico (or accept PNG as intended) and port the painted fallback, or mark intentional |
| P2 | Startup click-through: py delays 300 ms then checks `clickthrough_registered` (hud_window.py:144,724-735 `_safe_init_click_through`); Rust builds the window with `CLICK_THROUGH` flag at once and checks the hotkey afterwards (hud_window.rs:575-583, main.rs:558-572) | Window may be click-through for a moment before the lockout check turns it off; defer flag until hotkey result is known |
| P2 | Lockout warning balloon icon: py passes `tray_icon.icon()` (app icon), Rust uses `TrayMessageIcon::Warning`; click-through-enabled/autostart-failed balloons use `Information` instead of app icon (tray_icon.rs:511,533; main.rs:569) | Cosmetic; use app-icon balloon if toolkit allows |
| P2 | Claude profile menu: py `is_auto = current == "auto"`; Rust also treats empty and case-insensitive "auto" as auto, and profile ids are looked up by *index* at click time (`ACTION_CLAUDE_PROFILE_BASE+i` -> `discover_profiles()` again, tray_icon.rs:460-489,740-750) so a profile list change between menu build and click selects the wrong/no profile | Carry the profile id in the action data (or cache the list used to build the menu) |
| P2 | `toggle_visibility` show path: py also calls `activateWindow()` (hud_window.py:1122-1127); Rust `show()` only (hud_window.rs:1072-1078) | Add focus/activate if toolkit has it; Tool window so low impact |
| P2 | `_toggle_always_on_top`: py re-applies native click-through after `show()` when click-through is on (hud_window.py:1021-1029); Rust only calls `set_stays_on_top` (hud_window.rs:1093-1100) | Verify on desktop that click-through survives the topmost toggle (see unverified row) |
| P2 | `hide()`: Rust flushes + persists geometry and closes the sidebar flyout (hud_window.rs:1014-1020); py `hide` does neither (flyout only closes on mode switch) | Probably benign/safer; document as intentional |
| P2 | `set_opacity` clamps to 0.1-1.0 in Rust (hud_window.rs:1392); py stores the value unclamped | Menu only offers 0.3-1.0, so no observable effect |

Only one `missing` (double-click refresh); the rest are small `diff`s; `unverified` rows are listed in the matrix.

## Matrix

### A. Constants / timers

|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|`SIDEBAR_SNAP_PX = 24`, `RESIZE_GRIP_PX = 7`, `QWIDGETSIZE_MAX = 16777215` (hud_window.py:39-41)|hud_window.rs:1460-1463|same|Rust `sidebar_geometry_tests::grips_are_seven_pixels_wide_and_corners_combine`; Py none direct|values equal|
|`RESIZE_MARGIN = 8` for table-mode edge resize (hud_window.py:74, `_calc_edge` 901-920)|ferrokit `calc_frameless_edge` const `M: i32 = 8` (ferrokit-platform window.rs:2916), used hud_window.rs:716,760|same|none|py: x<m left, x>w-m right, y<m top, y>h-m bottom; Rust same comparisons; locked or sidebar -> no edge on both sides|
|`IDLE_FADE_FACTOR 0.6`, `IDLE_FADE_DELAY_MS 900`, clamp level 0.2-0.95, delay 200-30000 (hud_window.py:42-43,1022-1033)|config.rs:404-418 (`idle_fade_level_setting`, `idle_fade_delay_setting`), hud_window.rs:2138-2145|same|Py `test_idle_fade_level_and_delay_are_customisable`; Rust none in this slice (config tests elsewhere)|non-finite -> 0.6 in Rust, `TypeError/ValueError` -> 0.6 in py|
|`IDLE_LEVELS (80,60,40,25)`, `IDLE_DELAYS_MS (500,900,2000,5000)`, `SIDEBAR_SCALES`, `OPACITIES`, `INTERVALS` (hud_menu.py:13-17)|tray_icon.rs:70-72, per-id consts 41-51, 70-83|same|Rust `menu_parity_tests::sidebar_entries_list_the_size_the_row_order_and_the_idle_fade`, `behaviour_and_more_groups_list_the_python_items`; Py `test_idle_fade_submenu_lists_levels_and_delays`|OPACITIES 100/90/80/70/50/30 and INTERVALS 30/60/120/300 equal|
|`geometry_timer` single shot 250 ms (hud_window.py:85)|config.rs:707 `RESIZE_DEBOUNCE_MS = 250`, `ResizeDebouncer`|same|Rust `geometry_persist_tests::the_debounced_save_holds_the_latest_position_and_size`, `a_resize_is_saved_once_with_the_size_the_window_has`; Py none|py saves on move/resize via timer; Rust request_save on move/resize|
|`_blur_resume_timer` 220 ms single shot (hud_window.py:94)|hud_window.rs:371,737-742 `BLUR_RESUME_MS`, `resume_blur_or_retry` 431|same|Rust `table_size_tests::a_busy_hud_makes_the_resume_timer_arm_itself_again`; Py `test_blur_pauses_while_the_window_is_moved_or_resized_and_comes_back`|Rust adds retry when HUD is borrowed (intentional: toolkit re-entrancy)|
|`_snap_timer` 180 ms, `_fade_timer` start(1500) after mode pick, then `idle_fade_delay()` on leave (hud_window.py:133,440,1078-1080)|hud_window.rs:1469-1471,788-799,1816-1818,1767-1779|same|Rust none direct; Py `test_sidebar_dims_when_idle_and_recovers_on_hover`|enter stops the timer + applies opacity, leave starts it: both match|
|countdown timer 1000 ms; gap > 15 s triggers refresh of all providers (hud_window.py:737-740,822-834)|main.rs:588-600 clock timer 1000; hud_window.rs:319-329 `tick_gap_exceeded`, 1353-1382 `on_clock_tick`|same|Rust `tick_gap_is_false_for_the_first_tick_and_up_to_exactly_15_seconds`, `tick_gap_is_true_just_above_15_seconds`, `tick_gap_is_false_when_the_clock_moved_backwards`, `clock_tick_refreshes_every_provider_exactly_once_after_a_long_gap`; Py none|py uses `time.time()`; Rust SystemTime wall clock|
|`trim_memory`: 2500 ms after init, 150 ms after `hideEvent`, 1000 ms after busy->false (hud_window.py:114,617,455-459)|hud_window.rs:330-348,877,889,1058-1062|same|Rust `construction_schedules_initial_trim_at_2500ms`, `hide_trims_150ms_later_not_during_the_hide`, `close_request_trims_150ms_after_the_hide`, `last_provider_finishing_trims_a_second_later`, `completion_while_another_provider_runs_does_not_trim`; Py none|Rust extra: close request also trims (WM_CLOSE reaches Hide)|
|Window opacity default 0.88; menu check tolerance `abs(opacity - pct/100) < 0.05` (hud_window.py:167, hud_menu.py:84)|config.rs:210 `default_opacity` 0.88; tray_icon.rs:359-368|same|Rust `menu_parity_tests::checked_state_follows_config`; Py none|f32 vs float compare, same threshold|

### B. Window construction, flags, layout

|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|Flags: Frameless + Tool, + StaysOnTop if `always_on_top` (default True), translucent bg, mouse tracking (hud_window.py:158-168)|hud_window.rs:575-583|intentional-diff|none|Rust adds `CUSTOM_FRAMELESS`, `LAYERED` (toolkit needs layered window for per-pixel alpha) and `CLICK_THROUGH` when config says so|
|Root margins (12,8,12,10), spacing 6; compact: `COMPACT_MARGIN` all sides, spacing 0 (hud_window.py:172-176,387-392,408-415)|hud_window.rs:53-54,141-166,2216-2238|same|Rust `test_table_mode_table_takes_extra_height_like_pyside6`, `table_dials_meet_their_minimum_size_and_share_one_height`; Py `test_table_collapses_to_dials_when_shrunk_and_expands_when_enlarged`|sidebar: margins 0 spacing 0 on both|
|Header: status dot "●" (`#10b981`, 11px), title `HeaderTitle` "AI AGENT HUD (TABLE)", ghost "👻" tooltip "滑鼠穿透中 (Alt+Shift+C 解除)", time `HeaderStatus` "--:--:--"; order dot,title,ghost,stretch,time, spacing 6 (hud_window.py:178-196,377-385)|hud_window.rs:612-640,2216-2238|same|Rust `tool_tip_tests::card_and_header_ghost_tool_tips_follow_python`, `header_ghost_tool_tip_shows_on_hover`, `tests/test_label_header_row.rs`; Py `test_header_widgets_never_pop_up_as_windows_outside_table`|py title is "AI AGENT HUD" until table mode sets "(TABLE)"; never visible otherwise|
|`_sync_header`: header widgets visible only in table and not compact; ghost visible only if also click-through (hud_window.py:417-424)|hud_window.rs:128-139|same|Rust `test_switching_ui_mode_restyles_labels_from_the_new_sheet` (partial); Py `test_header_widgets_never_pop_up_as_windows_outside_table`|equal conditions|
|`_saved_table_size`: full default 450x350, undersized -> default; compact uses `compact_min_size`, `DEF_COMPACT_W/H` (hud_window.py:395-406)|hud_window.rs:58-91 (`saved_table_size`)|same|Rust `table_size_tests::saved_sizes_fall_back_to_the_defaults_when_undersized`, `stored_sizes_never_go_below_the_minimum_of_their_view`, `full_and_collapsed_sizes_are_stored_under_their_own_keys`; Py `test_saved_size_is_restored_and_undersized_values_reset`, `test_compact_and_full_sizes_are_remembered_separately`|py also requires `isinstance(int)`; Rust config types are u32 so non-int impossible (intentional)|
|`_set_table_compact`: set minimum to `compact_min_size`, margins, spacing, header sync, store `table_compact` (hud_window.py:408-415)|hud_window.rs:168-174,58-62,2121-2123|same|Rust `the_table_window_may_shrink_to_the_collapsed_minimum`; Py `test_table_can_be_dragged_down_to_the_compact_minimum`|min set in apply_ui_mode_internal (1127) rather than inside set_compact; end state equal|
|`_sync_table_state` on resize: `table_state(w,h,is_compact)`; skipped while restoring or not table (hud_window.py:426-432,963-969)|hud_window.rs:176-185, resize handler 808-830|same|Py `test_table_collapses_to_dials_when_shrunk_and_expands_when_enlarged`; Rust (table_layout.rs tests, other slice)|equal|
|Initial position: saved x,y if >=50x30 visible on some screen else default `max(x+10, x+w-width-40)`, `max(y+10,y+50)`; fallback (100,100) (hud_window.py:701-722)|placement.rs:62-82 `restore_or_default_position`, hud_window.rs:545-557|same|Rust `placement::tests::restore_or_default_position_matches_python` (oracle generated from Python); Py none (oracle script)|oracle-based|
|`_ensure_within_screen` (centre screen, clamp right/bottom/left/top using inclusive `right()`/`bottom()`) (hud_window.py:674-699)|placement.rs:87-125, hud_window.rs:1150-1161|same|Rust `placement::tests::ensure_within_screen_matches_python`, `qrect_right_bottom_and_center_are_inclusive`; Py none|Rust approximates `self.screen()` via `Screens::window_screen` (documented, ferrokit lacks it)|
|`_reset_geometry` table: expand if compact, size 450x350, move to primary `(x+w-450-40, y+50)` unclamped, persist (hud_window.py:1104-1120)|hud_window.rs:1419-1458, placement.rs:128-130 `reset_position`|same|Rust `placement::tests::reset_position_matches_python`; Py `test_reset_geometry_returns_the_table_to_its_full_default_size`|equal|
|`_reset_geometry` sidebar: edge right, orientation vertical, offset/x/y None, scale 1.0, then dock (hud_window.py:1105-1112)|hud_window.rs:1420-1437|same|none (Py `test_sidebar_edge_and_corner_drag_scales_proportionally` indirect)|equal keys|
|`_persist_geometry`: only in table mode; writes window_x/y, table_compact, and compact-or-full width/height clamped to minimum (hud_window.py:1000-1015)|hud_window.rs:93-115,194-203,1065-1070|same|Rust `geometry_persist_tests::mouse_release_persists_at_once_and_cancels_the_pending_save`, `close_persists_the_current_geometry`, `a_size_below_the_minimum_is_stored_clamped`; Py `test_sidebar_does_not_overwrite_table_geometry_and_can_switch_back`, `test_compact_table_is_restored_after_restart`|Rust updates config immediately on move and again on persist; py only on timer/release/close (equal end state)|
|`mousePressEvent`: edge -> `startSystemResize` (blur suspended first); else not locked -> `startSystemMove` (blur suspended first); locked -> falls through (hud_window.py:941-957)|hud_window.rs:748-775|same|none (needs desktop)|`post_system_resize/move` instead of Qt `startSystem*`|
|`mouseMoveEvent`: cursor shape per edge (Hor/Ver/FDiag/BDiag) (hud_window.py:922-939)|hud_window.rs:707-745|same|none|top-left & bottom-right -> FDiag in both; others BDiag|
|`mouseReleaseEvent`/`closeEvent` -> `_persist_geometry` (hud_window.py:959-961,979-981)|hud_window.rs:874-876 (`MouseButtonRelease`/`Close`)|same|Rust `geometry_persist_tests::mouse_release_persists_at_once_and_cancels_the_pending_save`, `close_persists_the_current_geometry`, `test_hud_window_close_flushes_pending_resize`; Py none|equal|
|`mouseDoubleClickEvent` (left) -> `trigger_async_refresh` (hud_window.py:991-994)|none|missing|none|P2, see top|
|`showEvent` -> `singleShot(0, _apply_theme)` (installs blur only on a shown window) (hud_window.py:983-985)|hud_window.rs:944-954 `show` -> `refresh_backdrop`|same|Rust `table_panel_is_translucent_over_an_installed_blur`, `table_panel_is_solid_when_the_blur_cannot_be_installed`; Py `test_blur_is_on_by_default_and_can_be_switched_off_and_on`|Rust applies synchronously inside `show()`; py deferred by one loop turn|
|`toggle_visibility`: hide if visible else show + `activateWindow` (hud_window.py:1122-1127)|hud_window.rs:1072-1078|diff|none|no activate (P2)|
|`close_application`: persist, stop countdown + refresh controller, close, `app.quit()` (hud_window.py:1129-1139)|main.rs:428-433,510-516 + `Drop for HUDWindow` (2271-2282)|intentional-diff|Rust `test_hud_window_drop_triggers_shutdown_and_worker_join`; Py none|Rust: persist, hide, `Application::exit(0)`; debouncer/worker joined on Drop|

### C. Click-through

|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|Win32: `WS_EX_TRANSPARENT` + `WS_EX_LAYERED` set on enable, only `WS_EX_TRANSPARENT` cleared on disable, then `SetWindowPos(NOMOVE, NOSIZE, NOZORDER, FRAMECHANGED)` (hud_window.py:836-850)|ferrokit window.rs:2221-2243 `set_click_through` via hud_window.rs:1080-1087|intentional-diff|Rust none in repo (ferrokit has own); Py none|Rust sets only `WS_EX_TRANSPARENT` (window is always LAYERED, so equivalent); no explicit SetWindowPos FRAMECHANGED - needs desktop check|
|macOS / other branches of `_apply_native_click_through` (hud_window.py:851-878)|none (Windows-only toolkit path)|intentional-diff|none|Rust HUD targets Windows (WINDOWS_HUD_ACCEPTANCE.md)|
|`set_click_through(enable, notify)`: set config, `_sync_header`, apply, balloon "👻 滑鼠穿透模式已啟用" / text, 4000 ms only when enabling + notify, `update_menu_state` (hud_window.py:880-894)|hud_window.rs:1080-1091; tray_icon.rs:507-517 `run_click_through_toggle`|same|Rust `tray_click_through_action_balloons_only_when_enabling`; Py none|title/text strings and 4000 ms equal (tray_icon.rs:511-514); Rust balloon icon `Information` vs py app icon (P2)|
|`toggle_click_through` (hud_window.py:896-899)|hud_window.rs:1089-1091, tray_icon.rs:565-571|same|Rust above|hotkey and tray share one path in Rust|
|Startup click-through: `singleShot(300, _safe_init_click_through)`; if hotkey manager says click-through hotkey not registered -> config False + balloon "⚠️ 穿透模式已暫停" 5000 ms, else `set_click_through(True, notify=False)` (hud_window.py:142-144,724-735)|main.rs:558-572; hotkey.rs:78; hud_window.rs:580-582 (flag at creation)|diff|Rust `hotkey.rs` tests (other slice); Py none|no 300 ms delay; window created click-through before check (P2); message text in Rust is `hotkey::LOCKOUT_WARNING_MESSAGE`, not the py literal (text compare belongs to hotkey slice -> `unverified`)|
|`_toggle_always_on_top`: set config, flag, `show()`, re-apply click-through, `update_menu_state` (hud_window.py:1021-1029)|hud_window.rs:1093-1100|unverified|none|Needs desktop check that topmost toggle keeps click-through|

### D. Theme, blur, mask

|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|`theme()`: appearance light/dark, auto follows `colorScheme()` else palette lightness<128 else dark (hud_window.py:249-265)|ui/mod.rs `resolve_is_dark` (used hud_window.rs:545), `get_theme(dark)`|unverified|none|`resolve_is_dark` is outside my files; palette-lightness fallback not checked|
|`set_appearance` (valid only) -> config, `_apply_theme`, `_rebuild_table` (hud_window.py:233-238)|tray_icon.rs:640-668 sets cfg then `hud.set_theme(bool)`; hud_window.rs:1181-1208|same|Py `test_panel_appearance_persists_and_keeps_data`, `test_theme_switch_keeps_latest_data_and_persists`, `test_invalid_theme_values_are_ignored`; Rust `test_switching_ui_mode_restyles_labels_from_the_new_sheet`, `theme_change_whose_blur_fails_switches_to_the_solid_panel`|invalid values impossible via fixed action ids (intentional)|
|`set_color_scheme` (valid only): config; if sidebar `_apply_theme`; `_rebuild_table` (hud_window.py:225-231)|tray_icon.rs:669-684 -> `hud.set_theme(hud.is_dark)`|same|Py `test_panel_appearance_persists_and_keeps_data`; Rust none|Rust also re-installs the backdrop in table mode (harmless)|
|System colour-scheme change: only when appearance == auto, deferred `singleShot(0)` re-theme + rebuild (hud_window.py:240-247)|hud_window.rs:1313-1324 `follow_system_theme`, main.rs:578-587|same|Py `test_panel_follow_system_only_in_auto_mode`; Rust none|Rust skips if dark unchanged|
|`_apply_theme` table: blur = `table_blur` (default True); no blur -> `vibrancy.clear`; `vibrant = blur and isVisible() and vibrancy.apply(...)`; stylesheet `get_hud_stylesheet(t, vibrant)` (hud_window.py:267-278)|hud_window.rs:353-366 `install_backdrop`, 956-970, 259-292 `panel_look`/painter|same|Rust `shown_table_hud`, `table_panel_is_translucent_over_an_installed_blur`, `table_panel_is_solid_when_the_blur_cannot_be_installed`; Py `test_blur_is_on_by_default_and_can_be_switched_off_and_on`|panel translucency via `theme.panel_bg_vibrant` vs `panel_bg` (styles values in other slice)|
|`_apply_theme` sidebar: blur off + `vibrancy.clear`, cards stylesheet, `sidebar.set_appearance(dark,'scale')`, each card `set_appearance(dark)` (hud_window.py:279-286)|hud_window.rs:1181-1208, 1832-1840|same|Py `test_sidebar_mode_docks_right_edge_and_switches_state`; Rust none|equal|
|`_refresh_mask`: mask only when ui_mode table and `mask_needed(blur_active)`; cache key `(w,h,radius 12)`; else `clearMask` (hud_window.py:288-300)|hud_window.rs:374-400 `refresh_mask`, 972-983|same|Py `test_rounded_mask_follows_the_native_blur_and_the_window_size`; Rust none for refresh_mask itself (only `round_mask::tests`)|`TABLE_RADIUS = 12` hard-coded vs `theme["radius"]` (both 12, styles.py:27,42)|
|`toggle_table_blur`: flip `table_blur`, `_apply_theme`, menu sync (hud_window.py:302-306)|hud_window.rs:1006-1012|same|Py `test_blur_is_on_by_default_and_can_be_switched_off_and_on`; Rust none|equal|
|`_suspend_blur`: only if blur active and table; suspend once, (re)start resume timer (hud_window.py:308-315)|hud_window.rs:985-996|same|Py `test_blur_pauses_while_the_window_is_moved_or_resized_and_comes_back`, `test_blur_is_never_suspended_without_native_blur_or_outside_the_table`; Rust none|equal|
|`_resume_blur`: re-apply Acrylic if suspended & active, clear flag (hud_window.py:317-320)|hud_window.rs:998-1004 -> `Window::resume_backdrop`|same|see timer row|py re-calls `apply` (Acrylic), Rust restores via `BackdropState::effective`; same result|
|Move/resize while suspended restart the resume timer (hud_window.py:971-977 move, 963-969 resize)|hud_window.rs:836-838 (resize handler), 853-855 (move)|same|Py same test as above; Rust none|equal|
|`_rebuild_table`: persist (table), new `UsageTable(theme, scheme, order)`, refill from `_latest_metrics`, re-apply mode if table, delete old (hud_window.py:322-335)|hud_window.rs:2088-2115|intentional-diff|Py `test_provider_order_applies_to_cards_table_and_sidebar`; Rust none|Rust refills from `raw_metrics` via `arranged()`, keeps collapse views via `adopt_views`, and rebuilds in every mode (comment 2074-2080) so a later switch is never stale|
|`set_provider_order`: valid permutation and changed; config; sidebar order; rebuild if table (hud_window.py:751-758)|hud_window.rs:2074-2086|same|Py `test_provider_order_applies_to_cards_table_and_sidebar`; Rust sidebar tests (other slice)|Rust always rebuilds table (see above)|
|Rounded mask geometry `rounded_rects(w,h,r)` (round_mask.py:9-39): `r=max(0,min(int r, w//2, h//2))`, per-row inset `ceil(r-dx-0.5)` clamped 0..r, band merge, top bands + middle + bottom bands|round_mask.rs:11-56|same|Rust `round_mask::tests::corners_are_cut_and_edges_and_centre_are_kept`, `shape_is_symmetric`, + 3 more (`rects_stay_inside...`, `area_matches...`); Py `test_round_mask.py::corners_are_cut...`, `shape_is_symmetric`, `rects_stay_inside_and_do_not_overlap`, `area_matches_a_rounded_rectangle`|line-for-line equal; Rust `radius.min(w/2).min(h/2).max(0)` == py `max(0, min(...))`|
|`vibrancy.mask_needed(blur, platform)`: blur and platform == win32 (vibrancy.py:119-121)|hud_window.rs:374-376 `blur_applied && cfg!(windows)`|same|Py `test_vibrancy.py::test_only_windows_with_native_blur_needs_the_rounded_mask`; Rust none|py takes platform arg for tests; Rust compile-time (no unit test for non-win platforms)|
|Windows Acrylic accent: `AccentState 4`, `AccentFlags 2`, gradient `0x99161a22` dark / `0x99f0f2f8` light (AABBGGRR), `WCA 19` (vibrancy.py:75-116)|ferrokit backdrop.rs:24-29,179-181,226-250|same|ferrokit `accent_tint_color` test (backdrop.rs:389-390); Py none|constants identical|
|`suspend` = accent state 2 with same tint; no-op off Windows (vibrancy.py:124-128)|ferrokit backdrop.rs:185-187 `FlatTint`, window.rs:2746-2761|same|ferrokit `test_suspend_and_resume_backdrop_keep_backdrop_active`; Py `test_suspend_is_a_noop_where_there_is_no_acrylic`|Rust returns whether state changed (py returns accent call result)|
|`clear`: `ACCENT_POLICY(0,0,0,0)` (vibrancy.py:131-161)|ferrokit backdrop.rs:131-155 `BackdropType::None`|intentional-diff|none|Rust also sets `DWMWA_USE_IMMERSIVE_DARK_MODE`, `DWMSBT_NONE`, and disables DWM blur-behind (superset)|
|`apply`: only if supported and widget visible; offscreen platform unsupported (vibrancy.py:19-35,164-176)|hud_window.rs:353-366 (`window.is_visible() && set(...)`)|same|Rust `table_panel_is_solid_when_the_blur_cannot_be_installed`; Py `test_blur_is_on_by_default_...`|no offscreen special-casing in Rust (not needed)|
|macOS `NSVisualEffectView` blur (vibrancy.py:12-72)|ferrokit `set_cocoa_window_backdrop` (not used by HUD for Mac)|unverified|none|Not checked; Rust HUD is Windows-only per acceptance doc|
|Rust-only: `DWM` dark-mode attr set on every `set_backdrop`|ferrokit backdrop.rs:105-110|intentional-diff|none|title-bar attr irrelevant for frameless window|

### E. Sidebar mode

|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|`_apply_ui_mode`: unknown mode -> sidebar; persist geometry then store mode; close flyout; reset min/max size; sidebar vs table branch (hud_window.py:337-393)|hud_window.rs:1102-1179|same|Py `test_hud_dual_mode_toggle`, `test_classic_cards_style_is_gone`, `test_sidebar_does_not_overwrite_table_geometry_and_can_switch_back`; Rust `test_switching_ui_mode_restyles_labels_from_the_new_sheet`|Rust also cancels any running edge-resize on mode change (intentional)|
|`_apply_sidebar_mode`: margins 0, fade timer 1500, scale/orientation/edge/state (invalid state -> strip), theme, dock (hud_window.py:434-446)|hud_window.rs:1815-1842|same|Py `test_sidebar_mode_docks_right_edge_and_switches_state`; Rust none|equal|
|`_sidebar_scale` clamp 0.6-2.0, round 2, invalid -> 1.0 (hud_window.py:448-453)|config.rs:386-391 `sidebar_scale_setting`|same|Rust config.rs test at 1120-1125 (`at(1.234)==1.23`, 0.1->0.6, 9->2.0, NaN->1.0); Py `test_sidebar_scale_steps_and_persists`|equal|
|`_sidebar_edge` (in EDGES else None), `_sidebar_orientation = orientation_for_edge(edge, saved)` (hud_window.py:455-460)|hud_window.rs:1845-1857|same|Py `test_sidebar_drag_snaps_to_any_edge_and_switches_orientation`; Rust dock_geometry tests (other slice)|Rust coerces any non-"horizontal" to vertical|
|`_sidebar_work_area`: screen holding window centre (hud_window.py:462-468)|hud_window.rs:1859-1870|same|none|uses `qt_center` (inclusive) in both|
|`_dock_sidebar`: preferred size; edge -> `dock_to_edge` with int/"end" offset; free -> saved x,y or bottom-right then `snap_rect(..., -10**6)`; set fixed size + geometry under `_docking` (hud_window.py:470-491)|hud_window.rs:1872-1903|same|Py `test_sidebar_mode_docks_right_edge_and_switches_state`; Rust none|Rust passes `DEFAULT_MARGIN` explicitly|
|`_finish_sidebar_move`: abort if not sidebar or docking; reschedule while mouse buttons held; `snap_rect_corner` with `SIDEBAR_SNAP_PX`, prefer current orientation; save edge/orientation/offset or x,y; re-dock (hud_window.py:493-516)|hud_window.rs:1905-1951|same|Py `test_sidebar_drag_snaps_to_any_edge_and_switches_orientation`, `test_sidebar_sticks_to_the_corner_without_turning_sideways`; Rust none|`edge or "none"` stored identically|
|`_start_sidebar_drag`: ignored when locked, else `startSystemMove` (hud_window.py:518-523)|hud_window.rs:1953-1958|same|none|equal|
|`_resize_zone`: pos within `adjusted(-2,-2,2,2)`; zone letters when `<7px` from edges (inclusive `right()`/`bottom()`) (hud_window.py:525-539)|hud_window.rs:1502-1522|same|Rust `sidebar_geometry_tests::grips_are_seven_pixels_wide_and_corners_combine`, `the_grip_reaches_two_pixels_outside_the_window_only`; Py `test_sidebar_ignores_edge_resize` (indirect)|equal|
|`_set_grip_cursor`: l/r Hor, t/b Ver, lt/rb FDiag, else BDiag; override cursor set/change/restore (hud_window.py:541-559)|hud_window.rs:1524-1531,1669-1689|same|Rust `cursors_follow_the_zone`; Py none|equal|
|`_begin_resize`: close flyout, stop snap timer, store start/geom/scale/zone, `grabMouse` (hud_window.py:561-565)|hud_window.rs:1961-1966, state set in filter 1783-1795|same|none|equal|
|`_resize_to`: ratios per edge, pick furthest from 1, scale = clamp(round(start*ratio,2),0.6,2.0), skip if unchanged, new size, origin keeps opposite edge (hud_window.py:567-594)|hud_window.rs:1540-1587 (`resize_scale`, `resize_origin`), 1968-1991|same|Rust `dragging_an_edge_scales_by_the_ratio_of_the_size`, `a_corner_uses_the_edge_that_moved_the_furthest`, `the_opposite_edge_or_centre_stays_put`; Py `test_sidebar_edge_and_corner_drag_scales_proportionally`|equal; py tie -> first max, Rust fold keeps earlier on tie (same)|
|`_end_resize`: release, clear state, cursor off, save scale (2 dp), `_finish_sidebar_move` (hud_window.py:596-601)|hud_window.rs:2001-2009|same|none|equal|
|Rust-only: resize drag cancelled on focus-out/leave, ended when a move arrives with no button held|hud_window.rs:1650-1652,1738-1755|intentional-diff|Rust `a_drag_without_a_held_button_lost_its_release`|robustness for lost mouse release in toolkit (Qt grabMouse guarantees release)|
|`eventFilter` (app-wide): only in sidebar; locked disables grips; Leave on window clears cursor; Move/Press inside the HUD tree sets cursor / begins resize on left press in zone (hud_window.py:603-628)|hud_window.rs:1691-1805 (`HudFilter`)|same|Rust `only_input_events_reach_the_filter_logic`, `the_filter_only_accepts_widgets_of_the_hud_tree`; Py `test_sidebar_ignores_edge_resize`|equal logic; Rust reads cursor with `Cursor::pos()`|
|`set_sidebar_scale`: clamp/round, store, if sidebar apply + dock (hud_window.py:630-638)|hud_window.rs:2011-2024|same|Py `test_sidebar_scale_steps_and_persists`; Rust none|equal|
|`_on_sidebar_scale_step` uses `step_scale` (hud_window.py:640-641)|hud_window.rs:2026-2030|same|dock_geometry tests (other slice)|equal|
|`set_sidebar_state`: valid and sidebar only; store; apply; dock (hud_window.py:643-648)|hud_window.rs:2032-2045|same|Py `test_sidebar_mode_docks_right_edge_and_switches_state`; Rust none|equal|
|`_fit_sidebar` / `_settle_sidebar` (dock after publish, redock once after loop if size differs) (hud_window.py:650-665)|hud_window.rs:2047-2072|same|Py `test_panel_fits_agy_five_bars_as_soon_as_the_data_arrives`, `test_panel_shrinks_back_when_the_agy_extra_bars_disappear`; Rust `sidebar.rs` tests (other slice)|equal; `singleShot(0)` both|
|Sidebar signals: stateChange, contextMenu, scaleStep, drag, orderChanged, card rowOrderChanged, app event filter (hud_window.py:111-130)|hud_window.rs:2171-2213|same|Py `test_dragging_a_row_title_in_the_agy_card_reorders_and_republishes`; Rust none in this file|context menu signal wired in main.rs:494-503 (`_show_sidebar_menu` equivalent)|
|`_show_sidebar_menu`: `build_hud_menu(self, self).exec(pos)` (hud_window.py:667-668)|main.rs:494-503|same|none|body menu (tray=false)|
|`_idle_fade_active`: sidebar, fade on, not click-through, not under mouse, no resize, no open detail page (hud_window.py:1039-1042)|hud_window.rs:2118-2127|same|Py `test_sidebar_dims_when_idle_and_recovers_on_hover`, `test_table_mode_never_dims`; Rust `the_idle_fade_multiplies_the_chosen_opacity`|equal|
|`_apply_opacity`: base * level when idle-fade active else base (hud_window.py:1044-1047)|hud_window.rs:2130-2136, 1589-1595|same|Rust `the_idle_fade_multiplies_the_chosen_opacity`; Py above|equal|
|`toggle_idle_fade`, `set_idle_fade_level(clamp 20-95 /100)`, `set_idle_fade_delay(clamp 200-30000)` (hud_window.py:1062-1071)|hud_window.rs:2148-2169|same|Py `test_idle_fade_level_and_delay_are_customisable`; Rust none|equal (delay setter does not touch opacity in either)|

### F. Data, provider handling

|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|`_on_data_fetched`: store raw, arrange, publish (hud_window.py:793-795)|hud_window.rs:1210-1217|same|Py `test_agy_main_rows_follow_the_setting_and_republish`; Rust none|equal|
|`_arranged`: agy -> `arrange_agy(agy_order)`; other providers `arrange_by_order` only if saved order and not table (hud_window.py:797-804)|hud_window.rs:1219-1231|same|Py `test_any_provider_rows_can_be_reordered_except_in_table_mode`; Rust none|equal|
|`_publish`: card, sidebar, fit, table, error flag, time label "%H:%M:%S" (hud_window.py:806-814)|hud_window.rs:1233-1248|same|none|py error flag per provider; Rust derives "any error" from sidebar cards each time (same result)|
|`_on_busy_changed`: dot `#38bdf8` busy, `#f59e0b` any error, `#10b981` ok; trim 1 s when idle (hud_window.py:816-820)|hud_window.rs:217-229,1327-1345|same|Rust `last_provider_finishing_trims_a_second_later`; Py none|Rust also recolours on each publish (py only on busy change); end state identical once the fetch ends|
|`agy_order`, `set_agy_order` (normalise, skip if equal, store, republish agy), `set_row_order` (agy delegates; others stored by title), `_republish_all`, `reset_agy_order` (hud_window.py:760-791)|hud_window.rs:1259-1310|same|Py `test_agy_main_rows_follow_the_setting_and_republish`, `test_dragging_a_row_title_in_the_agy_card_reorders_and_republishes`; Rust none|equal|
|`provider_order` validity (sorted == sorted(DEFAULT_ORDER)) (hud_window.py:745-749)|config.rs:393-402|same|Py `test_provider_order_applies_to_cards_table_and_sidebar`; Rust none|equal|
|`_set_opacity` (store, `_apply_opacity`) (hud_window.py:1035-1037)|hud_window.rs:1392-1403|same|Py none; Rust none|Rust clamps 0.1-1.0 (P2)|
|`_set_interval` (store, `refresh_controller.set_interval`) (hud_window.py:1082-1084)|hud_window.rs:1405-1410|same|none|equal|
|`_set_claude_profile`: store, set provider `profile_preference`, refresh (hud_window.py:1086-1092)|hud_window.rs:1412-1417|unverified|none|Rust provider is constructed with `with_config(Some(config))` (hud_window.rs:496-517) so presumably reads the stored id; not confirmed in `providers/claude.rs`|
|`trigger_async_refresh` (hud_window.py:742-743)|hud_window.rs:1388-1391|same|none|equal|
|`_toggle_autostart`: toggle, success -> config, failure -> balloon "開機啟動"/"設定失敗，請檢查系統權限" (hud_window.py:1094-1102)|tray_icon.rs:520-536,573-584|same|Rust `tray_autostart_action_balloons_and_keeps_config_when_setting_fails`, `tray_autostart_action_persists_new_value_without_balloon_when_setting_succeeds`; Py none|strings equal; Rust balloon 10000 ms (Qt default `showMessage` is 10000)|

### G. Menu (`hud_menu.py` vs `tray_icon.rs::build_hud_menu`)

|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|Top level: [tray: 顯示 / 隱藏 HUD (Alt+C)], 立即重新整理所有 AI (Refresh All), sep, 介面, 外觀, 行為, [!tray: 更多], sep, 結束程式 (Exit) (hud_menu.py:141-157)|tray_icon.rs:211-244|same|Rust `menu_parity_tests::body_menu_has_the_python_top_level`, `tray_menu_adds_show_hide_and_drops_more`; Py `test_context_menu_lists_only_table_and_sidebar_styles`|labels and order equal|
|介面 (Interface): two styles (儀表表格 (Modern Table), 側邊欄 (Sidebar)); sidebar only: sep, 細條/固定窄欄, sep, 大小 (Ctrl＋滾輪) with 6 scales; always: sep + AGY submenu (hud_menu.py:36-49)|tray_icon.rs:196-209,287-315|same|Rust `interface_group_offers_two_styles_and_the_sidebar_entries_only_in_the_sidebar_style`; Py `test_context_menu_lists_only_table_and_sidebar_styles`|checked state: `sidebar_state` default "panel" in py, Rust `cfg.sidebar_state` (default in config.rs, not read here)|
|AGY 項目順序 (Row order): disabled hint, sep, numbered disabled titles, sep, 還原預設順序 enabled only when order != default (hud_menu.py:52-64)|tray_icon.rs:317-341|same|Rust `sidebar_entries_list_the_size_the_row_order_and_the_idle_fade`; Py `test_menu_offers_agy_rows_inside_interface`|equal|
|外觀 (Appearance): 3 appearance choices (跟隨系統/淺色/深色), table only: sep, schemes (色階/雙色), 毛玻璃模糊 (Blur) check; sep; 視窗透明度 submenu 100/90/80/70/50/30% (hud_menu.py:67-84)|tray_icon.rs:343-371|same|Rust `appearance_group_shows_schemes_and_blur_only_in_the_table_style`; Py `test_blur_switch_is_offered_for_the_table_only`|equal; Py blur check default True via `config.get`, Rust `cfg.table_blur`|
|閒置時淡化: 啟用 check, sep, 淡化到 (Dim to) with `sorted(set(levels) ∪ {current}, reverse)`, 多久後淡化 (Delay) `"{ms/1000:g} 秒"` (hud_menu.py:87-100)|tray_icon.rs:373-411|same|Rust `sidebar_entries_list_the_size_the_row_order_and_the_idle_fade`; Py `test_idle_fade_submenu_lists_levels_and_delays`|`%g` implemented by `sidebar::format_g` (not re-verified here)|
|行為: 滑鼠點擊穿透 (Alt+Shift+C), 視窗永遠置頂 (Always on Top), 鎖定視窗位置 (Lock Drag), 閒置時淡化 submenu, 開機自動啟動 (Start on Boot), sep, ⏱️ 更新頻率 (Interval) 30/60/120/300 秒 (hud_menu.py:103-114)|tray_icon.rs:413-458|same|Rust `behaviour_and_more_groups_list_the_python_items`, `checked_state_follows_config`; Py `test_menu_labels_are_plain_text`|equal; autostart check from `is_autostart_enabled()` on both|
|Claude 帳號: auto entry titled "智慧自動追蹤 (目前: short_name)" when auto and active != default else "(最近活躍)"; sep; up to 40 profiles; selection rule incl. `.claude` == default (hud_menu.py:117-129)|tray_icon.rs:460-489|diff|Rust `behaviour_and_more_groups_list_the_python_items` (partial); Py none|Rust auto test is case-insensitive/empty-tolerant and the action id is the profile index not id (P2, see top)|
|更多: Claude 帳號, sep, 開啟記錄檔目錄 (Open Logs), 重設預設尺寸與位置, 隱藏 HUD (Alt+C 重新喚出) (hud_menu.py:132-138)|tray_icon.rs:491-503|same|Rust `behaviour_and_more_groups_list_the_python_items`; Py none|equal strings|
|Menu rebuilt every time it opens (`aboutToShow`), `update_menu_state` no-op (tray_icon.py:55-66; hud_menu docstring)|tray_icon.rs:558-563; main.rs:411-438 builds a fresh menu per popup|same|Rust `checked_state_follows_config`; Py none|equal|
|Menu actions dispatch: styles -> `_apply_ui_mode`; forms -> `set_sidebar_state`; scale/idle/appearance/scheme/blur/click-through/AOT/lock/opacity/interval/autostart/logs/reset/hide/profile/exit (hud_menu.py callbacks)|tray_icon.rs:586-757|same|Rust `items_dispatch_their_handler_ids`; Py none|Exit: Rust returns `true` and main.rs persists + hides + exits (py `close_application`, see row B)|
|Lock toggle (`_toggle_lock` just flips config) (hud_window.py:1031-1033)|tray_icon.rs:700-705|same|none|equal|
|HUD right-click: `contextMenuEvent` -> `build_hud_menu(self,self).exec(globalPos)` (hud_window.py:1018-1019)|main.rs:485-492 `set_context_menu_handler`|same|none|exec_popup returns chosen action, dispatched via `handle_action`|
|QMenu style, cards sheet: bg #161920 / border rgba(255,255,255,.18) / radius 6 / padding 4 0 / item `6px 24px 6px 20px`, 11 px, text #e2e8f0, selected bg #272f3d + #38bdf8, separator rgba(255,255,255,.12) margin 4 8, light variant mapped (styles.py:157-203)|tray_icon.rs:105-188 `hud_menu_style` (non-table branch)|same|Rust `test_menu_text_does_not_depend_on_window_offset`, `sidebar_disabled_menu_rows_keep_the_normal_text_colour`; Py none|dark and light RGBA decoded: 22,25,32 / 46 / 226,232,240 / 39,47,61 / 56,189,248; light 248,250,252 / 23,32,51 / 226,232,240 / 3,105,161; separator 31; all match|
|QMenu style, table sheet: bg `menu_bg` (.97), border `separator`, radius 8, padding 5, item `5px 26px 5px 22px`, 12 px, selected `menu_hover`, disabled `text3`, separator margin 5 10 (styles.py:229-238, theme dicts 28-52)|tray_icon.rs:119-152|same|none|dark 40,40,46,247 / 31 / 242,242,247 / 26 / 82; light 250,250,252,247 / 36 / 31,31,36 / 18 / 87 (all equal to alpha*255); Qt-vs-toolkit pixel layout `unverified` on desktop|
|Dark/light for popup menu follows `appearance` setting|tray_icon.rs:107-108,214 `resolve_is_dark(&cfg.appearance)`|same|Rust `tray_and_body_menus_share_dark_mode`; Py none|equal|

### H. Tray icon (`tray_icon.py` vs `tray_icon.rs`)

|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|Tooltip "AI HUD Monitor (3-in-1)\n• Alt+C: 顯隱\n• Alt+Shift+C: 穿透模式" (tray_icon.py:51)|tray_icon.rs:545-552|same|none|strings byte-equal|
|Left click (`Trigger`) toggles visibility (tray_icon.py:68-70)|main.rs:397-407|same|none|equal; Rust no `activateWindow` (P2)|
|Right click shows `build_hud_menu(tray=True)` (tray_icon.py:55-63)|main.rs:446-458 -> `show_hud_popup_menu(..., true)`|same|Rust `tray_menu_adds_show_hide_and_drops_more`|equal|
|`get_app_icon`: frozen `_MEIPASS`/assets, win32 `.ico`, mac `.icns`, else `.png`; painted fallback (rounded rect, "C") (tray_icon.py:11-42)|tray_icon.rs:89-101 (embedded PNG, fallback cyan square)|diff|none|P2, see top|
|`_toggle_autostart` in tray class is dead code in py (menu uses `hud._toggle_autostart`)|tray_icon.rs:573-584 (live)|intentional-diff|Rust autostart tests (see F)|Rust tray owns the toggle because it hosts the action handler|
|`set_tray_icon` back-reference and `showMessage(title, msg, icon, ms)` (tray_icon.py:49; hud_window.py:884-889)|tray_icon.rs:539-584; main.rs wires `tray` + `hud` through thread-locals|intentional-diff|none|no reference cycle in Rust; same behaviour|

## Counts (rows in the matrix section)

- same: 93
- intentional-diff: 10
- diff: 4
- missing: 1
- unverified: 4
- total rows: 112


---

# Widgets parity matrix (Python ui/{sidebar,dock_geometry,usage_table,provider_card,gauge,styles,table_layout,brand}.py vs Rust rust/src/ui/*)

Read-only analysis; nothing built or run. Sections below are the detailed matrices (core part = brand/table_layout/gauge/styles/dock_geometry; then provider_card, usage_table, sidebar).

## All diff / missing (severity, fix hint)

| Sev | Area | Item | Fix hint |
|---|---|---|---|
| P1 | usage_table | agy column badge: Python `_badge_text` joins both badges `"{b1} · {b2}"` (`剩餘:`->`:`); Rust (usage_table.rs:1197-1208) shows only badge1 else badge2. Rust agy provider fills both | port `_badge_text` (copy provider_card.rs:924 replace logic) |
| P2 | styles | Light cards QSS `QLabel#Badge` border is `rgba(15,23,42,0.08)` (styles.rs:310); Python maps 0.08->0.10 (styles.py:187 map) | change to 0.10 |
| P2 | dock_geometry | `clamp_scale` rounds `(x*10).round()/10` (dock_geometry.rs:255) vs Python `round(x,1)`; differs at exact .x5 ties; also non-number inputs | decimal-style round; unreachable via 0.1 UI steps |
| P2 | usage_table | legend ring / tick / caption halo pens use round cap/join (usage_table.rs:792-805,455); Python default square cap/bevel; tick dash not scaled by width 1.4 | use default-cap pens, scale dash |
| P2 | usage_table | separator span hard-coded 4 (usage_table.rs:1397,1448) vs `len(columns)+1` | `order.len()+1` |
| P2 | usage_table | `Some("")` error counts offline (rs:1130,1227); Python empty string = online | filter empty |
| P2 | usage_table | dial side `(min-2).max(10)` (rs:292) vs Python no floor | drop floor or accept |
| P2 | provider_card | title label has objectName `CardTitle` (provider_card.rs:631) absent in Python | confirm no stylesheet rule; drop |
| P2 | sidebar | `window_reset` threshold truncates (num_seconds) vs float total_seconds | use num_milliseconds |
| P2 | sidebar | `set_dock_edge` accepts unknown edge; Python raises | validate |
| P2 | sidebar | `hideEvent` -> close_flyout missing (HUD closes at known points) | add hide hook or accept |
| P2 | sidebar | `open_flyout` sets active before lookups succeed (sidebar.rs:2342-2345) | reorder |
| P2 | brand | Python test of >=4.5:1 contrast of names on tile bg not ported | add test |
| P2 | tests | No Rust widget-level tests for sidebar interactions, card drag events, table dial colours/tooltips | add tests |
(See each section's own list for the full set including unverified items; sidebar P2 minor items beyond the above are listed in the Sidebar section.)

## brand / table_layout / gauge / styles / dock_geometry rows

### brand (python/ui/brand.py, rust/src/ui/brand.rs)
|Python behaviour|Rust location|Status|Test coverage|Notes|
|---|---|---|---|---|
|BRAND claude #D97757/#B4532F, agy #8AB4F8/#1967D2, codex #10A37F/#0B7A5E (brand.py:9-11)|brand.rs:11-15|same|Rust brand.rs::every_service_has_a_colour_per_theme (claude only); Py tests/test_brand::test_every_service_has_a_colour_per_theme|agy/codex hex not asserted in Rust|
|FALLBACK #38bdf8/#0369a1 (brand.py:13)|brand.rs:16|same|brand.rs::every_service_has_a_colour_per_theme; Py test_brand::test_every_service_has_a_colour_per_theme||
|brand_color(pid,dark) -> hex str (brand.py:16)|brand_hex brand.rs:18-29, brand_color (Color) brand.rs:31-35|same|brand.rs::hex_is_parsed_into_a_colour; Py test_brand|Rust adds a Color wrapper; same lookup|
|Light names read >= 4.5:1 on tile bg|none|missing|none (Py test_brand::test_names_read_on_the_tile_background)|Contrast test not ported (colours identical, so low risk)|
|Colours distinct per theme|brand.rs:55-62|same|brand.rs::brand_colours_differ_from_each_other; Py test_brand::test_brand_colours_differ_from_each_other||

### table_layout (python/ui/table_layout.py, rust/src/ui/table_layout.rs)
|Python behaviour|Rust location|Status|Test coverage|Notes|
|---|---|---|---|---|
|FULL_MIN 380x280, HYSTERESIS 16 (table_layout.py:8-9)|table_layout.rs:7-9|same|table_layout.rs::full_size_threshold_is_the_old_minimum; Py test_table_layout::test_full_size_threshold_is_the_old_minimum||
|COMPACT_DIAL_MIN 48, GAP 8, MARGIN 6, FRAME 2, DEF_COMPACT 240x96 (:11-16)|table_layout.rs:12-22|same|table_layout.rs::default_compact_size_is_not_below_the_minimum; Py test_default_compact_size_is_not_below_the_minimum||
|table_state(w,h,was_compact) (:19-23)|table_layout.rs:25-30|same|table_layout.rs::collapses_below_either_dimension, ::compact_window_needs_the_hysteresis_margin_to_expand, ::no_flicker_while_resizing_across_the_threshold; Py test_table_layout same-named||
|compact_min_size(count=3) (:26-28)|table_layout.rs:33-39|same|table_layout.rs::compact_minimum_size_is_small_but_holds_three_dials (asserts 174x62); Py same||

### gauge (python/ui/gauge.py, rust/src/ui/gauge.rs)
|Python behaviour|Rust location|Status|Test coverage|Notes|
|---|---|---|---|---|
|STOPS (0 green,50 yellow,75 orange,90 red) (gauge.py:17)|gauge.rs:13|same|gauge.rs::stops_follow_the_status_bands; Py test_gauge::test_stops_follow_the_status_bands||
|SURFACES dark #0e1117/#1e1e24/#30303a, light #f8fafc/#f6f4fa/#e6e4ed (:19)|gauge.rs:27-41|same|none directly|Used via every_gauge_colour_reads_on_its_surfaces|
|_luminance sRGB WCAG, coefficients .2126/.7152/.0722 (:22-25)|gauge.rs:62-72|same|gauge.rs::every_gauge_colour_reads_on_its_surfaces; Py test_gauge::test_every_gauge_colour_reads_on_its_surfaces|Rust quantises to 8 bit via f32 channels; Qt uses float; sub-LSB difference only|
|contrast() (:28-30)|gauge.rs:75-79|same|same as above||
|readable(): step +-0.025, 40 iterations, min 3.0, fallback #f2f2f7 / #1f1f24 (:33-45)|gauge.rs:147-175|same|gauge.rs::readable_keeps_good_colours_and_only_moves_lightness; Py test_gauge::test_readable_keeps_good_colours_and_only_moves_lightness|HSL conversion is hand-written (to_hsl/from_hsl gauge.rs:82-143) instead of QColor; rounding equivalence with Qt unverified for every colour|
|HSL round trip == Qt getHslF/fromHslF (gauge.py:41-42)|gauge.rs:82-143|unverified|gauge.rs::hex_roundtrip (hex only)|Qt stores colours as 16-bit; Rust 8-bit. A candidate lightness could land one LSB away; needs a numeric dump of the 8 ramp stops from both sides|
|ramp_stops reads THEMES[..]['scale'] (:48-50)|gauge.rs:178-209 (scale_color duplicates hex table)|same|gauge.rs::stops_follow_the_status_bands|Hex table duplicated from styles.rs Theme; all 8 values equal styles.py:49,64|
|ramp_color linear blend, round() (:53-63)|gauge.rs:212-233 (round_ties_even)|same|gauge.rs::colour_warms_up_as_usage_rises; Py test_gauge::test_colour_warms_up_as_usage_rises|Python round = half-even, Rust round_ties_even: equal|
|GaugeBar default height 5, Expanding x Fixed, range 0..100 (:69-80)|gauge.rs:286-303|same|gauge.rs::api_matches_what_the_cards_use; Py test_gauge::test_api_matches_what_the_cards_use||
|setRange: max = max(min+1,max) (:85)|gauge.rs:305-309|same|same||
|setTextVisible no-op, set_dark only repaints on change (:97-104)|gauge.rs:321-328|same|same||
|fraction() clamp 0..1 (:107-109)|gauge.rs:343-350 (region)|same|gauge.rs::api_matches_what_the_cards_use|Read via test asserts only; body lines not fully read -> low risk|
|paintEvent: track rounded rect radius h/2 colour theme['track'] (:112-120)|gauge.rs:247-251,408-420|same|gauge.rs::paints_in_both_themes_and_when_empty; Py test_gauge::test_paints_in_both_themes_and_when_empty||
|Fill: skipped if fraction<=0 or muted; gradient left->right over whole track; width max(h, w*f) (:121-130)|gauge.rs:252-269|intentional-diff|gauge.rs::fill_is_a_revealed_gradient_not_one_flat_colour; Py test_gauge::test_fill_is_revealed_gradient_not_one_flat_colour|Rust draw_gauge_bar has min_fill_is_height flag: GaugeBar passes true (same as Py), sidebar tile bar passes false (sidebar.rs:1341). Python tile painting in sidebar.py must be checked in the sidebar matrix. Rust also clamps fraction to <=1|
|Cards' bars are GaugeBars and number colour == leading colour (test_gauge:74)|none|unverified|Py test_gauge::test_card_bars_are_gauges_and_numbers_match_the_leading_colour; Rust none|See provider card matrix|

### styles (python/ui/styles.py, rust/src/ui/styles.rs)
|Python behaviour|Rust location|Status|Test coverage|Notes|
|---|---|---|---|---|
|THEMES dark/light colour tokens: panel, panel_solid, border, text/text2/text3, neutral, separator, track, disc, halo, hatch (styles.py:24-60)|styles.rs:37-95|same|none|rgba floats converted to 0-255: .94->240, .55->140, .12->31, .62->158, .32->82, .34->87, .14->36; all match. Not tested in Rust|
|duo / duo_text / scale / scale_text hex (styles.py:34-37,49-52)|styles.rs:52-63,82-93|same|none||
|menu_bg / menu_hover tokens (styles.py:38,53)|styles.rs:397-398,409-410 (strings in SheetTheme)|same|none||
|radius 12 (:25)|styles.rs:392,404|same|none||
|get_progress_color 90/75/50 thresholds, 8 hex (styles.py:7-16)|styles.rs:107-109 delegating to provider_card.rs:23|same|none (Py also none)|Unused for cards on both sides|
|get_cards_stylesheet dark sheet (styles.py:70-176)|styles.rs:130-250|same|rust/tests/test_hud_cards_stylesheet.rs (only parse count 18, no content compare); Py none|All 18 rules compared line by line, equal|
|get_cards_stylesheet light: regex colour map (styles.py:179-195)|styles.rs:252-372|diff|test_hud_cards_stylesheet.rs (parse only)|QLabel#Badge border: Python maps rgba(255,255,255,0.08)->rgba(15,23,42,0.10), Rust has 0.08 (styles.rs:310). P2: change to 0.10. All other 17 mapped values equal; HeaderStatus/SubDetail #64748b unchanged in both|
|get_cards_stylesheet(vibrant) parameter (unused in Python body)|styles.rs:128 (no param)|intentional-diff|none|Param is dead in Python|
|get_hud_stylesheet(theme=None) -> cards sheet (styles.py:200-203)|hud_window.rs:232-236 (chooses by ui_mode)|intentional-diff|none|None-theme path replaced by explicit ui_mode switch|
|get_hud_stylesheet(theme, vibrant): panel vs panel_solid (:205-208)|styles.rs:417-475 (always panel_solid); vibrant handled in hud_window.rs:257-259 panel_look|intentional-diff|none|Verified by sibling slice (ParityWindow) if needed; not read here|
|HUD sheet QSS body (QLabel, HeaderTitle 11px/700 text2, HeaderStatus 10px, LayoutToggleBtn, hover, QMenu r8 pad 5px 0, item 5px 26px 5px 22px 12px, disabled, separator margin 5px 10px) (styles.py:209-233)|styles.rs:439-474|same|none||
|get_table_stylesheet (styles.py:236-250)|styles.rs:490-511|same|rust usage_table_relayout_tests.rs uses get_hud_stylesheet only; none direct|All 9 rules equal|
|SCHEMES ("scale","duo"), APPEARANCES tuples (styles.py:22-23)|none in styles.rs|unverified|none|May live in config.rs (other slice)|
|scale_colors / duo_colors helpers|styles.rs:111-127|missing (Py has no counterpart)|none|Rust-only; Python equivalent is inline in usage_table.py (see usage_table matrix)|

### dock_geometry (python/ui/dock_geometry.py, rust/src/ui/dock_geometry.rs)
|Python behaviour|Rust location|Status|Test coverage|Notes|
|---|---|---|---|---|
|constants EDGES, SCALE_MIN/MAX/STEP 0.6/2.0/0.1 (:6-7)|dock_geometry.rs:7-10|same|dock_geometry.rs::scale_helpers; Py test_dock_geometry::test_scale_helpers||
|snap threshold default 24, margin default 12 (:62,156)|dock_geometry.rs:11-12|same|dock_geometry.rs::snap_rect_cases||
|_clamp low wins (:10)|dock_geometry.rs:22-24|same|covered via all|
|dock_rect (:14-24)|dock_geometry.rs:31-37|same|dock_geometry.rs::dock_rect_standard_1920x1040, ::dock_rect_secondary_monitor_offset, ::dock_rect_clamps_oversized_dimensions_and_offset, ::dock_rect_dpi_scaling_scenarios; Py same-named||
|constrain_to_edge (:27-36)|dock_geometry.rs:40-46|same|dock_geometry.rs::constrain_to_edge_cases; Py test_constrain_to_edge||
|bottom_offset_from_rect (:39-43)|dock_geometry.rs:49-53|same|dock_geometry.rs::bottom_offset_from_rect_cases; Py test_bottom_offset_from_rect||
|pick_work_area; empty list raises ValueError (:46-59)|dock_geometry.rs:56-77 (Option, None when empty)|intentional-diff|dock_geometry.rs::pick_work_area_cases; Py test_pick_work_area|Rust Option instead of exception; containing test and nearest-centre logic equal|
|snap_rect (:62-93)|dock_geometry.rs:102-125|same|dock_geometry.rs::snap_rect_cases; Py test_snap_rect|Tie order left,right,top,bottom preserved|
|snap_rect_corner incl. prefer axis, near_end/near_start, offset "end"/int (:96-144)|dock_geometry.rs:130-197 (Offset::End/Start)|same|dock_geometry.rs::vertical_bar_in_bottom_right_corner_stays_vertical, ::horizontal_bar_..., ::without_preference_nearest_edge_wins, ::top_corners_and_plain_edge_offset, ::nothing_near_returns_none; Py same-named||
|orientation_for_edge (:147-153)|dock_geometry.rs:200-206|same|dock_geometry.rs::orientation_for_edge_cases; Py test_orientation_for_edge||
|dock_to_edge incl. offset None default (margin) / "end" / clamp; invalid edge ValueError (:156-175)|dock_geometry.rs:209-235 (Option for invalid edge)|same|dock_geometry.rs::dock_to_edge_cases, ::dock_to_edge_accepts_end; Py test_dock_to_edge, test_dock_to_edge_accepts_end|Error type differs (None vs ValueError), logic equal|
|offset_along_edge (:178-186)|dock_geometry.rs:238-247|same|dock_geometry.rs::offset_along_edge_round_trip; Py test_offset_along_edge_round_trip||
|clamp_scale: round(clamped,1), NaN/bool/non-number -> 1.0 (:189-194)|dock_geometry.rs:250-256 ((x*10).round()/10, NaN only)|diff|dock_geometry.rs::scale_helpers; Py test_scale_helpers|P2: rounding differs at exact .x5 ties (Py round(0.85,1)=0.8 via decimal repr, Rust 8.5.round()=9 -> 0.9). Unreachable via 0.1-step UI. Fix: use py-style decimal round (format {:.1} parse) if exact parity wanted|
|step_scale (:197-199)|dock_geometry.rs:259-261|same|dock_geometry.rs::scale_helpers||
|scale_size, banker's rounding, min 1 (:202-205)|dock_geometry.rs:264-280|same|dock_geometry.rs::py_round_is_bankers, ::scale_helpers; Py test_scale_helpers||
|test_no_qt_imported (Py :25)|n/a (Rust module has no GUI deps by construction)|intentional-diff|none|Python-only check|


---
# ProviderCard
# ProviderCard parity (python/ui/provider_card.py vs rust/src/ui/provider_card.rs)

Paths: PY = python/ui/provider_card.py, RS = rust/src/ui/provider_card.rs. Read in full: both files, python tests (test_ui.py CardTests 24-74, drag tests 876-947; test_gauge.py card test), rust inline tests + provider_card_relayout_tests.rs (setup/steps), rust/tests/test_hud_cards_stylesheet.rs.

## diff / missing list
No P0 or P1 discrepancies found by reading. Items:
- (P2, diff) Rust title label has objectName `CardTitle` (RS:600); Python title has none (PY:37). Fix: confirm cards stylesheet has no `#CardTitle` rule; otherwise drop name.
- (P2, diff) Python `get_progress_color` lives in python/ui/styles.py:7, Rust has it in provider_card.rs:23 (+ styles.rs:107 wrapper, dead_code). Values identical; location differs. Unused by the card in both (SPEC-sidebar-dock.md:105). Fix: none needed; optionally add a unit test.
- (P2, diff) Rust `bar_value` truncates float to i32 (RS:285); Python passes `min(100,max(0,val or 0))` possibly a float to `GaugeBar.setValue` (PY:292). Depends on GaugeBar impl; marked unverified below.
- (P2, missing test) Rust has no event-level drag test (Python test_ui.py:876 drives eventFilter). Only pure helpers (`drop_index`, `drop_line_y`, `reordered`) are tested. Fix: add a Rust test pressing/moving/releasing through `CardCore::press/drag_move/release`.
- (P2, missing test) Rust lacks tests for extra-row show/hide/reuse (py test_ui.py:25), tooltip (py:60,73), RowCell objectName/hover QSS (py:934).

## Matrix

|Python behaviour (file:line)|Rust location (file:line)|Status|Test coverage|Notes|
|---|---|---|---|---|
|PROVIDER_THEMES names: claude "CLAUDE CODE", agy "ANTIGRAVITY", codex "OPENAI CODEX"; unknown -> `provider_id.upper()`, color "#38bdf8" (PY:11-15,23)|RS:45-52 `default_provider_name`, RS:93-98 `theme_name`; brand fallback brand.rs:15 `("#38bdf8","#0369a1")`|same|none (brand.rs:44 tests fallback hex)|Python fallback dark "#38bdf8", light via brand.py:13 "#0369a1"; equal. `default_provider_name` returns "AI AGENT" for unknown but `theme_name` overrides with upper(); fine.|
|Theme color = brand_color(id, True) dark; light = brand_color(id, False) (PY:12-14,279-282)|RS:630 `brand_hex(provider_id,true)`, RS:735-743 set_appearance|same|rust card_logic_tests::card_bars_are_gauges... (set_appearance false); py test_gauge.py::test_card_bars_are_gauges_and_numbers_match_the_leading_colour|Brand tables compared only via fallback; per-provider hexes belong to brand file matrix.|
|Root layout margins (6,4,6,4), spacing 5 (PY:34-35)|RS:608-610|same|none (relayout geometry tests indirectly: provider_card_relayout_tests.rs::provider_card_relayout_matches_pyside6_at_dpr_1_25/_1_0)|Pixel rects checked against PySide6 probe.|
|Header HBox spacing 4; dot "●" QSS `color:<theme>; font-size:10px;` (PY:38-45)|RS:614-628|same|relayout tests|Rust also sets font Segoe UI 10 + label color; intentional (no QSS cascade in ferrokit).|
|Title QSS `color; font-size: 9.5px; font-weight: 800; letter-spacing: 0.4px;` (PY:47,281)|RS:122-124 `title_qss`, RS:636|same|rust card_logic_tests::font_sizes_scale_and_one_restores (string only)|Identical text.|
|Title sizePolicy Minimum/Preferred (PY:48)|RS:632-637|same|none||
|Title objectName none (PY:47)|RS:631 "CardTitle"|diff|none|P2, see list.|
|Badge QLabel "--" objectName "Badge"; badge2 "" hidden (PY:52-60)|RS:642-654|same|rust test_badge_font_weight_matches_python; test_badge_size_hint_matches_pyside6_at_both_ratios (windows only)|Font Consolas 9 set explicitly in Rust (mirrors app QSS); intentional.|
|Header stretch between title and badges (PY:51)|RS:640|same|relayout tests||
|Metric block: VBox spacing 1; header HBox with MetricTitle label + stretch + MetricValue (QSS `font-size: 14px;`) (PY:63-77,92-106)|RS:309-356 `build_metric_block`|same|relayout tests; card_logic_tests::card_bars...|Rust sets Consolas 14 bold / Segoe UI 10 bold explicitly (intentional, stylesheet emulation).|
|Default texts: m1 "SESSION 5H", m2 "WEEKLY 7D", vals "--", subs "重設於: --" (PY:65,70,84,98,103)|RS:656-659,335,346|same|rust test_stale..., rust::test_absent_timestamp_clears_previous_countdown|Initial state equal.|
|Bars: GaugeBar, range 0..100, value 0 (PY:79-81)|RS:294-299 `make_bar` (`GaugeBar::new(5)`)|unverified|rust card_logic_tests::card_bars_are_gauges_and_numbers_match_the_leading_colour; py test_gauge.py same name|Arg `5` (height?) vs Python `GaugeBar()` default not checked here; belongs to gauge matrix.|
|RowCell: objectName "RowCell", QSS `QWidget#RowCell {background: transparent; border-radius: 6px;} QWidget#RowCell:hover {background: rgba(127, 127, 127, 0.16);}`, margins (4,2,4,2) (PY:128-136)|RS:101-103 `ROW_CELL_QSS`, RS:271-278 `make_cell`|same|py test_ui.py::test_rows_are_hoverable_draggable_cells; rust none|QSS string byte-equal.|
|Cursor OpenHand on cell; WA_Hover; eventFilter installed on cell + every child (PY:138-142)|RS:282-284 `arm_cell` (cursor only); single filter on container RS:681-690|intentional-diff|none|ferrokit bubbles events to container filter. WA_Hover not needed (hover QSS handled by toolkit) -> hover visual unverified.|
|Extra container: VBox margins 0, spacing 1, appended to card layout on first need (PY:145-151)|RS:757-768|same|rust none; py test_ui.py::test_extra_rows_show_hide_reuse_and_countdown||
|Extra row: VBox spacing 0; header HBox margins 0 spacing 1; title MetricTitle, value MetricValue, bar 0..100, sub SubDetail; cell via _make_cell (PY:152-175)|RS:769-805|same|none (py test above)|Note Python extra value label has no initial `font-size:14px` QSS until update; Rust same (applied in update_extra_metrics).|
|`_reset_text`: no reset -> subtext or "重設於: --"; else `重設於: {stamp} · {countdown}` or `重設於: {countdown}` (PY:119-126)|RS:204-222 `reset_text`|same|rust card_logic_tests::reset_text_without_reset_uses_subtext_or_placeholder, reset_text_has_a_stamp_only_for_long_windows; py test_ui.py:38-39 (patched countdown), test_ui.py:949 weekly wall-clock|Truthiness of `reset` vs `is_none` equal for datetime.|
|`_update_extra_metrics`: create rows to len, container visible iff metrics, per-row visibility, title/text, color ramp or #64748b, bar.set_dark + clamp, sub color #64748b + reset_text (PY:177-194)|RS:807-833|same|rust card_logic_tests::card_bars_are_gauges... (value/color of extra row); py test_ui.py::test_extra_rows_show_hide_reuse_and_countdown|Show/hide/reuse not covered in Rust.|
|Value color: `ramp_color(val, dark)` if val not None else "#64748b"; style `color:..; font-size: 14px;` (PY:182,311,320)|RS:301-307 `value_hex`; RS:937-942,950-955|same|rust card_bars_are_gauges...; py test_gauge.py card test|ramp_color internals in gauge matrix.|
|Bar value `min(100,max(0,val or 0))` (PY:184,313,322)|RS:289-291 `bar_value` clamp + `as i32`|unverified|rust card_bars_are_gauges... (82,55 ints)|Float vals (e.g. 82.7) truncated in Rust; Python depends on PySide `setValue(float)` behaviour.|
|`_drag_rows`: [] when `(error and not stale)` or m1 label empty; else m1, m2, extras[:len(extra_metrics)] with titles/rects (PY:197-211)|RS:403-436 `drag_rows`; flag set RS:857-859|same|rust card_logic_tests::rows_are_not_reorderable_while_offline, card_bars_are_gauges... (titles); py test_ui.py:893-894, 944|Rect: Python full card width `QRect(0,top,width,h)`; Rust (top,bottom=top+h-1, plus cell left/right). Used only for hit-test/drop index; equivalent in effect.|
|`_cell_of`: walk parents to a cell (PY:213-217)|none (press hit-test by geometry RS:446-458)|intentional-diff|none (py test_ui.py:945)|Rust tests x/y inside cell rect instead of parent walk; no `_cell_of` API.|
|`_drop_index`: others = rows minus source; index = count(y > rect.center().y) (PY:219-222)|RS:224-233 `drop_index`|same|rust card_logic_tests::drop_index_counts_row_centres_above_the_pointer|Centre computed `(top+bottom)/2` with bottom=top+h-1 = Qt QRect.center.|
|`_show_drop_line`: 3px QFrame, transparent for mouse, QSS `background: <theme>; border-radius: 1px;`, y = first.top-2 if index==0 else prev.bottom+1, geometry (4,y,max(10,w-8),3), show+raise (PY:224-233)|RS:438-453 `show_drop_line`, RS:235-243 `drop_line_y`; widget RS:661-672|same|rust card_logic_tests::drop_line_sits_above_the_first_row_or_below_the_previous_one|Rust creates line eagerly (intentional, borrow constraints); guards empty `others` (Python would IndexError).|
|Drag threshold: manhattan > 6 on global pos -> active, ClosedHand cursor (PY:239-243)|RS:105-106 `DRAG_THRESHOLD=6`, RS:468-478|same|none|Rust uses card-local coords (card does not move).|
|Mouse press left: find cell row, start drag, `grabMouse`, consume (PY:266-274)|RS:459-470 `press`, RS:546-553 filter (button 1)|unverified|none|No `grabMouse` equivalent seen; needs desktop check that moves outside the cell still reach the filter.|
|Move while dragging updates target + drop line; event consumed (PY:236-250)|RS:472-492 `drag_move`|same|none (py test_ui.py:901-903)|Rust returns false (not consumed) when no drag in progress, as Python falls through.|
|Release: releaseMouse, OpenHand cursor, hide line, clear drag, emit `rowOrderChanged(titles)` only if active, target set and order changed (PY:251-265)|RS:496-523 `release`; `reordered` RS:245-253|same|rust card_logic_tests::reordered_moves_the_dragged_title; py test_ui.py::test_dragging_a_row_title_in_the_agy_card_reorders_and_republishes|Python emits all row titles; Rust same.|
|Signal emission timing: synchronous `emit` (PY:264)|RS:513-515 `WidgetCommandQueue::post_task`|intentional-diff|none|Deferred because card is mutably borrowed in filter. Consumer: hud_window.rs:2190 -> `set_row_order` via `with_hud_later`.|
|`rowOrderChanged` signal wired in HUD (hud_window.py:122)|RS:720-722; hud_window.rs:2190-2194|same|py test_ui.py:876|Wiring present.|
|`set_appearance(dark)`: theme recompute, title QSS, `update_metrics(current)` (PY:277-284)|RS:735-748|same|rust card_bars_are_gauges... (appearance false); py test_gauge.py card test||
|`update_metrics`: store data; extras `[]` when error and not stale (PY:287-288)|RS:853-860|same|py test_ui.py:47-48; rust none|Rust `error` truthiness via `error_text` (empty = no error), same as Python.|
|Title text = provider_name or theme name (PY:289-292)|RS:861-865|same|none||
|Tooltip = `data.error or ""` (PY:294)|RS:869-871 `set_tool_tip`|same|py test_ui.py:60,73; rust none||
|Offline branch: dot red (#ef4444 dark / #b91c1c light) 10px; m1_val "ERR" 13px red; bar 0; m1_sub first line of error, red; m2 "--", bar 0, sub ""; badge "OFFLINE" visible; badge2 hidden; return (PY:295-311)|RS:872-905; `error_hex` RS:108-114|same|rust test_error_recovery_clears_text; py test_ui.py::test_error_recovery_clears_text|Rust `set_bar(...,None)` skips set_dark exactly as Python.|
|Offline: does not change m1/m2 titles, m2 val style, m1_sub style beyond color (PY:295-311)|RS:872-905|same|none||
|Normal branch: dot theme color; m1_sub/m2_sub color #64748b (PY:313-315)|RS:907-914|same|none|QSS vs label color: visual equivalent, unverified at pixel level.|
|Sub reset before update: subtext if present, elif no reset -> "重設於: --" (PY:317-324)|RS:916-928|same|rust test_absent_timestamp_clears_previous_countdown; py test_ui.py::test_absent_timestamp_clears_previous_countdown||
|Metric title/text/color/bar for m1, m2 (PY:326-342)|RS:930-955|same|rust card_bars_are_gauges...||
|Badge compaction: replace " 剩餘:" and "剩餘:" with ":"; four-way show logic; fallback "--" (PY:345-364)|RS:957-993|same|none (relayout tests one_badge/both_badges check geometry)|Python `or ""` handles None; Rust String empty.|
|After metrics: `update_countdown()`; if stale: dot amber (#f59e0b / #92400e) 10px and badge "STALE" (PY:365-367)|RS:995-1005|same|rust test_stale_data_remains_visible_and_labelled; py test_ui.py::test_stale_data_remains_visible_and_labelled||
|Re-layout after update (not in Python; Qt relayouts automatically)|RS:1006, 902|intentional-diff|relayout tests|ferrokit needs explicit `update_layout`.|
|`update_countdown`: skip if error and not stale; m1/m2 sub via `_reset_text`; extras zip; stale -> m1_sub "舊資料 %m/%d %H:%M:%S" local time or "--", amber (PY:369-383)|RS:1009-1049|same|py test_ui.py:36-39,68-73; rust test_stale... (only val/badge, not sub text)|Stamp format and `--` equal; local-time conversion equal.|
|Stale sub style applied via QSS color (PY:383)|RS:1046 `set_label_color`|same|none||
|Per-card font scale: Python sidebar `_scale_card_style` rewrites QSS font-size (sidebar.py)|RS:128-170 `scale_font_sizes`, RS:171-218 `LocalStyles`, RS:728-733 `set_font_scale`|same|rust card_logic_tests::font_sizes_scale_and_one_restores, font_scale_applies_to_later_styles; py test_sidebar.py:169 test_independent_setter_order_and_card_font_restore|Sidebar side is in other matrix; not re-read here.|
|`get_progress_color(percent, dark)`: >=90 #ef4444/#b91c1c; >=75 #f59e0b/#92400e; >=50 #3b82f6/#1d4ed8; else #10b981/#047857 (styles.py:7-15)|RS:23-47; wrapper styles.rs:107|same|none (no Python test either)|All 8 colors equal; Rust returns Color not hex string. Dead code in both for cards.|
|`provider_accent_color` (none in Python)|RS:49-52|intentional-diff|none|dead_code helper, Rust-only.|
|Card size/geometry: Python card hint 117 high at DPR 1.25, badge height 14/15, relayout rects through badge show/hide, error, stale, long title, narrow, empty (PY layouts above)|RS:1209-1230 tests; provider_card_relayout_tests.rs:205,366|same|rust provider_card::tests::test_card_size_hint_matches_pyside6, test_badge_size_hint_matches_pyside6_at_both_ratios, provider_card_relayout_tests::provider_card_relayout_matches_pyside6_at_dpr_1_25 / _1_0|Oracle numbers were measured from PySide6; windows-only; error step only compares x/width (documented Qt 1px quirk). Not run by me.|
|Cards stylesheet parse (styles.py get_cards_stylesheet)|rust/tests/test_hud_cards_stylesheet.rs:6|unverified|rust test_hud_cards_stylesheet::test_parse_full_python_cards_stylesheet|Only asserts 18 rules parse; no content comparison with python styles.py and no card involved. test_hud_button_style.rs / test_label_header_row.rs: grep found no ProviderCard / RowCell / Badge references (not card tests).|

## Counts
- same: 31
- intentional-diff: 5
- diff: 1 (P2 objectName; plus get_progress_color location noted in the list)
- missing: 0 behaviours (3 missing tests, listed above)
- unverified: 5 (GaugeBar ctor arg, bar float truncation, grabMouse equivalent, hover QSS, cards stylesheet content)


---
# UsageTable
# Parity: UsageTable (python/ui/usage_table.py vs rust/src/ui/usage_table.rs)

Rust file = `rust/src/ui/usage_table.rs` (abbrev RS). Python file = `python/ui/usage_table.py` (abbrev PY). Python tests = `python/tests/test_ui.py` (TableTests L76, CompactTableTests L138). `rust/tests/*.rs` named in the brief (test_label_*, test_lcd_*, test_qss_label_border_paint, test_hud_cards_stylesheet) contain NO reference to UsageTable/UsageDial/ProviderColumn (grep: 0 matches); they test generic Label/QSS/LCD text machinery the table relies on. Relayout tests: `rust/src/ui/usage_table_relayout_tests.rs` (PySide6 oracle rectangles, scale/dark, DPR 1.25 and 1.0).

## Discrepancies (diff / missing)

| # | Sev | Item | Fix hint |
|---|-----|------|----------|
| 1 | P1 | **missing**: agy badge combining. PY `_badge_text` (PY:343-348) for `agy` with both badge1+badge2 shows `"{b1} · {b2}"` with ` 剩餘:`/`剩餘:` -> `:`. RS (1197-1208) picks badge1 else badge2 only. Rust agy provider does fill both (providers/agy.rs:334 `Argon 剩餘: x`, :247 `Gemini Models`), so Rust agy column shows only "C/G 剩餘: 84%", Python shows "C/G: 84% · Argon: 79%". Relayout oracle only uses agy with badge1 -> not covered. | Port `_badge_text` (provider_card.rs:924 already has the same replace logic to copy). |
| 2 | P2 | Legend `ring` glyph arc pen: PY `QPen(color,2.2)` = Qt default square cap/bevel join (PY:97-99); RS uses `round_pen` (round cap/join) (RS:792-794). | Use `Pen::new(..)` default cap, not round_pen. |
| 3 | P2 | Legend `tick` glyph: PY `QPen(text,1.4,CustomDashLine)` + `setDashPattern([1.5,1.3])` (Qt dash unit = pen width => 2.1/1.82 px) with square cap (PY:101-104). RS (RS:798-805) round cap and un-scaled dash `[1.5,1.3]` px. The dial pace tick (RS:196-201) DOES scale by width (1.6*1.6) -> inconsistent. | Scale dashes by 1.4 and use square cap. |
| 4 | P2 | Dial caption halo pen: PY `QPen(halo,2.5)` default square cap/bevel join (PY:245); RS `round_pen(halo,2.5)` (RS:455). Also the track ring pen default cap irrelevant (full ellipse). | Use non-round pen for caption halo. |
| 5 | P2 | Separators span: PY `len(self.columns)+1` (PY:480) ; RS hard-coded 4 (RS:1397,1448). Wrong only if provider_ids != 3 entries. | Use `order.len()+1`. |
| 6 | P2 | Offline predicate: PY `bool(data.error) and not data.stale` (PY:312-313, empty string = online); RS `error.is_some() && !stale` (RS:1130,1227) -> `Some("")` counts offline. | Filter empty string. |
| 7 | P2 | Dial `side`: PY `min(w,h)-2` (PY:163); RS `(min-2).max(10)` (RS:292). Only differs for dials < 12 px. | Drop `.max(10.0)` or accept. |
| 8 | P2 | `▲N` rounding: PY `f"{:.0f}"` banker's-even on exact .5 (PY:393); Rust `{:.0}` rounds half-to-even too in std (format uses round-half-even) -> same in practice; listed only because input is float `v2-m2`. (informational, no fix) | none |
| 9 | P2 | unverified: offline icon. PY `icon.setEnabled(False)` -> Qt generates disabled (greyed) pixmap (PY:371); RS recolors glyph with `theme.text3` (RS:1155-1161). Visual shade may differ. | Desktop check vs PySide6 screenshot. |

(8 is informational; real count of diff/missing rows: see end.)

## Table

| Python behaviour (file:line) | Rust location (file:line) | Status | Test coverage | Notes |
|---|---|---|---|---|
| `PROVIDER_NAMES` claude=Claude Code, codex=Codex, agy=Antigravity (PY:26) | `display_name` RS:51-58 | same | RS: `test_header_row_advances_match_qt` (86px "Claude Code"), `collapsed_tooltip_lists_stale_state_and_the_error` ("Antigravity"); PY: none | unknown id returns id in both |
| `PROVIDER_ORDER` = claude,codex,agy (PY:27) | `PROVIDER_ORDER` RS:34 | same | RS: `columns_follow_the_provider_order_it_is_given` (default order); PY: none | |
| `LABELS` 5 小時/1 週/重設/剩餘/內圈 5 小時/外環 1 週/平均進度/超出平均 (PY:29-31) | literals RS:63-75 (compact tip), 1411-1456 (grid/legend) | same | RS: `collapsed_dials_follow_...` (5 小時, 1 週, 重設, 剩餘); PY: `test_compact_tooltip_carries_what_the_screen_no_longer_shows` | strings identical |
| Table QSS `get_table_stylesheet` (styles.py:242-255): QLabel color, SectionTitle 13px/600, RowLabel text2 12px padding-left 18px, Legend text2 10px, Cell 14px pad 0 2px, Pill 15px/600 pad 0 2px, HeaderName 14px/600, HeaderBadge text3 9.5px/600 ls 0.6px, Separator, `QLabel[state="muted"]` text3 | `table_sheet` styles.rs:490-513, applied RS:1495-1497 and 1560-1561 | same | RS: `test_labels_use_the_app_default_family` (Cell 14px => 18px hint), relayout tests (all cell sizes); PY: none. `test_hud_cards_stylesheet.rs` tests the HUD cards QSS parse, not this sheet | text of rules identical; colours come from theme (not compared here) |
| Row/col spacing 10/4, margins 0 (PY:421-427) | RS:1379-1382 | same | RS: `test_grid_matches_qt_geometry` (windows), `usage_table_relayout_matches_pyside6_*`; PY: none | |
| Grid placement: header r0, sep r1, 5h title r2, reset r3, left r4, legend r5 (VCenter\|Left), dial r5, sep r6, 1週 title r7, reset r8, left r9; m1_reset r3, m1_cd r4, m2_val r7 (HCenter), m2_reset r8, m2_cd r9; colStretch 1; rowStretch(5)=1; rowMinHeight(2)=18 (PY:450-475) | RS:1395-1481 | same | RS: `test_grid_matches_qt_geometry`, `test_squeezed_table_keeps_the_dial_at_its_minimum`, relayout tests; PY: none | |
| Separator span `len(columns)+1` (PY:431) | RS:1397,1448 hard-coded 4 | diff | none | P2 #5 |
| Separator QFrame: 1px high, bg separator colour, no border | `SeparatorWidget` RS:828-935 (size hint 10x1, Expanding, draws 1px rect) | intentional-diff | RS: `test_table_layout_contains_icons_separators_and_glyphs`, grid test (row h=1); PY: none | custom widget instead of QFrame#Separator; same pixels expected |
| Scheme colours for glyph rows: duo -> inner/outer/hatch=duo.inner; else neutral/neutral/scale.yellow (PY:439-443) | RS:1383-1387, 1555-1563 | same | none | |
| `full_view`/`compact_view` in outer VBox (margins 0, spacing 0); compact hidden initially (PY:421-431) | RS:1498-1531 | same | RS: `collapsing_swaps_the_views_and_expanding_restores_them`; PY: `test_compact_keeps_only_the_dials_in_provider_order` | Rust also sets Expanding policy on both pages |
| Compact row: HBox margins 0, spacing `COMPACT_GAP`, each compact dial stretch 1, in provider order (PY:424-429) | RS:1485-1496 | same | RS: `collapsed_view_has_one_dial_per_service_in_column_order`; PY: `test_compact_keeps_only_the_dials_in_provider_order`, `test_compact_row_gives_each_dial_a_square_to_draw_in` (no Rust analogue for the square-width assertion) | `COMPACT_GAP`/`COMPACT_DIAL_MIN` both imported from table_layout (not re-compared here) |
| `is_compact/set_compact(bool)` toggles full/compact visibility (PY:478-486) | `TableViews::set_compact` RS:1339-1344, `UsageTable::set_compact` RS:1532 | same | RS: `collapsing_swaps_the_views_...`; PY: `test_compact_keeps_only_the_dials_in_provider_order` | |
| Shared collapse handle across rebuilds | `TableViews::adopt_pages`, `adopt_views` RS:1347-1351,1538 | intentional-diff | RS: `a_rebuilt_table_keeps_the_window_s_collapse_state_and_handle`; PY: none | Python keeps state in HUD window; Rust needs a handle because the table is owned elsewhere |
| `compact_dials()` list in column order (PY:488) | RS:1550-1556 (cfg(test) only) | same | RS: `columns_follow_the_provider_order_it_is_given`; PY: `test_compact_keeps_only_the_dials_in_provider_order` | |
| `update_metrics` routes by provider_id; unknown ignored (PY:490-492) | RS:1600-1604 | same | RS/PY: all table tests | |
| `update_countdowns` loops columns (PY:494-496) | RS:1606-1610 | same | none direct | |
| Theme change: Python rebuilds table | `UsageTable::set_theme`/`ProviderColumn::set_theme` RS:1077-1099,1558-1597 (recolour in place, re-run `update_metrics`, recolour 6 glyphs by index) | intentional-diff | RS: `names_and_icons_use_the_brand_colour_and_follow_the_theme`; PY: none | glyph colour array order matches creation order (row2, legend x4, row7) |
| `UsageDial.__init__`: min_size 84 default, Expanding/Expanding, initial `("--", None...)` (PY:131-141) | `UsageDial::with_min_size` RS:133-150 | same | RS: `test_dial_minimum_size_matches_python`; PY: `test_compact_dials_have_no_caption_and_a_smaller_minimum` (min 48) | Rust `size_hint` = min_size too |
| Compact dial min = `COMPACT_DIAL_MIN` (48 per py test), caption "" (PY:300-301, 322) | RS:1031-1033, 1117-1125 | same | RS: `collapsed_view_has_one_dial_per_service_in_column_order`, `collapsed_dials_follow_the_full...`; PY: `test_compact_dials_have_no_caption_and_a_smaller_minimum` | PY dial constructed with `provider_id=` kwarg (PY:301); Rust dial has no provider_id (not needed, order known by table) |
| `_deg(percent)` = -360*clamp(0,100)/100 (PY:149-151) | `UsageDial::deg` RS:173-175 | same | none | |
| Pace tick: skipped if mark None or muted; angle 90-360*mark/100; pen text, 1.6 wide, dash [1.6,1.4] (width units), square cap (PY:153-161) | `draw_pace_tick` RS:177-209 | same | none | Rust multiplies dashes by 1.6 explicitly |
| Dial geometry: side=min(w,h)-2, ring=max(5, side*0.07), ring_rect inset ring/2+1; gap=ring+side*0.06 (PY:163-189) | RS:287-300, 333-340 | same (see #7) | none | `.max(10)` only extra in Rust |
| Outer track: ellipse, pen track colour ring wide, no brush (PY:171-173) | RS:307-312 | same | none | |
| Outer arc: colour pen ring, FlatCap, from 90deg span `_deg(pct)` when pct not None and not muted (PY:174-179) | RS:314-320 (adds `pct > 0.0` guard, equivalent) | same | none | |
| Outer hatch over arc between mark and pct when pct>mark (PY:180-188; stroker ring wide Flat) | RS:321-336 (`create_donut_arc_path`, `Brush::Hatched`) | unverified | none | hatch tile (6x6, 1.6px lines at 3 diagonals, PY:34-44) vs ferrokit `Brush::Hatched` pattern not compared; needs pixel check |
| Outer pace tick radii `side/2-ring-2.5` .. `side/2+0.5` (PY:189) | RS:337-346 | same | none | |
| Inner disc: ellipse fill `disc`; pie fill colour (full ellipse if >=100, pie if >0); hatch wedge if pct>mark; tick radii 0.6r..r (PY:193-212) | RS:348-395 | same | none | hatch caveat as above (unverified) |
| Centre text: num strip %/space; pixel size max(8,int(inner_h*(0.30 pct else 0.22))); suffix 0.5x min 6; bold; `cy` raised by 5% when caption; advance + 1px; baseline = cy+capHeight/2 (PY:214-230) | RS:397-441 | same | none | font family: PY `self.font()` (app font), RS `APP_DEFAULT_FAMILY`; metrics are ferrokit's, text path glyph outlines vs QPainterPath.addText -> pixel-level unverified |
| Text halo: stroke path with halo colour 3px round cap/join when not muted; fill text, muted -> `text3` (PY:231-238) | RS:442-466 | same | none | Muted fill: `text3` is an `rgba(...)` string which QColor rejects -> opaque black; Rust deliberately reproduces `Color::BLACK` (RS:456-460). Re-verify if Python theme changes |
| Caption: bold, px max(9,int(inner_h*0.13)), centred, baseline center.y+inner_h*0.32, halo 2.5 + fill text (PY:240-248) | RS:469-486 (halo uses `round_pen`) | diff | none | P2 #4 pen cap/join |
| Antialiasing on all painters (PY:162,35,56,84) | `set_render_hint(Antialiasing)` RS:288,632-641,768 | same | none | |
| `provider_icon`: hi-DPI 2x pixmap, pen 1.5 round cap/join, colour = brand (PY:54-79) | `ProviderIconWidget::draw_glyph`/paint RS:508-556, 631-652 (2x pixmap, smooth downscale) | same | RS: `test_table_layout_contains_icons_separators_and_glyphs`, `names_and_icons_use_the_brand_colour_and_follow_the_theme`; PY: none | codex chevron: PY one polyline, RS two `draw_line`s with round join (visually same) |
| claude glyph: 8 rays, a=i*45+22.5, r1 0.14s, r2 0.44s odd / 0.36s even (PY:62-66) | RS:513-523 | same | none | `i%2` parity mapped correctly (RS: even->0.36) |
| codex glyph: rounded rect (.1,.18,.8,.64) r .14s; chevron (.28,.38)-(.42,.5)-(.28,.62); underline (.5,.64)-(.7,.64) (PY:67-71) | RS:524-538 | same | none | |
| agy glyph: cubic (.14,.86) c1 (.28,.1) c2 (.72,.1) end (.86,.86); bar (.34,.62)-(.66,.62) (PY:72-77) | RS:539-554 | same | none | |
| Icon size 18, fixed (PY:287) | RS:1043 `ProviderIconWidget::new(..,18.0)`, Fixed policy RS:491-494 | same | `test_header_row_advances_match_qt` (icon x=1) | |
| `legend_glyph` pie: disc `QColor(*disc).darker(110)` ellipse in rect(1.5,1.5,size-3), pie 90deg span -250 (PY:85-91) | RS:770-785 | same | RS: `test_legend_pie_disc_is_darker_than_the_theme_disc` (pixel oracle from PySide6); PY: none | |
| legend ring: inset r+1, track pen 2.2 + arc 2.2 span -250 (PY:92-98) | RS:786-797 | diff | none | P2 #2 round vs square cap |
| legend tick: dashed vertical line x=size/2 from 1 to size-1 (PY:99-104) | RS:798-806 | diff | none | P2 #3 |
| legend hatch: rounded rect radius 2 colour then hatch brush overlay (PY:105-110) | RS:807-815 | unverified | none | hatch pattern as above (PY fills `fillRect` square, Rust uses rounded rect for overlay: corners slightly differ) |
| `_glyph_row`: HBox margins 0 spacing 5, glyph icon, QLabel objectName obj, trailing stretch (PY:114-126) | `make_glyph_row` RS:938-961 | same | RS: `test_trailing_stretch_adds_no_spacing` | |
| Legend column: VBox margins 0 spacing 3, 4 rows size 11, colours (inner,outer,"" tick,hatch) (PY:437-447) | RS:1415-1436 (adds `add_stretch(1)` before and after rows; tick colour = theme.text) | unverified | RS: grid test (legend gutter width 11+5+52); PY: none | extra stretches centre the rows inside the cell; Python relies on VCenter alignment. Expect equal visual result; needs layout oracle |
| Section titles: pie glyph size 13 "5 小時" r2; ring glyph size 13 "1 週" r7 (PY:449,463) | RS:1403-1407, 1444-1447 | same | RS: relayout/grid tests | |
| `sub_label` RowLabel, VCenter\|Left alignment (PY:433-436) | `make_sub_label` RS:965-968 (no explicit alignment) | unverified | RS: grid row heights | grid default align vs `AlignVCenter\|AlignLeft`; QLabel default is left/vcenter-ish; only a layout probe can confirm |
| `_cell`: QLabel centred, objectName (PY:252-256) | `make_cell_label` RS:977-982 | same | none | |
| `_set_state` sets `state` property and re-polishes only if changed (PY:259-263) | `set_state` RS:972-974 (property set, restyle assumed by ferrokit) | unverified | none | needs check that ferrokit re-evaluates `[state="muted"]` selector on property set |
| Tabular digits (`tnum`) on 5 value cells (PY:311-314) | `make_value_cell` RS:1057-1063 | same | none | |
| Header: VBox(0,0): HBox(5) [stretch, icon, name, stretch] + badge " " HeaderBadge centred (PY:271-292) | RS:1009-1056 | same | RS: `test_header_row_advances_match_qt` (icon x1, name x24 w86), relayout tests | |
| Brand colour `brand_color(pid, dark)`; name label stylesheet `color: brand` (PY:279-285) | RS:1033-1041 | same | RS: `names_and_icons_use_the_brand_colour_and_follow_the_theme`; PY: none | Python dark test is `theme is THEMES["dark"] or ==`; Rust `theme.is_dark` |
| Initial texts `--:--`,`--:--`,`--`,`--:--`,`--:--:--` (PY:296-301) | RS:1065-1070 | same | `test_absent_timestamp_clears_previous_countdown` (both) | |
| `_colors`: duo -> duo[which],duo_text[which]; scale -> neutral/text2 when level None else scale[level],scale_text[level] (PY:317-324) | RS:1245-1253 using `duo_colors`/`scale_colors` (styles.rs:111-130) | same | PY: `test_scale_and_duo_colour_the_dial_differently`; RS: none | Rust has no test asserting dial colours per scheme. Only inner fill + outer fill/text are used |
| `_is_offline` (PY:326-328) | RS:1130,1227 | diff | none | P2 #6 empty-string error |
| `_set_dials` mirrors to compact with caption "" (PY:330-332) | `set_dials` RS:1102-1126 | same | RS: `collapsed_dials_follow_the_full_dials_...`; PY: `test_compact_dials_follow_the_data` | |
| Compact tooltip builder: name; per window `"{label}  {text}  重設 {reset(with_day only week)}  剩餘 {countdown}"` + `"  {run_out}"` if any; stale: `STALE 舊資料 {mm/dd HH:MM:SS local or --}`; error line only when stale (PY:334-349) | `compact_tip` RS:61-102 | same | RS: `collapsed_dials_follow_...`, `collapsed_tooltip_lists_stale_state_and_the_error`; PY: `test_compact_tooltip_carries_what_the_screen_no_longer_shows`, `test_offline_and_stale_states_reach_the_compact_dials` | Rust `error` filter non-empty equals Python truthiness |
| `update_metrics`: store data; tooltips of 5 value cells + header = `error or ""` (PY:353-355) | RS:1128-1144 | same | RS: n/a; PY: `test_error_recovery_clears_offline_state` (header tip '') | Rust does not assert header tooltip empty after recovery |
| Muted state -> `muted` on value cells + name else "" (PY:357-359) | RS:1146-1160 | unverified | none | see `_set_state` row |
| `icon.setEnabled(not muted)` (PY:360) | RS:1155-1161 recolour `text3` offline, brand online | unverified | none | #9 |
| Offline branch: badge "OFFLINE"; dials `(None,)*3,"ERR",(None,)*3,muted=True`; full dial tip = error; compact tip "name\nerror"; m2_val "--" and style cleared; value texts reset; return (PY:362-374) | RS:1162-1190 | same | RS: `test_error_recovery_clears_offline_state`, `offline_collapsed_dial_is_muted_with_the_error_in_its_tooltip`; PY: `test_error_recovery_clears_offline_state`, `test_offline_and_stale_states_reach_the_compact_dials` | full dial tip equals `error or ""`; Rust passes `error_tip` |
| Online badge: `⏱ STALE` if stale else `_badge_text` (PY:376) | RS:1197-1209 | diff | RS: `test_stale_data_...` (contains STALE), relayout "stale"; PY: `test_stale_data_remains_visible_and_labelled` | agy compact badge missing, #1 (P1) |
| `_badge_text` non-agy: `badge1 or badge2 or " "` (PY:344-348) | RS:1198-1208 | same | relayout tests (badge1 only) | |
| Stale header tooltip `舊資料 {stamp}\nerror` (PY:377-379) | RS:1211-1223 | same | PY: `test_stale_data_remains_visible_and_labelled` (assertIn timeout); RS: none for tooltip | |
| `update_countdown`: skipped when offline; texts `format_reset_time(m1,with_day=False)`, `format_countdown_hm`, `format_reset_time(m2,True)`, `format_countdown_dhm` (PY:351-361) | RS:1226-1240 | same | RS/PY: `test_absent_timestamp_clears_previous_countdown` | pace.rs functions are other files' parity |
| Windows `window_seconds(title, FIVE_HOURS/ONE_WEEK)`, `elapsed_fraction`, `pace_mark` (PY:364-369) | RS:1242-1246 | same | RS/PY: `test_ahead_of_pace_shows_overage_on_weekly_pill` | |
| Dial values `(val, mark, fill)`, caption `window_caption(m1_title,"5H")` (PY:375-377) | RS:1263-1270 | same | RS: `test_non_default_window_gets_caption` (checks m2 text only); PY: same test asserts `col.dial.caption == '1D'` | Rust test does not check dial caption |
| Weekly pill: `pct = text.replace("%"," %")`; ` · cap` if cap; `  ▲N` if val-mark>0.5; colour `text2` via style sheet (PY:379-387) | RS:1272-1291 | same | RS/PY: `test_ahead_of_pace_...` (▲48), `test_stale_data_...` ("10 %"), `test_non_default_window_gets_caption` | |
| Tooltips: m2_val = tip2 + stale err; full dial = "5 小時：tip1","1 週：tip2", stale err; compact = `_compact_tip` (PY:389-396) | RS:1293-1306 | same | PY: `test_ahead_of_pace_...` ('用完' in m2 tip); RS: none asserting it | Rust has no test on pill/dial tooltip text |
| Stale err line in tooltip only when `stale and error` (PY:391) | RS:1296-1299 | same | none | |
| Dial text/colour/caption/muted fields exposed for tests (`inner`, `outer`, `caption`, `muted`, `inner_text`) | same-named private fields in UsageDial (RS:116-126); tests read them within module | same | RS: `collapsed_dials_follow_...` | |
| Scheme-dependent dial fill in tests (PY test L126-135) | RS:1245-1262 | same | PY: `test_scale_and_duo_colour_the_dial_differently`; RS: none | add Rust test |
| Relayout under badge/pill/offline/stale changes (no PY test; oracle probe `usage_table_relayout.py`) | `usage_table_relayout_tests.rs` (`usage_table_relayout_matches_pyside6_at_dpr_1_25/1_0`, `stale_badge_height_matches_pyside6_*`) | same (via oracle) | RS: those 4 tests; PY: probe script only | windows-only oracle; geometry per column of 10 cells x 6 steps |
| `rust/tests/test_label_text_metric_rounding.rs`, `test_label_header_row.rs`, `test_qss_label_border_paint.rs`, `test_lcd_text_parity.rs`, `test_hud_cards_stylesheet.rs` | n/a (no table reference) | unverified | n/a | generic toolkit parity (Label widths with family list, header-row layout, QSS border pixels, LCD text, cards QSS parse). They back the table only indirectly (labels in table). `test_hud_cards_stylesheet` covers the HUD cards sheet, NOT `get_table_stylesheet`; no Rust test parses/verifies the table sheet text |

## Counts per status

Rows by status (table above): same = 44, intentional-diff = 3, diff = 5 (separator span, legend ring, legend tick, caption halo, offline predicate/badge row counted separately: see below), missing = 0 rows as standalone (agy badge combine is recorded as the diff row for badge), unverified = 8.

Precise: diff rows = separator span, caption halo, legend ring, legend tick, `_is_offline`, online badge (agy compact) = 6. Behavioural discrepancies list at top: 1 P1 (agy badge), 8 P2 (items 2-7, 9; item 8 informational, no fix).

Test gaps (not discrepancies): no Rust test for scale/duo dial colours, pill/dial tooltips, dial caption, header tooltip reset; no Python test for brand colours, glyph/icon drawing, separators, grid geometry.


---
# Sidebar
# Sidebar parity: python/ui/sidebar.py vs rust/src/ui/sidebar.rs

Root: C:/Users/sss19/AppData/Local/Temp/wt_retire. Paths below are relative to `python/` or `rust/src/ui/`. PY = python/ui/sidebar.py, RS = rust/src/ui/sidebar.rs.
Read-only analysis; nothing was built or run.

Tests read: PY `python/tests/test_sidebar.py` (all), sidebar parts of `python/tests/test_ui.py` (HUD-level dock/scale/state/fade tests, only referenced where they touch the sidebar). RS in-file `sidebar.rs::tests` and `sidebar.rs::qss_scale_tests`, `tool_tip_tests.rs::card_and_header_ghost_tool_tips_follow_python`. Other `*_tests.rs` (button_hover_tests, geometry_persist_tests, provider_card_relayout_tests, usage_table_relayout_tests, qss_box_audit) and `rust/tests/*.rs` contain no sidebar reference (grep -i sidebar: no hits). **No Rust test constructs a `SidebarWidget` or drives the hover/pin/flyout/reorder/wheel/key state machine; only pure helper fns are tested.**

## diff / missing list

| Sev | Item | Fix hint |
|---|---|---|
| P2 | `window_reset` threshold: PY `(reset-old).total_seconds() > 600` (float) vs RS `.num_seconds() > 600` (truncated). A delta of 600.1–600.99 s flashes in Python, not in Rust. (RS:230, PY:631) | use `num_milliseconds() > 600_000` |
| P2 | `set_dock_edge`: PY raises ValueError on unknown edge; RS accepts any `Option<&str>` and treats unknown as "right" (RS:1772, flyout_position `_` arm). | validate against left/right/top/bottom/None |
| P2 | `open_flyout` sets `active` before looking up card widget / tile rect; if either lookup fails RS returns with `active` set but no flyout (RS:2342-2345). PY would raise instead. Practically unreachable. | reorder: look up first, then set `active` |
| P2 | `_maybe_close` hit test uses `Rect::contains` in global coords (RS:2458); Qt `QRect.contains` includes right/bottom edge pixel. Depends on ferrokit `Rect::contains` semantics (unverified). | check inclusive edge |
| P2 | Missing `SidebarWidget.hideEvent -> close_flyout` (PY:814). Rust relies on HUD calling `sidebar.close_flyout()` at hud_window.rs:1018,1133,1146,1962,2278. Other ways the sidebar becomes hidden would leave the flyout open. | acceptable; or add hide hook |
| P2 | Event filter scope: PY installs filter on `self` and every child (buttons, strip, panel, storage, cards at init) so Ctrl+wheel / Esc / context-menu work wherever the cursor is. RS installs only on `container` and `flyout_root` (RS:1856-1873). Whether ferrokit propagates wheel/key to the parent filter is unverified. | runtime check Ctrl+wheel over a tile and over the flyout card |
| P2 | `QSS radius` string: PY `border-radius:6.0px`, RS `6px` (format_g). Equivalent in QSS. | none |
| P2 | `_Button.event` ToolTip, enter/leave/press/move/release: in RS they call owner via `defer` (queued), PY runs synchronously. Intentional (borrow rules) but timing differs by one event-loop turn. | none |
| P2 | No Rust test covers hover/pin/unpin/flyout/reorder/wheel/keyboard/ownership/card-font restore flows; PY test_sidebar.py covers all of them. | add widget-level tests in sidebar.rs or a *_tests.rs |

No P0 or P1 discrepancy found in code read.

## Parity table

| Python behaviour (file:line) | Rust location (file:line) | Status | Test coverage | Notes |
|---|---|---|---|---|
| SIDEBAR_TOKENS dark: bg #1e1e24, text #f2f2f7, muted #a6a6b3, hover #30303a, pressed #40404c, focus #38bdf8, separator #454550, stale #f59e0b, alert #dc2626, Segoe UI 12, spacing 4, radius 6, focus_width 2, hover_ms 300, leave_ms 600, anim 160 (PY:21) | RS:56-74 | same | RS sidebar.rs::tests::tokens_are_complete_for_both_themes_and_match_python (stale, bg, ms only); PY test_sidebar.py::test_themes | Every field compared literally. |
| SIDEBAR_TOKENS light: #f6f4fa, #1f1f24, #555563, #e6e4ed, #d8d6e1, #0369a1, #ceced8, #92400e, #b91c1c, same font/ms (PY:22) | RS:76-94 | same | RS tokens_are_complete_... (stale only) | all literals equal. |
| tokens['providers'] = BRAND[k][dark/light] (PY:21,22) | RS:48-50 `Tokens::provider` -> brand::brand_color(key, dark) | same | none | brand table itself compared by another peer; indirection only. |
| BASE_BUTTON_W/H 64, BASE_PANEL_W 220 (PY:24-25) | RS:98-100 | same | RS geometry_of_the_strip_and_panel; PY test_geometry_scaling_and_layout | |
| _NAMES / DEFAULT_ORDER claude,agy,codex (PY:26-27) | RS:102-103 | same | none direct | |
| HOLD_MS 350, ALERT_PCT 90, FLASH_MS 1800, FLASH_STEP_MS 60 (PY:28-32) | RS:104,109,111,112 | same | RS tile_shows_an_alert_mark_from_ninety_percent; PY test_tile_shows_an_alert_mark_from_ninety_percent | |
| RAMP_STOPS constant (PY:29) | RS:106-107 | same | RS scale_bars_are_a_green_to_red_gauge | Constant unused in both (gauge.ramp_stops used instead). |
| Flyout max width 260 (PY:743 `min(260, area.width())`), scale clamp 0.6..2.0 (PY:477) | RS:114,116-117 | same | PY test_keyboard_and_geometry_without_motion (width 260); RS geometry_of_the_strip_and_panel (clamp) | |
| `_row_label`: prefixes GEMINI/C/G/ARGON -> 'Ge '/'CG '/'Ar ' + rest of upper title, else short_window (PY:35-41) | RS:126-134 | same | RS row_labels_follow_the_chosen_rows; PY test_row_labels_follow_the_chosen_rows | short_window body is peer scope (core). |
| `_color(value, which, force_scale)`: duo -> theme['duo'][which] unless force_scale, else scale[level] or neutral (PY:578-583) | RS:160-181 `status_color` | same | RS metrics_and_offline_tiles, agy_dots_follow_the_extra_rows; PY test_tile_colors_and_contrast | scale_level / theme values: peer scope. |
| `_row_values`: m1, m2, extra rows (PY:618-622) | RS:186-205 | same | RS tooltip_lists_every_row_with_a_countdown | |
| row_state keyed by `(title or '').upper()` (PY:593-594) | RS:211-216 | same | RS a_window_reset_needs_a_later_reset_and_lower_usage | |
| `_window_reset`: skip if any None; `>600 s` later reset and val lower (PY:624-633) | RS:220-235 | diff | RS a_window_reset_needs_a_later_reset_and_lower_usage; PY test_tile_flashes_when_a_window_resets | P2: total_seconds float vs num_seconds truncated (see list). Tests use 5 h and 2 min so don't expose it. |
| offline = bool(error and not stale); stale = bool(stale) (PY:591-592) | RS:256-258, 306 | same | RS metrics_and_offline_tiles; PY test_metrics_and_external_ownership | RS `error.is_some()` vs PY truthy: an empty-string error is offline in Rust, not in Python (only matters if error can be `Some("")`; UsageMetrics.error type in peer scope). Treated same, noted. |
| `_refresh_tile`: values None when offline; labels via row_label; colour per row with which inner/outer (PY:602-607) | RS:262-282 | same | RS metrics_and_offline_tiles; PY test_metrics_and_external_ownership | |
| display `--` or `f'{value:.0f}%'`; offline -> ['OFF','--'] (PY:608-610) | RS:283-287 | same | RS metrics_and_offline_tiles | Python `.0f` uses banker-free round-half-even of the binary value; Rust `{:.0}` also round-half-even on exact repr: same for .5 cases. |
| dot_colors only for agy: `_color(row.val, force_scale=True)` of extra rows (PY:611-612) | RS:288-295 | same | RS agy_dots_follow_the_extra_rows; PY test_agy_dots | |
| alert = not offline and any row val >= 90 (PY:613-614) | RS:296-299 | same | RS tile_shows_an_alert_mark_from_ninety_percent; PY same-name | |
| Tooltip: heading name; rows `{label} 已用 {text}，重設於 {when}`; label = row_label except agy raw title; `stamp · countdown`; alert line `! 接近上限（已用 ≥ 90%）`; error; `STALE` (PY:651-669) | RS:311-338 | same | RS tooltip_lists_every_row_with_a_countdown, metrics_and_offline_tiles(STALE), tile_shows_an_alert_mark...; PY test_tooltip_countdown_only_and_no_dial_import, test_metrics_and_external_ownership | Strings compared char by char. |
| Tooltip also set as accessibleDescription (PY:670) | RS:2205-2208 | same | none | |
| `update_countdown` refreshes tooltips for all known keys only (PY:672-674) | RS:1806-1811 | same | PY test_tooltip_countdown (countdown call_count 4); RS none (no call-count test) | |
| text_color / ramp_stops / ramp / row_color: gauge.readable/ramp_* against (background, hover, pressed); duo or None value -> text_color(row colour), else ramp(value) (PY:261-282) | RS:342-368 | same | RS tile_colors_read_on_every_tile_surface, bar_and_number_always_share_one_colour, scale_bars_are_a_green_to_red_gauge; PY same-name tests | gauge internals: peer scope. PY hue-lock assertion for duo has no RS equivalent. |
| `contrast()` staticmethod WCAG (PY:252-259) | gauge::contrast (used in RS tests only) | unverified | RS tests via gauge::contrast | defined in gauge.rs, not this file. |
| `_Indicators`: transparent for mouse, no system background (PY:46-50) | RS:711-717 | same | none | WA_NoSystemBackground has no explicit Rust call; Rust widget has no bg by default (unverified). |
| Alert triangle: size 11*scale, x=right-size-2*scale, y=1.5*scale, apex/base 0.92, alert colour, white bold "!" px max(6,round(8*scale)) in rect (x,y+0.28*size,size,0.64*size); right -= size+4*scale (PY:58-72) | RS:792-816 | same | none (PY test grabs indicators only to prove no raise) | Rust `.round()` on f32 vs Python banker's round differs only at exact .5 (scale 1.5625 etc.): P2 theoretical. |
| Stale dot: ellipse at (right-10*scale, 0, 6s, 6s), stale colour (PY:73-76) | RS:817-826 | same | none | |
| agy dots: x=(60-8*count+8*i)*scale, y=10*scale, 6*scale, pen = background width scale, fill muted when offline else colour (PY:77-82) | RS:827-840 | same | none | |
| `_Button` init: stale/offline/alert False, flash 0, dot_colors [neutral dark]*3, rows 5H/7D neutral, display `--`, show_labels = scale>=0.8 (PY:89-100) | RS:869-881, 944-965 | same | none | |
| Focus policy StrongFocus, mouse tracking, PointingHand cursor, accessible name + tooltip = name or 'Toggle sidebar' (PY:102-106) | RS:888-915 | same | none | |
| ToolTip event swallowed while flyout open (PY:108-112) | RS:978-983 | same | none | |
| Return/Enter -> click, Escape -> close_flyout, else default (PY:114-120) | RS:1186-1193 | same | PY test_keyboard_and_geometry_without_motion; RS none | Rust close is deferred. |
| Space press/release activation (QPushButton default, PY implicit) | RS:1190,1195-1200 | same | PY test_keyboard_... (Key_Space); RS none | Rust reimplements QAbstractButton behaviour by hand. |
| `_begin_reorder` guards: press_pos set, not dragged, kind in _NAMES, state strip; sets reordering, down False, ClosedHand, `owner._start_reorder` (PY:122-128) | RS:681-701 | same | PY test_press_hold_drag_reorders_and_emits_once; RS none | |
| mousePress: left -> press_pos, dragged False, hold timer 350 ms for providers; then default press (down) (PY:130-136) | RS:1119-1129 | same | none | |
| mouseMove: while reordering -> `_move_reorder(global)` accept; else >6 px hypot from press stops hold, dragged, up, stops hover timer, emits dragRequested; no super when dragged (PY:138-152) | RS:1132-1154 | same | PY test_context_wheel_drag; RS none | Rust uses `Cursor::pos()` for global instead of the event; queued emits. |
| mouseRelease: stop hold; reordering -> reset, cursor, `_finish_reorder`, accept; dragged left -> up, accept; else default (click inside) (PY:154-170) | RS:1157-1183 | same | PY test_press_hold_drag...; RS none | Rust re-implements click-if-inside via geometry rect. |
| enterEvent -> update + owner._hover for providers; leaveEvent -> update, stop hover timer, schedule_close (PY:172-182) | RS:1100-1116 | intentional-diff | PY test_unpinned_leave_and_hover; RS none | Enter's `hover` call is deferred (borrow), leave is synchronous. |
| `_Button.paintEvent` background: pressed / hover(underMouse) / background, rounded rect inset by scale, radius*scale (PY:189-190) | RS:1215-1226 | same | none | |
| Flash glow: provider colour alpha min(0.45, 0.45*flash) over same rect (PY:191-196) | RS:1228-1237 | same | none | |
| Identity stripe rect (1.5s,7s,2.5s,h-14s) r=1.2s, muted if offline else brand (PY:197-201) | RS:1238-1252 | same | none | |
| Focus ring: provider brand colour or t.focus, width focus_width*scale, rect inset 2*scale (PY:202-206) | RS:1253-1266 | same | none | |
| Font: pixel max(8, round(12*scale)) Segoe UI (PY:207-209) | RS:1267-1273,1295 | same | none | |
| Toggle glyph `≡` in strip, `▥` in panel, text colour, centered (PY:210-213) | RS:1268-1287 | same | none | non-bold in both. |
| Title: bold, muted if offline else brand, rect (6s,2s,w*s,16s) w=30 for agy else 52, left/vcenter (PY:214-218) | RS:1289-1303 | same | none | |
| Row layout: y=(19+21*i)*s; label 10 px muted rect (6s,y,30s,15s) only when show_labels; value 12 px bold right-aligned (6s,y,52s,15s), muted if offline else row_color (PY:220-233) | RS:1306-1328 | same | none | |
| Track rect (6s,y+15s,52s,4s) r=2s theme['track']; fill width = track*clamp(value,0,100)/100 only if value not None and not offline (PY:234-240) | RS:1329-1333,1341 via gauge::draw_gauge_bar | unverified | none | Fill/clamp logic equal; actual pixel output depends on gauge::draw_gauge_bar (peer). |
| Fill brush: duo -> single colour `shown`; scale -> linear gradient across the track using ramp_stops (PY:241-250) | RS:1334-1341 | unverified | PY test_scale_bars_are_a_green_to_red_gauge (grab no raise); RS none | Rust approximates duo gradient with stops (0,shown),(100,shown). Needs desktop render check that gradient spans full track, not fill. |
| `_Flyout`: Tool|Frameless|StaysOnTop, ShowWithoutActivating, translucent bg, RADIUS 10, bg + 1px separator pen, rect inset 0.5 (PY:286-302) | RS:557-558, 1673-1695, 1502-1526 (Window passed in from hud_window.rs:655-674) | unverified | none | Paint code equal; window flags set outside this file (hud_window.rs). |
| SidebarWidget signals: stateChangeRequested(str), contextMenuRequested(QPoint), scaleStepRequested(int), dragRequested, orderChanged(list) (PY:306-310) | RS:1403-1408, 1731-1745 | same | PY test_states_and_signals, test_context_wheel_drag | |
| __init__ defaults: state strip, vertical, scale 1.0, edge None, dark, scheme 'scale', empty metrics, order default (PY:315-323) | RS:1528-1541 | same | none | |
| Timers: hover single-shot -> `_open_flyout(_hover_id)`; leave single-shot -> `_maybe_close` (PY:325-330) | RS:1625-1646 | same | PY test_unpinned_leave_and_hover | RS skips if hover_id None. |
| Layout order toggle, strip(3 tiles), panel hidden, storage hidden; cards reparented to storage, hidden, minWidth 0, SizePolicy(Ignored, Preferred) (PY:331-357) | RS:1505-1623 | same | none | Rust has no storage widget: cards simply hidden / removed from layouts (intentional, ferrokit has no re-parent). |
| Flyout layout margins 8,8,8,8 QVBoxLayout (PY:359-360) | RS:1512-1514 | same | none | |
| Open animation: QPropertyAnimation OutCubic on windowOpacity (PY:362-363, 747-755) | RS:1544,1647-1658, 2382-2400, 2404-2420 | intentional-diff | none | Rust drives the tween itself with a 16 ms timer; same curve/duration/start/end. Rendering/timing unverified. |
| Event filters on self, flyout, every child (PY:361,364-366) | RS:1856-1873 | unverified | none | Only container + flyout root (see list). |
| set_state: ValueError on bad; no-op if same; close flyout; show strip/panel; panel: add cards in `_order` with `SidebarDivider` QFrame between; strip: take widgets, restore card style, hide cards, delete dividers (PY:378-404) | RS:1875-1937 `State::parse` + HUD ignores bad | same | RS none; PY test_states_and_signals, test_flyout_reparent_pin_and_restore | Bad strings rejected in HUD (hud_window.rs:2033) instead of raise. Rust additionally resets in-progress button interaction (intentional). |
| `_restore_card_style` / `_scale_card_style`: regex-scales `font-size: Npx` in the card and children by scale, remembers originals (PY:484-501) | RS:1929 `set_font_scale(1.0)`, 2088 `set_font_scale(s)`; provider_card.rs:124-155,730 | unverified | PY test_independent_setter_order_and_card_font_restore; RS none in sidebar.rs | Mechanism different (LocalStyles in provider_card.rs, peer scope). |
| set_order: filter to known ids, ValueError unless permutation, no-op if equal, reapply, rebuild panel if open (PY:409-426) | RS:1752-1760 `valid_order`, 1939-1955 | same | RS order_rejects_bad_values; PY test_order_applies_to_strip_and_panel_and_rejects_bad_values | None instead of raise; HUD calls it with already-validated order. |
| `_start_reorder`: close flyout, stop hover timer (PY:428-430) | RS:1965-1971 | same | none | deferred. |
| `_move_reorder`: coordinate vs centers of other tiles, new order, relayout strip (PY:432-449) | RS:1974-2006, `reorder` 473-487 | same | RS press_hold_drag_reorders; PY test_press_hold_drag_reorders_and_emits_once | Centre `g.y+g.height/2` vs QRect.center() `(top+bottom)//2` = y+(h-1)//2: differs by 1 px for even heights at the strict `>` boundary (P2, only matters when cursor sits exactly on centre). |
| `_finish_reorder` emits orderChanged(order) once (PY:451-452) | RS:2009-2012 | same | PY same test; RS none | |
| set_orientation validated, closes flyout, relayout (PY:454-461) | RS:1762-1769 | same | PY test_geometry_scaling_and_layout | enum instead of ValueError. |
| set_dock_edge: validate; if active flyout, stop animation and move it (PY:463-471) | RS:1772-1783, `place_flyout` 2311-2327 | diff | PY test_flyout_directions; RS flyout_opens_beside_the_tile (pure fn only) | P2: no validation of edge name. |
| set_scale: float, non-finite -> ValueError, clamp 0.6..2.0, no-op if same, relayout + update_countdown (PY:473-482) | RS:396-398, 1786-1795 | same | RS geometry_of_the_strip_and_panel(clamp); PY test_geometry_scaling_and_layout | |
| `_relayout`: direction by orientation; padding=round(4*s) margins; spacing 0; panel spacing round(4*s); handle round(22*s) (PY:503-513) | RS:2015-2036 | same | PY test_geometry_scaling_and_layout; RS none | `py_round` used for banker's rounding. |
| Toggle button: vertical -> insert at 0, min 0, max QMAX, fixed (round(64*s), handle); horizontal -> append, min (handle,handle), max (handle, QMAX), policy Fixed/Expanding (PY:514-524) | RS:2037-2056 | same | none | |
| Tiles: fixed round(64*s) square, show_labels = s>=0.8, indicators geometry/raise/update (PY:525-531) | RS:2057-2068 | same | PY test_metrics_and_external_ownership (show_labels at 0.6 / 1) | |
| Panel stylesheet = cards stylesheet with every `Npx` scaled via `%g` (PY:532-533) | RS:2069-2072, `scale_px_in_qss` 1458-1482, `format_g` 1485-1497 | same | RS qss_scale_tests::{every_px_length_of_a_sheet_is_scaled, non_length_numbers_and_text_are_untouched, g_format_has_six_significant_digits_and_no_trailing_zeros} | format_g sig-digit logic only compared with spot values; exponent form (>=1e6 / <1e-4) not handled (unreachable for px). |
| Panel cards: alignment Top when horizontal, none when vertical; width = round(220*s)-2*padding (vertical) or round(220*s); polish and layout activate (PY:534-543) | RS:2073-2098 | same | PY test_geometry_scaling_and_layout (card ordering x/y), test_ui `_assert_panel_fits`; RS none | min=max=width equals setFixedWidth. |
| Strip state: cards min width 0 / max QMAX (PY:544-546) | RS:2099-2103 | same | none | |
| Dividers: fixed thickness max(1,round(s)) height (vertical) or width (horizontal) (PY:547-553) | RS:2105-2117 | same | none | |
| Relayout tail: update toggle, invalidate/activate layouts, updateGeometry (PY:554-561) | RS:2118-2123 | same | none | |
| set_appearance: validate scheme, store dark/scheme, tokens, objectName 'Sidebar', stylesheet = cards sheet + `QWidget#Sidebar{background;border-radius:radius*scale px;}` + divider rule; flyout sheet = cards sheet + `QWidget{background-color:transparent;}`; relayout; refresh tiles; update_countdown (PY:563-576) | RS:2127-2149; objectName at RS:1504 | same | PY test_themes; RS none | Scheme via enum parse. Python formats `6.0px`, Rust `6px`. |
| HUD only ever passes scheme 'scale' to the sidebar | hud_window.rs:1192,1838 | same | none | outside this file; shows Duo only exercised in tests. |
| update_metrics: ignore unknown provider; store; offline/stale; row_state; flash on reset when not offline; refresh tile + tooltip; relayout if panel (PY:585-600) | RS:2152-2178 | same | PY test_metrics_and_external_ownership, test_tile_flashes_when_a_window_resets; RS none (pure helpers only) | Python also asserts card.update_metrics NOT called by sidebar; Rust same by construction (untested). |
| `_flash`: flash=1; steps=1800//60=30; reduced motion -> single shot clear at 1800 ms; else tick every 60 ms, -1/30 each (PY:635-649) | RS:2212-2242 | same | PY test_tile_flashes_when_a_window_resets; RS none | |
| `_animations_enabled`: Qt animationDurationFactor, else Windows SPI_GETCLIENTAREAANIMATION (PY:757-773) | RS:573-575 `GuiApplication::animation_duration_factor() != 0.0` | unverified | PY test_keyboard_... patches it; RS none | Whether ferrokit reads the Windows accessibility flag needs desktop check. |
| preferred_size strip vertical: (round(64s)+pad, round(22s)+3*round(64s)+pad); horizontal: (3*round(64s)+round(22s)+pad, round(64s)+pad) (PY:679-682) | RS:406-420, 2245-2251 | same | RS geometry_of_the_strip_and_panel; PY test_geometry_scaling_and_layout (72 wide) | |
| preferred_size panel: vertical (round(220s), round(22s)+layout h+pad); horizontal width = n*round(220s)+max(0,n-1)*(2*round(4s)+max(1,round(s))), height max(round(22s), h)+pad (PY:683-689) | RS:424-441, 2252-2260 | same | RS geometry_of_the_strip_and_panel; PY test_ui panel fit tests | layout height from ferrokit BoxLayout (peer/runtime). |
| `_hover`: stop leave timer; ignore unless strip; when a flyout is active switch immediately to other key; else start 300 ms hover timer (PY:691-701) | RS:2264-2279 | same | PY test_unpinned_leave_and_hover, test_click_pins...; RS none | |
| `_click_provider`: pinned+same key -> close; else open and pin (PY:703-709) | RS:2282-2290 | same | PY test_click_pins_then_second_click_on_same_tile_unpins; RS none | |
| `_flyout_position`: left -> right of tile, top -> below, bottom -> above, else left of tile; clamp into area (PY:711-723) | RS:446-468 | same | RS flyout_opens_beside_the_tile_and_stays_inside_the_screen; PY test_flyout_directions | RS test also uses edge "right" which falls in the `_` arm like PY. |
| Screen area: `button.screen().availableGeometry()` (PY:714,742) | RS:2293-2299 `Screens::window_screen(tile) or primary or 1920x1040` | intentional-diff | none | fallback constant only if no screen info. |
| `_open_flyout`: strip only and card exists; same key -> stop leave timer; keep pin across internal close; restore card style; card min/max width reset; add to flyout layout; size = (min(260,area.w), min(hint h, area.h)); position; show; fade-in if animations enabled else opacity 1 (PY:725-755) | RS:2330-2401 | same | PY test_flyout_reparent_pin_and_restore, test_keyboard_...; RS none | See P2 about early return. Rust sets opacity 0 before show when animating; Python sets start value on animation start. |
| `close_flyout`: stop timers+animation; card back to storage + hide; hide flyout; clear active/pinned (PY:775-779) | RS:2423-2440 | same | PY same tests | |
| `_schedule_close`: leave timer 600 ms unless pinned (PY:780-781) | RS:2443-2447 | same | PY test_unpinned_leave_and_hover | |
| `_maybe_close`: skip if pinned/inactive; keep if cursor over tile or flyout; else close (PY:782-787) | RS:2450-2466 | same | PY test_flyout_reparent_pin_and_restore, test_unpinned_leave_and_hover; RS none | Edge-inclusivity of contains: unverified (see list). |
| eventFilter ContextMenu (not from flyout window) -> contextMenuRequested(globalPos), accept (PY:789-792) | not in sidebar.rs; flyout via RS:1662-1672; sidebar body via window handler main.rs:485-490 | intentional-diff | PY test_context_wheel_drag; RS none | Rust: tile/body right-click bubbles to HUD window's own context-menu handler, which shows the same menu; flyout window handled separately. Python explicitly excludes the flyout *window* but cards inside it still emit; Rust flyout handler emits for any click. |
| eventFilter Ctrl+Wheel: accumulate angleDelta.y, emit +/-1 per 120, keep remainder, accept; without Ctrl pass through (PY:793-800) | RS:505-514 `wheel_steps`, 2489-2506 | same | RS ctrl_wheel_notches_accumulate_into_steps; PY test_context_wheel_drag | Scope of filter targets: unverified (see list). Emission is deferred. |
| eventFilter flyout Enter -> stop leave timer; Leave -> schedule_close (PY:801-805) | RS:2507-2514 | same | none | filter on flyout root widget rather than the window. |
| eventFilter Escape key -> close_flyout, consumed (PY:806-808); SidebarWidget.keyPressEvent Escape (PY:811-813) | RS:2515-2519 | same | PY test_keyboard_and_geometry_without_motion; RS none | |
| hideEvent -> close_flyout (PY:814-815) | none in sidebar.rs; hud_window.rs:1018,1133,1146,1962,2278 | missing | none | P2; HUD compensates at known hide points. |
| Public API parity: state(), orientation(), scale(), order(), `_active` (PY:369-376,406) | RS:1710-1729 | same | none | |
| Sidebar never calls card.set_appearance / update_metrics itself (PY test asserts) | RS: no such calls in sidebar.rs; HUD does it hud_window.rs:1193,1839 | same | PY test_themes, test_metrics_and_external_ownership; RS none | |
| Python test `no 'UsageDial' in source`, no `settingsRequested` | n/a | same | PY test_tooltip_countdown_only_and_no_dial_import | Rust file has neither. |

## Counts

Table data rows: 95 (counted by Status column).

- same: 81
- intentional-diff: 4
- diff: 2 (window_reset threshold; set_dock_edge validation)
- missing: 1 (hideEvent -> close_flyout)
- unverified: 7

Additional P2 items listed above without their own diff row: open_flyout early return, `_maybe_close` edge inclusivity, event-filter scope, flyout-center 1 px rounding in `_move_reorder`.


---
# Counts per status (table rows, counted by grep; the usage_table section's own footer tally is unreliable, use these)

| Section | same | intentional-diff | diff | missing | unverified |
|---|---|---|---|---|---|
| core (brand, table_layout, gauge, styles, dock_geometry) | 41 | 6 | 2 | 2 | 3 |
| provider_card | 39 | 5 | 1 | 0 | 4 |
| usage_table | 54 | 3 | 6 | 0 | 9 |
| sidebar | 81 | 4 | 2 | 1 | 7 |
| **Total** | 215 | 18 | 11 | 3 | 23 |
