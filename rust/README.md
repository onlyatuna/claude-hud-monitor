# Claude HUD Monitor — Rust Port

A complete Rust rewrite of the [Python original](../python), using the in-repo [qtrs](qtrs/) toolkit (Qt/PySide6-semantics crates) for the GUI.

Parity status document: [`qtrs/PYTHON_QT_SEMANTIC_CONTRACT.md`](qtrs/PYTHON_QT_SEMANTIC_CONTRACT.md).

## Architecture

```
main.rs                  — entry point, window / tray / hotkey wiring
config.rs                — ConfigManager (JSON, atomic save, platform paths)
refresh_controller.rs    — per-provider scheduling, backoff, stale data
hotkey.rs                — Win32 RegisterHotKey (Alt+C / Alt+Shift+C)
autostart.rs             — Windows registry / macOS LaunchAgent
logger.rs                — logging setup, open log directory
memory.rs                — process working-set trimming
pace.rs                  — time and pace helpers for the usage table
providers/
  mod.rs                 — provider registry
  base.rs                — UsageMetrics, Provider trait, helpers
  claude.rs              — Claude Code (Anthropic OAuth API)
  agy.rs                 — Antigravity CLI (subprocess + JSON)
  codex.rs               — OpenAI Codex (WHAM API)
  stub.rs                — test-only stub provider
ui/
  mod.rs                 — UI module root
  hud_window.rs          — main HUD window (header, cards/table stack, ghost mode)
  provider_card.rs       — provider metric card
  usage_table.rs         — usage table view
  styles.rs              — style constants
  tray_icon.rs           — system tray icon
  placement.rs           — window placement
bin/inspect_hwnd.rs      — auxiliary binary (HWND inspection)
qtrs/                    — in-repo Qt-semantics GUI toolkit
  crates/                — qtrs-core, qtrs-gui, qtrs-widgets, qtrs-platform, qtrs-derive
```

## Features

| Feature | Python (PySide6) | Rust (qtrs) |
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
- Linux: X11/GL dev packages for the qtrs platform layer, as installed in `.github/workflows/release.yml`: `libxkbcommon-dev libx11-dev libxrandr-dev libxcursor-dev libxi-dev libxinerama-dev libgl1-mesa-dev libgles2-mesa-dev pkg-config`

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
- **qtrs widget model** — Qt-semantics widget tree and signals (see the parity contract) rather than egui immediate mode
- **Blocking HTTP** — `ureq` + `std::thread`, no Tokio (matches Python threading model)

## Known Limitations

- Click-through ghost mode is fully supported on Windows (`WS_EX_TRANSPARENT` + layered hit-test passthrough with ghost indicator); macOS/Linux click-through passthrough is currently layout/docking based
- macOS hotkeys not yet implemented (use tray menu as fallback)
- Linux tray icon requires libappindicator
