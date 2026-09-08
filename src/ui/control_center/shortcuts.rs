//! Shortcuts for the control center.

use super::native::invalidate;
use super::state::SettingsUi;
use crate::config::validate::Violation;
use crate::keyboard::binding::{Hotkey, ModifierMask, VirtualKey};
use crate::ui::controls::ControlValue;
use crate::ui::layout::{ElementId, HotkeySlot};
use crate::ui::presentation::{format_modifier as format_modifier_display, format_optional_hotkey};
use std::borrow::Cow;
use windows::Win32::Foundation::HWND;

fn modifier_for_vk(vk: u16) -> Option<ModifierMask> {
    Some(match vk {
        0x11 | 0xA2 | 0xA3 => ModifierMask::CTRL,
        0x12 | 0xA4 | 0xA5 => ModifierMask::ALT,
        0x10 | 0xA0 | 0xA1 => ModifierMask::SHIFT,
        0x5B | 0x5C => ModifierMask::WIN,
        _ => return None,
    })
}

pub(super) unsafe fn key_down(vk: i32) -> bool {
    // SAFETY: GetKeyState is thread-affine read-only state.
    unsafe { (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(vk) as u16 & 0x8000) != 0 }
}

impl SettingsUi {
    pub(super) fn active_profile_hotkey(&self) -> Option<Hotkey> {
        let id = self.draft.display_profiles.active()?.id.as_str();
        self.draft
            .hotkeys
            .display_profiles
            .iter()
            .find(|binding| binding.profile_id.eq_ignore_ascii_case(id))
            .map(|binding| binding.hotkey)
    }

    pub(super) fn set_active_profile_hotkey(&mut self, hotkey: Option<Hotkey>) {
        let Some(id) = self
            .draft
            .display_profiles
            .active()
            .map(|profile| profile.id.clone())
        else {
            return;
        };
        self.draft
            .hotkeys
            .display_profiles
            .retain(|binding| !binding.profile_id.eq_ignore_ascii_case(&id) || hotkey.is_some());
        if let Some(hotkey) = hotkey {
            if let Some(binding) = self
                .draft
                .hotkeys
                .display_profiles
                .iter_mut()
                .find(|binding| binding.profile_id.eq_ignore_ascii_case(&id))
            {
                binding.hotkey = hotkey;
            } else {
                self.draft.hotkeys.display_profiles.push(
                    crate::config::model::DisplayProfileHotkey {
                        profile_id: id,
                        hotkey,
                    },
                );
            }
        }
    }

