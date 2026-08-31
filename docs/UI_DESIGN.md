# WinShort Control Center UI

**Status: Implemented** — this document describes the native Control Center in the final redesign branch.

WinShort remains a resident native Windows utility. The primary user-facing surface is a single owner-drawn Control Center window, not a configuration editor or a browser surface.

## Native stack

| Layer | Implementation |
|---|---|
| Window | User32 overlapped window, DWM rounded chrome, `WinShort.ControlCenter` class |
| Rendering | Direct2D `ID2D1HwndRenderTarget` with DirectWrite text |
| Typography | Segoe UI Variable Text with Segoe UI fallback |
| Overlay | Existing WIC/DIB layered window; no activation, taskbar, or Alt-Tab entry |
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

The shell is designed around a 960 × 660 DIP starting frame with a 760 × 540 DIP minimum. A fixed navigation rail and top search bar remain visible while the selected page scrolls independently. Navigation labels remain text-first; vector line icons are secondary scanning aids.

`src/ui/control_center.rs` owns the window state and message lifecycle. `src/ui/layout.rs` produces one logical element model used for painting, hit testing, focus traversal, and UI Automation bounds. `src/ui/controls.rs` contains the shared surface, row, button, navigation, search, home-card, profile-card, slider, and icon drawing vocabulary.

## Design tokens

`src/ui/theme.rs::UiTokens` centralizes the shell geometry and spacing language:

- 216 DIP navigation rail;
- 80 DIP top bar;
- 34 DIP status footer;
- 32 DIP page margins;
- 58 DIP setting rows with an 8 DIP rhythm;
- 7–12 DIP control/card radii;
- 184 DIP shortcut value boxes and 206 DIP picker/slider boxes.

All coordinates are 96-DPI logical units. The renderer retargets Direct2D/DirectWrite to the window's current PMv2 DPI. The visual system uses the existing light/dark semantic theme pairs and high-contrast system pairs.

Every shared interactive control has idle, hover, pressed, focus, and disabled treatment. Focus is a visible outline rather than a color-only state. Shadows are disabled automatically in high contrast.

## Page experience

### Home

Home answers “What is WinShort doing right now?” with real cached runtime state:

- current speaker name and mute/volume status;
- current microphone name and mute/input-volume status;
- current normal desktop when the native backend can resolve it;
- Special Workspace state (`Ready`, `Off`, or `Unavailable`);
- previous-desktop and Special quick actions;
- selected display profile summary without claiming that it matches the active topology;
- shortcut count and conflict health;
- degraded subsystem notice with a Details route to Diagnostics.

The page does not display endpoint identifiers, roles, GUIDs, HRESULTs, or backend names.

### Shortcuts

Shortcuts are grouped by actions:

- Audio: Mute microphone, Mute speakers, Next microphone, Next speaker, Mute current app, and current-app volume up/down;
- Workspaces: Desktop 1–9, Previous desktop, Move window to Special, and Open / close Special;
- Display profiles: shortcut for the selected profile.

Selecting a shortcut enters the existing global capture mode. Captured chords are validated against the canonical conflict and reserved-family rules, persisted through the atomic config path, and published as one coherent runtime change. Escape cancels capture. A failed save restores the previous draft and reports a human recovery message; the hook is not reinstalled.

### Audio

Audio is divided into Speakers, Microphones, and Current app audio. Current speaker/microphone values come from cached worker state and current default metadata. Device changes use human labels and a `Follow Windows default` mode; opaque endpoint strings stay internal.

The Next speaker and Next microphone pickers retain the real active-device allowlist semantics:

- Use all available devices;
- Disable cycling;
- explicit device selections;
- unavailable saved devices remain visible as reconnectable choices without leaking their IDs.

Current app audio explains the mute and ±5% volume actions without exposing session GUIDs, process resolution stages, or endpoint scans. Windows default roles remain available in Advanced.

### Workspaces

The page uses normal workspace language. It shows the cached normal desktop/count when available, the Special Workspace readiness state, and the existing controls for:

- workspace shortcuts;
- configurable Desktop 1–9 family;
- move-and-follow and silent move modifiers;
- Previous desktop;
- Move window to Special;
- Open / close Special.

Numbered desktop and Special behavior remains owned by the pinned desktop backend. The Special Workspace is never included in normal ordinals. The UI does not expose Shell ABI details, GUIDs, or persistence files.

### Displays

Displays presents saved profiles as a responsive two-column card grid when space permits. Cards show the real profile name, a human display-route summary, its profile shortcut, and whether it is ready to activate or needs a test. Selecting a card updates the stable active profile reference; activating an already selected confirmed card sends the existing validated apply request.

The page includes a visible three-step guide:

1. Which displays — select active/inactive connected routes while preserving same-panel/different-GPU distinction internally;
2. How they work — choose Extend or Duplicate through the existing picker;
3. Name and shortcut — use Rename and the stable profile shortcut recorder.

New/Capture Current, Update, Rename, Duplicate, Delete, profile selection, profile shortcut binding, advanced route selection, and route editing remain reachable. Output/topology edits are local until Test profile; Discard display edits restores the last saved profile. Profile IDs are never replaced by names. Duplicate creates a new ID and does not steal the original binding.

When a display test is pending, the guide becomes a recovery banner explaining Keep/Revert and the automatic timeout. A failed recovery keeps Revert available. Normal overlay messages are human-readable; technical causes remain in logs and Diagnostics.

### Overlay

Overlay has a visual preview panel and concise controls:

- enabled;
- System, Light, or Dark appearance;
- position and monitor;
- show changes made outside WinShort;
- Small/Normal/Large size;
- Low/Normal/High opacity;
- Short/Normal/Long duration;
- Preview.

Preview sends the edited `OverlayCfg` directly to the existing layered overlay and never persists or replaces the live `ConfigHandle`. Exact slider ranges remain available through truthful UI Automation RangeValue semantics and the technical backend, not as required normal-user vocabulary.

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
- toggles expose TogglePattern;
- sliders expose RangeValuePattern;
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

The owner window remains PMv2-aware, responds to `WM_DPICHANGED`, persists/restores reachable bounds, and stops its timer when motion, capture, or feedback is idle. Reduced Windows animation preferences skip shell hover/toggle tweens; the layered runtime overlay retains its established reduced-motion policy.

## Diagnostics & Support

Diagnostics remains a separate native window backed by `App::diagnostics_snapshot`. It preserves Copy Diagnostics, Open Logs, Support Bundle, Self-Test, sanitized endpoint/path/config projections, bounded log collection, and no telemetry. The Home and System pages route friendly Details actions there instead of leaking technical state into normal controls.

## Degraded mode

The shell is constructed from cached state and never requires every optional worker to be present. Audio, workspace, overlay, keyboard, display inventory, and startup failures appear as human state or an actionable Details path. Painting does not enumerate COM devices, query DisplayConfig, scan files, or inspect processes.

## Verification expectations

Native visual acceptance remains a real Windows/manual concern. Automated tests cover search policy, first-run safety, layout geometry, profile-card layout, terminology, picker semantics, UIA snapshots/actions, overlay copy, and existing backend safety state machines. No screenshot or manual acceptance is claimed unless exercised on the final revision.
