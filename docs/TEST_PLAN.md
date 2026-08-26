# Test Plan

## Classification (#37)

Every behavior in this plan is classified as one of:

- **AUTOMATED IN CI** — deterministic `cargo test` coverage running on every push/PR.
- **PROPERTY TEST** — proptest-generated coverage over an invariant domain (runs in CI).
- **FUZZ-STRATEGY / arbitrary-input property coverage** — bounded arbitrary-input tests inside
  `cargo test` (see Fuzzing note below); no separate libFuzzer job.
- **MANUAL / HARDWARE-DEPENDENT** — requires a real desktop session, physical devices,
  or shell state; must be verified by hand per release.

Current verified hosted baseline: Windows runners execute fmt, clippy `-D warnings`, the full
`cargo test` suite (moving count; check CI for the live count), x86_64 release build with
embedded-manifest byte-check, i686 and aarch64 compile checks, an MSRV 1.85 job, and cargo-deny.
See `.github/workflows/ci.yml`.

## Diagnostics & support (AUTOMATED IN CI + MANUAL / HARDWARE-DEPENDENT)

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
## Settings interaction and accessibility (AUTOMATED IN CI + MANUAL / HARDWARE-DEPENDENT)

Automated coverage:

- Picker geometry chooses below/above placement and clamps to work areas, including negative
  coordinates.
- Explicit unavailable devices remain in picker choices; their opaque selection is not rewritten.
- Endpoint roles are disabled when an explicit device is selected.
- Reset requires two activations and changes draft state only.
- Restored Settings rectangles are fully contained in the selected work area and use target-DPI
  scaling.
- Custom Settings UIA snapshot nodes expose logical control types, names/help, bounds, offscreen,
  enabled/focus state, toggle state, and slider range/value semantics without child HWND creation.
- Direct provider ABI tests verify S_OK/null unsupported patterns, navigation boundaries, hosted-root
  and child RuntimeIds, root-only host providers, truthful Button/Invoke mappings, read-only
  ValuePattern failure, and root/child/outside point queries.
- UIA focus tests distinguish Settings HWND, native picker LISTBOX, and outside focus; focus actions
  are queued to the Settings HWND. Snapshot publication filters property events to actual focus,
  toggle, slider value, enabled, offscreen, bounds, name, and displayed-value changes.
- UIA actions are queued to the Settings HWND; UIA `SetFocus` publishes actual Settings focus
  after Win32 confirms it, while an open picker publishes native LISTBOX focus.
- Native picker/listbox retains fixed-order keyboard navigation, generation-checked close, and
  idempotent commit/cancel behavior; picker typography, hover, geometry, and DPI policies are pure-tested.

- UIA publication is two-phase: a SettingsUi `RefCell` may commit and queue state changes, but
  only the later borrow-free Settings HWND flush may simulate UIA delivery. Tests cover provider
  re-query during flush, initial-notification suppression, and target/property coalescing.
- Raw property tests verify `VT_EMPTY` for inapplicable values, BSTR/BOOL values for ValuePattern,
  and normal Boolean pattern-availability properties. COM identity tests verify root-only
  FragmentRoot support, shared root identity, and stable child navigation.
- Invoke event tests verify one deferred Invoked notification per accepted Button action; picker
  construction tests verify HWND registration before activation and direct Settings-to-Picker focus.

Manual matrix:

- Input/output picker: Default, available explicit device, disconnected explicit device.
- Hover/focus non-obvious controls and verify delayed native help text closes on pointer/focus change.
- Role rows enable only for Default and announce the reason when disabled.
- Overlay position and monitor picker: Foreground, Primary, each `Device(String)`, disconnected
  configured monitor, long labels, popup above/below and DPI changes.
- Mouse and keyboard: Tab/Shift-Tab, Enter, Space, Escape, arrows, Home/End, Page Up/Down,
  picker focus loss, hotkey recorder transitions, Save, Cancel, and reset confirmation.
- Settings restore after restart, removed monitor, negative coordinates, 100/125/150/200% DPI.
- Narrator or Accessibility Insights: Settings window, toggle names/values, picker selection,
  disabled role help, slider range/value, Save/Cancel, and sensible focus order.
- Dark/light themes, large DPI, focus visibility, and disconnected-device presentation.

If an interactive desktop is unavailable, GUI and Narrator results remain unverified; automated
geometry/state tests must not be described as live accessibility evidence.

