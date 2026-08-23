# Keyboard Hook Design

## Structure

```
keyboard/
├── engine.rs      pure state machine — no Windows calls, fully unit-tested
├── keystate.rs    physical down-set + normalized modifier mask
├── binding.rs     ModifierMask, VirtualKey, BindingTable (HashMap exact match)
├── hook.rs        WH_KEYBOARD_LL install, message loop, KBDLLHOOKSTRUCT -> RawKeyEvent
└── dispatcher.rs  posts recognized actions to the main window
```

`engine.rs` and `keystate.rs`/`binding.rs` compile without any `windows` import so
`cargo test` runs them on any host.

## Hook rules

* Installed once at startup. Config changes swap an `Arc<BindingsSnapshot>` read by the engine;
  the hook is reinstalled **only** if `SetWindowsHookExW` fails or a persistent
  `CallNextHookEx`-chain error is detected.
* Callback work: map `KBDLLHOOKSTRUCT` → `RawKeyEvent { vk, scan_code, up, injected, extended }`,
  feed engine, optionally post one `PostMessageW`, return. No allocation, no locks beyond the
  snapshot `Arc` clone (lock-free atomic), no COM/files/logging.
* Injected events (`LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED`) are passed straight through:
  never bound-matched, never suppressed → no hotkey→SendInput→hook recursion.
* Matched bindings return non-zero from the callback to suppress delivery of that key event.
  Everything else continues via `CallNextHookEx`.
* Repeat: trigger fires only when the key's physical state transitions up→down; autorepeat
  (`down` while already down) never re-fires but *is* suppressed for matched bindings.

## Modifier model

Left/right tracked distinctly from `VK_LCONTROL/VK_RCONTROL/VK_LMENU/VK_RMENU/VK_LWIN/VK_RWIN`
(LL hook reports right-ctrl/right-shift as their L/R virtual keys with `LLKHF_EXTENDED`;
right-alt arrives as `VK_RMENU`). Normalized mask collapses sides for matching generic bindings;
a side-specific bit exists in `ModifierMask` for future per-side bindings.

Matching is **exact**: current normalized mask must equal the binding's mask. Extra held
modifiers mean a different chord.

## Win-key state machine

States: `Idle`, `WinOnly { since }`, `Chord`.

| Event | Idle | WinOnly | Chord |
|---|---|---|---|
| Win down | → WinOnly | (other side tracked) | stays |
| other key down | normal | → Chord (+binding check first) | binding check |
| Win up | normal | **tap**: shell will show Start — see below | → Idle, pass through |
| all others released while Win still down | | stays Chord until Win up? no: → WinOnly | |

Start-menu suppression for the internal (COM) backend: swallowing the digit means the shell sees
a clean Win tap and opens Start after Win release. Countermeasure executed at recognition time,
while Win is physically down: inject a harmless `VK_CONTROL` down+up pair (`SendInput`). The shell
then sees Win+Ctrl activity → chord consumed → no Start. Residual race (user releases Win within
the injection latency window, ~1 ms) is accepted and documented; the fallback backend has no such
race because its switch input itself dirties the chord.

For the keyboard-fallback backend we inject `Ctrl+Win+Arrow` while the physical Win is already
down; the injected events are ignored by our own hook (INJECTED flag) and naturally consume the
chord.

Win taps with no binding hit (e.g. Win+E): engine marks Chord, passes everything through.
Win+Shift+S, Win+L etc. are untouched because they don't match the binding table
(`win_number_switching` covers digits only).

## Binding table updates mid-chord

The engine resolves the snapshot `Arc` once per event. If Save swaps bindings while modifiers are
held, the next event simply matches against the new table; no stale state exists because modifier
state lives in the engine, not the table.

## Dispatch

Recognized action → `PostMessageW(main_hwnd, WM_APP_ACTION, pack(action), 0)` from the hook
thread. The main thread routes:

* audio toggles → audio thread command channel
* `SwitchDesktop(n)` → desktop thread command channel (backend decides COM vs fallback)

The hook thread never performs the actions itself.
