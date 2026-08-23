//! Binding types: modifier masks, virtual keys, the exact-match binding table.
//!
//! Pure logic — no `windows` imports — so the engine is testable anywhere.
//! Raw strings exist only at the parse/display boundary.

use std::collections::HashMap;
use std::fmt;

use crate::event::HotkeyAction;

/// Side-collapsing modifier bitmask used for binding match keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModifierMask(u8);

impl ModifierMask {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1 << 0);
    pub const ALT: Self = Self(1 << 1);
    pub const SHIFT: Self = Self(1 << 2);
    pub const WIN: Self = Self(1 << 3);

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Canonical display order: Ctrl Alt Shift Win.
    pub fn parts(self) -> [(&'static str, Self); 4] {
        [("Ctrl", Self::CTRL), ("Alt", Self::ALT), ("Shift", Self::SHIFT), ("Win", Self::WIN)]
    }
}

impl fmt::Display for ModifierMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (name, m) in self.parts() {
            if self.contains(m) {
                if !first {
                    f.write_str("+")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        Ok(())
    }
}

/// A named virtual key the binder understands. Newtype over the platform VK code
/// (normalized: side-specific codes collapse to their generic value; numpad digits
/// normalize to top-row digits so Win+Numpad3 behaves like Win+3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VirtualKey(pub u16);

impl VirtualKey {
    pub const fn code(self) -> u16 {
        self.0
    }

    /// Human-facing name used in config strings and UI chips.
    pub fn name(self) -> String {
        let v = self.0;
        match v {
            0x08 => "Backspace".into(),
            0x09 => "Tab".into(),
            0x0D => "Enter".into(),
            0x13 => "Pause".into(),
            0x14 => "Caps Lock".into(),
            0x1B => "Esc".into(),
            0x20 => "Space".into(),
            0x21 => "Page Up".into(),
            0x22 => "Page Down".into(),
            0x23 => "End".into(),
            0x24 => "Home".into(),
            0x25..=0x28 => ["Left", "Up", "Right", "Down"][(v - 0x25) as usize].into(),
            0x2D => "Insert".into(),
            0x2E => "Delete".into(),
            0x30..=0x39 => char::from(b'0' + (v - 0x30) as u8).to_string(),
            0x41..=0x5A => char::from(b'A' + (v - 0x41) as u8).to_string(),
            0x60..=0x69 => char::from(b'0' + (v - 0x60) as u8).to_string(), // Numpad0-9
            0x6A => "Numpad *".into(),
            0x6B => "Numpad +".into(),
            0x6D => "Numpad -".into(),
            0x6E => "Numpad .".into(),
            0x6F => "Numpad /".into(),
            0x70..=0x87 => format!("F{}", v - 0x70 + 1),
            0x90 => "Num Lock".into(),
            0x91 => "Scroll Lock".into(),
            0xBA => ";".into(),
            0xBB => "=".into(),
            0xBC => ",".into(),
            0xBD => "-".into(),
            0xBE => ".".into(),
            0xBF => "/".into(),
            0xC0 => "`".into(),
            0xDB => "[".into(),
            0xDC => "\\".into(),
            0xDD => "]".into(),
            0xDE => "'".into(),
            _ => format!("VK_{v:02X}"),
        }
    }

    /// Parse a single key token (case-insensitive). Modifiers are rejected here.
    pub fn parse(token: &str) -> Option<Self> {
        let t = token.trim();
        let upper = t.to_ascii_uppercase();
        Some(match upper.as_str() {
            "BACKSPACE" | "BS" => Self(0x08),
            "TAB" => Self(0x09),
            "ENTER" | "RETURN" => Self(0x0D),
            "PAUSE" => Self(0x13),
            "CAPSLOCK" | "CAPS" => Self(0x14),
            "ESC" | "ESCAPE" => Self(0x1B),
            "SPACE" => Self(0x20),
            "PGUP" | "PAGEUP" => Self(0x21),
            "PGDN" | "PAGEDOWN" => Self(0x22),
            "END" => Self(0x23),
            "HOME" => Self(0x24),
            "LEFT" => Self(0x25),
            "UP" => Self(0x26),
            "RIGHT" => Self(0x27),
            "DOWN" => Self(0x28),
            "INS" | "INSERT" => Self(0x2D),
            "DEL" | "DELETE" => Self(0x2E),
            "NUMPAD*" | "NUM*"=> Self(0x6A),
            "NUMPAD+" | "NUM+" => Self(0x6B),
            "NUMPAD-" | "NUM-" => Self(0x6D),
            "NUMPAD." | "NUM." => Self(0x6E),
            "NUMPAD/" | "NUM/" => Self(0x6F),
            "NUMLOCK" => Self(0x90),
            "/" => Self(0xBF),
            "`" | "~" | OEM3 => Self(0xC0),
            "[" => Self(0xDB),
            "\\" => Self(0xDC),
            "]" => Self(0xDD),
            "'" => Self(0xDE),
            _ => {
                if let Some(rest) = upper.strip_prefix('F') {
                    if let Ok(n) = rest.parse::<u16>() {
                        if (1..=24).contains(&n) {
                            return Some(Self(0x70 + n - 1));
                        }
                    }
                    return None;
                }
                if upper.len() == 1 {
                    let c = upper.chars().next().unwrap();
                    if c.is_ascii_digit() {
                        return Some(Self(0x30 + (c as u16 - b'0' as u16)));
                    }
                    if c.is_ascii_alphabetic() {
                        return Some(Self(0x41 + (c as u16 - b'A' as u16)));
                    }
                }
                return None;
            }
        })
    }
}

const OEM1: &str = "OEM_1";
const OEM3: &str = "OEM_3";

impl fmt::Display for VirtualKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}


