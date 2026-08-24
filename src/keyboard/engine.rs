//! The keyboard engine: pure state machine turning raw key events into
//! dispatch/swallow decisions. No Windows imports — fully unit-tested
//! (see KEYBOARD_HOOK_DESIGN.md and TEST_PLAN.md §A).

use super::binding::{BindingTable, ModifierMask, VirtualKey};
use super::keystate::{normalize_vk, vks, KeyState};
use crate::event::HotkeyAction;

/// One low-level keyboard event after mapping from `KBDLLHOOKSTRUCT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKeyEvent {
    /// Virtual key code exactly as reported by the hook (pre-normalization).
    pub vk: u16,
    pub extended: bool,
    pub down: bool,
    /// LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED.
    pub injected: bool,
}

impl RawKeyEvent {
    pub fn down(vk: u16) -> Self {
        Self { vk, extended: false, down: true, injected: false }
    }
    pub fn up(vk: u16) -> Self {
        Self { vk, extended: false, down: false, injected: false }
    }
    pub fn injected(mut self) -> Self {
        self.injected = true;
        self
    }
    pub fn ext(mut self) -> Self {
        self.extended = true;
        self
    }
}

/// What the hook must do with the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineOutcome {
    /// Forward to the next hook in the chain.
    Pass,
    /// Suppress delivery; no action.
    Swallow,
    /// Suppress delivery AND perform the action.
    Dispatch {
        action: HotkeyAction,
        /// True when the shell has observed nothing but a bare Win-down so far;
        /// unless a harmless chord-dirtying injection happens before Win release,
        /// the shell will pop the Start menu (spec §17).
        dirty_win_chord: bool,
    },
}

/// Fixed-capacity set: keys whose DOWN was suppressed and whose UP must be too.
#[derive(Debug, Default)]
struct SuppressionSet([Option<u8>; 8]);

impl SuppressionSet {
    fn insert(&mut self, vk: u16) {
        let b = vk as u8;
        if self.0.iter().any(|s| *s == Some(b)) {
            return;
        }
        for slot in self.0.iter_mut() {
            if slot.is_none() {
                *slot = Some(b);
                return;
            }
        }
    }
    fn take(&mut self, vk: u16) -> bool {
        let b = vk as u8;
        for slot in self.0.iter_mut() {
            if *slot == Some(b) {
                *slot = None;
                return true;
            }
        }
        false
    }
    fn clear(&mut self) {
        self.0 = [None; 8];
    }
}

#[derive(Debug, Default)]
pub struct KeyboardEngine {
    state: KeyState,
    /// A non-modifier key event PASSED THROUGH to the system since Win went
    /// down. Swallowed events don't count — the shell never saw them.
    passthrough_while_win: bool,
    /// We swallowed a Win DOWN because a digit-first chord completed; its UP
    /// must be swallowed too so the shell never sees this Win cycle.
    win_down_swallowed: bool,
    suppressed_ups: SuppressionSet,
}

impl KeyboardEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one raw event. `table` is consulted fresh per call, so config swaps
    /// apply immediately without resetting engine state (spec §54 matrix item).
    pub fn on_event(&mut self, ev: RawKeyEvent, table: &BindingTable) -> EngineOutcome {
        // Never bind or suppress injected input: breaks the
        // hotkey → SendInput → hook recursion (spec §14).
        if ev.injected {
            return EngineOutcome::Pass;
        }

        let vk = normalize_vk(ev.vk, ev.extended);
        if vk >= 256 {
            return EngineOutcome::Pass;
        }

        if ev.down {
            self.on_key_down(vk, table)
        } else {
            self.on_key_up(vk)
        }
    }

    fn on_key_down(&mut self, vk: u16, table: &BindingTable) -> EngineOutcome {
        if KeyState::is_modifier(vk) {
            self.state.set_down(vk, true);

            // Digit-first completion: a non-modifier is already held and this
            // Win press completes a binding. That earlier key-down already
            // passed through, so we complete the action here and swallow the
            // ENTIRE Win press cycle from the shell (no Start, no latch).
            if KeyState::is_win_key(vk) && self.state.any_win() {
                let mask = self.state.modifiers();
                let held: Vec<u16> = self.state.pressed_nonmods().collect();
                for key in held {
                    if let Some(action) = table.lookup(mask, VirtualKey(key)) {
                        self.win_down_swallowed = true;
                        self.suppressed_ups.insert(key);
                        self.passthrough_while_win = false;
                        return EngineOutcome::Dispatch { action, dirty_win_chord: false };
                    }
                }
            }
            return EngineOutcome::Pass;
        }

        // Non-modifier key down.
        let autorepeat = self.state.is_down(vk);
        let win_held = self.state.any_win();

        match table.lookup(self.state.modifiers(), VirtualKey(vk)) {
            Some(action) => {
                self.state.set_down(vk, true);
                self.state.track_nonmod(vk, true);

                if autorepeat {
                    // Holding a bound key fires exactly once (spec §15).
                    return EngineOutcome::Swallow;
                }
                self.suppressed_ups.insert(vk);
                let dirty_win_chord = win_held && !self.passthrough_while_win;
                return EngineOutcome::Dispatch { action, dirty_win_chord };
            }
            None => {
                self.state.set_down(vk, true);
                self.state.track_nonmod(vk, true);
                if win_held {
                    self.passthrough_while_win = true;
                }
                EngineOutcome::Pass
            }
        }
    }

    fn on_key_up(&mut self, vk: u16) -> EngineOutcome {
        if KeyState::is_modifier(vk) {
            self.state.set_down(vk, false);
            if KeyState::is_win_key(vk) {
                if self.state.any_win() {
                    return EngineOutcome::Pass; // other side still held
                }
                if self.win_down_swallowed {
                    // Hide our synthetic Win cycle completely from the shell.
                    self.win_down_swallowed = false;
                    self.passthrough_while_win = false;
                    return EngineOutcome::Swallow;
                }
                self.passthrough_while_win = false;
            }
            return EngineOutcome::Pass;
        }

        self.state.set_down(vk, false);
        self.state.track_nonmod(vk, false);
        if self.suppressed_ups.take(vk) {
            return EngineOutcome::Swallow; // hide the up of a swallowed down
        }
        if self.state.any_win() {
            // An unbound key released mid-chord still counts as chord activity.
            self.passthrough_while_win = true;
        }
        EngineOutcome::Pass
    }

    pub fn current_modifiers(&self) -> ModifierMask {
        self.state.modifiers()
    }

    /// Force-clear all state (focus/session loss safety valve).
    pub fn reset(&mut self) {
        self.state.clear_all();
        self.passthrough_while_win = false;
        self.win_down_swallowed = false;
        self.suppressed_ups.clear();
    }
}

// Re-export for hook-layer convenience.
pub use vks::VK_LWIN;
