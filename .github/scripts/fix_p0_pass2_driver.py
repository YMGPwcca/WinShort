from pathlib import Path

p = Path('.github/scripts/p0_ux_pass2.py')
s = p.read_text(encoding='utf-8')

replacements = [
    (
        "insert_marker = '''                ElementId::DesktopStripItem(index) => {\\n'''",
        "insert_marker = '''                ElementId::DesktopStripItem(index) => {\\n                    controls::draw_desktop_item(\\n'''",
        'paint marker',
    ),
    (
        '''insert_before(\n    "src/ui/control_center.rs",\n    ''' + "'''            ElementId::MicHotkey\\n            | ElementId::OutputHotkey\\n'''" + ''',\n    ''' ,
        '''insert_before(\n    "src/ui/control_center.rs",\n    ''' + "'''            ElementId::MicHotkey\\n            | ElementId::OutputHotkey\\n            | ElementId::ForegroundHotkey\\n            | ElementId::CycleInputHotkey\\n            | ElementId::CycleOutputHotkey\\n            | ElementId::ForegroundVolumeUpHotkey\\n            | ElementId::ForegroundVolumeDownHotkey\\n            | ElementId::PreviousDesktopHotkey\\n            | ElementId::AssignScratchpadHotkey\\n            | ElementId::ToggleScratchpadHotkey\\n            | ElementId::DisplayProfileHotkey => {\\n                self.recording = Some(id);\\n'''" + ''',\n    ''',
        'activation marker',
    ),
]

for old, new, label in replacements:
    if s.count(old) != 1:
        raise SystemExit(f'pass2 {label} patch mismatch: {s.count(old)}')
    s = s.replace(old, new)

start = '# Recording writes through the new slot helper, preserving disabled state.\n'
end = '# ---------------------------------------------------------------------------\n# Docs: schema and UX contract.\n'
a = s.index(start)
b = s.index(end, a)
record_patch = r'''# Recording writes through the new slot helper, preserving disabled state.
record_key_old = ''' + "'''" + r'''        match id {
            ElementId::MicHotkey => self.draft.hotkeys.toggle_microphone = Some(hotkey),
            ElementId::OutputHotkey => self.draft.hotkeys.toggle_output = Some(hotkey),
            ElementId::ForegroundHotkey => {
                self.draft.hotkeys.toggle_foreground_audio = Some(hotkey)
            }
            ElementId::CycleInputHotkey => self.draft.hotkeys.cycle_input_device = Some(hotkey),
            ElementId::CycleOutputHotkey => self.draft.hotkeys.cycle_output_device = Some(hotkey),
            ElementId::ForegroundVolumeUpHotkey => {
                self.draft.hotkeys.foreground_volume_up = Some(hotkey)
            }
            ElementId::ForegroundVolumeDownHotkey => {
                self.draft.hotkeys.foreground_volume_down = Some(hotkey)
            }
            ElementId::PreviousDesktopHotkey => {
                self.draft.virtual_desktops.previous_desktop = Some(hotkey)
            }
            ElementId::AssignScratchpadHotkey => {
                self.draft.virtual_desktops.scratchpad_assign = Some(hotkey)
            }
            ElementId::ToggleScratchpadHotkey => {
                self.draft.virtual_desktops.scratchpad_toggle = Some(hotkey)
            }
            ElementId::DisplayProfileHotkey => self.set_active_profile_hotkey(Some(hotkey)),
            _ => {}
        }
''' + "'''" + r'''
record_key_new = ''' + "'''" + r'''        if let Some(slot) = HotkeySlot::from_capture_id(id) {
            self.set_recorded_hotkey(slot, hotkey);
        }
''' + "'''" + r'''
replace("src/ui/control_center.rs", record_key_old, record_key_new)

finish_old = ''' + "'''" + r'''                    match id {
                        ElementId::MicHotkey => self.draft.hotkeys.toggle_microphone = Some(hotkey),
                        ElementId::OutputHotkey => self.draft.hotkeys.toggle_output = Some(hotkey),
                        ElementId::ForegroundHotkey => {
                            self.draft.hotkeys.toggle_foreground_audio = Some(hotkey)
                        }
                        ElementId::CycleInputHotkey => {
                            self.draft.hotkeys.cycle_input_device = Some(hotkey)
                        }
                        ElementId::CycleOutputHotkey => {
                            self.draft.hotkeys.cycle_output_device = Some(hotkey)
                        }
                        ElementId::ForegroundVolumeUpHotkey => {
                            self.draft.hotkeys.foreground_volume_up = Some(hotkey)
                        }
                        ElementId::ForegroundVolumeDownHotkey => {
                            self.draft.hotkeys.foreground_volume_down = Some(hotkey)
                        }
                        ElementId::PreviousDesktopHotkey => {
                            self.draft.virtual_desktops.previous_desktop = Some(hotkey)
                        }
                        ElementId::AssignScratchpadHotkey => {
                            self.draft.virtual_desktops.scratchpad_assign = Some(hotkey)
                        }
                        ElementId::ToggleScratchpadHotkey => {
                            self.draft.virtual_desktops.scratchpad_toggle = Some(hotkey)
                        }
                        ElementId::DisplayProfileHotkey => {
                            if key == VirtualKey(0x2E) && chord.modifiers.is_empty() {
                                self.set_active_profile_hotkey(None);
                            } else {
                                self.set_active_profile_hotkey(Some(hotkey));
                            }
                        }
                        _ => {}
                    }
''' + "'''" + r'''
finish_new = ''' + "'''" + r'''                    if let Some(slot) = HotkeySlot::from_capture_id(id) {
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
''' + "'''" + r'''
replace("src/ui/control_center.rs", finish_old, finish_new)

'''
s = s[:a] + record_patch + s[b:]

p.write_text(s, encoding='utf-8', newline='\n')
print('pass2 paint, activation, and record paths scoped')
