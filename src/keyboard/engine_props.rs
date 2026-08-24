//! Property/state-machine tests for [`KeyboardEngine`] (#37).
//!
//! Invariants verified over generated event sequences (proptest):
//! - **A** down/up disposition symmetry (#5 generalized)
//! - **B** quiescence after full release
//! - **C** autorepeat cardinality bound + single-UP cleanup
//! - **D** modifier release permutations end neutral, zero dispatches
//! - **E** reset() from any reachable prefix yields fresh state

use super::binding::{BindingTable, Hotkey, ModifierMask, VirtualKey};
use super::engine::{EngineOutcome, KeyboardEngine, RawKeyEvent};
use crate::event::HotkeyAction;
use proptest::prelude::*;

const MODS: [u16; 8] = [0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x5B, 0x5C];
const KEYS: [u16; 14] = [
    b'M' as u16,
    b'O' as u16,
    b'K' as u16,
    b'1' as u16,
    b'5' as u16,
    b'9' as u16,
    0x60,
    0x63,
    0x14,
    0xBA,
    0xBB,
    0xBD,
    0xBE,
    0xBF,
];

#[derive(Debug, Clone, Copy)]
enum Ev {
    Down(u16),
    Up(u16),
    Reset,
}

fn ev_strategy() -> impl Strategy<Value = Ev> {
    prop_oneof![
        3 => prop::sample::select(KEYS.to_vec()).prop_map(Ev::Down),
        3 => prop::sample::select(KEYS.to_vec()).prop_map(Ev::Up),
        4 => prop::sample::select(MODS.to_vec()).prop_map(Ev::Down),
        4 => prop::sample::select(MODS.to_vec()).prop_map(Ev::Up),
        1 => Just(Ev::Reset),
    ]
}

fn to_raw(e: &Ev) -> RawKeyEvent {
    match *e {
        Ev::Down(vk) => RawKeyEvent::down(vk),
        Ev::Up(vk) => RawKeyEvent::up(vk),
        Ev::Reset => unreachable!("caller handles Reset"),
    }
}

