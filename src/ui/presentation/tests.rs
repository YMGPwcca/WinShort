use super::*;

fn device(name: &str) -> DeviceId {
    DeviceId {
        endpoint: name.into(),
        name: name.into(),
    }
}

#[test]
fn normalizes_only_known_enumeration_wrappers() {
    let label = friendly_device_name(
        "3 - SAMSUNG (2- AMD High Definition Audio Device)",
        AudioDeviceKind::Speaker,
    );
    assert_eq!(label.primary, "SAMSUNG");
    assert_eq!(
        label.detail.as_deref(),
        Some("AMD High Definition Audio Device")
    );

    let unchanged = friendly_device_name("Studio (USB Audio)", AudioDeviceKind::Speaker);
    assert_eq!(unchanged.primary, "Studio (USB Audio)");
    assert!(unchanged.detail.is_none());
}

#[test]
fn removes_redundant_category_prefix_without_touching_identity() {
    let label = friendly_device_name("Microphone (SIMGOT EW300 DSP)", AudioDeviceKind::Microphone);
    assert_eq!(label.primary, "SIMGOT EW300 DSP");
}

#[test]
fn default_and_legacy_explicit_render_the_same_system_endpoint() {
    let current = device("current");
    let following = device_selection_presentation(
        &DeviceSelection::Default,
        std::slice::from_ref(&current),
        Some(&current),
        AudioDeviceKind::Speaker,
    );
    let legacy_explicit = device_selection_presentation(
        &DeviceSelection::Endpoint("current".into()),
        std::slice::from_ref(&current),
        Some(&current),
        AudioDeviceKind::Speaker,
    );
    assert_eq!(following.primary, "current");
    assert_eq!(following.secondary, None);
    assert_eq!(following.status, None);
    assert_eq!(legacy_explicit.primary, following.primary);
    assert_eq!(legacy_explicit.secondary, following.secondary);
    assert_eq!(legacy_explicit.status, None);
}

#[test]
fn audio_selection_keeps_device_identity_without_redundant_status_badges() {
    let current = device("speaker");
    let mut named = current.clone();
    named.name = "3 - SAMSUNG (2- AMD High Definition Audio Device)".into();
    let devices = [named.clone()];
    for selection in [
        DeviceSelection::Default,
        DeviceSelection::Endpoint(named.endpoint.clone()),
    ] {
        let presentation = device_selection_presentation(
            &selection,
            &devices,
            Some(&named),
            AudioDeviceKind::Speaker,
        );
        assert_eq!(presentation.primary, "SAMSUNG");
        assert_eq!(
            presentation.secondary.as_deref(),
            Some("AMD High Definition Audio Device")
        );
        assert_eq!(presentation.status, None);
        assert!(!presentation.accessible_value().contains("default"));
        assert!(!presentation.accessible_value().contains("Explicit"));
    }
    assert_eq!(
        device_choice_label(&named, Some(&named), AudioDeviceKind::Speaker),
        "SAMSUNG"
    );
}
#[test]
fn normal_audio_controls_expose_only_the_canonical_primary_name() {
    for (kind, raw, expected) in [
        (
            AudioDeviceKind::Speaker,
            "GS25F2 (AMD High Definition Audio Device)",
            "GS25F2",
        ),
        (
            AudioDeviceKind::Speaker,
            "SAMSUNG (AMD High Definition Audio Device)",
            "SAMSUNG",
        ),
        (
            AudioDeviceKind::Microphone,
            "SIMGOT EW300 DSP",
            "SIMGOT EW300 DSP",
        ),
    ] {
        let device = DeviceId {
            endpoint: raw.into(),
            name: raw.into(),
        };
        assert_eq!(device_choice_label(&device, None, kind), expected);
        assert_eq!(
            device_selection_presentation(
                &DeviceSelection::Default,
                std::slice::from_ref(&device),
                Some(&device),
                kind,
            )
            .primary,
            expected
        );
    }
}

#[test]
fn device_names_normalize_singular_and_plural_category_wrappers() {
    let microphone = friendly_device_name(
        "2 - Microphone (3- AMD High Definition Audio Device)",
        AudioDeviceKind::Microphone,
    );
    assert_eq!(microphone.primary, "Unknown microphone");
    assert_eq!(
        microphone.detail.as_deref(),
        Some("AMD High Definition Audio Device")
    );

    let speakers = friendly_device_name("Speakers (Realtek USB Audio)", AudioDeviceKind::Speaker);
    assert_eq!(speakers.primary, "Realtek USB Audio");
    assert!(speakers.detail.is_none());
}

#[test]
fn duplicate_friendly_devices_get_neutral_option_numbers() {
    let mut first = device("one");
    let mut second = device("two");
    first.name = "USB microphone".into();
    second.name = "USB microphone".into();
    let devices = vec![first, second];
    assert_eq!(
        device_choice_label_at(&devices, 0, None, AudioDeviceKind::Microphone)
            .expect("first label"),
        "USB microphone · Option 1"
    );
    assert_eq!(
        device_choice_label_at(&devices, 1, None, AudioDeviceKind::Microphone)
            .expect("second label"),
        "USB microphone · Option 2"
    );
}

#[test]
fn hotkeys_use_human_spacing_and_direction_glyphs() {
    let hotkey = Hotkey::parse("Shift+Win+Up").expect("hotkey");
    assert_eq!(format_hotkey(hotkey), "Shift + Win + ↑");
    assert_eq!(format_desktop_modifier(ModifierMask::WIN), "Win + 1–9");
}

#[test]
fn invalid_desktop_modifier_gets_actionable_copy() {
    assert_eq!(
        format_desktop_modifier(ModifierMask::from_bits(0)),
        "Choose a modifier"
    );
}

#[test]
fn allowlist_modes_preserve_persisted_semantics() {
    assert_eq!(allowlist_mode(None), AllowlistMode::All);
    assert_eq!(allowlist_mode(Some(&[])), AllowlistMode::Disabled);
    assert_eq!(
        allowlist_mode(Some(&["endpoint".to_string()])),
        AllowlistMode::Selected
    );
}

#[test]
fn wizard_steps_are_ordered_and_numbered() {
    assert_eq!(
        DisplayWizardStep::Displays.next(),
        Some(DisplayWizardStep::Arrangement)
    );
    assert_eq!(
        DisplayWizardStep::Review.previous(),
        Some(DisplayWizardStep::NameAndShortcut)
    );
    assert_eq!(DisplayWizardStep::NameAndShortcut.number(), 3);
}
