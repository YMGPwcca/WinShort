# WinShort

Native Windows tray utility for audio control and virtual desktop switching.
Pure Rust against Win32/COM — no Electron, WebView, .NET, C++, or GUI frameworks.

```
┌──────────────────────────────────────────┐
│  WinShort                          ─ □ × │
│                                          │
│  General                                 │
│  ┌────────────────────────────────────┐  │
│  │ Start with Windows            [on] │  │
│  │ Start hotkeys enabled         [on] │  │
│  └────────────────────────────────────┘  │
│  Hotkeys                                 │
│  ┌────────────────────────────────────┐  │
│  │ Microphone          Ctrl+Alt+M     │  │
│  │ Output              Ctrl+Alt+O     │  │
│  │ Current app         Ctrl+Alt+P     │  │
│  └────────────────────────────────────┘  │
│  Audio · Virtual Desktops · Overlay      │
│                                          │
│  Everything is up to date   Cancel  Save │
└──────────────────────────────────────────┘
```

## Features

- **System tray presence** — single instance, context menu, `TaskbarCreated` resilience,
  suspend-hotkeys state reflected in the icon.
- **Global hotkeys** — user-definable `WH_KEYBOARD_LL` shortcuts, editable live from Settings,
  applied instantly on Save without reinstalling the hook.
- **Audio control** — microphone mute, output mute, and foreground-application session mute
  (all sessions of the owning PID toggle together; mixed state mutes all).
- **Status overlay** — per-pixel-alpha HUD: topmost, no-activate, click-through, fade/slide
  animation, monitor-aware placement. Never steals focus from games.
- **Virtual desktop switching** — `Win+1`…`Win+9` jump directly to Desktop 1–9 via the
  undocumented Shell COM interface (build-pinned to Windows 11 24H2/25H2) with a
  best-effort `Ctrl+Win+Arrow` fallback when the native backend is unavailable.
- **Hot-reload config** — typed TOML model, atomic writes, inline validation, draft/live
  separation. Cancel always restores the live snapshot.
- **Event-driven idle** — no polling loops; idle CPU ≈ 0%.

## Default shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Alt+M` | Toggle microphone mute |
| `Ctrl+Alt+O` | Toggle output mute |
| `Ctrl+Alt+P` | Toggle foreground app audio |
| `Win+1`…`Win+9` | Switch to virtual desktop 1–9 |

`Win+E`, `Win+R`, `Win+D`, `Win+L`, `Win+Shift+S` and every other Windows shortcut pass
through untouched.

## Build

Requirements: **Rust 1.85+** (edition 2021 toolchain tested with 1.96), Windows 10/11.

```powershell
cargo build --release
```

The binary lands at `target/release/winshort.exe` (GUI subsystem, no console window,
~700 KB). Run it — a tray icon appears; double-click opens Settings.

## Install

1. Copy `winshort.exe` anywhere user-writable (e.g. `%LOCALAPPDATA%\WinShort\`).
2. Run it once, open Settings → General → **Start with Windows** → Save
   (writes the `HKCU\...\Run` entry; no elevation, no service).
3. Optional: tweak hotkeys, overlay position/duration, device roles.

Configuration lives at `%LOCALAPPDATA%\WinShort\config.toml`; logs in
`%LOCALAPPDATA%\WinShort\logs\`. See [docs/CONFIG_SCHEMA.md](docs/CONFIG_SCHEMA.md).

## Virtual desktop compatibility

The documented `IVirtualDesktopManager` cannot switch desktops, so WinShort uses the
undocumented `IVirtualDesktopManagerInternal` interface — **pinned per OS build** and
fail-closed on unknown layouts:

| Builds | Backend |
|---|---|
| 26100 (24H2), 26200–26299 (25H2) | Native Shell COM — absolute switching, enumeration |
| anything else | Keyboard fallback (`Ctrl+Win+Arrow` walking, best effort) |

The Settings → Advanced row always shows the active backend and reason. Details and
test evidence: [docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md).

## Documentation

| File | Contents |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Subsystems, threading model, event flow, startup/shutdown order |
| [docs/WIN32_LIFETIME.md](docs/WIN32_LIFETIME.md) | Handle/COM ownership rules, shutdown ordering, failure handling |
| [docs/KEYBOARD_HOOK_DESIGN.md](docs/KEYBOARD_HOOK_DESIGN.md) | Hook rules, suppression, Win-key state machine, injected input |
| [docs/AUDIO_DESIGN.md](docs/AUDIO_DESIGN.md) | Endpoints, notifications, foreground-session semantics, edge cases |
| [docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md) | Interface GUIDs/vtables per build, tested results, fallback guarantees |
| [docs/UI_DESIGN.md](docs/UI_DESIGN.md) | Design tokens, widget set, overlay behavior, quality gate |
| [docs/CONFIG_SCHEMA.md](docs/CONFIG_SCHEMA.md) | `config.toml` reference and validation rules |
| [docs/TEST_PLAN.md](docs/TEST_PLAN.md) | Unit matrices and live test procedures |

## Development

```powershell
cargo test            # 31 unit tests (keyboard engine, config, bindings — Windows-free core)
cargo check           # fast type check
cargo build --release # ship binary
```

The keyboard engine, binding parser, config model, and validation are pure logic with no
`windows` imports, so the test suite runs anywhere. Live integration procedures
(audio endpoints, desktop switching, overlay focus behavior) are documented in the test plan.

## Privacy

WinShort never logs keystrokes. Only recognized configured shortcuts are logged, and only
when diagnostic level is enabled. No network access, no telemetry.

## License

MIT — see [LICENSE](LICENSE).
