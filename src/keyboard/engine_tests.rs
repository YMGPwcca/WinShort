//! Unit tests for the pure keyboard engine (TEST_PLAN §A).
//!
//! Every sequence from the spec's Win-key matrix lives here. The engine is
//! Windows-free so these run on any host.

use super::binding::{BindingTable, Hotkey, ModifierMask, VirtualKey};
use super::engine::{EngineOutcome, KeyboardEngine, RawKeyEvent};
use crate::event::HotkeyAction;

fn table_with_win_digits() -> BindingTable {
    let mut t = BindingTable::default();
    for d in 1u16..=9 {
        t.insert(
            Hotkey { modifiers: ModifierMask::WIN, key: VirtualKey(0x30 + d) },
            HotkeyAction::SwitchDesktop((d - 1) as u8),
        );
    }
    t.insert(
        Hotkey {
            modifiers: ModifierMask::CTRL.union(ModifierMask::ALT),
            key: VirtualKey(b'M' as u16),
        },
        HotkeyAction::ToggleMicrophone,
    );
    t.insert(
        Hotkey {
            modifiers: ModifierMask::CTRL.union(ModifierMask::ALT),
            key: VirtualKey(b'O' as u16),
        },
        HotkeyAction::ToggleOutput,
    );
    t
}

const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_E: u16 = b'E' as u16;
const VK_R: u16 = b'R' as u16;
const VK_D: u16 = b'D' as u16;
const VK_L: u16 = b'L' as u16;
const VK_S: u16 = b'S' as u16;
const VK_SHIFT: u16 = 0xA0; // left shift normalized form arrives as VK_LSHIFT

fn feed(engine: &mut KeyboardEngine, table: &BindingTable, events: &[RawKeyEvent]) -> Vec<EngineOutcome> {
    events.iter().map(|e| engine.on_event(*e, table)).collect()
}

fn expect_dispatch(out: EngineOutcome, action: HotkeyAction, dirty: bool) {
    assert_eq!(out, EngineOutcome::Dispatch { action, dirty_win_chord: dirty }, "wrong outcome");
}

#[test]
fn win_tap_alone_does_not_dispatch_or_suppress() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(&mut e, &t, &[RawKeyEvent::down(VK_LWIN), RawKeyEvent::up(VK_LWIN)]);
    assert_eq!(out, vec![EngineOutcome::Pass, EngineOutcome::Pass]);
}

#[test]
fn win_plus_1_switches_desktop_1_and_suppresses() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(VK_LWIN),
            RawKeyEvent::down(b'1' as u16),
            RawKeyEvent::up(b'1' as u16),
            RawKeyEvent::up(VK_LWIN),
        ],
    );
    expect_dispatch(out[1], HotkeyAction::SwitchDesktop(0), true); // dirty: shell saw bare Win
    assert_eq!(out[0], EngineOutcome::Pass);
    assert_eq!(out[2], EngineOutcome::Swallow); // digit up hidden
    assert_eq!(out[3], EngineOutcome::Pass); // win up passes; dispatcher injected dirtier
}

#[test]
fn right_win_symmetric() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(
        &mut e,
        &t,
        &[RawKeyEvent::down(VK_RWIN), RawKeyEvent::down(b'2' as u16), RawKeyEvent::up(VK_RWIN)],
    );
    expect_dispatch(out[1], HotkeyAction::SwitchDesktop(1), true);
    assert_eq!(out[2], EngineOutcome::Pass);
}

#[test]
fn win_plus_9() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(&mut e, &t, &[RawKeyEvent::down(VK_LWIN), RawKeyEvent::down(b'9' as u16)]);
    expect_dispatch(out[1], HotkeyAction::SwitchDesktop(8), true);
}

#[test]
fn unbound_win_shortcuts_fully_pass_through() {
    for seq_key in [VK_E, VK_R, VK_D, VK_L] {
        let mut e = KeyboardEngine::new();
        let t = table_with_win_digits();
        let out = feed(
            &mut e,
            &t,
            &[RawKeyEvent::down(VK_LWIN), RawKeyEvent::down(seq_key), RawKeyEvent::up(seq_key), RawKeyEvent::up(VK_LWIN)],
        );
        assert!(
            out.iter().all(|o| *o == EngineOutcome::Pass),
            "{seq_key:#x} sequence must pass through untouched, got {out:?}"
        );
    }
}

#[test]
fn win_shift_s_passes_through() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(VK_LWIN),
            RawKeyEvent::down(VK_SHIFT),
            RawKeyEvent::down(VK_S),
            RawKeyEvent::up(VK_S),
            RawKeyEvent::up(VK_SHIFT),
            RawKeyEvent::up(VK_LWIN),
        ],
    );
    assert!(out.iter().all(|o| *o == EngineOutcome::Pass), "{out:?}");
}

