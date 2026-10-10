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
| Pickers | Temporary `WS_CHILD` host with a native LISTBOX, keyboard selection, and generation-safe teardown |
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

The shell is a fixed 960 × 660 DIP utility window with DPI-scaled physical dimensions. A fixed navigation rail, an 80 DIP sidebar brand row, a single custom top-chrome row, and page content remain stable while the selected page scrolls independently with immediate wheel and Page Up/Down updates. Scrollbar dragging stays direct and never competes with a tween. Navigation begins 8 DIP below the brand row without an extra title band. Ordinary pages use a page-aware leading content column capped between 860 and 1260 DIP; Home and Displays can use wider grids intentionally. Navigation labels remain text-first; authored vector icons use one compact stroke vocabulary and the app mark shares the audio/microphone motif.

`src/ui/control_center.rs` is the feature facade; its private modules separate state, commands, presentation and native message handling. `src/ui/layout.rs` exposes the shared element model and domain page builders used by painting, hit testing, focus traversal and UI Automation. `src/ui/controls.rs` exposes stateless control families. See [UI_ARCHITECTURE.md](UI_ARCHITECTURE.md) for dependency direction, native ownership and state invariants.

## Design tokens

`src/ui/theme.rs::UiTokens` centralizes the shell geometry and spacing language:

- 216 DIP navigation rail;
- 34 DIP status footer;
- 32 DIP page margins;
- 40 DIP single top-chrome row with one 44 × 32 DIP Close hit target and a blank draggable caption gap; the sidebar brand remains in its separate 80 DIP row;
- 80 DIP page headers and 68 DIP section headers (32 DIP when no description is needed); non-page headings share one 20 DIP inter-section breathing-room token;
- 12 DIP section-content gap before a section's first control and 8 DIP ROW_GAP between sibling rows/cards;
- 58 DIP setting rows with an 8 DIP rhythm;
- one shared 206 DIP right-side control column for dropdowns, keycaps, and equivalent value controls, with a compact 136 DIP Monitor variant for the split Overlay status row;
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
- Special Desktop state (`Available`, `Off`, or `Unavailable`; normal content is centered without a redundant status badge);
- previous-desktop and Special Desktop quick actions;
- selected display profile summary without claiming that it matches the active topology;
- shortcut count and conflict health;
- degraded subsystem notice with a Details route to Diagnostics.

Home actions use `Choose` only for cards that open a device picker and `Open` for navigation; Special Desktop uses `Enable` only when its master switch is off.

The page does not display endpoint identifiers, roles, GUIDs, HRESULTs, or backend names.

### Shortcuts

Shortcuts are grouped by actions:

- Audio: Mute microphone, Mute speakers, Next microphone, Next speaker, Mute current app, and current-app volume up/down;
- Workspaces: Desktop 1–9, Previous desktop, Move window to Special Desktop, and Open / close Special Desktop;
- Display profiles: shortcut for the selected profile.

Each shortcut card exposes its keycap, an Enable/Disable action, and an explicit Unassign action. Selecting the keycap enters the existing global capture mode. Disabled shortcuts keep their chord so they can be re-enabled without recording again. Captured chords are validated against the canonical conflict and reserved-family rules, persisted through the atomic config path, and published as one coherent runtime change. Escape cancels capture. A failed save restores the previous draft and reports a human recovery message; the hook is not reinstalled.

### Audio

Audio is divided into Speakers, Microphones, and Current app audio. Current speaker/microphone values come from cached worker state and current default metadata. Every normal control uses one canonical primary endpoint name (`GS25F2`, `SAMSUNG`, `SIMGOT EW300 DSP`); adapter/driver suffixes remain available only through accessibility/diagnostic detail. The speaker and microphone pickers expose only active real endpoints; selecting one changes the Windows system default through the audio worker. The selected row itself identifies the current endpoint, so the Control Center does not add redundant default/explicit badges. System speaker/microphone mute and volume feedback is left to Windows instead of stacking a duplicate WinShort OSD. Opaque endpoint strings and the internal follow-default binding stay out of the picker.
Next speaker and Next microphone use three mutually exclusive modes: all available devices, selected devices, or don't cycle. Selecting the middle mode progressively reveals a real device checkbox list. The native LISTBOX fallback uses the same mode model and preserves `None`, explicit endpoint sets, and `Some(empty)` semantics.

Current app audio explains that WinShort itself is excluded: users switch to another app before controlling its sessions. When no external target is available, the page keeps the current-app shortcut controls without rendering a large empty status card. Windows default roles remain available in Advanced and are enabled only while the corresponding direction follows the Windows default.

### Workspaces

