//! Physical keyboard state tracking. Pure logic, no Windows imports.

use super::binding::ModifierMask;

/// Side-distinct virtual keys.
pub mod vks {
    pub const VK_LSHIFT: u16 = 0xA0;
    pub const VK_RSHIFT: u16 = 0xA1;
    pub const VK_LCONTROL: u16 = 0xA2;
    pub const VK_RCONTROL: u16 = 0xA3;
    pub const VK_LMENU: u16 = 0xA4;
    pub const VK_RMENU: u16 = 0xA5;
    pub const VK_LWIN: u16 = 0x5B;
    pub const VK_RWIN: u16 = 0x5C;
}

#[derive(Debug)]
pub struct KeyState {
    down: [bool; 256],
    nonmod_pressed: heapless_like::SmallVec, // insertion-ordered held non-modifier keys
}

impl Default for KeyState {
    fn default() -> Self {
        Self {
            down: [false; 256],
            nonmod_pressed: heapless_like::SmallVec::default(),
        }
    }
}

/// Minimal fixed-capacity vec avoiding allocation in the hook path.
mod heapless_like {
    #[derive(Debug, Default)]
    pub struct SmallVec([Option<u8>; 8]);

    impl SmallVec {
        pub fn push(&mut self, v: u8) -> bool {
            for slot in self.0.iter_mut() {
                if slot.is_none() {
                    *slot = Some(v);
                    return true;
                }
            }
            false // more than 8 simultaneous non-modifier keys: ignore tracking, matching still works
        }
        /// Insert only when absent (autorepeat dedupe).
        pub fn push_unique(&mut self, v: u8) {
            if !self.contains(v) {
                self.push(v);
            }
        }
        pub fn remove(&mut self, v: u8) {
            for slot in self.0.iter_mut() {
                if *slot == Some(v) {
                    *slot = None;
                }
            }
        }
        pub fn contains(&self, v: u8) -> bool {
            self.0.contains(&Some(v))
        }
        pub fn iter(&self) -> impl Iterator<Item = u8> + '_ {
            self.0.iter().filter_map(|s| *s)
        }
        pub fn clear(&mut self) {
            self.0 = [None; 8];
        }
    }
}

impl KeyState {
    pub fn is_down(&self, vk: u16) -> bool {
        vk < 256 && self.down[vk as usize]
    }

    pub fn set_down(&mut self, vk: u16, down: bool) {
        if vk < 256 {
            self.down[vk as usize] = down;
        }
    }

    /// Track held non-modifier keys. Autorepeat re-delivers the same DOWN;
    /// keep one entry per key (#11).
    pub fn track_nonmod(&mut self, vk: u16, down: bool) {
        if vk < 256 {
            if down {
                self.nonmod_pressed.push_unique(vk as u8);
            } else {
                self.nonmod_pressed.remove(vk as u8);
            }
        }
    }

    pub fn pressed_nonmods(&self) -> impl Iterator<Item = u16> + '_ {
        self.nonmod_pressed.iter().map(|v| v as u16)
    }

    pub fn clear_all(&mut self) {
        self.down = [false; 256];
        self.nonmod_pressed.clear();
    }

    /// Left/right collapsed into the generic binding mask.
    pub fn modifiers(&self) -> ModifierMask {
        let mut m = ModifierMask::NONE;
        if self.is_down(vks::VK_LCONTROL) || self.is_down(vks::VK_RCONTROL) || self.is_down(0x11) {
            m = m.union(ModifierMask::CTRL);
        }
        if self.is_down(vks::VK_LMENU) || self.is_down(vks::VK_RMENU) || self.is_down(0x12) {
            m = m.union(ModifierMask::ALT);
        }
        if self.is_down(vks::VK_LSHIFT) || self.is_down(vks::VK_RSHIFT) || self.is_down(0x10) {
            m = m.union(ModifierMask::SHIFT);
        }
        if self.is_down(vks::VK_LWIN) || self.is_down(vks::VK_RWIN) {
            m = m.union(ModifierMask::WIN);
        }
        m
    }

    pub fn any_win(&self) -> bool {
        self.is_down(vks::VK_LWIN) || self.is_down(vks::VK_RWIN)
    }

    /// Caps Lock (0x14) is deliberately NOT a modifier: it is a bindable
    /// key (#11).
    pub fn is_modifier(vk: u16) -> bool {
        matches!(
            vk,
            0x10..=0x12 | vks::VK_LSHIFT..=vks::VK_RMENU | vks::VK_LWIN | vks::VK_RWIN
        )
    }

    pub fn is_win_key(vk: u16) -> bool {
        vk == vks::VK_LWIN || vk == vks::VK_RWIN
    }
}

/// Normalize an LL-hook virtual key: side-generic codes become their explicit
/// sides using the EXTENDED flag. Numpad keys stay distinct from top-row keys
/// so bindings can tell them apart (#11).
pub fn normalize_vk(raw: u16, extended: bool) -> u16 {
    match raw {
        0x10 => {
            if extended {
                vks::VK_RSHIFT
            } else {
                vks::VK_LSHIFT
            }
        }
        0x11 => {
            if extended {
                vks::VK_RCONTROL
            } else {
                vks::VK_LCONTROL
            }
        }
        0x12 => {
            if extended {
                vks::VK_RMENU
            } else {
                vks::VK_LMENU
            }
        }
        _ => raw,
    }
}