#[test]
fn rapid_win_1_2_3_dispatch_three_times() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let mut events = vec![RawKeyEvent::down(VK_LWIN)];
    for d in ['1', '2', '3'] {
        events.push(RawKeyEvent::down(d as u16));
        events.push(RawKeyEvent::up(d as u16));
    }
    events.push(RawKeyEvent::up(VK_LWIN));
    let out = feed(&mut e, &t, &events);
    assert_eq!(out[1], EngineOutcome::Dispatch { action: HotkeyAction::SwitchDesktop(0), dirty_win_chord: true });
    assert_eq!(out[2], EngineOutcome::Swallow);
    assert_eq!(out[3], EngineOutcome::Dispatch { action: HotkeyAction::SwitchDesktop(1), dirty_win_chord: true });
    assert_eq!(out[4], EngineOutcome::Swallow);
    assert_eq!(out[5], EngineOutcome::Dispatch { action: HotkeyAction::SwitchDesktop(2), dirty_win_chord: true });
}

#[test]
fn holding_win_and_digit_fires_once_despite_autorepeat() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let repeat = RawKeyEvent { vk: b'3' as u16, extended: false, down: true, injected: false };
    let out = feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(VK_LWIN),
            RawKeyEvent::down(b'3' as u16),
            repeat,
            repeat,
            repeat,
            RawKeyEvent::up(b'3' as u16),
        ],
    );
    assert_eq!(out[1], EngineOutcome::Dispatch { action: HotkeyAction::SwitchDesktop(2), dirty_win_chord: true });
    assert_eq!(&out[2..5], &[EngineOutcome::Swallow, EngineOutcome::Swallow, EngineOutcome::Swallow]);
}

#[test]
fn digit_first_then_win_completes_and_hides_win_cycle() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(b'4' as u16), // leaks through (OS semantics identical)
            RawKeyEvent::down(VK_LWIN),     // completes chord -> switch, swallow win down
            RawKeyEvent::up(VK_LWIN),       // win up must ALSO be swallowed
            RawKeyEvent::up(b'4' as u16),
        ],
    );
    assert_eq!(out[0], EngineOutcome::Pass);
    expect_dispatch(out[1], HotkeyAction::SwitchDesktop(3), false);
    assert_eq!(out[2], EngineOutcome::Swallow);
    assert_eq!(out[3], EngineOutcome::Pass); // #5: down passed, so up passes too
}

#[test]
fn modifiers_released_in_strange_orders_stay_consistent() {
    // Win down, 5 down, 5 up, Win up — then again reversed order.
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let first = feed(
        &mut e,
        &t,
        &[RawKeyEvent::down(VK_RWIN), RawKeyEvent::down(b'5' as u16), RawKeyEvent::up(b'5' as u16), RawKeyEvent::up(VK_RWIN)],
    );
    expect_dispatch(first[1], HotkeyAction::SwitchDesktop(4), true);

    // Digit released before its win-up partner in one order, win released first in another.
    let second = feed(
        &mut e,
        &t,
        &[RawKeyEvent::down(VK_LWIN), RawKeyEvent::down(b'6' as u16), RawKeyEvent::up(VK_LWIN), RawKeyEvent::up(b'6' as u16)],
    );
    expect_dispatch(second[1], HotkeyAction::SwitchDesktop(5), true);
    assert_eq!(second[2], EngineOutcome::Pass);
    assert_eq!(second[3], EngineOutcome::Swallow);
}

#[test]
fn config_swap_while_modifiers_held_applies_immediately() {
    let mut e = KeyboardEngine::new();
    let old = table_with_win_digits();

    // New table moves microphone to Win+7.
    let mut new_table = BindingTable::default();
    new_table.insert(
        Hotkey { modifiers: ModifierMask::WIN, key: VirtualKey(b'7' as u16) },
        HotkeyAction::ToggleMicrophone,
    );

    assert_eq!(e.on_event(RawKeyEvent::down(VK_LWIN), &old), EngineOutcome::Pass);
    // While Win held, swap happens.
    let out = e.on_event(RawKeyEvent::down(b'M' as u16), &old); // old table: no Win+M
    assert_eq!(out, EngineOutcome::Pass);
    let out = e.on_event(RawKeyEvent::up(b'M' as u16), &old);
    let out = e.on_event(RawKeyEvent::down(b'7' as u16), &new_table); // new table active
    // The shell already saw `M` during this chord, so no dirtier is needed.
    assert_eq!(
        out,
        EngineOutcome::Dispatch { action: HotkeyAction::ToggleMicrophone, dirty_win_chord: false }
    );
}


#[test]
fn injected_events_never_bind_or_suppress() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(
        &mut e,
        &t,
        &[RawKeyEvent::down(VK_LWIN).injected(), RawKeyEvent::down(b'1' as u16).injected()],
    );
    assert!(out.iter().all(|o| *o == EngineOutcome::Pass));
}

