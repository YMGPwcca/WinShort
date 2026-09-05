use super::*;

#[test]
fn managed_shortcut_buttons_expose_dynamic_uia_names() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = empty_settings_ui();
    ui.page = Page::Shortcuts;
    ui.rebuild_layout(hwnd);
    ui.install_automation(hwnd);

    let snapshot = ui.automation.as_ref().expect("automation").snapshot();
    let state = snapshot
        .nodes
        .iter()
        .find(|node| node.id == ElementId::HotkeyEnabled(HotkeySlot::Microphone))
        .expect("shortcut state node");
    assert_eq!(state.name, "Disable Mute microphone shortcut");
    assert!(state.enabled);
    let unassign = snapshot
        .nodes
        .iter()
        .find(|node| node.id == ElementId::HotkeyUnassign(HotkeySlot::Microphone))
        .expect("shortcut unassign node");
    assert_eq!(unassign.name, "Unassign Mute microphone shortcut");
    assert!(unassign.enabled);
}

#[test]
fn disabled_shortcut_re_recording_keeps_it_disabled() {
    let mut ui = empty_settings_ui();
    let replacement = Hotkey::parse("Ctrl+Alt+F20").unwrap();
    ui.draft
        .hotkeys
        .set_disabled_hotkey("cycle_output_device".into(), replacement);

    ui.set_recorded_hotkey(
        HotkeySlot::CycleOutput,
        Hotkey::parse("Ctrl+Alt+F21").unwrap(),
    );

    assert!(ui.draft.hotkeys.cycle_output_device.is_none());
    assert_eq!(
        ui.draft.hotkeys.disabled_hotkey("cycle_output_device"),
        Some(Hotkey::parse("Ctrl+Alt+F21").unwrap())
    );
    assert_eq!(
        ui.configured_hotkey(HotkeySlot::CycleOutput),
        Some(Hotkey::parse("Ctrl+Alt+F21").unwrap())
    );
    assert!(!ui.hotkey_enabled(HotkeySlot::CycleOutput));
    assert!(!ui.is_disabled(ElementId::HotkeyEnabled(HotkeySlot::CycleOutput)));
}

#[test]
fn unassigning_a_shortcut_removes_active_and_disabled_copies() {
    let mut ui = empty_settings_ui();
    let hotkey = Hotkey::parse("Ctrl+Alt+F20").unwrap();
    ui.draft.hotkeys.cycle_output_device = Some(hotkey);
    ui.draft
        .hotkeys
        .set_disabled_hotkey("cycle_output_device".into(), hotkey);
    let action = ui.hotkey_action(HotkeySlot::CycleOutput).unwrap();

    ui.set_active_hotkey(HotkeySlot::CycleOutput, None);
    ui.draft.hotkeys.clear_disabled_hotkey(&action);

    assert!(ui.draft.hotkeys.cycle_output_device.is_none());
    assert!(ui.draft.hotkeys.disabled_hotkey(&action).is_none());
    assert!(ui.is_disabled(ElementId::HotkeyUnassign(HotkeySlot::CycleOutput)));
}

#[test]
fn phase_one_hotkey_capture_maps_to_each_config_field() {
    let hotkey = Hotkey {
        modifiers: ModifierMask::CTRL.union(ModifierMask::ALT),
        key: VirtualKey(0x7C),
    };

    let mut ui = empty_settings_ui();
    for id in [
        ElementId::CycleInputHotkey,
        ElementId::CycleOutputHotkey,
        ElementId::ForegroundVolumeUpHotkey,
        ElementId::ForegroundVolumeDownHotkey,
    ] {
        ui.recording = Some(id);
        ui.finish_recording(crate::keyboard::hook::CapturedChord {
            modifiers: hotkey.modifiers,
            key: Some(hotkey.key),
        });
    }
    assert_eq!(ui.draft.hotkeys.cycle_input_device, Some(hotkey));
    assert_eq!(ui.draft.hotkeys.cycle_output_device, Some(hotkey));
    assert_eq!(ui.draft.hotkeys.foreground_volume_up, Some(hotkey));
    assert_eq!(ui.draft.hotkeys.foreground_volume_down, Some(hotkey));
}

#[test]
fn home_shortcut_health_explains_pause_instead_of_claiming_activity() {
    let mut ui = empty_settings_ui();
    ui.draft.general.start_hotkeys_enabled = false;
    let (value, detail, _) = ui.shortcut_health_copy();
    assert_eq!(value, "Shortcuts paused");
    assert!(detail.contains("configured"));
    assert!(!value.contains("active"));
}
