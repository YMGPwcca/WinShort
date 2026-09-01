# WinShort Control Center UI

**Status: Implemented** — this document describes the native Control Center in the final redesign branch.

WinShort remains a resident native Windows utility. The primary user-facing surface is a single owner-drawn Control Center window, not a configuration editor or a browser surface.

## Native stack

| Layer | Implementation |
|---|---|
| Window | User32 overlapped window, DWM rounded chrome, `WinShort.ControlCenter` class |
| Rendering | Direct2D `ID2D1HwndRenderTarget` with DirectWrite text |
| Typography | Segoe UI Variable Text with Segoe UI fallback |
| Overlay | DWM system backdrop via `DWMWA_SYSTEMBACKDROP_TYPE`/`DWMSBT_TRANSIENTWINDOW` with a Direct2D `ID2D1HwndRenderTarget`; opaque accessible fallback |
| Pickers | Existing native LISTBOX popup with keyboard selection and generation-safe teardown |
| Accessibility | Custom UI Automation fragment provider with a snapshot/action boundary |
| State | Cached event-driven runtime snapshot plus typed configuration draft for risky display work |

No Electron, WebView, React, Vue, Svelte, Qt, GTK, WPF, WinUI, or other UI framework is used.

## Shell architecture

The shell uses an explicit `Page` model:

```text
Control Center
├── Home
├── Shortcuts
├── Audio
├── Workspaces
├── Displays
├── Overlay
├── System
└── Advanced
```

Diagnostics & Support remains a separate native owner-drawn window because it has a dense read-only technical surface and its existing support actions are already isolated safely.

The shell starts at 960 × 660 DIP with a 760 × 540 DIP minimum. A fixed navigation rail, custom titlebar, and top search bar remain visible while the selected page scrolls independently with short, accumulated wheel retargets and fast Page Up/Down motion. Scrollbar dragging stays direct. Ordinary pages use a page-aware leading content column capped between 860 and 1260 DIP; Home and Displays can use wider grids intentionally. Navigation labels remain text-first; authored vector icons use one compact stroke vocabulary and the app mark shares the audio/microphone motif.

`src/ui/control_center.rs` owns the window state and message lifecycle. `src/ui/layout.rs` produces one logical element model plus task-shaped visual regions used by painting, hit testing, focus traversal, and UI Automation. `src/ui/controls.rs` contains the shared surface, spaced section header, semantic button, navigation, search, dashboard-card, profile-card, choice, shortcut-card, titlebar-button, slider-cluster, and icon vocabulary.

## Design tokens

`src/ui/theme.rs::UiTokens` centralizes the shell geometry and spacing language:

- 216 DIP navigation rail;
- 80 DIP top bar;
- 34 DIP status footer;
- 32 DIP page margins;
- 92 DIP page headers and 88 DIP section headers; non-page headings share one 20 DIP inter-section breathing-room token;
- 58 DIP setting rows with an 8 DIP rhythm;
- one shared 206 DIP right-side control column for dropdowns, keycaps, and equivalent value controls;
- 32 DIP native picker rows with GDI-metric-sized text envelopes;
- compact slider tracks sized from the content column rather than the window edge.

All coordinates are 96-DPI logical units. The renderer retargets Direct2D/DirectWrite to the window's current PMv2 DPI. The visual system uses the existing light/dark semantic theme pairs and high-contrast system pairs.

Every shared interactive control has idle, hover, pressed, keyboard-visible focus, and disabled treatment. Pointer focus remains available to UI Automation and keyboard navigation but does not add a heavy painted ring. Shadows are disabled automatically in high contrast.

## Page experience

### Home

Home answers “What is WinShort doing right now?” with real cached runtime state:

- current speaker name and mute/volume status;
- current microphone name and mute/input-volume status;
- current normal desktop when the native backend can resolve it;
- Special Workspace state (`Available`, `Off`, or `Unavailable`; normal content is centered without a redundant status badge);
- previous-desktop and Special quick actions;
- selected display profile summary without claiming that it matches the active topology;
- shortcut count and conflict health;
- degraded subsystem notice with a Details route to Diagnostics.

Home actions use `Choose` only for cards that open a device picker and `Open` for navigation; Special Workspace uses `Enable` only when its master switch is off.

The page does not display endpoint identifiers, roles, GUIDs, HRESULTs, or backend names.

### Shortcuts

Shortcuts are grouped by actions:

- Audio: Mute microphone, Mute speakers, Next microphone, Next speaker, Mute current app, and current-app volume up/down;
- Workspaces: Desktop 1–9, Previous desktop, Move window to Special, and Open / close Special;
- Display profiles: shortcut for the selected profile.