#[test]
fn extra_modifier_prevents_exact_match() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(0xA2),          // LCtrl
            RawKeyEvent::down(0xA4),          // LAlt
            RawKeyEvent::down(VK_SHIFT),      // +Shift held
            RawKeyEvent::down(b'M' as u16),   // Ctrl+Alt+Shift+M != Ctrl+Alt+M
        ],
    );
    assert_eq!(out[3], EngineOutcome::Pass);
}

#[test]
fn numpad_digits_match_top_row_bindings() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    // Right-side path: numpad 3 arrives as VK_NUMPAD3 (0x63).
    let out = feed(
        &mut e,
        &t,
        &[RawKeyEvent::down(VK_LWIN), RawKeyEvent { vk: 0x63, extended: false, down: true, injected: false }],
    );
    assert_eq!(out[1], EngineOutcome::Dispatch { action: HotkeyAction::SwitchDesktop(2), dirty_win_chord: true });
}

#[test]
fn reset_clears_stuck_state() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let _ = e.on_event(RawKeyEvent::down(VK_LWIN), &t);
    e.reset();
    // After reset the engine must behave as fresh: plain tap passes.
    let out = feed(&mut e, &t, &[RawKeyEvent::down(VK_LWIN), RawKeyEvent::up(VK_LWIN)]);
    assert!(out.iter().all(|o| *o == EngineOutcome::Pass));
}

#[test]
fn modifier_release_clears_state_for_every_side() {
    // Regression for issue #4: non-Win modifier key-ups never cleared the
    // engine's down-state, so a released Ctrl/Alt/Shift stayed "held" forever
    // and later chords matched the wrong (or no) binding.
    let sides = [
        ("LCTRL", 0xA2u16),
        ("RCTRL", 0xA3),
        ("LALT", 0xA4),
        ("RALT", 0xA5),
        ("LSHIFT", 0xA0),
        ("RSHIFT", 0xA1),
        ("LWIN", 0x5B),
        ("RWIN", 0x5C),
    ];
    for (name, vk) in sides {
        let mut e = KeyboardEngine::new();
        let t = table_with_win_digits();
        feed(&mut e, &t, &[RawKeyEvent::down(vk), RawKeyEvent::up(vk)]);
        assert_eq!(e.current_modifiers(), ModifierMask::NONE, "{name} stuck down");
    }
}

#[test]
fn modifier_autorepeat_then_single_release_clears() {
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(0xA2),
            RawKeyEvent::down(0xA2),
            RawKeyEvent::down(0xA2),
            RawKeyEvent::up(0xA2),
        ],
    );
    assert_eq!(e.current_modifiers(), ModifierMask::NONE);
}

#[test]
fn mixed_order_modifier_releases_converge_empty() {
    // Ctrl down, Shift down, Shift up, Ctrl up — and the mirror order.
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(0xA2),
            RawKeyEvent::down(0xA0),
            RawKeyEvent::up(0xA0),
            RawKeyEvent::up(0xA2),
        ],
    );
    assert_eq!(e.current_modifiers(), ModifierMask::NONE);

    let mut e = KeyboardEngine::new();
    feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(0xA0),
            RawKeyEvent::down(0xA2),
            RawKeyEvent::up(0xA2),
            RawKeyEvent::up(0xA0),
        ],
    );
    assert_eq!(e.current_modifiers(), ModifierMask::NONE);
}

#[test]
fn digit_first_win_up_then_digit_up_passes_digit_up() {
    // Regression for #5: 1↓ (passed), Win↓ completes → dispatch, Win↑
    // swallowed, then 1↑ must PASS — the shell saw the down.
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(b'1' as u16),
            RawKeyEvent::down(VK_LWIN),
            RawKeyEvent::up(VK_LWIN),
            RawKeyEvent::up(b'1' as u16),
        ],
    );
    assert_eq!(out[0], EngineOutcome::Pass);
    expect_dispatch(out[1], HotkeyAction::SwitchDesktop(0), false);
    assert_eq!(out[2], EngineOutcome::Swallow); // hide the Win cycle
    assert_eq!(out[3], EngineOutcome::Pass);
}

#[test]
fn digit_first_digit_up_before_win_up_still_symmetric() {
    // 1↓ Win↓ 1↑ Win↑: digit up passes, Win up swallowed.
    let mut e = KeyboardEngine::new();
    let t = table_with_win_digits();
    let out = feed(
        &mut e,
        &t,
        &[
            RawKeyEvent::down(b'1' as u16),
            RawKeyEvent::down(VK_RWIN),
            RawKeyEvent::up(b'1' as u16),
            RawKeyEvent::up(VK_RWIN),
        ],
    );
    assert_eq!(out[0], EngineOutcome::Pass);
    expect_dispatch(out[1], HotkeyAction::SwitchDesktop(0), false);
    assert_eq!(out[2], EngineOutcome::Pass);
    assert_eq!(out[3], EngineOutcome::Swallow);
}
