# Claude HUD Monitor — Rust Port

A complete Rust rewrite of the [Python original](../claude-hud-monitor-main), using [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui) for the GUI.

## Architecture

```
main.rs
  ├── config.rs           — ConfigManager (JSON, atomic save, platform paths)
  ├── providers/
  │   ├── base.rs         — Provider trait, UsageMetrics, helpers
  │   ├── claude.rs       — Claude Code (Anthropic OAuth API)
  │   ├── agy.rs          — Antigravity CLI (subprocess + JSON)
  │   └── codex.rs        — OpenAI Codex (WHAM API)
  ├── refresh_controller.rs — Per-provider scheduling, backoff, stale data
  ├── ui/
  │   ├── hud_app.rs      — eframe::App (window, layout, context menu)
  │   ├── provider_card.rs — Card rendering (egui immediate-mode)
  │   └── styles.rs       — Color constants, egui visual config
  ├── hotkey.rs           — Win32 RegisterHotKey (Alt+C / Alt+Shift+C)
  ├── autostart.rs        — Windows registry / macOS LaunchAgent
  └── logger.rs           — env_logger setup, open log directory
```

## Features

| Feature | Python (PySide6) | Rust (egui) |
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
- Linux: `libgtk-3-dev` for tray icon

### Dev build

```powershell
cargo build
```

### Release build

```powershell
cargo build --release
# Executable: target/release/claude-hud-monitor.exe
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
- **egui immediate-mode** — no widget tree state, simpler rendering model
- **Tokio not used** — blocking reqwest + `std::thread` (matches Python threading model)

## Known Limitations

- Click-through ghost mode is fully supported on Windows (`WS_EX_TRANSPARENT` + layered hit-test passthrough with ghost indicator); macOS/Linux click-through passthrough is currently layout/docking based
- macOS hotkeys not yet implemented (use tray menu as fallback)
- Linux tray icon requires libappindicator
