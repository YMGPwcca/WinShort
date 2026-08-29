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
- **Audio cycling allowlists** — independently select stable input/output endpoints;
  disconnected IDs remain configured and re-enter the cycle after reconnect.
- **Status overlay** — per-pixel-alpha HUD: topmost, no-activate, click-through, fade/slide
  animation, monitor-aware placement. Never steals focus from games.
- **Virtual desktop workflow** — configurable numbered switching creates only missing
  desktops through the build-pinned Shell COM backend; optional move/follow, silent
  move, previous-desktop focus restoration, executable routing, and runtime scratchpad
  show/hide actions stay disabled or unassigned by default.
- **Display profiles** — capture and persist documented Windows DisplayConfig topologies,
  resolve stable monitor paths, and apply with an explicit 15-second Undo window.
- **Hot-reload config** — typed TOML model, atomic writes, inline validation, draft/live
  separation. Cancel always restores the live snapshot.
- **Diagnostics & support** — separate native status page, passive self-test, Unicode
  diagnostics copy, direct log-folder opening, and a bounded sanitized support ZIP. No upload
  or telemetry.
- **Event-driven idle** — no polling loops; idle CPU ≈ 0%.

## Default shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Alt+M` | Toggle microphone mute |
| `Ctrl+Alt+O` | Toggle output mute |
| `Ctrl+Alt+P` | Toggle foreground app audio |
| `Win+1`…`Win+9` | Switch to virtual desktop 1–9 |

Optional desktop workflow and scratchpad shortcuts are unassigned by default and can be
configured in Settings; the scratchpad assignment is runtime-only and is forgotten when
the window closes.

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

2. Run it once, open Settings → General → **Start with Windows**, and toggle it on
   (the registry-backed setting applies immediately; no elevation, no service).
3. Optional: tweak hotkeys, overlay position/duration, device roles.

Configuration lives at `%LOCALAPPDATA%\WinShort\config.toml`; logs in
`%LOCALAPPDATA%\WinShort\logs\`. See [docs/CONFIG_SCHEMA.md](docs/CONFIG_SCHEMA.md).
Logging defaults to **Info** in release builds and **Debug** in debug builds. Advanced
Settings exposes temporary Debug logging until restart; it is not persisted in
`config.toml`. Daily logs use a 14-day retention window, buffered normal writes,
Warn/Error flushes, a bounded five-second dirty flush, and synchronous panic records.

## Virtual desktop compatibility

The documented `IVirtualDesktopManager` cannot switch desktops, so WinShort uses the
undocumented `IVirtualDesktopManagerInternal` interface — **pinned per OS build** and
fail-closed on unknown layouts:

| Builds | Backend |
|---|---|
| 26100 (24H2), 26200–26299 (25H2) | Native Shell COM — absolute switching, enumeration |
| anything else | Keyboard fallback (`Ctrl+Win+Arrow` walking, best effort) |

The Settings → Advanced area opens Diagnostics & Support; the diagnostics page shows the active
backend and reason. Details and test evidence: [docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md).

## Documentation

| File | Contents |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Subsystems, threading model, event flow, diagnostics, startup/shutdown order |
| [docs/WIN32_LIFETIME.md](docs/WIN32_LIFETIME.md) | Handle/COM ownership rules, support worker/clipboard lifetime, shutdown ordering |
| [docs/KEYBOARD_HOOK_DESIGN.md](docs/KEYBOARD_HOOK_DESIGN.md) | Hook rules, suppression, Win-key state machine, injected input |
| [docs/AUDIO_DESIGN.md](docs/AUDIO_DESIGN.md) | Endpoints, notifications, foreground-session semantics, edge cases |
| [docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md) | Interface GUIDs/vtables per build, tested results, fallback guarantees |
| [docs/UI_DESIGN.md](docs/UI_DESIGN.md) | Settings and Diagnostics UI, design tokens, widget set, overlay behavior |
| [docs/CONFIG_SCHEMA.md](docs/CONFIG_SCHEMA.md) | `config.toml` reference and validation rules |
| [docs/TEST_PLAN.md](docs/TEST_PLAN.md) | Unit matrices, diagnostics privacy tests, and live procedures |

## Development

```powershell
cargo test            # full suite (deterministic unit + property tests; Windows-free core)
cargo build --release # ship binary
```

Hosted CI additionally verifies fmt, clippy `-D warnings`, the x86_64 release build with an
embedded-manifest check, i686 + aarch64 compile checks, an MSRV 1.85 job, and cargo-deny —
see `.github/workflows/ci.yml`. Tagging `vX.Y.Z` packages signed-ready x86_64/i686 ZIPs with
SHA256SUMS via `.github/workflows/release.yml`.

The keyboard engine, binding parser, config model, and validation are pure logic with no
`windows` imports, so the test suite runs anywhere. Live integration procedures
(audio endpoints, desktop switching, overlay focus behavior) are documented in the test plan.

## Privacy and support

WinShort never logs raw keystrokes. Only recognized configured shortcuts are logged, and only
when diagnostic level is enabled. Diagnostics exports exclude raw key history, window titles,
command lines, unrelated process identity, and raw endpoint IDs. Paths are reduced to safe
profile tokens or basename + report-local path tokens; endpoint IDs receive report-local
pseudonyms. Support bundles are created under `%LOCALAPPDATA%\WinShort\Support` and include
only sanitized diagnostics, sanitized config, the newest three bounded log files, and a manifest.
No network access, upload, or telemetry is performed.

## License

MIT — see [LICENSE](LICENSE).

## Support matrix

| Architecture | Compile / CI | Release artifact |
|--------------|--------------|------------------|
| x86_64       | yes          | yes              |
| i686         | yes          | yes              |
| aarch64      | compile-check only | no         |

Windows requirements: Windows 10 (24H2-era virtual desktop ABI) and Windows 11.
The virtual-desktop integration is pinned to specific build families — see
[docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md).
MSRV: Rust 1.85 (`rust-version` in Cargo.toml, verified by the MSRV CI job).
