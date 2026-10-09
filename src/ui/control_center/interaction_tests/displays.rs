use super::*;

#[test]
fn reselecting_a_profile_does_not_open_review_or_change_the_draft() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.enabled = true;
    ui.draft.display_profiles.profiles = vec![sample_profile("selected", "Selected setup", false)];
    ui.draft.display_profiles.active_profile = Some("selected".into());
    let before = ui.draft.clone();
    ui.activate_display_profile_card(HWND::default(), 0);
    assert_eq!(ui.draft, before);
    assert!(!ui.display.is_editing());
}

#[test]
fn advanced_display_topology_stays_selected_in_picker() {
    let mut config = Config::default();
    let mut profile = sample_profile("custom", "Custom layout", true);
    profile.topology = crate::display::DisplayTopology::Custom;
    config.display_profiles.profiles = vec![profile];
    config.display_profiles.active_profile = Some("custom".into());

    let model = picker_model(
        PickerKind::DisplayTopology,
        &config,
        &Default::default(),
        &[],
        None,
    )
    .expect("display topology picker model");
    let selected = model.current().expect("current topology");
    assert_eq!(
        model.choices()[selected].label(),
        "Current arrangement (advanced)"
    );
    assert_eq!(
        model.choices()[selected].commit_value(),
        Some(&PickerCommit::DisplayTopology(
            crate::display::DisplayTopology::Custom
        ))
    );
}

#[test]
fn display_outputs_value_hides_raw_displayconfig_identity() {
    let mut ui = empty_settings_ui();
    let route = crate::display::DisplayRoute {
        target_path: r"\\?\DISPLAY#MONITOR-A".into(),
        target_adapter: 1,
        target_id: 2,
        output_technology: 5,
        ..Default::default()
    };
    ui.inventory = DisplayInventory::Available(vec![crate::display::DisplayOutput {
        route: route.clone(),
        monitor_name: "Desk Monitor".into(),
        adapter_name: "AMD Radeon Graphics".into(),
        connector_name: "HDMI".into(),
        active: true,
    }]);
    if matches!(ui.inventory, DisplayInventory::Unqueried) {
        ui.inventory = DisplayInventory::Available(Vec::new());
    }
    ui.draft.display_profiles.profiles = vec![crate::display::DisplayProfile {
        id: "ai".into(),
        name: "AI".into(),
        topology: crate::display::DisplayTopology::Custom,
        confirmed: false,
        routes: vec![route],
    }];
    ui.draft.display_profiles.active_profile = Some("ai".into());
    match ui.value_for(ElementId::DisplayOutputs) {
        ControlValue::Text(value) => {
            assert!(value.contains("Desk Monitor"));
            assert!(value.contains("AMD Radeon Graphics"));
            assert!(!value.contains("DISPLAY#"));
        }
        _ => panic!("unexpected control value variant"),
    }
}

#[test]
fn display_topology_changes_stay_local_until_explicit_keep() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("display", "Display", true)];
    ui.draft.display_profiles.active_profile = Some("display".into());

    ui.apply_picker(PickerCommit::DisplayTopology(
        crate::display::DisplayTopology::Clone,
    ));

    assert!(ui.display.is_dirty());
    assert_eq!(ui.display.step(), Some(DisplayWizardStep::Review));
    assert_eq!(
        ui.draft.display_profiles.active().unwrap().topology,
        crate::display::DisplayTopology::Clone
    );
    ui.replace_draft(Config::default());
    assert!(!ui.display.is_dirty());
    assert!(!ui.display.is_editing());
}

#[test]
fn display_profile_rename_preserves_id_confirmation_and_hotkey_reference() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("a4c", "Gaming", true)];
    ui.draft.display_profiles.active_profile = Some("a4c".into());
    let hotkey = Hotkey::parse("Ctrl+Alt+F1").unwrap();
    ui.draft
        .hotkeys
        .display_profiles
        .push(crate::config::model::DisplayProfileHotkey {
            profile_id: "a4c".into(),
            hotkey,
        });

    ui.rename_display_profile("a4c", "  Gaming 240Hz  ");

    let profile = ui.draft.display_profiles.active().unwrap();
    assert_eq!(profile.id, "a4c");
    assert_eq!(profile.name, "Gaming 240Hz");
    assert!(profile.confirmed);
    assert_eq!(ui.draft.hotkeys.display_profiles[0].profile_id, "a4c");
}

#[test]
fn display_profile_duplicate_gets_new_id_without_hotkey_or_confirmation() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("gaming", "Gaming", true)];
    ui.draft.display_profiles.active_profile = Some("gaming".into());
    ui.draft
        .hotkeys
        .display_profiles
        .push(crate::config::model::DisplayProfileHotkey {
            profile_id: "gaming".into(),
            hotkey: Hotkey::parse("Ctrl+Alt+F2").unwrap(),
        });

    ui.duplicate_active_display_profile();

    let duplicate = ui.draft.display_profiles.active().unwrap();
    assert_ne!(duplicate.id, "gaming");
    assert_eq!(duplicate.name, "Gaming Copy");
    assert!(!duplicate.confirmed);
    assert_eq!(ui.draft.hotkeys.display_profiles.len(), 1);
    assert_eq!(ui.draft.hotkeys.display_profiles[0].profile_id, "gaming");
}

