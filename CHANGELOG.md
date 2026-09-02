# Changelog

All notable changes to WinShort are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed
- Restored the sidebar brand to its original 80 DIP row and added a visible
  blinking caret for focused Search editing.
- Made dark/light theme swaps transactional so a failed brush rebuild cannot
  leave the Control Center with an incomplete palette or crash on repaint.
- Tightened Overlay placement: Position has no description, status and Monitor
  share an equal-width row, and monitor targets are Primary or Cursor position.
  The status-card appearance control remains clearly scoped to Overlay.
- Fixed the overlay crash at its root cause by preparing an owned ShowPlan,
  releasing OverlayState before reentrant HWND operations, and keeping WM_SIZE
  D2D resizing active.
- Stopped the top-chrome separator at the content boundary, removed fake preview
  text in favor of a placement-only silhouette, and made the Monitor helper fully
  readable in its compact half-row.
- Search now clears editing focus on blank/control clicks and external focus loss;
  keyboard handlers and caret visibility use the same real editing-focus predicate.
- Simplified the runtime overlay to one card surface with no outer halo or
  custom shadow stack across System, Dark, and Light styles.
- Tightened the sidebar brand-to-navigation gap to the shared row rhythm,
  made the Overlay Preview and Position columns equal in width and height, and
  bounded native picker widths to compact 320–400 DIP limits.
- Removed the large empty Current app audio status card while retaining its
  actionable shortcut controls.
- Renamed the user-facing dedicated desktop to Special Desktop while retaining
  scratchpad compatibility keys and GUID storage.
- Added the isolated release Windows UI acceptance harness with themed picker
  captures and runtime overlay surface checks.
- Corrected the fixed Control Center acceptance model: brand icon/text now share
  one row, Search and Close share one top-chrome row, and sibling card/row
  spacing uses named section-content, section, and ROW_GAP semantics.
- Removed Control Center minimize, maximize, resize, Snap, and double-click
  maximize behavior. The window now uses a fixed DPI-scaled size with one
  Close action that hides to the tray without ending WinShort.
- Fixed Overlay placement to the supported side-by-side Preview/Position/Monitor
  geometry; monitor aspect fitting and monitor-as-preview rendering remain intact.
- Corrected the failed Control Center acceptance pass: Settings scrolling is immediate
  with no scroll tween state, the Shortcuts icon is an unmistakable keyboard, Current
  app audio is a compact single-heading status row, and the custom titlebar is blank
  with restrained caption glyphs.
- Reworked Overlay placement into one fixed side-by-side preview/control layout
  with a monitor-as-preview surface, and kept the native LISTBOX picker host
  inside the active Control Center child hierarchy without foreground transfer.

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
- Simplified the Control Center shell hierarchy: page titles no longer repeat in
  the top bar, section groups have stronger separation, and workspace/Special
  surfaces use compact single-purpose layouts.
- Replaced eased Settings scrolling with immediate wheel and Page Up/Down updates;
  pointer focus remains available to UI Automation and keyboard navigation.
- Tightened the Overlay preview, aligned the app/navigation icon language with
  the packaged speaker mark, and corrected native picker font metrics so
  endpoint descenders remain visible.
- Reworked the corrective Control Center pass: wheel scrolling is direct with no
  queued target/tween state, section dividers sit in shared breathing room, and
  Workspaces no longer renders a desktop-switcher strip or standalone Special
  Workspace status card.
- Unified standard right-side control widths, canonicalized compact audio
  endpoint names, and replaced the microphone/shortcut glyphs with recognizable
  vector symbols.
- Made the Overlay preview fit the selected monitor work-area aspect ratio and
  refactored the runtime overlay to a non-layered Direct2D HWND using documented
  DWM Desktop Acrylic with opaque accessibility fallbacks.
- Replaced the resizable custom Control Center frame with a fixed-size blank
  top-chrome row and one Close UIA Button/Invoke action; Close keeps WinShort
  alive and routes through the existing hide-to-tray lifecycle.
- Associated every published config revision with its commit origin in one
  coherent live stamp, so audio preflight drift preserves DeviceCycle
  provenance and delayed ConfigChanged notifications remain stale-safe.
- Audio endpoint rebuilds now own one `ConfigSnapshot` for Capture and Render,
  preventing a newer publication from entering a rebuild already planned for
  an older revision.
- Corrected DisplayConfig confirmation expiry: Test Apply is temporary and timeout
  automatically restores the captured topology; Keep is explicit, Revert is immediate,
  timer-start/persistence failures are surfaced, and failed recovery retains a retryable
  rollback token.
- Removed the unreleased executable-to-desktop routing experiment from active product scope;
  legacy routing tables are parse-compatible, ignored, and omitted on the next Save.
- Replaced hidden-window Scratchpad ownership with a dedicated native Virtual Desktop:
  Special Desktop actions no longer hide/show or force-focus application HWNDs, numbered 1–9
  excludes the desktop, the desktop is named and re-pinned to the tail of Shell ordering,
  its exact GUID is persisted for crash/reboot reclaim, and graceful disable/shutdown removes it
  through Shell with a normal fallback.

### Added

- Four optional Phase-1 hotkeys: cycle the Windows input/output defaults and
  adjust foreground application session volume by ±5%.
- Schema v4 adds one configurable numbered-desktop modifier family plus
  optional move/follow, silent-move, and previous-desktop actions without
  stealing existing hotkey slots. Explicit Settings endpoint selections remain
  persistently saved; desktop workflow hotkeys remain unassigned by default.
- Schema v5's optional `scratchpad_assign` / `scratchpad_toggle` wire names remain compatible;
  their current behavior sends windows to and toggles the dedicated Special Desktop. The
  desktop GUID is operational recovery state stored outside `config.toml`; the return desktop
  remains process-only.
- Numbered Virtual Desktop switching now ensures missing normal desktops through the
  native Shell backend; move/follow, silent move, previous-desktop navigation,
  per-desktop foreground restoration, and the dedicated Special Desktop are available
  as conservative configurable actions.
- Schema v7 adds independent input/output endpoint allowlists for device cycling.
  Omitted means all active endpoints, while an explicit empty list disables that
  direction; offline IDs remain configured and reconnects are notification-driven.
- Schema v8 adds persisted DisplayConfig topology profiles with stable target-path
  identities, Capture Current, profile CRUD, route validation, and a bounded
  15-second Undo window after explicit Apply.
- Schema v9 adds stable-ID display-profile hotkeys, complete New/Update/Rename/Duplicate/
  Delete workflows, supported route editing, confirmation state, and centralized cleanup
  and conflict repair.
- Schema v10 adds independently enabled/disabled configurable shortcuts. Disabled
  chords remain persisted outside the active binding table and can be changed or
  unassigned without being reactivated.

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
