# Test Plan

## A. Keyboard engine unit tests (pure, no Windows — run everywhere)

Harness: `keyboard::engine` fed `RawKeyEvent` sequences; assertions on emitted
`EngineOutput { dispatch: Option<Action>, swallow: bool }`.

Win-key matrix (§55):

| Sequence | Expected |
|---|---|
| LWIN down/up | tap; no dispatch, no swallow |
| LWIN down, 1 down/up, LWIN up | SwitchDesktop(1) once; digit swallowed; Win up passes |
| RWIN variants of the above | identical via right side |
| Win+9 / Win+2 | SwitchDesktop(9/2) |
| LWIN down, E down/up, LWIN up | nothing swallowed, no dispatch |
| Win+R, Win+D, Win+L, Win+Shift+S | pass through untouched |
| rapid Win+1 → Win+2 → Win+3 | three dispatches in order |
| hold Win, hold 3 | exactly one dispatch despite autorepeat events |
| 1 down first, then Win down, 1 already down? (press order digit-then-win with digit re-tap) | dispatch on completion transition |
| modifiers released in odd orders (digit released before Win etc.) | no stray dispatch/suppression |
| config swapped while Win held | next event uses new table |
| injected Win+1 | passed to chain, ignored by binder |
| autorepeat on bound key after match | repeats swallowed, zero extra dispatches |
| Ctrl+Alt+M binding fires and is suppressed; Ctrl+Alt+Shift+M does not fire it (exact-match rule) | as stated |

Start-menu countermeasure: recognition of Win+n while Win physically down emits
`inject_chord_dirtier` flag → verified injected VK_CONTROL pair order.

## B. Binding table / parsing tests

parse/display round-trips, conflict detection errors name both actions, invalid combos rejected
(modifier-only), case-insensitive parse, canonical display ordering.

## C. Config tests

defaults load when file missing; corrupt file → defaults + warning; validation rejects out-of-range
and conflicting hotkeys with actionable messages; atomic save leaves no temp residue; round-trip
serialize→parse equality; unknown fields tolerated.

## D. Audio matrix (live Windows, manual + logged smoke)

mute/unmute default mic · change default mic while running · disconnect mic (state event,
no crash) · output device switch (overlay "Output changed") · external volume change arrives via
callback · foreground app mute · app without audio ("No audio session") · multi-session app
(aggregate Mixed→mute-all) · app exits mid-enumeration · audio service restart (`net stop audiosrv`)
→ endpoints rebuild.

Automated where scriptable: a test binary mode exercising toggle paths against real endpoints,
asserting state flips and events posted.

## E. Virtual desktop matrix (live)

1/2/5/9 desktops · Win+1 from desktop N≠1 lands on 1 · Win+n while already there (no-op, no flicker)
· rapid sequences · desktop add/remove/reorder/rename between switches · Task View switch then
Win+number · unsupported-build simulation (force-detect off) → fallback backend active + status line
"Unsupported build" in diagnostics. Backend trait unit-tested with fake for index clamping.

## F. Overlay matrix (live)

DPI 100–200% crispness (screenshot QA) · single/multi monitor · per-monitor DPI moves ·
foreground-monitor follow · over fullscreen/borderless game (never steals focus — verified by focus
probe) · rapid updates coalesce into one window with reset timer · settings open simultaneously OK.

## G. UI quality gate (screenshot-driven)

`winshort --debug-screenshot-settings [light|dark] [--scale N]` renders the window off-screen and
writes a PNG; same for overlay states. Reviewed at 100%/150%/200%, dark/light. Checks: spacing grid,
alignment, hover/focus/disabled/error states captured programmatically where possible.

## H. Process-level smoke

release exe launches silently (no console) · tray icon present · double-click opens one settings
window · close keeps process alive · Exit removes icon and process ends · second launch activates
first instance and exits · idle CPU ≈ 0 (measured via typeperf over 30 s idle).

---

# Test classification (#37)

Every behavior in this plan is classified as one of:

- **AUTOMATED IN CI** — deterministic `cargo test` coverage running on every push/PR.
- **PROPERTY TEST** — proptest-generated coverage over an invariant domain (runs in CI).
- **FUZZ TARGET** — robustness strategy expressed as bounded arbitrary-input tests inside
  `cargo test` (see Fuzzing note below); no separate libFuzzer job.
- **MANUAL / HARDWARE-DEPENDENT** — requires a real desktop session, physical devices,
  or shell state; must be verified by hand per release.

## Fuzzing note (platform reality)

`cargo-fuzz`/libFuzzer targets require nightly + a sanitizer runtime and are not
supported for `windows-msvc` targets in this repository's setup. The practical
equivalent implemented here: **bounded arbitrary-input strategies via proptest**
against the hotkey parser and config TOML parser (no panic, bounded time), executed
as normal `#[test]`s in CI. These run on every push; no continuous fuzz campaign is
claimed.

## Manual regression matrix

### Keyboard
- Win+E / Win+R / Win+D / Win+L / Win+Shift+S
- Win+1..9 desktop switching
- Digit-first Win+number (digit held before Win)
- AltGr typing on a real AltGr layout (e.g. German)
- Hotkey recorder capture of an already-bound hotkey
- Rapid recorder cancel → re-arm → keypress (#47/#48 race window)
- Overlapping LWin/RWin over a held digit (#57)

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