Each shortcut card exposes its keycap, an Enable/Disable action, and an explicit Unassign action. Selecting the keycap enters the existing global capture mode. Disabled shortcuts keep their chord so they can be re-enabled without recording again. Captured chords are validated against the canonical conflict and reserved-family rules, persisted through the atomic config path, and published as one coherent runtime change. Escape cancels capture. A failed save restores the previous draft and reports a human recovery message; the hook is not reinstalled.

### Audio

Audio is divided into Speakers, Microphones, and Current app audio. Current speaker/microphone values come from cached worker state and current default metadata. Every normal control uses one canonical primary endpoint name (`GS25F2`, `SAMSUNG`, `SIMGOT EW300 DSP`); adapter/driver suffixes remain available only through accessibility/diagnostic detail. The speaker and microphone pickers expose only active real endpoints; selecting one changes the Windows system default through the audio worker. The selected row itself identifies the current endpoint, so the Control Center does not add redundant default/explicit badges. System speaker/microphone mute and volume feedback is left to Windows instead of stacking a duplicate WinShort OSD. Opaque endpoint strings and the internal follow-default binding stay out of the picker.
Next speaker and Next microphone use three mutually exclusive modes: all available devices, selected devices, or don't cycle. Selecting the middle mode progressively reveals a real device checkbox list. The native LISTBOX fallback uses the same mode model and preserves `None`, explicit endpoint sets, and `Some(empty)` semantics.

Current app audio explains that WinShort itself is excluded: users switch to another app before controlling its sessions. Windows default roles remain available in Advanced and are enabled only while the corresponding direction follows the Windows default.

### Workspaces

The page configures workspace behavior only. Numbered desktop switching remains available through the configured 1–9 shortcut family, while Home may show the current normal desktop. One master Workspace shortcuts switch owns the dependent desktop and Special actions. When it is off, the page explains the dependency and exposes disabled Special shortcut cards. Special Workspace is a normal heading with its description followed by the Move window to Special and Open / close Special cards; no desktop switcher strip or standalone status card is rendered.

### Displays

Displays is an overview first: saved profiles appear as cards with profile name, friendly screen summary, topology, shortcut, readiness, and a contextual Activate or Review action. Management actions sit in a selected-profile toolbar; Create from current and Replace from current are named separately, Delete is destructive and confirmed twice, and the overview does not duplicate the selected profile in a second generic picker.

Editing enters a real four-step workflow:

1. Which screens — visual screen cards show monitor, adapter, and connector metadata while preserving stable route identity internally;
2. Arrangement — visual Extend and Duplicate choices show the selected screen names, while a one-screen profile is explicitly shown as Single display;
3. Name & shortcut — a focused profile-name prompt and the safe shortcut recorder;
4. Review — Test profile is the primary action and Discard changes appears only for a dirty draft.

Output/topology edits remain local until Test and Keep. Moving between steps never applies DisplayConfig. A pending test replaces the editor with a high-priority Keep/Revert surface; failed recovery keeps Revert available. Advanced route editing remains under Advanced, not in the ordinary overview.

### Overlay

Overlay has a compact visual schematic preview and concise controls:

- enabled;
- System, Light, or Dark appearance;
- a 3×3 position grid with accessible Top left through Bottom right cells;
- monitor;
- Small/Normal/Large size;
- Low/Normal/High opacity;
- Short/Normal/Long duration;
- Show on screen.

The preview derives its simulated monitor ratio from the selected work area: the applicable foreground monitor, Primary, or an available saved monitor. Missing or disconnected targets use a 16:9 fallback. The monitor is fit inside bounded content geometry without stretching, and the sample card uses the same normalized left/center/right and top/center/bottom placement semantics as the real overlay. `Show on screen` sends the edited `OverlayCfg` without saving it.

The runtime overlay is a non-layered, click-through HWND rendered through Direct2D and backed by documented DWM Desktop Acrylic: `DwmSetWindowAttribute` with `DWMWA_SYSTEMBACKDROP_TYPE` and `DWMSBT_TRANSIENTWINDOW`, plus `DwmExtendFrameIntoClientArea` for the client surface. Overlay opacity scales the drawn card over the blurred DWM material. Windows 10/API failure, High Contrast, and `SPI_GETDISABLEOVERLAPPEDCONTENT` fall back to a fully opaque accessible surface. Topmost, no-activate, tool-window, monitor placement, DPI, and bounded event-driven animation remain intact.

### System

System is intentionally short:

- Start WinShort with Windows;
- Pause all shortcuts;
- Diagnostics and support;
- Open configuration folder;
- deliberate two-step Reset WinShort action;
- About with the actual package version.