    pub(super) fn hotkey_value(&self, id: ElementId, hotkey: Option<Hotkey>) -> ControlValue<'_> {
        if self.interaction.capture_target() == Some(id) {
            let modifiers = self.interaction.capture_modifiers();
            if modifiers.is_empty() {
                ControlValue::Text(Cow::Borrowed("Press a shortcut…"))
            } else {
                ControlValue::Text(Cow::Owned(format!(
                    "{} …",
                    format_modifier_display(modifiers)
                )))
            }
        } else {
            ControlValue::Text(Cow::Owned(format_optional_hotkey(hotkey)))
        }
    }

    pub(super) fn hotkey_action(&self, slot: HotkeySlot) -> Option<String> {
        Some(match slot {
            HotkeySlot::Microphone => "toggle_microphone".into(),
            HotkeySlot::Output => "toggle_output".into(),
            HotkeySlot::Foreground => "toggle_foreground_audio".into(),
            HotkeySlot::CycleInput => "cycle_input_device".into(),
            HotkeySlot::CycleOutput => "cycle_output_device".into(),
            HotkeySlot::ForegroundVolumeUp => "foreground_volume_up".into(),
            HotkeySlot::ForegroundVolumeDown => "foreground_volume_down".into(),
            HotkeySlot::PreviousDesktop => "previous_desktop".into(),
            HotkeySlot::AssignSpecial => "scratchpad_assign".into(),
            HotkeySlot::ToggleSpecial => "scratchpad_toggle".into(),
            HotkeySlot::DisplayProfile => format!(
                "display_profile:{}",
                self.draft.display_profiles.active()?.id
            ),
        })
    }

    pub(super) fn hotkey_subject(slot: HotkeySlot) -> &'static str {
        match slot {
            HotkeySlot::Microphone => "Mute microphone shortcut",
            HotkeySlot::Output => "Mute speakers shortcut",
            HotkeySlot::Foreground => "Mute current app shortcut",
            HotkeySlot::CycleInput => "Next microphone shortcut",
            HotkeySlot::CycleOutput => "Next speaker shortcut",
            HotkeySlot::ForegroundVolumeUp => "Current app volume up shortcut",
            HotkeySlot::ForegroundVolumeDown => "Current app volume down shortcut",
            HotkeySlot::PreviousDesktop => "Previous desktop shortcut",
            HotkeySlot::AssignSpecial => "Move window to Special shortcut",
            HotkeySlot::ToggleSpecial => "Open or close Special shortcut",
            HotkeySlot::DisplayProfile => "Selected display profile shortcut",
        }
    }

    pub(super) fn active_hotkey(&self, slot: HotkeySlot) -> Option<Hotkey> {
        match slot {
            HotkeySlot::Microphone => self.draft.hotkeys.toggle_microphone,
            HotkeySlot::Output => self.draft.hotkeys.toggle_output,
            HotkeySlot::Foreground => self.draft.hotkeys.toggle_foreground_audio,
            HotkeySlot::CycleInput => self.draft.hotkeys.cycle_input_device,
            HotkeySlot::CycleOutput => self.draft.hotkeys.cycle_output_device,
            HotkeySlot::ForegroundVolumeUp => self.draft.hotkeys.foreground_volume_up,
            HotkeySlot::ForegroundVolumeDown => self.draft.hotkeys.foreground_volume_down,
            HotkeySlot::PreviousDesktop => self.draft.virtual_desktops.previous_desktop,
            HotkeySlot::AssignSpecial => self.draft.virtual_desktops.scratchpad_assign,
            HotkeySlot::ToggleSpecial => self.draft.virtual_desktops.scratchpad_toggle,
            HotkeySlot::DisplayProfile => self.active_profile_hotkey(),
        }
    }

    pub(super) fn configured_hotkey(&self, slot: HotkeySlot) -> Option<Hotkey> {
        self.active_hotkey(slot).or_else(|| {
            self.hotkey_action(slot)
                .as_deref()
                .and_then(|action| self.draft.hotkeys.disabled_hotkey(action))
        })
    }

    pub(super) fn hotkey_enabled(&self, slot: HotkeySlot) -> bool {
        self.active_hotkey(slot).is_some()
    }

    pub(super) fn set_active_hotkey(&mut self, slot: HotkeySlot, hotkey: Option<Hotkey>) {
        match slot {
            HotkeySlot::Microphone => self.draft.hotkeys.toggle_microphone = hotkey,
            HotkeySlot::Output => self.draft.hotkeys.toggle_output = hotkey,
            HotkeySlot::Foreground => self.draft.hotkeys.toggle_foreground_audio = hotkey,
            HotkeySlot::CycleInput => self.draft.hotkeys.cycle_input_device = hotkey,
            HotkeySlot::CycleOutput => self.draft.hotkeys.cycle_output_device = hotkey,
            HotkeySlot::ForegroundVolumeUp => self.draft.hotkeys.foreground_volume_up = hotkey,
            HotkeySlot::ForegroundVolumeDown => self.draft.hotkeys.foreground_volume_down = hotkey,
            HotkeySlot::PreviousDesktop => self.draft.virtual_desktops.previous_desktop = hotkey,
            HotkeySlot::AssignSpecial => self.draft.virtual_desktops.scratchpad_assign = hotkey,
            HotkeySlot::ToggleSpecial => self.draft.virtual_desktops.scratchpad_toggle = hotkey,
            HotkeySlot::DisplayProfile => self.set_active_profile_hotkey(hotkey),
        }
    }

    pub(super) fn set_recorded_hotkey(&mut self, slot: HotkeySlot, hotkey: Hotkey) {
        let Some(action) = self.hotkey_action(slot) else {
            return;
        };
        if self.active_hotkey(slot).is_none()
            && self.draft.hotkeys.disabled_hotkey(&action).is_some()
        {
            self.draft.hotkeys.set_disabled_hotkey(action, hotkey);
        } else {
            self.set_active_hotkey(slot, Some(hotkey));
            self.draft.hotkeys.clear_disabled_hotkey(&action);
        }
    }

    pub(super) fn toggle_hotkey_enabled(&mut self, hwnd: HWND, slot: HotkeySlot) {
        let Some(action) = self.hotkey_action(slot) else {
            return;
        };
        let before = self.draft.clone();
        if let Some(hotkey) = self.active_hotkey(slot) {
            self.set_active_hotkey(slot, None);
            self.draft.hotkeys.clear_disabled_hotkey(&action);
            self.draft.hotkeys.set_disabled_hotkey(action, hotkey);
        } else if let Some(hotkey) = self.draft.hotkeys.take_disabled_hotkey(&action) {
            self.set_active_hotkey(slot, Some(hotkey));
            let violations = crate::config::validate(&self.draft);
            if !violations.is_empty() {
                self.replace_draft(before);
                self.validation = violations;
                return;
            }
        } else {
            return;
        }
        self.commit_local_change(hwnd, before);
    }

    pub(super) fn unassign_hotkey(&mut self, hwnd: HWND, slot: HotkeySlot) {
        let Some(action) = self.hotkey_action(slot) else {
            return;
        };
        let before = self.draft.clone();
        self.set_active_hotkey(slot, None);
        self.draft.hotkeys.clear_disabled_hotkey(&action);
        if self.draft != before {
            self.commit_local_change(hwnd, before);
        }
    }

    pub(super) fn stop_capture(&mut self) {
        if self.interaction.clear_capture() {
            self.access.end_capture();
        }
    }

    pub(super) fn record_key(&mut self, hwnd: HWND, vk: u16, down: bool) -> bool {
        let Some(id) = self.interaction.capture_target() else {
            return false;
        };
        let before = self.draft.clone();
        if vk == 0x1B && down {
            self.stop_capture();
            invalidate(hwnd);
            return true;
        }
        if id == ElementId::DisplayProfileHotkey && vk == 0x2E && down {
            let action = self.hotkey_action(HotkeySlot::DisplayProfile);
            self.set_active_profile_hotkey(None);
            if let Some(action) = action {
                self.draft.hotkeys.clear_disabled_hotkey(&action);
            }
            self.stop_capture();
            self.validation = crate::config::validate(&self.draft);
            if self.display.is_editing() && self.draft != before {
                self.display.mark_dirty();
            }
            if self.validation.is_empty() && self.draft != before && !self.display.is_dirty() {
                self.commit_local_change(hwnd, before.clone());
            }
            invalidate(hwnd);
            return true;
        }
        if let Some(modifier) = modifier_for_vk(vk) {
            let current = self.interaction.capture_modifiers();
            self.interaction.set_capture_modifiers(if down {
                current.union(modifier)
            } else {
                current.without(modifier)
            });
            invalidate(hwnd);
            return true;
        }
        if !down {
            return true;
        }
        let modifiers = self.interaction.capture_modifiers();
        if modifiers.is_empty() {
            self.validation = vec![Violation {
                field: "hotkeys".into(),
                message: "Use Ctrl, Alt, Shift, or Win with the key".into(),
            }];
            invalidate(hwnd);
            return true;
        }

        let hotkey = Hotkey {
            modifiers,
            key: VirtualKey(vk),
        };
        if let Some(slot) = HotkeySlot::from_capture_id(id) {
            self.set_recorded_hotkey(slot, hotkey);
        }
        if self.display.is_editing() && self.draft != before {
            self.display.mark_dirty();
        }
        self.stop_capture();
        self.validation = crate::config::validate(&self.draft);
        if self.validation.is_empty() && self.draft != before && !self.display.is_dirty() {
            self.commit_local_change(hwnd, before);
        }
        invalidate(hwnd);
        true
    }

    /// Apply a chord delivered by hook capture mode (#14).
    pub(super) fn finish_recording(&mut self, chord: crate::keyboard::hook::CapturedChord) {
        match chord.key {
            None => {
                // Esc: cancel recording.
                self.interaction.clear_capture();
                self.validation.clear();
            }
            Some(key) => {
                if let Some(id) = self.interaction.capture_target() {
                    let hotkey = Hotkey {
                        modifiers: chord.modifiers,
                        key,
                    };
                    if let Some(slot) = HotkeySlot::from_capture_id(id) {
                        if slot == HotkeySlot::DisplayProfile
                            && key == VirtualKey(0x2E)
                            && chord.modifiers.is_empty()
                        {
                            self.set_active_hotkey(slot, None);
                            if let Some(action) = self.hotkey_action(slot) {
                                self.draft.hotkeys.clear_disabled_hotkey(&action);
                            }
                        } else {
                            self.set_recorded_hotkey(slot, hotkey);
                        }
                    }
                }
                if self.display.is_editing() {
                    self.display.mark_dirty();
                }
                self.interaction.clear_capture();
                self.validation = crate::config::validate(&self.draft);
            }
        }
    }
}
