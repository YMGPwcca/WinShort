# Test Plan

## Classification (#37)

Every behavior in this plan is classified as one of:

- **AUTOMATED IN LOCAL GATE** — deterministic `cargo test` coverage exercised by the required final local validation.
- **PROPERTY TEST** — proptest-generated coverage over an invariant domain (runs in the local gate).
- **FUZZ-STRATEGY / arbitrary-input property coverage** — bounded arbitrary-input tests inside
  `cargo test` (see Fuzzing note below); no separate libFuzzer job.
- **MANUAL / HARDWARE-DEPENDENT** — requires a real desktop session, physical devices,
  or shell state; must be verified by hand per release.

The canonical Windows verification entry point is `tools/final_validation.ps1`. From a clean
worktree it executes fmt, all-target/all-feature check, strict Clippy, the full test suite,
x86_64 release build plus embedded manifest/icon checks, the release nullable-UIA ABI regression,
i686/aarch64 compile checks, Rust 1.85 MSRV, cargo-deny, machine UI acceptance, and the existing
visual-sanity capture. The generated visual sheet still requires human review. Push/PR hosted CI
is intentionally not used.

## Local Windows UI acceptance harness

Run the isolated release-path Control Center acceptance loop with one command:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/ui_acceptance.ps1
```

The harness refuses to run beside an existing `winshort.exe`, uses
`WINSHORT_DATA_DIR` for an isolated configuration root, exercises the real
second-instance Control Center activation and Win32 overlay path, captures
Control Center/picker/runtime-overlay PNGs, and writes a JSON summary below
`target/ui-acceptance-results/`.

## Diagnostics & support (AUTOMATED IN LOCAL GATE + MANUAL / HARDWARE-DEPENDENT)

Automated coverage (`src/diagnostics/support.rs`) verifies:

- Windows path redaction, including case-insensitive profile paths and private project folders.
- Report-local endpoint pseudonym stability and raw endpoint exclusion.
- Config projection preserves semantic fields without exporting raw endpoint IDs.
- Sensitive log fields (`last_key`, `recent_key`, `window_title`, `command_line`, and capture
  history) are removed before export.
- ZIP assembly contains only `diagnostics.txt`, `config.sanitized.toml`, bounded recent logs,
  and `bundle-info.txt`; the archive is readable and sanitized.

The bundle policy is newest three `winshort-*.log` files, at most 512 KiB per file and 2 MiB
total. Current-file read races are best-effort; skipped/truncated files are listed in the
manifest. Bundle work runs off the UI thread and is joined during shutdown.

Manual checks:

- Open Diagnostics & Support repeatedly; verify no extra window/resource accumulation.
- Copy Diagnostics and paste Unicode text into a text editor.
- Open Logs opens `%LOCALAPPDATA%\\WinShort\\logs` (or the configured fallback directory).
- Create Support Bundle produces a ZIP that Explorer can open; inspect the disclosure, manifest,
  sanitized config, and sanitized logs.
- Close Diagnostics or exit WinShort while a bundle is being created.

No automatic upload, telemetry, raw config attachment, or raw log attachment is implemented.

## Production logging (AUTOMATED)

Automated coverage verifies release/debug default levels, immediate runtime
level transitions, buffered Info/Debug writes, Warn/Error flush behavior,
dirty periodic flush state, exact-name 14-day retention, rollover, local
date/time formatting, future/malformed file preservation, support-bundle
flush integration, panic emergency persistence, and panic-path sanitization.
The panic acceptance uses a deterministic child test process; no machine
timezone mutation or production crash flag is used.

## Control Center interaction and accessibility (AUTOMATED IN LOCAL GATE + MANUAL / HARDWARE-DEPENDENT)

Automated coverage:

- Picker geometry chooses below/above placement and clamps to the Control Center client
  viewport, including DPI-scaled anchors.
- Device pickers expose only active real endpoints; disconnected legacy explicit bindings remain configured but are not selectable system targets.
- Endpoint roles are disabled when an explicit device is selected.
- Reset requires two activations and changes draft state only.
- Restored Control Center rectangles retain the fixed logical window size, use
  target-DPI scaling, and clamp only the saved position to the selected work area.
- Custom Control Center UIA snapshot nodes expose logical control types, names/help, bounds,
  offscreen, enabled/focus state, toggle state, and slider range/value semantics without
  inventing child HWNDs for painted rows.
- Direct provider ABI tests verify S_OK/null unsupported patterns, navigation boundaries, hosted
  root and child RuntimeIds, root-only host providers, truthful Button/Invoke mappings, read-only
  ValuePattern failure, and root/child/outside point queries.
- UIA focus tests distinguish Control Center HWND, native picker LISTBOX, and outside focus;
  focus actions are queued to the Control Center HWND. Snapshot publication filters property
  events to actual focus, toggle, slider value, enabled, offscreen, bounds, name, and displayed
  value changes.
- UIA actions are queued to the Control Center HWND; UIA `SetFocus` publishes actual Control
  Center focus after Win32 confirms it, while an open child picker publishes native LISTBOX
  focus without transferring top-level foreground ownership.
- Native picker/listbox retains fixed-order keyboard navigation, generation-checked close, and
  idempotent commit/cancel behavior; picker typography, hover, client placement, child-host
  style, and DPI policies are pure-tested.

- UIA publication is two-phase: the Control Center UI `RefCell` may commit and queue state
  changes, but only the later borrow-free Control Center flush may simulate UIA delivery. Tests
  cover provider re-query during flush, initial-notification suppression, and target/property
  coalescing.
- Raw property tests verify `VT_EMPTY` for inapplicable values, BSTR/BOOL values for ValuePattern,
  and normal Boolean pattern-availability properties. COM identity tests verify root-only
  FragmentRoot support, shared root identity, and stable child navigation.
- Invoke event tests verify one deferred Invoked notification per accepted Button action; picker
  construction tests verify HWND registration, `WS_CHILD` hosting, no foreground transfer, and
  direct focus into the real Control-Center-owned LISTBOX.

- Direct provider HRESULT tests distinguish disabled (`UIA_E_ELEMENTNOTENABLED`), stale
  (`UIA_E_ELEMENTNOTAVAILABLE`), unsupported (`UIA_E_NOTSUPPORTED`), and invalid argument
  (`E_INVALIDARG`) paths without changing the live unsupported-property `VT_EMPTY` contract.
- The `nullable_provider_abi_regression` test calls each successful-null COM output path through
  its raw vtable and runs in both normal and release profiles; the final local gate explicitly
  runs the release-profile case.
- Control Center regression seams verify direct parent-wheel picker dismissal, outside-click and
  focus-loss closure, close ordering before parent hide, pending activation blocking, and focus
  repair when a local mutation disables the current control.
- Managed shortcut coverage verifies assigned, disabled, re-enabled, changed-while-disabled,
  and unassigned chords; disabled chords stay out of the active binding table.
- Bounded value rendering tests verify chevron reservation and DirectWrite trailing-character
  trimming; applied status text remains generic.
- Control Center layout tests cover removal of the Workspaces desktop strip, Special Desktop
  hierarchy, exact sibling ROW_GAP/SECTION_CONTENT_GAP geometry across Audio, Home, and
  Workspaces, shared right-side control widths, the separate 80 DIP sidebar brand row,
  one-row Search/Close chrome, one Close action, fixed-window capability geometry, the
  side-by-side Overlay placement with equal Preview/Position dimensions, and the
  equal-width Overlay status row.
- Search caret geometry tests cover a visible empty-field caret and a text-end caret;
  renderer palette tests cover complete dark/light brush tables and transactional theme
  replacement.
- Overlay show-plan coverage verifies mutable overlay state is released before
  reentrant SetWindowPos/ShowWindow work; WM_SIZE remains the D2D resize path.
- Top-chrome separator geometry starts at the content boundary and never intersects
  the sidebar brand row.
- Overlay preview coverage keeps the monitor frame content-free except for the
  normalized status-card silhouette; Position moves only that silhouette.
- Compact Monitor-row coverage verifies complete `Overlay location` helper text,
  aligned picker geometry, and readable Primary/Cursor values.
- Search focus coverage verifies blank-click clearing, transfer to another control,
  external focus loss, stale Search guards, and caret/predicate agreement.
- Overlay monitor picker tests expose exactly Primary and Cursor position; legacy foreground
  config values parse to Cursor and serialize as `cursor`.
- Overlay geometry tests cover equal Preview/Position columns, dynamic monitor aspect fitting,
  monitor-as-preview containment, normalized position semantics, and the absence of a nested
  preview-container surface.
- Motion tests cover hover/toggle channels only; wheel and Page Up/Down scroll update the model
  directly without a Scroll channel, target, or timer tween.
- Fixed-window tests cover a DPI-scaled constant size, blocked minimize/maximize/resize system
  commands, no non-client resize hit zones, no double-click maximize path, and Close-only UIA
  Button/Invoke exposure.
- Overlay policy tests cover opaque fallback for High Contrast, disabled overlapped content,
  and unavailable DWM backdrop APIs.
- Audio presentation tests cover canonical primary endpoint names with diagnostic-only adapter
  metadata.
- Navigation/search tests verify case-insensitive deterministic user-concept matching and reject
  internal configuration names from the normal search index.
- Onboarding policy tests verify that a meaningful existing `config.toml` suppresses the
  first-run flow and that UI state remains separate from configuration.

Manual matrix:

- Scroll behavior: wheel notches update the viewport immediately and accumulate across rapid
  input with no queued/tweened motion; Page Up/Down update immediately; scrollbar dragging is
  direct and reduced-motion settings do not alter scrolling.
- Sidebar brand geometry: the app mark and one-line WinShort label share the exact brand-row
  vertical center.
- Top chrome: the search field and sole Close button share one row; the gap is draggable caption
  space, while Search and Close remain HTCLIENT.
- Section rhythm: each divider sits in whitespace between sections, every cyan accent is
  vertically centered on its title line, and sibling cards/rows use exact ROW_GAP.
- Workspaces page: no Normal desktops strip is present; numbered 1–9 shortcut configuration
  remains available; Special Desktop has one heading, description, and two evenly spaced
  shortcut cards.
- Control widths: dropdowns, keycaps, and value controls share the same right-side width and
  alignment; managed shortcut actions divide that same column.
- Audio iconography: the microphone reads as a capsule microphone with stem/base, not a speaker,
  and the same glyph is used on Home, Audio navigation, and overlay surfaces.
- Shortcut iconography: the navigation glyph reads as a rounded keyboard with upper-row key
  marks and a lower spacebar line, not arbitrary plus/hash marks or a dot box.
- Audio naming: speaker/microphone controls consistently show only canonical primary names such
  as GS25F2, SAMSUNG, and SIMGOT EW300 DSP; adapter metadata is absent from normal visible rows.
- Current app audio: the section keeps its shortcut controls available but omits
  a large empty status card when no external target is available.
- Overlay preview: the fixed-size window always places the monitor preview beside Position.
  The status row splits Show status overlay and Monitor into equal halves. Primary and Cursor
  position targets use the correct work-area ratio without stretching, and the monitor frame
  is the sole preview surface.
- Overlay card styles: System, Dark, and Light each render one status card only,
  with no outer slab, halo, or duplicate shadow.
- Overlay controls display the actual 0.7×–1.6× size multiplier, and Blur uses one
  five-stop slider with keyboard and pointer transitions across all five treatments.
- Custom titlebar: the fixed-size window has one blank draggable top region and one compact Close
  button. Minimize, Maximize/Restore, resize edges/corners, double-click maximize, and Snap
  Layout are unavailable; DPI, dark/light/high-contrast themes, and rounded corners remain
  intact. Switching between dark and light must leave the Control Center alive.
- Close lifecycle: clicking Close cancels any picker first, discards only the uncommitted UI draft
  under existing policy, hides Control Center with SW_HIDE, preserves the process/tray, and
  allows the tray Open action to show the same fixed-size window again.
- Overlay backdrop: Transparent disables the material, Light/Medium/Heavy blur
  select distinct Gaussian blur treatments, and Solid renders an opaque accessible
  card. High Contrast, disabled overlapped content, API failure, and
  transparency-disabled states use an opaque accessible surface.
- Native picker: the child host appears above D2D content, the real LISTBOX owns
  keyboard focus, arrows/Home/End/PageUp/PageDown/Tab/Escape remain deterministic,
  and outside click/focus loss closes it.
- Focus and automation: pointer clicks avoid a lingering heavy ring; clicking Search shows a
  visible caret and reports editable focus truthfully; Tab, Shift-Tab, keyboard activation,
  titlebar commands, and UIA focus remain truthful; UIA events arrive only after mutable
  SettingsUi borrows are released.

If an interactive desktop is unavailable, GUI, blur, titlebar, and Narrator results remain
unverified; automated geometry/state tests must not be described as live accessibility evidence.

## A. Keyboard engine (AUTOMATED IN LOCAL GATE + PROPERTY TEST)

Pure harness: `KeyboardEngine` fed `RawKeyEvent`s, asserting emitted
`EngineOutcome { Pass, Swallow, Dispatch { action, dirty_win_chord } }`.

Deterministic Win-key matrix (`engine_tests.rs`):

| Sequence | Expected |
|---|---|
| LWIN down/up | tap; no dispatch, no swallow |
| LWIN down, 1 down/up, LWIN up | SwitchDesktop(1) once; digit swallowed; Win up passes |
| RWIN variants of the above | identical via right side |
| Win+9 / Win+2 | SwitchDesktop(9/2) |
| LWIN down, E down/up, LWIN up | nothing swallowed, no dispatch |
| Win+R, Win+D, Win+L, Win+Shift+S | pass through untouched |
| rapid Win+1 → Win+2 → Win+3 | three dispatches in order |
| hold Win, hold 3 | exactly one dispatch despite autorepeat |
| modifiers released in odd orders | no stray dispatch/suppression |
| config swapped while Win held | next event uses new table |
| injected Win+1 | passed to chain, ignored by binder |
| autorepeat on bound key after match | repeats swallowed, zero extra dispatches |
| Ctrl+Alt+M fires; Ctrl+Alt+Shift+M does not (exact match) | as stated |
| overlapping LWin/RWin over held digit (#57) | per-side UP debt; both Win-ups swallowed |

Property invariants (`engine_props.rs`, #37): A disposition symmetry (UP mirrors DOWN's
classification; pinned regression seeds), B quiescence after full release, C autorepeat bounds,
D modifier-release permutations never dispatch, E reset from any reachable prefix yields fresh
state.

Start-menu countermeasure: `dirty_win_chord` flag posted with the action; main-thread
`dispatcher::dirty_win_chord()` injects the VK_CONTROL pair (KEYBOARD_HOOK_DESIGN.md).

## B. Binding table / parsing (AUTOMATED IN LOCAL GATE)

parse/display round-trips for every supported key; conflict detection errors name both actions;
invalid combos rejected (modifier-only, no-modifier #35, empty token #58); case-insensitive
parse; canonical display ordering; numpad distinct from top row (#11).

## C. Config (AUTOMATED IN LOCAL GATE + PROPERTY TEST)

defaults load when file missing; corrupt file → defaults + warning; schema v2 → v3 migration
preserves existing values and leaves the four new hotkeys unassigned; schema v9 → v10 migration
preserves existing values and defaults the new disabled list to empty; schema v10 → v11 migration
migrates legacy opacity/external-audio settings and defaults blur/category notifications; validation
violations and repair idempotence
(`config_props.rs`: endpoint-ID round-trips over generated opaque IDs, boundary repair
idempotence); future-schema read-only latch (deterministic + arbitrary TOML fuzz strategy proving
latched configs never enable writes); atomic save leaves no temp residue; round-trip
serialize→parse equality; unknown-field warnings; failed persistence never publishes a new
`ConfigHandle` snapshot and successful persistence increments its revision once.

## D. Audio matrix (MANUAL / HARDWARE-DEPENDENT)

mute/unmute default mic · change default mic while running · disconnect mic (state event, no
crash) · output device switch (Windows default/device-cycle feedback) · external volume change
arrives via callback (own events filtered) · foreground app mute · foreground app volume ±5% with
clamping · app without audio ("no audio session") · multi-session app (aggregate Mixed→mute-all) ·
app exits mid-enumeration · audio service restart (`net stop audiosrv`) → endpoints rebuild ·
same-basename different installations (ambiguous refusal) · same-full-path independent instances
(accepted limitation) · input/output cycle through active real endpoints and set all three Windows
default roles, including unavailable-endpoint recovery and duplicate friendly names.

Policy-level properties run in the local gate: resolver ladder grouping invariants (#37).

## E. Virtual desktop matrix (live; policy AUTOMATED IN LOCAL GATE)

Win+1 from desktop N≠1 lands on 1 · already-there no-op · rapid sequences · desktop
add/remove/reorder between switches · Task View switch then Win+number · unsupported build →
fallback active + status line. Policy properties in the local gate: semantic errors never fall back,
only RPC/backend-unavailable permits one bounded retry then fallback (#37). Real COM switching
remains MANUAL (build-pinned; see VIRTUAL_DESKTOP_COMPAT.md tested-results table).

## F. Overlay / DPI matrix (MANUAL / HARDWARE-DEPENDENT)

crispness at DPI 100–200% · single/multi monitor · per-monitor DPI moves incl. 150%→100%
(#49) · foreground-monitor follow · over fullscreen game (never steals focus) · rapid updates
coalesce with timer reset · negative virtual-screen coordinates · monitor unplug/replug.

- The runtime overlay uses the existing non-layered Composition-backed window and
  Gaussian blur effect graph. The selected blur treatment must visibly change the
  material; fallback conditions must remain opaque and accessible.
- Preview uses unsaved draft appearance, scale, blur, position, monitor, and
  duration without saving. Per-category notification toggles filter microphone,
  speaker, current-app audio, workspace, and display-profile events; WinShort
  actions and status requests remain subject to those category settings. Delayed
  status results refresh the multi-row status presentation. Coalesced updates
  preserve the full settled hold.

Screenshot-driven QA automation is **planned**, not implemented (no `--debug-screenshot-*`
flag exists).
## G. Process-level smoke (MANUAL / HARDWARE-DEPENDENT)

release exe launches silently · tray icon present · double-click opens one Control Center window ·
close keeps process alive · Exit removes icon and process ends · second launch activates first
instance and exits · idle CPU ≈ 0.

## Fuzzing note (platform reality)

`cargo-fuzz`/libFuzzer requires nightly plus a sanitizer runtime and is not supported for
windows-msvc targets in this repository's setup. Implemented instead: **bounded
arbitrary-input strategies via proptest** — keyboard event sequences (engine invariants) and
arbitrary TOML documents against config loading (no panic, bounded time, latched configs never
writable). Hotkey strings are exercised through those TOML documents; the hotkey parser itself
has deterministic round-trip coverage. These run as normal tests in the local gate; no continuous
fuzz campaign is claimed or running.

## Manual regression matrix

### Keyboard
- Win+E / Win+R / Win+D / Win+L / Win+Shift+S
- Win+1..9 desktop switching
- Digit-first Win+number (digit held before Win)
- Overlapping LWin/RWin over a held digit (#57)
- AltGr typing on a real AltGr layout (e.g. German)
- Hotkey recorder capture of an already-bound hotkey (conflict error)
- Rapid recorder cancel → re-arm → keypress (#47/#48 race window)

### Lifecycle
- Second instance while running (activation)
- Second instance launched during shutdown
- Repeated start/exit cycles
- Lock/unlock (engine reset)
- Sleep/resume

### Tray
- Menu open/close ×20 (GDI/handle stability)
- Explorer restart → tray recreation

### Audio
- Input/output mute toggles
- Default output device change
- Device unplug/replug
- Windows Audio service restart
- Multi-session foreground app (browser with media)
- Same-basename different installations (#46 ambiguous case)
- Same-full-path independent instances (accepted limitation)
- Next microphone/Next speaker hotkeys change the Windows system default for all three roles
- Foreground volume up/down changes only matched application sessions by 5 percentage points
- Control Center allowlist picker selects multiple input/output devices and offers explicit
  Use all available devices and Disable cycling controls
- An allowlisted device unplugged from Windows is skipped without losing its configured ID;
  reconnect notification makes it eligible again

### Virtual desktop
- Configurable numbered modifier family and `1..9` action derivation
- Centralized conflicts between numbered families, previous-desktop, and ordinary hotkeys
- Desktop 1 → Desktop 9 creates only missing desktops through the native backend
- Unsupported native creation reports a capability error without synthetic creation
- Move foreground + follow restores focus to the moved HWND
- Silent foreground move leaves the source desktop active
- Per-desktop last-focused HWND tracking excludes WinShort, shell, invisible, stale, and cloaked windows
- Previous-desktop toggles back and forth and clears deleted identities
- Send foreground window → Special Desktop moves it off the current normal desktop without hiding it
- Multiple sent windows coexist on the same dedicated Special Desktop
- Toggle from a normal desktop → Special Desktop → toggle again returns to that exact normal desktop
- Focus another application before entering the Special Desktop; the real desktop switch exposes usable workspace windows without hide/show focus hacks
- Numbered Desktop 1..9 ordinals exclude the Special Desktop, including missing-normal-desktop creation
- Previous Desktop history is not polluted by entering/leaving the Special Desktop
- Disable the feature or exit cleanly removes the Special Desktop and Shell relocates its windows to a normal fallback desktop
- External deletion of the Special Desktop clears stale runtime identity and the next use creates a fresh workspace
- Unsupported/native-failed builds report Special Desktop unavailable; keyboard fallback never simulates its create/move/toggle semantics
- Hard process termination or Windows reboot may leave the dedicated VD alive; relaunch must reclaim the exact persisted GUID without creating a duplicate, while a missing GUID is treated as external deletion

### Display profiles
- Capture Current on single/multi-monitor topology persists stable target paths,
  source/target IDs, positions, modes, and detected topology kind
- Profile picker supports New from Current, Update from Current, Select, Rename,
  Duplicate with a new ID, Delete, and Save; rename preserves the profile hotkey
  reference and delete removes it
- Route editor accepts only bounded position, positive mode/refresh, and supported
  rotation values; edits clear confirmation
- Config validation rejects missing/ambiguous routes, invalid modes, incompatible
  clone/extend source shapes, duplicate IDs/names, and hotkey conflicts; repair
  removes malformed profiles and stale profile bindings
- Test Apply validates before mutation, applies temporarily, and exposes explicit
  Keep and Revert controls; timeout automatically restores the captured topology
- Failed apply/verification or failed rollback surfaces an error and never reports
  success; a failed rollback keeps Revert available
- Profile hotkeys resolve stable profile IDs after rename, reject stale/unconfirmed
  profiles, and reload through the lock-free binding snapshot

Manual Windows hardware acceptance remains required before closing #91:
iGPU HDMI → monitor; dGPU DP → same monitor; dGPU HDMI → TV; iGPU↔dGPU
switching; monitor + TV Extend; Duplicate where supported; disconnect/reconnect;
rejected invalid topology; explicit Keep/Revert; and automatic timeout rollback.
Record each scenario only after direct observation; unobserved scenarios remain pending.

### DPI / overlay
- 100% → 150% monitor move (scale up)
- 150% → 100% monitor move (scale down — #49 regression)
- Alternating overlay targets between monitors
- Negative virtual-screen coordinates
- Monitor unplug/replug during overlay