#[test]
fn display_profile_delete_removes_hotkey_and_selects_remaining_profile() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![
        sample_profile("first", "First", true),
        sample_profile("second", "Second", true),
    ];
    ui.draft.display_profiles.active_profile = Some("first".into());
    ui.draft
        .hotkeys
        .display_profiles
        .push(crate::config::model::DisplayProfileHotkey {
            profile_id: "first".into(),
            hotkey: Hotkey::parse("Ctrl+Alt+F3").unwrap(),
        });
    ui.draft.hotkeys.set_disabled_hotkey(
        "display_profile:first".into(),
        Hotkey::parse("Ctrl+Alt+F4").unwrap(),
    );

    ui.delete_active_display_profile();

    assert_eq!(
        ui.draft.display_profiles.active_profile.as_deref(),
        Some("second")
    );
    assert!(ui.draft.display_profiles.active().is_some());
    assert!(ui.draft.hotkeys.display_profiles.is_empty());
    assert!(ui
        .draft
        .hotkeys
        .disabled_hotkey("display_profile:first")
        .is_none());
    assert_eq!(ui.display.selected_route(), None);
}

#[test]
fn display_route_editor_changes_supported_values_and_untrusts_profile() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("work", "Work", true)];
    ui.draft.display_profiles.active_profile = Some("work".into());

    ui.edit_display_route("work", 0, "-1920,0,2560,1440,144,90");

    let route = &ui.draft.display_profiles.active().unwrap().routes[0];
    assert_eq!(route.source_position_x, -1920);
    assert_eq!(route.source_width, 2560);
    assert_eq!(route.source_height, 1440);
    assert_eq!(route.refresh_numerator, 144);
    assert_eq!(route.rotation, 2);
    assert!(!ui.draft.display_profiles.active().unwrap().confirmed);
    assert_eq!(ui.display.selected_route(), Some(0));
    assert!(ui.display.is_dirty());
    assert_eq!(ui.display.step(), Some(DisplayWizardStep::Review));
}

#[test]
fn display_route_editor_rejects_unsupported_values() {
    assert!(parse_display_route_values("0,0,1920,1080,60,45").is_err());
    assert!(parse_display_route_values("0,0,0,1080,60,0").is_err());
    assert!(parse_display_route_values("0,0,1920,1080,60,0,extra").is_err());
}

#[test]
fn display_route_editor_preserves_refresh_rate_rationals() {
    let edit = parse_display_route_values("0,0,1920,1080,60000/1001,0").expect("valid rational");
    let mut route = sample_profile("id", "name", true).routes.remove(0);
    edit.apply_to(&mut route);
    assert_eq!(
        (
            route.refresh_numerator,
            route.refresh_denominator,
            route.rotation
        ),
        (60000, 1001, 1)
    );
    assert!(parse_display_route_values("0,0,1920,1080,60/0,0").is_err());
    assert!(parse_display_route_values("0,0,1920,1080,60/1001/2,0").is_err());
}

#[test]
fn profile_hotkey_capture_and_clear_uses_stable_profile_id() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("ai-id", "AI", true)];
    ui.draft.display_profiles.active_profile = Some("ai-id".into());
    let hotkey = Hotkey::parse("Ctrl+Alt+F4").unwrap();

    ui.interaction
        .start_capture(ElementId::DisplayProfileHotkey);
    ui.finish_recording(crate::keyboard::hook::CapturedChord {
        modifiers: hotkey.modifiers,
        key: Some(hotkey.key),
    });
    assert_eq!(ui.draft.hotkeys.display_profiles.len(), 1);
    assert_eq!(ui.draft.hotkeys.display_profiles[0].profile_id, "ai-id");
    assert_eq!(ui.draft.hotkeys.display_profiles[0].hotkey, hotkey);

    ui.interaction
        .start_capture(ElementId::DisplayProfileHotkey);
    ui.finish_recording(crate::keyboard::hook::CapturedChord {
        modifiers: ModifierMask::NONE,
        key: Some(VirtualKey(0x2E)),
    });
    assert!(ui.draft.hotkeys.display_profiles.is_empty());
}

#[test]
fn dirty_display_edits_block_other_changes_without_mutating_draft() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = empty_settings_ui();
    ui.display.open(DisplayWizardStep::Review, true);
    let before = ui.draft.clone();
    ui.draft.overlay.enabled = !ui.draft.overlay.enabled;

    assert!(!ui.commit_local_change(hwnd, before.clone()));
    assert_eq!(ui.draft, before);
    assert!(ui
        .validation
        .iter()
        .any(|violation| violation.field == "Displays"));
}