The page configures workspace behavior only. Numbered desktop switching remains available through the configured 1–9 shortcut family, while Home may show the current normal desktop. One master Workspace shortcuts switch owns the dependent desktop and Special actions. When it is off, the page explains the dependency and exposes disabled Special shortcut cards. Special Desktop is a normal heading with its description followed by the Move window to Special Desktop and Open / close Special Desktop cards; no desktop switcher strip or standalone status card is rendered.

### Displays

Displays is an overview first: saved profiles appear as cards with profile name, friendly screen summary, topology, shortcut, readiness, and a contextual Activate or Review action. Card bodies only select; Activate/Review are separate keyboard/UIA controls. Management actions show the selected profile name directly below its row; Create from current and Replace from current are named separately, Delete is destructive and confirmed twice, and the overview does not duplicate the selected profile in a second generic picker.

Editing enters a real four-step workflow:

1. Which screens — visual screen cards show monitor, adapter, and connector metadata while preserving stable route identity internally;
2. Arrangement — visual Extend and Duplicate choices show the selected screen names, while a one-screen profile is explicitly shown as Single display;
3. Name & shortcut — a focused profile-name prompt and the safe shortcut recorder;
4. Review — Test profile is the primary action and Discard changes appears only for a dirty draft.

Output/topology edits remain local until Test and Keep. Moving between steps never applies DisplayConfig. A pending test replaces the editor with a high-priority Keep/Revert surface; failed recovery keeps Revert available. Advanced route editing remains under Advanced, not in the ordinary overview.

### Overlay

Overlay uses one fixed side-by-side placement area at the supported Control Center size:

- the monitor preview sits beside Position's 3×3 selector;
- Preview and Position use the same column width, height, top edge, and bottom edge;
- the Position heading is compact, with no explanatory description beneath it;
- the monitor frame itself is the preview surface; there is no outer preview card around it;
- a same-row, equal-width pair for Show status overlay and Monitor;
- a 3×3 position grid with accessible Top left through Bottom right cells;
- Overlay style: Follow Windows, Light, or Dark for the status card;
- 0.7×–1.6× actual size multiplier presentation over the existing scale model;
- a five-stop Blur slider: Transparent, Light blur, Medium blur, Heavy blur, or Solid;
- Duration shown as the actual configured seconds;
- per-category notification toggles for Microphone, Speaker, Current app audio, Workspace, and Display profile;
- Show on screen.

Monitor targeting exposes only Primary and Cursor position. The preview derives its simulated monitor ratio from the selected work area, using the monitor containing the pointer for Cursor position. Missing monitor information uses a 16:9 fallback. The monitor is fit inside bounded content geometry without stretching, and the frame contains only a minimal status-card silhouette whose normalized placement matches runtime overlay placement; it has no illustrative text. The Monitor helper uses concise `Overlay location` copy so the complete description remains visible beside its selector. `Show on screen` sends the edited `OverlayCfg` without saving it.

The runtime overlay is a click-through layered HWND rendered through Direct2D and backed by the existing Windows Composition effect graph. Transparent disables the material, the three blur treatments select the Gaussian blur amount and tint, and Solid renders an opaque card. Windows 10/API failure, High Contrast, and `SPI_GETDISABLEOVERLAPPEDCONTENT` fall back to a fully opaque accessible surface. Topmost, no-activate, tool-window, monitor placement, DPI, and bounded event-driven animation remain intact.

Audio cards use 10 DIP padding and 48 DIP rows, so a one-row card is 68 DIP high. Width follows the widest title/detail measured in the painting font by DirectWrite, bounded to 200–360 DIP; long labels use an ellipsis. Measurements are cached per content update and scaled together with the existing Size setting.

Muted microphone and executable audio cards stay visible until unmuted. After one second at full size they shrink into a 52 DIP square. Microphone uses its existing slashed mic glyph; programs use the executable's real icon with a small mute marker. Icon pixels are extracted once, with target-owned Direct2D bitmap caches; unavailable executable icons use the existing application glyph. Unmuting expands the same logical member, shows explicit feedback, and then expires using Duration. Reduced motion skips the size tween. Microphone mute, each muted executable, output changes, and app volume feedback keep independent identities and expiry. Program badges persist across foreground-window changes. A worker-owned read-only inventory groups sessions by normalized full executable path, with PID-private identities when the path is unavailable, and refreshes once per second and immediately after app toggle actions. Unmute or process/session exit removes only the affected program; a failed inventory scan preserves the previous known state. Late foreground query results remain discarded. Hover opacity retains its configurable fade and click-through.

