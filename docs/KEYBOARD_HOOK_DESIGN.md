# Keyboard Hook Design

**Status: Implemented** (runtime engine + hook); property/state-machine coverage per #37.
Recorder capture-mode design notes marked **Planned** where behavior is not yet built.

## Structure

```
keyboard/
├── engine.rs        pure state machine — no Windows calls
├── keystate.rs      physical down-set + normalized modifier mask
├── binding.rs       ModifierMask, VirtualKey, BindingTable (HashMap exact match)
├── hook.rs          WH_KEYBOARD_LL thread, capture state machine, service handle
├── dispatcher.rs    main-thread Start-menu countermeasure injection
├── engine_tests.rs  spec Win-key matrix unit tests   ([cfg(test)])
└── engine_props.rs  proptest invariants A–E (#37)    ([cfg(test)])
```

`engine`, `keystate`, and `binding` compile without any `windows` import so `cargo test` runs
them on any host.

## Hook rules

* Installed once at startup on the keyboard thread. Config changes swap an
  `arc_swap::ArcSwap<BindingTable>` read by the engine (`ConfigHandle::bindings()` — wait-free
  load, no HashMap rebuild in the callback, #10). The hook is reinstalled only if
  `SetWindowsHookExW` fails.
* Callback path: map `KBDLLHOOKSTRUCT` → `RawKeyEvent { vk, scan_code, up, injected, extended }`,
  feed engine, optionally post one `PostMessageW(WM_APP_ACTION, …)`, return. No Mutex/RwLock,
  no COM, no file/registry/logging. The single heap allocation on this path is a small bounded
  `Vec<u16>` of currently-held non-modifier keys, collected only in the digit-first completion
  arm when Win goes down with keys already held; everything else uses fixed-capacity storage
  (`[bool; 256]` bitmap, `[Option<u8>; 8]` sets).
* Injected events (`LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED`) are passed straight through:
  never bound-matched, never suppressed → no hotkey→SendInput→hook recursion. This also makes
  the countermeasure's own synthetic input self-invisible.
* Matched bindings return non-zero from the callback to suppress that key event; everything
  else continues via `CallNextHookEx`.
* Repeat: trigger fires only on physical up→down transition; autorepeat never re-fires but *is*
  suppressed for matched bindings.

## Engine state model

There is **no explicit Idle/WinOnly/Chord enum** — chord state is implicit in five fields
(src/keyboard/engine.rs), all cleared by `reset()`:

| Field | Meaning |
|---|---|
| `state: KeyState` | physical down bitmap + insertion-ordered held non-modifiers |
| `passthrough_while_win` | a non-modifier event was PASSED since Win went down (shell saw chord activity) |
| `win_ups_to_swallow: u8` | per-side Win-UP debt: bit0 = LWin, bit1 = RWin (digit-first completions, #57) |
| `altgr_active` | AltGr signature present — skip binding lookups (#35/#44) |
| `suppressed_ups` | VKs whose DOWN was swallowed; their UP must be swallowed too |

## Modifier matching

Left/right tracked distinctly (`VK_LCONTROL/VK_RCONTROL/VK_LMENU/VK_RMENU/VK_LWIN/VK_RWIN`;
right-ctrl/right-shift arrive as L/R virtual keys with `LLKHF_EXTENDED`). The normalized mask
collapses sides for matching generic bindings. Matching is **exact**: current mask must equal
the binding's mask; extra held modifiers mean a different chord.

## Digit-first completion and per-side Win debt (#57)

When Win goes down while non-modifier keys are already held, each held key is looked up as a
completed chord ("digit-first"): the action dispatches on the Win DOWN, the completing side's
debt bit arms (`win_ups_to_swallow |= 1 for LWin / 2 for RWin`), and `dirty_win_chord` is
*false* because the entire Win press cycle will be hidden from the shell.

Each Win-UP consumes exactly its own side's bit before anything else; while the other side is
still physically held its UP passes normally (#5 invariant: a passed key's UP passes). With both
Wins held over one digit, both sides arm their own bit and both UPs are swallowed — no lone
Win-UP ever reaches Explorer. Property invariant A plus a pinned shrunk counterexample
(`proptest-regressions/keyboard/engine_props.txt`) guard this.

## Start-menu countermeasure

For ordinary (non-digit-first) Win-chord dispatches where the shell has seen nothing but a bare
Win-down, recognition sets `dirty_win_chord` and posts it in the `WM_APP_ACTION` LPARAM. The
**main thread** then runs `dispatcher::dirty_win_chord()`: exactly one `INPUT_KEYBOARD` event
with `wVk = 0xFF` and `KEYEVENTF_KEYUP`. This non-semantic dummy event follows the pattern used
by Microsoft's PowerToys centralized keyboard hook: the shell sees Win-chord activity → chord
consumed → no Start menu on release. The `SendInput` count is checked directly; there is no
modifier partial-send cleanup because the dirtier presses no modifier. Residual race: the
injection happens on the main thread shortly after dispatch, so an extremely fast physical Win
release can still beat it; accepted and logged. Digit-first completions bypass the countermeasure
entirely — their full Win cycle is swallowed instead.

The keyboard-fallback backend needs no countermeasure: its injected `Ctrl+Win+Arrow` events are
self-ignored (INJECTED flag) and naturally consume the chord.

## AltGr heuristic (#35/#44)

`altgr_active = RMENU down && (LCTRL or RCTRL down)`, recomputed from live state after every
modifier change so both arrival orders work and it clears on either release. While active,
binding lookups are skipped entirely so layout characters never fire hotkeys; keys still pass
through. Accepted heuristic: AltGr layouts always pair RAlt with a Ctrl.

## Capture mode (hotkey recorder)

Global recorder state lives in `CAPTURE_STATE: AtomicU64` (src/keyboard/hook.rs) — layout
`[generation u32][state 2 bits][chord 24 bits]`, states Inactive/Armed/Completed. All ops are
CAS loops:

* `begin_capture()` bumps the generation and arms; `end_capture()` disarms same-generation.
* First supported non-modifier DOWN completes the session (Esc completes a cancel marker);
  `complete_capture` CAS fails for stale generations, so a stale event can never complete or
  disarm a newer session (#47).
* `take_captured_chord()` consumes Completed → Inactive exactly once (mailbox polled by the UI).
* Disposition is independent of completion: unsupported/unclassifiable events pass; every
  supported-domain event in a live-or-stale capture window is swallowed (#48).
* Runtime engine state and recorder state are separate: `capture_event` feeds its own engine
  instance and resets it after completion; production reads use `capture_token_live()`.

This is process-global state; tests serialize via the shared latch guard (#47).

## Lifecycle resets (#13/#42)

`App::reset_keyboard_state` posts `WM_APP_RESET_STATE` to the keyboard thread, which resets its
engine in place (never cross-thread memory writes). Triggers:

* `WM_WTSSESSION_CHANGE` lock (7) / unlock (8)
* `WM_POWERBROADCAST` suspend / resume-automatic / resume-suspend
* tray suspend/resume toggle

Teardown order on the keyboard thread after its message loop exits: drop `HookGuard`
(`UnhookWindowsHookEx`) → null the `HOOK_STATE` pointer → free the boxed state. The state
pointer is published *before* `SetWindowsHookExW` and treated as pass-through when null, so a
callback can never observe freed memory (#42).

## Dispatch

Recognized action → `PostMessageW(main_hwnd, WM_APP_ACTION, pack(action), dirty_flag)` from the
hook thread. The main thread routes:

* audio toggles → audio thread command channel
* `SwitchDesktop(n)` → desktop thread command channel (backend decides COM vs fallback)

The hook thread never performs actions itself.

## Verification

Invariants A–E are proptested (`engine_props.rs`, #37): disposition symmetry, quiescence after
full release, autorepeat bounds, modifier-release permutations, reset-from-any-prefix. The
deterministic Win-key matrix lives in `engine_tests.rs`. Fuzzing platform reality:
cargo-fuzz/libFuzzer is unavailable for windows-msvc here; arbitrary-input robustness is covered
by bounded proptest strategies (see TEST_PLAN.md).
