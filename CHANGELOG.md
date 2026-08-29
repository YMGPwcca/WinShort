# Changelog

All notable changes to WinShort are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- Hardened the custom Settings UI Automation provider: unsupported patterns,
  navigation boundaries, outside point queries, and logical child host providers
  now return successful null results where required; RuntimeIds and root host
  ownership follow Win32 fragment contracts.
- Exposed picker and hotkey rows as Button/Invoke actions with a read-only current
  ValuePattern; read-only writes return `UIA_E_INVALIDOPERATION`.
- Tracked Settings, native picker, and outside focus ownership and filtered UIA
  property/focus events to actual state changes.
- Deferred UIA event delivery beyond the SettingsUi borrow, coalesced typed
  notifications, split root/node provider identities, raised Invoke events, and
  staged native picker activation after HWND registration.
- Distinguished disabled, stale, unsupported, and invalid-argument provider
  operations using their dedicated UI Automation HRESULTs.
- Replaced the unsound nullable `windows-rs` Interface construction in the
  Settings UI Automation provider with raw ABI vtable overrides; optimized
  nullable-boundary coverage now runs in both local and hosted release tests.
- Closing Settings now cancels and hides active picker surfaces before the
  parent window hides, and parent scrolling dismisses open pickers instead of
  leaving them detached from their rows.
- Focus repair prevents Save or dynamically disabled controls from remaining
  logically focused after a model change.
- Bounded Settings value text uses DirectWrite character trimming and reserves
  the dropdown chevron area; the applied footer uses generic "Changes applied"
  status copy.
- Associated every published config revision with its commit origin in one
  coherent live stamp, so audio preflight drift preserves DeviceCycle
  provenance and delayed ConfigChanged notifications remain stale-safe.
- Audio endpoint rebuilds now own one `ConfigSnapshot` for Capture and Render,
  preventing a newer publication from entering a rebuild already planned for
  an older revision.

### Added

- Four optional Phase-1 hotkeys: cycle the Windows input/output defaults and
  adjust foreground application session volume by ±5%.
- Schema v4 adds one configurable numbered-desktop modifier family plus
  optional move/follow, silent-move, and previous-desktop actions without
  stealing existing hotkey slots. Explicit Settings endpoint selections remain
  persistently saved; desktop workflow hotkeys remain unassigned by default.
- Numbered Virtual Desktop switching now ensures missing desktops through the
  native Shell backend; move/follow, silent move, previous-desktop navigation,
  and per-desktop foreground restoration are available as conservative
  configurable actions.

## [0.1.0] - 2026-08-24

Initial release.

### Added

- Tray presence: single instance (named mutex + activation event), context menu
  (Open Settings / Show Status / Suspend Hotkeys / Start with Windows / Exit),
  `NOTIFYICON_VERSION_4`, `TaskbarCreated` resilience, runtime-rendered D2D icons.
- Settings window: owner-drawn Fluent 2 surface (Direct2D + DirectWrite + DWM dark
  mode/rounded corners), scrollable sections (General, Hotkeys, Audio, Virtual
  Desktops, Overlay, Advanced), hover/press/focus states, keyboard navigation,
  per-monitor DPI awareness.
- Keybind recorder: click-to-capture, live modifier display, Esc to cancel,
  inline conflict detection before Save.
- Configuration: typed TOML model at `%LOCALAPPDATA%\WinShort\config.toml`,
  atomic save (temp + rename), field validation with inline errors, draft/live
  snapshot separation, hot reload on Save without restart.
- Keyboard engine: `WH_KEYBOARD_LL` hook on a dedicated message-loop thread;
  exact-match binding table swapped via revision-checked snapshot (hook never
  reinstalled for config changes); injected-input pass-through; autorepeat
  suppression; explicit Win-key state machine (digit-first chord swallowing and
  chord-dirtying injection to prevent Start-menu activation).
- Core Audio: MTA worker with endpoint volume/mute for capture and render,
  configurable endpoint roles, `IMMNotificationClient` + `IAudioEndpointVolumeCallback`
  event-driven state (no polling), friendly-name resolution via property store.
- Foreground application audio: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` tracking,
  per-PID session enumeration, aggregate mute semantics (mixed → mute all),
  self-window avoidance, truthful "No audio session" state.
- Overlay: layered per-pixel-alpha HUD (`WS_EX_TOPMOST|TOOLWINDOW|NOACTIVATE|TRANSPARENT`),
  vector icons, fade/slide phases, monitor/position/scale/opacity configuration,
  single persistent HWND updated in place.
- Virtual desktops: build-pinned `IVirtualDesktopManagerInternal` backend for
  Windows 11 24H2/25H2 (26100, 26200–26299) with fail-closed detection, STA worker,
  Explorer-restart proxy rebuild, and `Ctrl+Win+Arrow` keyboard fallback.
- `Win+1`…`Win+9` desktop override with taskbar-shortcut suppression.
- Diagnostics: per-day log files under `%LOCALAPPDATA%\WinShort\logs\`, no keystroke
  logging (recognized shortcuts only, at debug level).
- Documentation set: ARCHITECTURE, WIN32_LIFETIME, KEYBOARD_HOOK_DESIGN, AUDIO_DESIGN,
  VIRTUAL_DESKTOP_COMPAT, UI_DESIGN, CONFIG_SCHEMA, TEST_PLAN.

### Verified

- 31 unit tests green (keyboard matrix, bindings, config round-trip, validation).
- Live smoke on Windows 11 25H2 (26200.9168): endpoint mute toggles, foreground
  session aggregate toggle, desktop count 9 + absolute switch 1↔2, recorder capture
  and conflict flow, idle CPU 0.000% over 15 s, release binary without console.