/// A fully resolved hotkey: exact modifier set + one non-modifier key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub modifiers: ModifierMask,
    pub key: VirtualKey,
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.is_empty() {
            write!(f, "{}", self.key)
        } else {
            write!(f, "{}+{}", self.modifiers, self.key)
        }
    }
}

impl Hotkey {
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut mask = ModifierMask::NONE;
        let mut key: Option<VirtualKey> = None;
        for token in s.split('+') {
            let t = token.trim();
            if t.is_empty() {
                return Err(format!("empty component in `{s}`"));
            }
            let upper = t.to_ascii_uppercase();
            let m = match upper.as_str() {
                "CTRL" | "CONTROL" => Some(ModifierMask::CTRL),
                "ALT" => Some(ModifierMask::ALT),
                "SHIFT" => Some(ModifierMask::SHIFT),
                "WIN" | "SUPER" | "META" => Some(ModifierMask::WIN),
                _ => None,
            };
            if let Some(m) = m {
                if mask.contains(m) {
                    return Err(format!("duplicate modifier `{t}` in `{s}`"));
                }
                mask = mask.union(m);
            } else {
                if key.is_some() {
                    return Err(format!("multiple keys in `{s}`"));
                }
                let k = VirtualKey::parse(t).ok_or_else(|| format!("unknown key `{t}`"))?;
                if matches!(k.0, 0xA0..=0xA5 | 0x5B | 0x5C) {
                    return Err(format!("`{t}` is a modifier, not a key"));
                }
                key = Some(k);
            }
        }
        match key {
            Some(key) => Ok(Hotkey { modifiers: mask, key }),
            None => Err(format!("no key in `{s}` (modifiers alone cannot be bound)")),
        }
    }
}

/// Exact-match binding table. The key includes the FULL normalized modifier
/// state, so extra held modifiers never cause accidental matches.
#[derive(Debug, Clone, Default)]
pub struct BindingTable {
    map: HashMap<(ModifierMask, VirtualKey), HotkeyAction>,
}

impl BindingTable {
    pub fn insert(&mut self, hk: Hotkey, action: HotkeyAction) {
        self.map.insert((hk.modifiers, hk.key), action);
    }

    pub fn lookup(&self, mods: ModifierMask, key: VirtualKey) -> Option<HotkeyAction> {
        self.map.get(&(mods, key)).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Hotkey, HotkeyAction)> + '_ {
        self.map.iter().map(|((m, k), a)| (Hotkey { modifiers: *m, key: *k }, *a))
    }

    pub fn conflicts(&self, candidate: &Hotkey) -> Option<HotkeyAction> {
        self.lookup(candidate.modifiers, candidate.key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Parse a set of named hotkey strings into a conflict-checked table entry list.
pub fn parse_binding(spec: &str) -> Result<Hotkey, String> {
    Hotkey::parse(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_display_round_trip() {
        for s in ["Ctrl+Alt+M", "Ctrl+Shift+M", "Win+7", "Ctrl+Alt+P", "F5", "Space"] {
            let hk = Hotkey::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(hk.to_string(), s, "round trip failed");
        }
    }

    #[test]
    fn letter_codes_are_correct() {
        assert_eq!(VirtualKey::parse("M").unwrap().code(), 0x4D);
        assert_eq!(VirtualKey::parse("A").unwrap().code(), 0x41);
        assert_eq!(VirtualKey::parse("3").unwrap().code(), 0x33);
        assert_eq!(VirtualKey::parse("F1").unwrap().code(), 0x70);
        assert_eq!(VirtualKey::parse("F24").unwrap().code(), 0x87);
    }

    #[test]
    fn modifier_only_rejected() {
        assert!(Hotkey::parse("Ctrl+Alt").is_err());
        assert!(Hotkey::parse("Ctrl+Ctrl+M").is_err());
        assert!(Hotkey::parse("").is_err());
    }

    #[test]
    fn case_insensitive_any_order() {
        let a = Hotkey::parse("alt+ctrl+m").unwrap();
        let b = Hotkey::parse("Ctrl+Alt+M").unwrap();
        assert_eq!(a, b);
    }
}
