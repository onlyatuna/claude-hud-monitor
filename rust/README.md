# Claude HUD Monitor — Rust Port

A complete Rust rewrite of the [Python original](../python), using the [ferrokit](https://github.com/onlyatuna/ferrokit) toolkit (a Rust implementation of a subset of Qt®, pinned as a git dependency) for the GUI.

Parity status document: [`docs/qt_parity/PYTHON_QT_SEMANTIC_CONTRACT.md`](docs/qt_parity/PYTHON_QT_SEMANTIC_CONTRACT.md).

## Architecture

```
main.rs                  — entry point, window / tray / hotkey wiring
config.rs                — ConfigManager (JSON, atomic save, platform paths)
refresh_controller.rs    — per-provider scheduling, backoff, stale data
hotkey.rs                — global hotkeys via ferrokit GlobalHotkeyManager (Alt+C / Alt+Shift+C)
autostart.rs             — Windows registry / macOS LaunchAgent
logger.rs                — logging setup, open log directory
memory.rs                — process working-set trimming
pace.rs                  — time and pace helpers for the usage table
agy_rows.rs              — agy row order (agy_order / row_order), port of core/agy_rows.py
providers/
  mod.rs                 — provider registry
  base.rs                — UsageMetrics, Provider trait, helpers
  claude.rs              — Claude Code (Anthropic OAuth API)
  agy.rs                 — Antigravity CLI (subprocess + JSON)
  codex.rs               — OpenAI Codex (WHAM API)
  stub.rs                — test-only stub provider
ui/
  mod.rs                 — UI module root
  hud_window.rs          — main HUD window (header, cards/table/sidebar stack, ghost mode, sidebar docking, snap, resize, idle fade)
  sidebar.rs             — sidebar style: strip / panel / flyout tiles (port of ui/sidebar.py)
  dock_geometry.rs       — pure dock / snap / scale geometry (port of ui/dock_geometry.py)
  dock_preview.rs        — translucent outline of where the dragged sidebar will dock (port of ui/dock_preview.py)
  gauge.rs               — green-to-red gauge colours and GaugeBar (port of ui/gauge.py)
  brand.rs               — service identity colours
  provider_card.rs       — provider metric card (gauge bars, drag-reorderable rows)
  usage_table.rs         — usage table view
  styles.rs              — style constants
  tray_icon.rs           — system tray icon
  placement.rs           — window placement
bin/inspect_hwnd.rs      — auxiliary binary (HWND inspection)
docs/qt_parity/         — Qt parity contract and specs
tests/                  — HUD-specific comparison tests against PySide6 and their recorded data
tools/                  — PySide6 probes, measurement and audit scripts
```

## Sidebar style

`ui_mode = "sidebar"` is the default and is selectable from the UI Style menu next to `table` (the Classic Cards style is retired, as in the Python HUD; an old `ui_mode = "cards"` file migrates to the sidebar panel). The bar docks to a screen edge (`sidebar_edge`, `sidebar_offset`, or a free `sidebar_x`/`sidebar_y` when the edge is `none`), snaps to edges and corners when dragged, resizes by dragging any edge or corner or with Ctrl+wheel (`sidebar_scale`, 0.6–2.0), shows tiles (`strip`) or full cards (`panel`) per `sidebar_state`, opens a flyout on hover or click, and lets you reorder tiles (press-hold and drag; `provider_order`) and card rows (drag a row; `row_order`, `agy_order`). While idle it dims to `sidebar_idle_level` of the chosen opacity after `sidebar_idle_delay_ms` (`sidebar_idle_fade`). The keys, their names and defaults are those of the Python `config.json`, so the two builds share one file.

While the bar is dragged within 24 px of a screen edge or corner, a translucent outline (`ui/dock_preview.rs`: a frameless, click-through, non-activating window; topmost only when `always_on_top`) shows the rectangle, size and orientation the bar takes when the drag ends. It is computed by the same `plan_sidebar_snap` / `dock_to_edge` steps `finish_sidebar_move` uses, so it cannot differ from the result; it is hidden when the drag ends, leaves the threshold, the bar is locked, in the table style, or during an edge/corner resize. Fill alpha 46, border alpha 235 (of 255), 2 px border, sidebar focus colour of the current theme, corner radius = sidebar radius × scale; the Python HUD uses the same values.

The outline is an unowned top-level window (Windows keeps an owned window above its owner) in the same stacking band as the HUD (`always_on_top`), and `DockPreview::stack_below` puts it directly below the HUD after every update with ferrokit's `Window::stack_under`, so the dragged bar always stays over it; it never takes the focus.

On release the bar glides (not jumps) onto that rectangle: 140 ms cubic ease-out in 16 ms frames (`SNAP_ANIM_MS`, `SNAP_ANIM_FRAME_MS`, `interpolate_rect` in `ui/dock_geometry.rs`; position and size, so an orientation flip is included). The last frame is the plain docking call, so the final geometry and the saved config are the same as without it, and the config is written once, at the release. There is no glide when the target is the current rectangle, the window is hidden, or the system reduces motion. A mouse press, a move that is not the glide's own, an edge/corner resize or a style switch cancels it and the window stays where it is; a lock or a hide places the bar on its docked rectangle at once.

## Features

| Feature | Python (PySide6) | Rust (ferrokit) |
|---------|-----------------|-------------|
| Claude Code usage | ✅ | ✅ |
| Antigravity usage | ✅ | ✅ |
| OpenAI Codex usage | ✅ | ✅ |
| Vertical / Horizontal layout | ✅ | ✅ |
| Click-through ghost mode | ✅ | ✅ (Win32) |
| Global hotkeys (Alt+C) | ✅ | ✅ (Windows) |
| System tray icon | ✅ | ✅ |
| Autostart (Win/macOS) | ✅ | ✅ |
| Per-provider exponential backoff | ✅ | ✅ |
| Stale data preservation | ✅ | ✅ |
| Sleep/resume detection | ✅ | ✅ |
| Config persistence (atomic JSON) | ✅ | ✅ |
| CJK font support | ✅ | ✅ (msjh.ttc) |

## Building

### Prerequisites

- Rust 1.75+ (`rustup install stable`)
- Windows: Visual Studio Build Tools (MSVC target)
- macOS: Xcode Command Line Tools
- Linux: X11/GL dev packages for the ferrokit platform layer, as installed in `.github/workflows/release.yml`: `libxkbcommon-dev libx11-dev libxrandr-dev libxcursor-dev libxi-dev libxinerama-dev libgl1-mesa-dev libgles2-mesa-dev pkg-config`

### Dev build

```powershell
cargo build
```

### Release build

```powershell
cargo build --release
# Executable: target/release/ClaudeHUD.exe
```

## Configuration

Config file location:
- **Windows**: `%APPDATA%\ClaudeHUDMonitor\config.json`
- **macOS**: `~/Library/Application Support/ClaudeHUDMonitor/config.json`
- **Linux**: `~/.config/ClaudeHUDMonitor/config.json`

Default values match the Python original exactly.

Tray number (both HUDs read the same two keys):

- `tray_quota`: `"off"` (default, the plain app icon), `"auto"` (the quota with the highest used percentage), or the key of one quota row. A key is the row's key prefixed with its provider unless it already is: `claude:5h`, `claude:7d`, `claude:weekly_scoped:fable`, `codex:WINDOW 5H`, `agy:C/G 5H`. A missing or non-text value is `"off"`; a key that matches no current row (for example a model scope that went away) is kept and shown as unavailable (`--`), never replaced.
- `tray_quota_mode`: `"used"` (default) or `"remaining"`. The number is whole percent without the `%`, at most two digits (a full quota shows `99` when counting used, `0` when counting remaining). Remaining mode adds a bar under the digits. Any other value is `"used"`.

The number is coloured like the gauge (green, yellow, orange, red by used percentage), amber while the last refresh failed and the previous value is kept, grey when the value is older than 30 minutes or missing, `!` in red when the provider failed with nothing to show. The tray menu's 系統匣數字 (Tray number) submenu chooses the quota and the mode; the tooltip gets a first line such as `Claude FABLE 7D: 84% used`.

AGY dynamic rows (both HUDs): a quota bucket the HUD has no fixed row for becomes an extra row on its own, shown after the five fixed ones (`GEMINI 5H`, `GEMINI 7D`, `C/G 5H`, `C/G 7D`, `ARGON 7D`). Sources are the status line cache (`agy-quota.json`) keys other than the five known ones, and the buckets of summary groups that are not Gemini, Claude/GPT or Argon; the per-model buckets of `retrieveUserQuota` never become rows. The title comes from the key (`gemini-carbon-wtu-weekly` is `CARBON 7D`; a key of another shape is its upper-cased self with `-` as spaces, so no row is lost) or from the bucket's display name. The row key, and so the `tray_quota` value, is `agy:<slug of the title>` (`agy:carbon-7d`). A dynamic title in a saved `agy_order` is kept while the row exists and dropped otherwise; fixed-only orders behave as before.

### Two Claude accounts at once (sidebar)

- `accounts` (list, absent by default): extra Claude accounts, `{"id": "claude@<slug>", "provider": "claude", "profile": "<profile id>", "nickname": "<optional>"}`. The id is generated once when you tick 更多 → Claude 帳號 → 同時顯示 (Also show) and then kept; the default profile and a profile the main tile is pinned to cannot be added; at most 6. Invalid entries are ignored, never rewritten. Row keys of an account are `claude@<slug>:5h`, `claude@<slug>:weekly_scoped:fable`, ...; a stored `claude:5h` still means the main tile.
- `sidebar_tiles` (list, absent by default): the visible sidebar tiles in order (`claude`, `claude@<slug>`, `agy`, `codex`). Absent = every tile in `provider_order`, the accounts after `claude`. Drag a tile in the strip to reorder (writes this list and the base-id `provider_order`); 介面 → 側邊欄項目 (Sidebar tiles) shows or hides one. Hiding a tile does not stop its fetching.
- The main Claude tile stays on `claude_profile`: pin it (`auto` flips between profiles). The tile text of an account is its directory name without the leading dot, cut to 8 characters (`claude-a` fits); the panel card header is the label only, cut to 13 characters (it shares the line with two badges), menu entries 16; the full name is in the tooltip. Both keys apply live, no restart. The table style ignores them.

## Credential Files

| Provider | File |
|----------|------|
| Claude Code | `~/.claude/.credentials.json` |
| Antigravity | `agy` binary on PATH |
| OpenAI Codex | `~/.codex/auth.json` |

## Hotkeys

| Keys | Action |
|------|--------|
| `Alt+C` | Toggle HUD visibility |
| `Alt+Shift+C` | Toggle click-through ghost mode |

## Differences from Python Version

- **No Python/Qt runtime required** — single native executable (~5–15 MB release)
- **Lower memory** — ~30–50 MB vs ~100–200 MB for PySide6
- **Faster startup** — instant vs 1–3s for Python
- **ferrokit widget model** — Qt-semantics widget tree and signals (see the parity contract) rather than egui immediate mode
- **Blocking HTTP** — `ureq` + `std::thread`, no Tokio (matches Python threading model)

## Known Limitations

- **Supported platform: Windows x64 only.** macOS and Linux builds are experimental: they compile and run the CI tests, but the HUD has never been run on them, and the macOS native input, hotkey, clipboard and dialog code has only been type-checked and mock-tested, never run on a Mac
- Click-through ghost mode is fully supported on Windows (`WS_EX_TRANSPARENT` + layered hit-test passthrough with ghost indicator); macOS/Linux click-through passthrough is currently layout/docking based
- macOS hotkeys are implemented (Carbon `RegisterEventHotKey`) but untested on a real Mac; the tray menu is the fallback
- Linux tray icon requires libappindicator
