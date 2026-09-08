# WinShort

WinShort is a native Windows control center for fast audio, workspace, display, and status-overlay shortcuts. It stays quietly in the system tray and uses Rust, Win32, Direct2D, DirectWrite, and Windows APIs — no Electron, WebView, .NET, C++, or GUI framework.

The Control Center is organized around what a person wants to do:

```text
WinShort
├── Home        What is active right now
├── Shortcuts   Everyday actions and hotkey recording
├── Audio       Speakers, microphones, and current-app audio
├── Workspaces  Desktops and Special Desktop
├── Displays    Visual display profiles with safe testing
├── Overlay     Live preview and status-card appearance
└── System      Startup, pause, support, and reset
```

## Features

- **Native Control Center** — a PMv2-aware owner-drawn Direct2D/DirectWrite window with a text-first navigation rail, local search, light/dark themes, High Contrast support, visible keyboard focus, and localized save feedback.
- **System tray presence** — single instance, concise context menu, `TaskbarCreated` resilience, and a pause-shortcuts state reflected in the tray icon.
- **Global shortcuts** — user-definable `WH_KEYBOARD_LL` shortcuts with safe capture mode, canonical conflict validation, AltGr/modifier handling, and immediate atomic persistence without reinstalling the hook.
- **Audio control** — microphone mute, speaker mute, current-app mute, current-app volume ±5%, and actual Windows default-device switching.
- **Audio cycling allowlists** — independently choose the devices used by Next microphone and Next speaker. Disconnected IDs remain configured and become eligible again after reconnect.
- **Status overlay** — per-pixel-alpha, monitor-aware HUD with human-readable speaker, microphone, current-app, and workspace feedback. It never steals focus.
- **Workspaces** — numbered Desktop 1–9 switching, optional move-and-follow and silent move, Previous desktop, and a dedicated Special Desktop excluded from normal ordinals.
- **Display profiles** — visual profile cards, capture/update/rename/duplicate/delete, stable profile-ID shortcuts, documented DisplayConfig validation, and temporary Test profile with explicit Keep, Revert, and automatic 15-second rollback.
- **Overlay settings** — System/Light/Dark appearance, position, monitor, Small/Normal/Large size, Low/Normal/High opacity, Short/Normal/Long duration, and non-persistent draft Preview.
- **Diagnostics & support** — separate native technical status page, passive Self-Test, Unicode diagnostics copy, direct log-folder opening, and a bounded sanitized support ZIP. No upload or telemetry.
- **Event-driven idle** — no polling loops; device, desktop, foreground, theme, and runtime updates are event-driven.

## Default shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Alt+M` | Toggle microphone mute |
| `Ctrl+Alt+O` | Toggle speaker mute |
| `Ctrl+Alt+P` | Toggle current-app audio |
| `Win+1`…`Win+9` | Switch to normal desktop 1–9 |

Optional device-cycle, current-app-volume, move/follow, Previous desktop, Special Desktop, and display-profile shortcuts are configurable from **Shortcuts**. The Special Desktop is a real dedicated Windows Virtual Desktop, kept outside the numbered ordinals. Its exact identity is persisted only as operational recovery state so a desktop that survives a hard kill or reboot can be reclaimed instead of duplicated; orderly exit or feature disable removes it.

Windows shortcuts such as `Win+E`, `Win+R`, `Win+D`, `Win+L`, and `Win+Shift+S` pass through untouched.

## Build

Requirements: **Rust 1.85+** (edition 2021 toolchain), Windows 10/11.

```powershell
cargo build --release
```

The binary lands at `target/release/winshort.exe` as a GUI subsystem executable. Run it — a tray icon appears; open it from the tray to launch the WinShort Control Center.

## Install

1. Run WinShort once.
2. Open **System** and choose **Start WinShort with Windows** if desired. This registry-backed choice applies immediately without elevation or a service.
3. Configure shortcuts, audio devices, displays, and overlay behavior as needed.

A concise first-run flow appears only when there is no existing WinShort configuration. Existing users are never reset or re-onboarded merely because the Control Center was added.

Configuration lives at `%LOCALAPPDATA%\WinShort\config.toml`; logs live at `%LOCALAPPDATA%\WinShort\logs\`. The UI-state onboarding marker is `%LOCALAPPDATA%\WinShort\control-center-ui-state.txt` and is separate from configuration. See [docs/CONFIG_SCHEMA.md](docs/CONFIG_SCHEMA.md).

## Virtual desktop compatibility

The documented `IVirtualDesktopManager` cannot switch desktops, so WinShort uses the build-pinned `IVirtualDesktopManagerInternal` layout only on known Windows builds and fails closed elsewhere:

| Builds | Backend |
|---|---|
| 26100 (24H2), 26200–26299 (25H2) | Native Shell COM — absolute switching, enumeration, Special Desktop |
| Anything else | Keyboard fallback for safe existing numbered-desktop switching |

Special Desktop creation, movement, and GUID-addressed navigation require the native backend; unsupported builds never simulate those actions with synthetic input. Details and verified behavior are documented in [docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md).

## Documentation

| File | Contents |
|---|---|
| [docs/UI_DESIGN.md](docs/UI_DESIGN.md) | Control Center shell, pages, local save model, search, onboarding, UIA, overlay, and accessibility |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Subsystems, threading model, event flow, diagnostics, and startup/shutdown order |
| [docs/WIN32_LIFETIME.md](docs/WIN32_LIFETIME.md) | Handle/COM ownership, provider lifetime, popup teardown, and shutdown ordering |
| [docs/KEYBOARD_HOOK_DESIGN.md](docs/KEYBOARD_HOOK_DESIGN.md) | Hook rules, suppression, capture, and injected input |
| [docs/AUDIO_DESIGN.md](docs/AUDIO_DESIGN.md) | Devices, notifications, Windows defaults, current-app audio, and cycling |
| [docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md) | Interface compatibility, Special Desktop recovery, and fallback guarantees |
| [docs/CONFIG_SCHEMA.md](docs/CONFIG_SCHEMA.md) | `config.toml` fields, migrations, validation, and future-schema safety |
| [docs/TEST_PLAN.md](docs/TEST_PLAN.md) | Unit/property matrices and live procedures |

## Development

Fast development checks can be run directly:

```powershell
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Before merge or release, run the canonical Windows local gate:

```powershell
.\tools\final_validation.ps1
```

The gate adds the x86_64 release build and embedded manifest/icon checks, the release nullable-UIA ABI regression, i686 and aarch64 compile checks, Rust 1.85 MSRV, `cargo deny check`, machine UI acceptance, and a visual-sanity capture. The generated visual sheet still needs human review. Pushes and pull requests intentionally do not use GitHub-hosted CI; tag/manual release automation remains separate.

## Privacy and support

WinShort never logs raw keystrokes. Diagnostics exports exclude raw key history, window titles, command lines, unrelated process identity, and raw endpoint IDs. Paths are reduced to safe profile tokens or basename plus report-local path tokens; endpoint IDs receive report-local pseudonyms. Support bundles contain only sanitized diagnostics, sanitized config, the newest three bounded log files, and a manifest. No network access, upload, or telemetry is performed.

## License

MIT — see [LICENSE](LICENSE).

## Support matrix

| Architecture | Compile / local gate | Release artifact |
|---|---|---|
| x86_64 | yes | yes |
| i686 | yes | yes |
| aarch64 | compile-check only | no |

Windows requirements: Windows 10/11. Virtual desktop integration is pinned to specific build families; see [docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md). MSRV: Rust 1.85 (`rust-version` in Cargo.toml).