Overlay motion uses one shared high-resolution waitable timer worker, with per-card frame intervals derived from the selected monitor's active refresh rational. Display/visual changes refresh this cadence. The worker posts at most one pending frame per card, skips missed frames, and leaves all drawing on the UI thread. Idle cards stop frame wakes; hold/expiry deadlines remain one-shot. Hiding, recycling or destroying a window removes its clock target, and generation checks discard obsolete wakes. A normal waitable timer is available when Windows cannot create a high-resolution timer.

Microphone mute/unmute changes also tween the measured width and crossfade the labels over 160 ms while the card is expanded. Rapid reversals retain the current width and text weights, with at most one outgoing label; the icon shows the latest state immediately. Expanding a compact badge reveals only the new label. Reduced motion applies the final content immediately, and settled content stops frame wakes.

When a mute badge is already compact, a newly muted microphone or program displays its full card in the normal notification lane below that badge, matching unmute feedback (bottom anchors stack upward). After its one-second hold, contraction and movement into the adjacent icon slot share one 220 ms clock. The common bar is 52 DIP high and 52 + 44 × (member count − 1) DIP wide; it grows away from the first badge's anchor. Settled slots retain their order during metadata refreshes; newcomers append and removing a middle member closes the gap. Bars that exceed the monitor's usable width continue in another lane. The joining card moves into the side column before reaching the badge row, keeping existing icons clear. Per-frame placement preserves window stacking. Composition retains its backing canvas while contracting and draws larger canvases before publishing them, so resize never binds an empty content surface. Each member retains its logical entry and HWND, parking its surface only after its icon has joined the host. Hidden member metadata still updates. Unmuting a member restores its feedback card below the remaining bar. Changing the selected app leaves every muted badge visible; disabling a category removes that category's surfaces. Bars dissolve and regroup from their current geometry, preserve click-through and hover opacity, and stop frame wakes when settled. Separate monitors and differing surface styles retain separate bars.

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

Typing is handled by the owner-drawn shell; a focused Search field shows a real text caret that blinks while the Control Center owns editing focus. UI Automation exposes the search control as an editable Edit/ValuePattern node. Enter opens the first result, while selecting a result navigates to its page and stable target. No network, telemetry, or raw config-key labels are involved.

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
- picker triggers remain Button/Invoke with read-only displayed values;
- the custom titlebar Close node remains a UIA Button with truthful Invoke semantics;
- stale providers return `UIA_E_ELEMENTNOTAVAILABLE`;
- UIA reads consume immutable snapshots only;
- actions are posted back to the Control Center HWND;
- UIA event delivery is deferred until state borrows are released;
- focus is repaired when a mutation disables the focused control;
- native picker LISTBOX focus is distinct from logical shell focus, while the child host remains under the active Control Center top-level window;
- child picker teardown is idempotent and happens before the owner hides;
- no child HWND is invented for painted Control Center rows.

The owner window is PMv2-aware, fixed-size, and uses a blank custom top-chrome row with a Close hit target and DWM rounded corners. It responds to `WM_DPICHANGED` by preserving the logical client size, persists/restores only its reachable position, hides to the tray on Close, and stops its timer when hover/toggle motion, capture, or feedback is idle. Reduced Windows animation preferences skip shell hover/toggle tweens; scrolling is always direct. The Composition-backed overlay retains its reduced-motion policy, applies the selected blur treatment, and falls back to an opaque surface when Composition is unavailable or disabled.

Diagnostics remains a separate native window backed by `App::diagnostics_snapshot`. It preserves Copy Diagnostics, Open Logs, Support Bundle, Self-Test, sanitized endpoint/path/config projections, bounded log collection, and no telemetry. The Home and System pages route friendly Details actions there instead of leaking technical state into normal controls.

## Degraded mode

The shell is constructed from cached state and never requires every optional worker to be present. Audio, workspace, overlay, keyboard, display inventory, and startup failures appear as human state or an actionable Details path. Painting does not enumerate COM devices, query DisplayConfig, scan files, or inspect processes.

## Verification expectations

Native visual acceptance remains a real Windows/manual concern. Automated tests cover search policy, first-run safety, layout geometry, profile-card layout, terminology, picker semantics, UIA snapshots/actions, overlay copy, and existing backend safety state machines. No screenshot or manual acceptance is claimed unless exercised on the final revision.

### Draft navigation and build identity

Leaving Displays retains pending edits. Other pages expose a Continue display edits footer action; Test/Keep or explicit Discard still owns the display draft transaction. Search clicks and Enter reveal and focus their destination, falling back to the master switch when the requested control is unavailable.

System > About shows the compiled revision (including a modified marker), UTC build date and a Copy version info action. Copied info includes a build ID that distinguishes builds from the same commit.