fn table() -> BindingTable {
    let mut t = BindingTable::default();
    for d in 1u16..=9 {
        t.insert(
            Hotkey {
                modifiers: ModifierMask::WIN,
                key: VirtualKey(0x30 + d),
            },
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
    t
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(512))]

    /// Invariant A (#5 generalized): hook disposition of a key's UP must
    /// equal the engine's own suppressed-up bookkeeping — the UP is swallowed
    /// iff the engine recorded suppressed-up debt for that key. Mirrors engine
    /// state exactly, so it fails if a pair is ever split again.
    #[test]
    fn prop_down_up_disposition_symmetry(
        seq in prop::collection::vec(ev_strategy(), 0..64),
    ) {
        let mut e = KeyboardEngine::new();
        let t = table();
        let mut suppressed_model: std::collections::HashSet<u16> = Default::default();
        let mut held: std::collections::HashSet<u16> = Default::default();

        for ev in &seq {
            match *ev {
                Ev::Reset => {
                    e.reset();
                    suppressed_model.clear();
                    held.clear();
                    continue;
                }
                Ev::Down(vk) => {
                    let out = e.on_event(RawKeyEvent::down(vk), &t);
                    if matches!(out, EngineOutcome::Dispatch { .. }) && held.insert(vk) {
                        // Non-autorepeat dispatch inserts suppressed-up debt.
                        suppressed_model.insert(vk);
                    }
                }
                Ev::Up(vk) => {
                    let out = e.on_event(RawKeyEvent::up(vk), &t);
                    let expected = suppressed_model.remove(&vk);
                    let _ = held.remove(&vk);
                    let got = !matches!(out, EngineOutcome::Pass);
                    let ctx = format!("up must mirror debt for vk {vk:#x}");
                    prop_assert_eq!(expected, got, "{}", ctx);
                }
            }
        }
    }

    /// Invariant B: after releasing every pressed key, engine == fresh.
    #[test]
    fn prop_quiescent_after_full_release(
        downs in prop::collection::vec(
            prop::sample::select(MODS.to_vec().into_iter().chain(KEYS).collect::<Vec<u16>>()),
            0..24,
        ),
    ) {
        let mut e = KeyboardEngine::new();
        let t = table();
        for &vk in &downs {
            let _ = e.on_event(RawKeyEvent::down(vk), &t);
        }
        for &vk in downs.iter().rev() {
            let _ = e.on_event(RawKeyEvent::up(vk), &t);
        }
        prop_assert!(e.is_neutral(), "engine not neutral after full release");
    }

    /// Invariant C: autorepeat keeps at most one held entry per key; one UP
    /// clears both the hold and any suppressed-up debt.
    #[test]
    fn prop_autorepeat_bounded(repeats in 1u8..=8, vk in prop::sample::select(KEYS.to_vec())) {
        let mut e = KeyboardEngine::new();
        let t = table();
        for _ in 0..repeats {
            let _ = e.on_event(RawKeyEvent::down(vk), &t);
            let held = e.held_nonmods().iter().filter(|&&k| k == vk).count();
            prop_assert!(held <= 1, "autorepeat duplicated held key {vk:#x}");
        }
        let _ = e.on_event(RawKeyEvent::up(vk), &t);
        prop_assert!(e.is_neutral(), "one matching up clears repeat debt");
    }

    /// Invariant D: modifier subsets released in ANY permutation end neutral;
    /// releases alone never dispatch.
    #[test]
    fn prop_modifier_release_permutation(
        picks in prop::collection::vec(any::<bool>(), MODS.len()),
        seed in any::<u64>(),
    ) {
        let mut e = KeyboardEngine::new();
        let t = table();
        let subset: Vec<u16> =
            MODS.iter().zip(picks.iter()).filter(|(_, &p)| p).map(|(&v, _)| v).collect();
        // Deterministic Fisher-Yates from a small LCG seeded by `seed`
        // (avoids proptest TestRng, which asserts on direct construction).
        let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut order = subset.clone();
        for i in (1..order.len()).rev() {
            let j = (next() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        for vk in order {
            let out = e.on_event(RawKeyEvent::up(vk), &t);
            let dispatched = matches!(out, EngineOutcome::Dispatch { .. });
            prop_assert!(!dispatched, "release must not dispatch");
        }
        prop_assert!(e.is_neutral());
    }

    /// Invariant E: reset() from any reachable prefix yields fresh state.
    #[test]
    fn prop_reset_yields_fresh_engine(seq in prop::collection::vec(ev_strategy(), 0..48)) {
        let mut e = KeyboardEngine::new();
        for ev in &seq {
            match *ev {
                Ev::Reset => e.reset(),
                other => {
                    let raw = to_raw(&other);
                    let _ = e.on_event(raw, &table());
                }
            }
        }
        e.reset();
        prop_assert!(e.is_neutral());
        prop_assert_eq!(e.current_modifiers(), ModifierMask::NONE);
    }
}

#[test]
fn debug_two_win_overlap_trace() {
    let mut e = KeyboardEngine::new();
    let t = table();
    let o1 = e.on_event(RawKeyEvent::down(b'1' as u16), &t);
    eprintln!("down49: {o1:?}");
    let o2 = e.on_event(RawKeyEvent::down(0x5C), &t);
    eprintln!("down rwin(0x5C): {o2:?}");
    let o3 = e.on_event(RawKeyEvent::down(0x5B), &t);
    eprintln!("down lwin(0x5B): {o3:?}");
    let o4 = e.on_event(RawKeyEvent::up(0x5B), &t);
    eprintln!("up lwin: {o4:?}");
    let o5 = e.on_event(RawKeyEvent::up(0x5C), &t);
    eprintln!("up rwin: {o5:?}");
    eprintln!("neutral: {}", e.is_neutral());
}
