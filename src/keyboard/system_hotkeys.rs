//! Win-chord registrations handled by Windows instead of the low-level hook.
//!
//! Registration, update, and teardown run on the thread that owns the main HWND.
//! A failed registration leaves the chord in the hook's binding table.

use std::sync::Arc;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

use crate::config::ConfigHandle;
use crate::event::HotkeyAction;
use crate::keyboard::binding::{Hotkey, ModifierMask, VirtualKey};

#[derive(Clone, Copy)]
struct RegisteredHotkey {
    id: i32,
    chord: Hotkey,
    action: Option<HotkeyAction>,
}

/// Owned by the main thread; the registered HWND outlives this service.
pub struct SystemHotkeys {
    hwnd: HWND,
    config: Arc<ConfigHandle>,
    registered: Vec<RegisteredHotkey>,
    revision: u64,
}

impl SystemHotkeys {
    pub fn new(hwnd: HWND, config: Arc<ConfigHandle>) -> Self {
        Self {
            hwnd,
            revision: config.revision(),
            config,
            registered: Vec::new(),
        }
    }

    pub fn refresh(&mut self, suspended: bool) {
        if suspended {
            self.clear();
            return;
        }
        let snapshot = self.config.snapshot();
        let bindings = crate::keyboard::hook::build_bindings(&snapshot.value);
        let wanted: Vec<_> = bindings.iter().filter(system_candidate).collect();

        self.registered.retain_mut(|entry| {
            if let Some((_, action)) = wanted.iter().find(|(chord, _)| *chord == entry.chord) {
                entry.action = Some(*action);
                return true;
            }
            match unsafe { UnregisterHotKey(Some(self.hwnd), entry.id) } {
                Ok(()) => false,
                Err(error) => {
                    crate::warn_!(
                        "could not unregister Windows hotkey {}: {error}",
                        entry.chord
                    );
                    entry.action = None;
                    true
                }
            }
        });
        for (chord, action) in wanted {
            if self.registered.iter().any(|entry| entry.chord == chord) {
                continue;
            }
            let id = id_for(chord);
            match unsafe {
                RegisterHotKey(
                    Some(self.hwnd),
                    id,
                    windows_modifiers(chord.modifiers),
                    chord.key.0 as u32,
                )
            } {
                Ok(()) => {
                    self.registered.push(RegisteredHotkey {
                        id,
                        chord,
                        action: Some(action),
                    });
                    crate::info!("Windows hotkey registered: {chord}");
                }
                Err(error) => crate::warn_!(
                    "Windows hotkey registration failed for {chord}: {error}; using keyboard hook"
                ),
            }
        }
        self.registered.sort_unstable_by_key(|entry| entry.id);
        let owned: Vec<_> = self.registered.iter().map(|entry| entry.chord).collect();
        self.config.set_os_owned_hotkeys(&owned);
        self.revision = snapshot.stamp.revision;
    }

    pub fn action_for_message(&self, id: i32, lparam: isize) -> Option<HotkeyAction> {
        // A config publication invalidates queued WM_HOTKEY messages until the
        // main-thread config event has updated the registrations and actions.
        if self.revision != self.config.revision() {
            return None;
        }
        let key = VirtualKey(((lparam as u32) >> 16) as u16);
        let modifiers = from_windows_modifiers(lparam as u32 & 0xFFFF);
        self.registered
            .iter()
            .find(|entry| entry.id == id && entry.chord == Hotkey { modifiers, key })
            .and_then(|entry| entry.action)
    }

    pub fn clear(&mut self) {
        self.registered.retain_mut(|entry| {
            if let Err(error) = unsafe { UnregisterHotKey(Some(self.hwnd), entry.id) } {
                crate::warn_!(
                    "could not unregister Windows hotkey {}: {error}",
                    entry.chord
                );
                entry.action = None;
                true
            } else {
                false
            }
        });
        let owned: Vec<_> = self.registered.iter().map(|entry| entry.chord).collect();
        self.config.set_os_owned_hotkeys(&owned);
    }
}

impl Drop for SystemHotkeys {
    fn drop(&mut self) {
        self.clear();
    }
}

fn system_candidate(&(chord, _): &(Hotkey, HotkeyAction)) -> bool {
    chord.modifiers.contains(ModifierMask::WIN)
        // Win+1..9 must keep the hook's digit-first desktop behavior.
        && !(0x31..=0x39).contains(&chord.key.0)
}

fn id_for(chord: Hotkey) -> i32 {
    0x1000 | (i32::from(chord.modifiers.bits()) << 8) | i32::from(chord.key.0)
}

fn windows_modifiers(mask: ModifierMask) -> HOT_KEY_MODIFIERS {
    let mut value = MOD_NOREPEAT.0;
    if mask.contains(ModifierMask::CTRL) {
        value |= MOD_CONTROL.0;
    }
    if mask.contains(ModifierMask::ALT) {
        value |= MOD_ALT.0;
    }
    if mask.contains(ModifierMask::SHIFT) {
        value |= MOD_SHIFT.0;
    }
    if mask.contains(ModifierMask::WIN) {
        value |= MOD_WIN.0;
    }
    HOT_KEY_MODIFIERS(value)
}

fn from_windows_modifiers(value: u32) -> ModifierMask {
    let mut mask = ModifierMask::NONE;
    if value & MOD_CONTROL.0 != 0 {
        mask = mask.union(ModifierMask::CTRL);
    }
    if value & MOD_ALT.0 != 0 {
        mask = mask.union(ModifierMask::ALT);
    }
    if value & MOD_SHIFT.0 != 0 {
        mask = mask.union(ModifierMask::SHIFT);
    }
    if value & MOD_WIN.0 != 0 {
        mask = mask.union(ModifierMask::WIN);
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_hotkeys_cover_win_function_chords_without_stealing_digit_first() {
        let mut config = crate::config::Config::default();
        config.hotkeys.toggle_foreground_audio = Some(Hotkey::parse("Win+F2").unwrap());
        config.hotkeys.toggle_microphone = Some(Hotkey::parse("Shift+Win+F2").unwrap());
        let bindings = crate::keyboard::hook::build_bindings(&config);
        let candidates: Vec<_> = bindings.iter().filter(system_candidate).collect();
        assert!(candidates.contains(&(
            Hotkey::parse("Win+F2").unwrap(),
            HotkeyAction::ToggleForegroundAppAudio
        )));
        assert!(candidates.contains(&(
            Hotkey::parse("Shift+Win+F2").unwrap(),
            HotkeyAction::ToggleMicrophone
        )));
        assert!(!candidates
            .iter()
            .any(|(chord, _)| chord.key.0 == b'1' as u16));
    }

    #[test]
    fn windows_modifier_encoding_keeps_alt_and_control_distinct() {
        let alt = Hotkey::parse("Alt+Win+F2").unwrap().modifiers;
        let ctrl = Hotkey::parse("Ctrl+Win+F2").unwrap().modifiers;
        assert_eq!(
            windows_modifiers(alt).0,
            MOD_ALT.0 | MOD_WIN.0 | MOD_NOREPEAT.0
        );
        assert_eq!(
            windows_modifiers(ctrl).0,
            MOD_CONTROL.0 | MOD_WIN.0 | MOD_NOREPEAT.0
        );
        assert_eq!(from_windows_modifiers(windows_modifiers(alt).0), alt);
        assert_eq!(from_windows_modifiers(windows_modifiers(ctrl).0), ctrl);
    }
}