## A. Keyboard engine (AUTOMATED IN CI + PROPERTY TEST)

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

## B. Binding table / parsing (AUTOMATED IN CI)

parse/display round-trips for every supported key; conflict detection errors name both actions;
invalid combos rejected (modifier-only, no-modifier #35, empty token #58); case-insensitive
parse; canonical display ordering; numpad distinct from top row (#11).

## C. Config (AUTOMATED IN CI + PROPERTY TEST)

defaults load when file missing; corrupt file → defaults + warning; validation violations and
repair idempotence (`config_props.rs`: endpoint-ID round-trips over generated opaque IDs,
boundary repair idempotence); future-schema read-only latch (deterministic + arbitrary TOML
fuzz strategy proving latched configs never enable writes); atomic save leaves no temp residue;
round-trip serialize→parse equality; unknown-field warnings.

## D. Audio matrix (MANUAL / HARDWARE-DEPENDENT)

mute/unmute default mic · change default mic while running · disconnect mic (state event, no
crash) · output device switch (overlay "Output changed") · external volume change arrives via
callback (own events filtered) · foreground app mute · app without audio ("no audio session")
· multi-session app (aggregate Mixed→mute-all) · app exits mid-enumeration · audio service
restart (`net stop audiosrv`) → endpoints rebuild · same-basename different installations
(ambiguous refusal) · same-full-path independent instances (accepted limitation).

Policy-level properties run in CI: resolver ladder grouping invariants (#37).

## E. Virtual desktop matrix (live; policy AUTOMATED IN CI)

Win+1 from desktop N≠1 lands on 1 · already-there no-op · rapid sequences · desktop
add/remove/reorder between switches · Task View switch then Win+number · unsupported build →
fallback active + status line. Policy properties in CI: semantic errors never fall back,
only RPC/backend-unavailable permits one bounded retry then fallback (#37). Real COM switching
remains MANUAL (build-pinned; see VIRTUAL_DESKTOP_COMPAT.md tested-results table).

## F. Overlay / DPI matrix (MANUAL / HARDWARE-DEPENDENT)

crispness at DPI 100–200% · single/multi monitor · per-monitor DPI moves incl. 150%→100%
(#49) · foreground-monitor follow · over fullscreen game (never steals focus) · rapid updates
coalesce with timer reset · negative virtual-screen coordinates · monitor unplug/replug.

#30 accessibility/source matrix: System/Dark/Light appearance, Windows animation-off yields
settled overlay with no fade/slide, high-contrast uses paired system colors with an opaque
surface/strong border/no shadow, overlapped-content preference removes translucency, and
setting changes refresh an already-visible overlay. Preview uses unsaved draft appearance,
scale, opacity, position, monitor, and duration without saving. External audio changes obey
the saved policy; WinShort actions and status requests remain visible. Delayed status results
refresh the multi-row status presentation. Coalesced updates preserve the full settled hold.

Screenshot-driven QA automation is **planned**, not implemented (no `--debug-screenshot-*`
flag exists).

## G. Process-level smoke (MANUAL / HARDWARE-DEPENDENT)

release exe launches silently · tray icon present · double-click opens one settings window ·
close keeps process alive · Exit removes icon and process ends · second launch activates first
instance and exits · idle CPU ≈ 0.

## Fuzzing note (platform reality)

`cargo-fuzz`/libFuzzer requires nightly plus a sanitizer runtime and is not supported for
windows-msvc targets in this repository's setup. Implemented instead: **bounded
arbitrary-input strategies via proptest** — keyboard event sequences (engine invariants) and
arbitrary TOML documents against config loading (no panic, bounded time, latched configs never
writable). Hotkey strings are exercised through those TOML documents; the hotkey parser itself
has deterministic round-trip coverage. These run as normal tests on every push; no continuous
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

### Virtual desktop
- Win+1..9 target changes (registry CurrentVirtualDesktop verification)
- Target beyond desktop count (must NOT inject fallback keys)
- External desktop add/remove/reorder
- Elevated foreground app / UIPI fallback refusal log

### DPI / overlay
- 100% → 150% monitor move (scale up)
- 150% → 100% monitor move (scale down — #49 regression)
- Alternating overlay targets between monitors
- Negative virtual-screen coordinates
- Monitor unplug/replug during overlay