Startup remains registry-authoritative and applies immediately. Pause persists the existing `start_hotkeys_enabled` value through the canonical config commit path.

### Advanced

Advanced contains only technical user-configurable controls that already exist:

- Windows default-device roles;
- exact display route editing;
- temporary debug logging;
- Diagnostics entry.

It is not a dump of runtime diagnostics. Dense implementation state remains in Diagnostics.

## Local search

The shell search box is a local, case-insensitive deterministic index in `src/ui/navigation.rs`. Descriptors contain human titles, keywords, page, section, and target control. Search results rank exact/title matches before keyword matches, prefer common user destinations, and are capped to a small result set.

Typing is handled by the owner-drawn shell; UI Automation exposes the search control as an editable Edit/ValuePattern node. Enter opens the first result, while selecting a result navigates to its page and stable target. No network, telemetry, or raw config-key labels are involved.

## First run

`src/ui/first_run.rs` stores one atomic UI-owned marker at `%LOCALAPPDATA%\\WinShort\\control-center-ui-state.txt`. It never changes the config schema.

On a genuinely new installation with no config file and no completion marker, the shell shows:

1. speaker allowlist choice;
2. microphone allowlist choice;
3. Desktop 1–9 choice;
4. a ready screen showing actual configured microphone, desktop, and Special shortcuts.

Existing users are not inferred from an absent new field. The presence of any real `config.toml` suppresses onboarding, even when it contains only defaults. Completing onboarding writes only the UI marker.

## Persistence model

Simple controls commit locally:

1. clone the current draft;
2. mutate the selected typed value;
3. run canonical validation and atomic save;
4. publish one `ConfigApplied` snapshot;
5. refresh dependent workers and UI;
6. show “Changes applied”.

This includes toggles, audio/workspace/overlay pickers, sliders, safe profile CRUD, profile selection, and accepted hotkeys. A failure restores the pre-action value in the UI and keeps the original error for Diagnostics/logging.

Display output and topology edits remain explicit local draft/risky work. Test profile captures rollback state, validates and applies through DisplayConfig, starts the existing bounded confirmation timer, and never treats timeout as acceptance. Keep persists only after explicit confirmation; Revert, timeout, and Discard display edits restore the saved topology/profile.

## Accessibility and lifetime

The Control Center preserves the established custom provider rules:

- navigation nodes are Buttons with truthful Invoke semantics;
- the root name includes the current page and selected navigation is announced in its accessible name;
- search is an editable Edit with mutable ValuePattern;
- toggles and device checkbox options expose TogglePattern;
- mutually exclusive audio modes, topology choices, and overlay positions expose RadioButton/SelectionItem semantics;
- sliders expose RangeValuePattern;
- custom titlebar Minimize, Maximize/Restore, and Close nodes remain UIA Buttons with truthful Invoke semantics;
- picker triggers remain Button/Invoke with read-only displayed values;
- unsupported patterns return successful null/empty results rather than fabricated interfaces;
- stale providers return `UIA_E_ELEMENTNOTAVAILABLE`;
- UIA reads consume immutable snapshots only;
- actions are posted back to the Control Center HWND;
- UIA event delivery is deferred until state borrows are released;
- focus is repaired when a mutation disables the focused control;
- native picker LISTBOX focus is distinct from logical shell focus;
- popup teardown is idempotent and happens before the owner hides;
- no child HWND is invented for painted Control Center rows.

The owner window is PMv2-aware, uses a native custom frame with supported resize hit testing and DWM rounded corners, responds to `WM_DPICHANGED`, persists/restores reachable bounds, and stops its timer when motion, capture, or feedback is idle. Reduced Windows animation preferences skip shell hover/toggle tweens; the DWM-backed overlay retains its reduced-motion policy and falls back to an opaque surface when acrylic is unavailable or disabled.

## Diagnostics & Support

Diagnostics remains a separate native window backed by `App::diagnostics_snapshot`. It preserves Copy Diagnostics, Open Logs, Support Bundle, Self-Test, sanitized endpoint/path/config projections, bounded log collection, and no telemetry. The Home and System pages route friendly Details actions there instead of leaking technical state into normal controls.

## Degraded mode

The shell is constructed from cached state and never requires every optional worker to be present. Audio, workspace, overlay, keyboard, display inventory, and startup failures appear as human state or an actionable Details path. Painting does not enumerate COM devices, query DisplayConfig, scan files, or inspect processes.

## Verification expectations

Native visual acceptance remains a real Windows/manual concern. Automated tests cover search policy, first-run safety, layout geometry, profile-card layout, terminology, picker semantics, UIA snapshots/actions, overlay copy, and existing backend safety state machines. No screenshot or manual acceptance is claimed unless exercised on the final revision.
