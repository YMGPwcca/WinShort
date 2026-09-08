use super::*;

#[test]
fn audio_modes_are_mutually_exclusive_and_selected_devices_are_progressive() {
    let context = super::super::LayoutContext {
        input_cycle_mode: AllowlistMode::Selected,
        input_device_count: 3,
        output_cycle_mode: AllowlistMode::All,
        output_device_count: 2,
        ..Default::default()
    };
    let layout = SettingsLayout::build_shell_with_context(
        1200.0,
        900.0,
        0.0,
        Page::Audio,
        "",
        context,
        None,
    );
    assert!(layout.element(ElementId::InputCycleMode(0)).is_some());
    assert!(layout.element(ElementId::InputCycleMode(1)).is_some());
    assert!(layout.element(ElementId::InputCycleMode(2)).is_some());
    assert!(layout.element(ElementId::InputCycleDevice(2)).is_some());
    assert!(layout.element(ElementId::OutputCycleDevice(0)).is_none());
}

#[test]
fn current_app_audio_omits_empty_status_but_keeps_shortcuts() {
    let empty = SettingsLayout::build_shell(1200.0, 900.0, 0.0, Page::Audio, "", 0, None);
    assert_eq!(
        empty
            .sections
            .iter()
            .find(|section| section.title == "Current app audio")
            .expect("current app heading")
            .description,
        "Available when another app has audio."
    );
    assert!(empty
        .regions
        .iter()
        .all(|region| region.kind != RegionKind::AudioCurrentApp));
    assert!(empty
        .element(ElementId::HotkeyCard(super::super::HotkeySlot::Foreground))
        .is_some());

    let meaningful = SettingsLayout::build_shell_with_context(
        1200.0,
        900.0,
        0.0,
        Page::Audio,
        "",
        super::super::LayoutContext {
            current_app_audio_available: true,
            ..Default::default()
        },
        None,
    );
    let status = meaningful
        .regions
        .iter()
        .find(|region| region.kind == RegionKind::AudioCurrentApp)
        .expect("current app status");
    assert_eq!(status.rect.h, 64.0);
    assert!(status.rect.h < super::super::UiTokens::CARD_HEIGHT);
}
