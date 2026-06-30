# VibeGE Runtime

The native desktop runtime for the VibeGE gaming overlay — the companion application for AI-assisted software development.

Press a hotkey, play a game, return to coding. No Alt-Tab, no browser, no distractions.

## Features

- **Overlay Mode** — Always-on-top window, toggle with Ctrl+Shift+V (configurable)
- **Native Rendering** — GPU-accelerated 2D via wgpu (Vulkan, Metal, DX12)
- **Native Audio** — Low-latency playback via rodio
- **System Tray** — Right-click menu for overlay controls
- **Game Library** — Browse and launch installed games from the overlay
- **Game Store** — Browse the VibeGE registry and install games directly
- **Settings** — Configure hotkey, position, startup behaviour, audio, and more
- **First Run Wizard** — Guided onboarding on first launch
- **Background Mode** — Starts in tray, zero distraction until needed

## Quick Start

```bash
# Run the runtime (opens in background, tray only)
./vibege-runtime.exe

# Run with window visible
./vibege-runtime.exe --show

# Run as overlay (always-on-top)
./vibege-runtime.exe --overlay --show
```

## Architecture

The runtime is structured as 11 independent crates in a Cargo workspace:

| Crate | Purpose |
|-------|---------|
| `vibege-core` | Foundation: errors, events, metrics, lifecycle |
| `vibege-input` | Keyboard, mouse, and gamepad input |
| `vibege-renderer` | GPU-accelerated 2D rendering (wgpu) |
| `vibege-audio` | Audio playback (rodio) |
| `vibege-config` | Player configuration (`~/.vibege/config.toml`) |
| `vibege-sdk` | Official game development SDK |
| `vibege-scene` | Scene Manager and platform scenes |
| `vibege-window` | Window management (winit) |
| `vibege-suspension` | Game state save/restore |
| `vibege-tray` | System tray icon (Windows) |
| `vibege-runtime-app` | Main binary entry point |

## Platform Scenes

The runtime uses a stack-based Scene Manager. Current scenes:

- **BootScene** — Loads config, detects first-run, routes correctly
- **FirstRunScene** — 7-step onboarding wizard
- **HomeScene** — Game library with keyboard shortcuts
- **LibraryScene** — Installed games with metadata and update detection
- **StoreScene** — Game store with backend integration
- **SettingsScene** — 6-panel settings overlay
- **GameScene** — Isolated Lua VM per game session

## Development

```bash
# Build all crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Build release binary
cargo build --release --workspace -p vibege-runtime-app
```

## License

MIT
