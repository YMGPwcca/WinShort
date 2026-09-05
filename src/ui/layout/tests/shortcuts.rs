use super::*;

#[test]
fn compatibility_layout_keeps_phase_one_hotkeys() {
    let layout = SettingsLayout::build(610.0, 720.0, 0.0);
    for id in [
        ElementId::CycleInputHotkey,
        ElementId::CycleOutputHotkey,
        ElementId::ForegroundVolumeUpHotkey,
        ElementId::ForegroundVolumeDownHotkey,
    ] {
        assert!(layout.element(id).is_some());
    }
    assert!(matches!(
        layout
            .element(ElementId::OverlayDuration)
            .map(|element| element.kind),
        Some(ElementKind::Slider)
    ));
}

#[test]
fn special_workspace_uses_a_heading_and_consistent_shortcut_cards() {
    let layout = SettingsLayout::build_shell_with_context(
        960.0,
        900.0,
        0.0,
        Page::Workspaces,
        "",
        super::super::LayoutContext::default(),
        None,
    );
    let special = layout
        .sections
        .iter()
        .find(|section| section.title == "Special Desktop")
        .expect("special desktop heading");
    assert_eq!(
        special.description,
        "A dedicated place for windows you want nearby but out of the way."
    );
    let move_card = layout
        .element(ElementId::HotkeyCard(HotkeySlot::AssignSpecial))
        .expect("move card");
    let toggle_card = layout
        .element(ElementId::HotkeyCard(HotkeySlot::ToggleSpecial))
        .expect("toggle card");
    assert_eq!(
        toggle_card.rect.y - move_card.rect.bottom(),
        super::super::UiTokens::ROW_GAP
    );
    assert_eq!(toggle_card.rect.x, move_card.rect.x);
    assert_eq!(toggle_card.rect.w, move_card.rect.w);
    assert!(layout.regions.is_empty());
}

#[test]
fn special_shortcut_card_gap_is_stable_at_multiple_scroll_positions() {
    for requested_scroll in [0.0, 120.0, 480.0] {
        let layout = SettingsLayout::build_shell_with_context(
            960.0,
            660.0,
            requested_scroll,
            Page::Workspaces,
            "",
            super::super::LayoutContext::default(),
            None,
        );
        let move_card = layout
            .element(ElementId::HotkeyCard(HotkeySlot::AssignSpecial))
            .expect("move card");
        let toggle_card = layout
            .element(ElementId::HotkeyCard(HotkeySlot::ToggleSpecial))
            .expect("toggle card");
        assert_eq!(
            toggle_card.rect.y - move_card.rect.bottom(),
            super::super::UiTokens::ROW_GAP
        );
        assert_eq!(toggle_card.rect.w, move_card.rect.w);
    }
}

#[test]
fn workspace_off_still_exposes_disabled_special_shortcuts() {
    let layout = SettingsLayout::build_shell_with_context(
        960.0,
        900.0,
        0.0,
        Page::Workspaces,
        "",
        super::super::LayoutContext {
            workspace_enabled: false,
            ..Default::default()
        },
        None,
    );
    assert!(layout
        .regions
        .iter()
        .any(|region| region.kind == RegionKind::WorkspaceNotice));
    assert!(layout
        .sections
        .iter()
        .any(|section| section.title == "Special Desktop"));
    assert!(layout
        .element(ElementId::HotkeyCard(HotkeySlot::AssignSpecial))
        .is_some());
    assert!(layout
        .element(ElementId::HotkeyCard(HotkeySlot::ToggleSpecial))
        .is_some());
}

#[test]
fn managed_shortcut_cards_include_record_state_and_unassign_controls() {
    let layout = SettingsLayout::build_shell(960.0, 900.0, 0.0, Page::Shortcuts, "", 0, None);
    for (slot, capture) in [
        (HotkeySlot::Microphone, ElementId::MicHotkey),
        (HotkeySlot::CycleOutput, ElementId::CycleOutputHotkey),
        (HotkeySlot::DisplayProfile, ElementId::DisplayProfileHotkey),
    ] {
        let card = layout
            .element(ElementId::HotkeyCard(slot))
            .expect("shortcut card");
        let keycap = layout.element(capture).expect("shortcut keycap");
        let enabled = layout
            .element(ElementId::HotkeyEnabled(slot))
            .expect("shortcut state button");
        let unassign = layout
            .element(ElementId::HotkeyUnassign(slot))
            .expect("shortcut unassign button");
        assert!(card.rect.contains(keycap.rect.x, keycap.rect.y));
        assert!(card.rect.contains(enabled.rect.x, enabled.rect.y));
        assert!(card
            .rect
            .contains(unassign.rect.right(), unassign.rect.bottom()));
        assert!(layout.focus_order().contains(&capture));
        assert!(layout.focus_order().contains(&enabled.id));
        assert!(layout.focus_order().contains(&unassign.id));
    }
}

#[test]
fn onboarding_steps_expose_real_choices_and_shortcut_values() {
    let first = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, Some(1));
    assert!(first.element(ElementId::OutputAllowlist).is_some());
    assert!(first.element(ElementId::OnboardingContinue).is_some());
    let ready = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, Some(2));
    assert!(ready.element(ElementId::MicHotkey).is_some());
    assert!(ready.element(ElementId::OnboardingOpen).is_some());
}