#[test]
fn display_inventory_failure_is_reported_as_unknown_not_unavailable() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("unknown", "Baseline", true)];
    ui.draft.display_profiles.active_profile = Some("unknown".into());
    ui.inventory = DisplayInventory::Failed("inventory unavailable".into());
    let (_, detail) = ui.display_summary();
    assert!(detail.contains("Readiness unknown"));
    assert!(!detail.contains("route(s) unavailable"));
}

#[test]
fn home_display_summary_does_not_assume_inventory_before_first_refresh() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("pending", "Baseline", true)];
    ui.draft.display_profiles.active_profile = Some("pending".into());
    let (_, detail) = ui.display_summary();
    assert!(detail.contains("Readiness pending"));
    assert!(!detail.contains("unavailable"));
}

#[test]
fn home_status_names_display_profile_warning_truthfully() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("status", "Baseline", true)];
    ui.draft.display_profiles.active_profile = Some("status".into());
    ui.inventory = DisplayInventory::Failed("display query failed".into());
    let (title, value, detail) = ui.home_diagnostics_copy();
    assert_eq!(title, "Display profile needs attention");
    assert!(value.contains("Readiness unknown"));
    assert!(detail.contains("display information"));
}

#[test]
fn display_test_remains_enabled_when_backend_revalidates_inventory() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("test", "Baseline", true)];
    ui.draft.display_profiles.active_profile = Some("test".into());
    ui.display.open(DisplayWizardStep::Review, false);
    ui.inventory = DisplayInventory::Failed("display query failed".into());
    assert!(!ui.is_disabled(ElementId::TestApplyDisplayProfile));
    assert!(ui.layout_context().display_inventory_unknown);
    assert!(ui
        .display_review_readiness(ui.draft.display_profiles.active().unwrap())
        .starts_with("Unknown"));
}

#[test]
fn display_review_summary_contains_draft_identity_and_readiness() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("review", "Baseline", true)];
    ui.draft.display_profiles.active_profile = Some("review".into());
    ui.inventory = DisplayInventory::Failed("display query failed".into());
    let profile = ui.draft.display_profiles.active().unwrap().clone();
    assert_eq!(ui.display_review_screen_names(&profile), "Saved screen 1");
    assert!(ui
        .display_review_readiness(&profile)
        .contains("Windows display information unavailable"));
}

#[test]
fn display_readiness_marks_unavailable_screens_as_attention() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("missing", "Baseline", true)];
    if matches!(ui.inventory, DisplayInventory::Unqueried) {
        ui.inventory = DisplayInventory::Available(Vec::new());
    }
    ui.draft.display_profiles.active_profile = Some("missing".into());
    assert!(ui.display_profile_needs_attention());
    let (_, detail) = ui.display_summary();
    assert!(detail.contains("Needs attention"));
    assert!(detail.contains("unavailable"));
}

#[test]
fn display_editor_back_and_next_move_one_step_without_applying() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("wizard", "Wizard", true)];
    ui.draft.display_profiles.active_profile = Some("wizard".into());
    ui.display.open(DisplayWizardStep::Displays, false);
    ui.move_display_editor(HWND::default(), true);
    assert_eq!(ui.display.step(), Some(DisplayWizardStep::Arrangement));
    assert_eq!(
        ui.draft
            .display_profiles
            .active()
            .expect("profile")
            .topology,
        crate::display::DisplayTopology::Extend
    );
    ui.move_display_editor(HWND::default(), false);
    assert_eq!(ui.display.step(), Some(DisplayWizardStep::Displays));
}

#[test]
fn display_editor_requires_a_route_before_advancing() {
    let mut ui = empty_settings_ui();
    ui.display.open(DisplayWizardStep::Displays, false);
    ui.move_display_editor(HWND::default(), true);
    assert_eq!(ui.display.step(), Some(DisplayWizardStep::Displays));
    assert!(ui
        .validation
        .iter()
        .any(|violation| violation.message.contains("at least one")));
}

#[test]
fn delete_profile_requires_a_second_confirmation() {
    let mut ui = empty_settings_ui();
    ui.draft.display_profiles.profiles = vec![sample_profile("delete", "Delete me", true)];
    ui.draft.display_profiles.active_profile = Some("delete".into());
    assert!(!ui.is_disabled(ElementId::DeleteDisplayProfile));
    assert!(!ui
        .interaction
        .confirmations_mut()
        .request_or_consume(ConfirmationTarget::DeleteDisplayProfile));
    assert!(ui
        .interaction
        .confirmations()
        .is_pending(ConfirmationTarget::DeleteDisplayProfile));
    assert!(ui
        .interaction
        .confirmations_mut()
        .request_or_consume(ConfirmationTarget::DeleteDisplayProfile));
    assert!(!ui
        .interaction
        .confirmations()
        .is_pending(ConfirmationTarget::DeleteDisplayProfile));
}
