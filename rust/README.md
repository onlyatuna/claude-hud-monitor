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
