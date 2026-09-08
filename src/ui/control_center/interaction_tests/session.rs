use super::*;

#[test]
fn reset_requires_two_explicit_activations() {
    let mut ui = empty_settings_ui();
    assert!(!ui
        .interaction
        .confirmations_mut()
        .request_or_consume(ConfirmationTarget::ResetSettings));
    assert!(ui
        .interaction
        .confirmations()
        .is_pending(ConfirmationTarget::ResetSettings));
    assert!(ui
        .interaction
        .confirmations_mut()
        .request_or_consume(ConfirmationTarget::ResetSettings));
    assert!(!ui
        .interaction
        .confirmations()
        .is_pending(ConfirmationTarget::ResetSettings));
}

#[test]
fn endpoint_roles_apply_only_to_default_selection() {
    assert!(SettingsUi::endpoint_role_enabled(&DeviceSelection::Default));
    assert!(!SettingsUi::endpoint_role_enabled(
        &DeviceSelection::Endpoint("opaque".into(),)
    ));
}

#[test]
fn slider_keyboard_steps_stay_within_validation_ranges() {
    assert_eq!(
        SettingsUi::slider_value(ElementId::OverlayDuration, 500.0, -1.0),
        500.0
    );
    assert_eq!(
        SettingsUi::slider_value(ElementId::OverlayDuration, 500.0, 1.0),
        600.0
    );
    assert_eq!(
        SettingsUi::slider_value(ElementId::OverlayDuration, 500.0, f32::INFINITY),
        10_000.0
    );
    assert!(
        (SettingsUi::slider_value(ElementId::OverlayScale, 0.7, -1.0) - 0.7).abs() < f32::EPSILON
    );
    assert!(
        (SettingsUi::slider_value(ElementId::OverlayScale, 0.7, 1.0) - 0.8).abs() < f32::EPSILON
    );
    assert_eq!(
        SettingsUi::slider_value(ElementId::OverlayBlur, 2.0, -1.0),
        1.0
    );
    assert_eq!(
        SettingsUi::slider_value(ElementId::OverlayBlur, 2.0, 1.0),
        3.0
    );
    assert_eq!(
        SettingsUi::slider_value(ElementId::OverlayBlur, 2.0, f32::NEG_INFINITY),
        0.0
    );
    assert_eq!(
        SettingsUi::slider_value(ElementId::OverlayBlur, 2.0, f32::INFINITY),
        4.0
    );
}
#[test]
fn blur_slider_snaps_to_discrete_treatments() {
    let mut ui = empty_settings_ui();
    ui.set_slider_from_ratio(ElementId::OverlayBlur, 0.74);
    assert_eq!(
        ui.draft.overlay.blur,
        crate::config::model::OverlayBlur::BlurHeavy
    );
    assert!(ui.set_slider_from_value(ElementId::OverlayBlur, 0.0));
    assert_eq!(
        ui.draft.overlay.blur,
        crate::config::model::OverlayBlur::Transparent
    );
    assert!(ui.set_slider_from_value(ElementId::OverlayBlur, 4.0));
    assert_eq!(
        ui.draft.overlay.blur,
        crate::config::model::OverlayBlur::Solid
    );
}

#[test]
fn current_app_layout_flag_tracks_external_runtime_target() {
    let mut ui = empty_settings_ui();
    assert!(!ui.layout_context().current_app_audio_available);

    ui.runtime.foreground = crate::audio::AppAudioState {
        app_name: Some("Player".into()),
        aggregate: crate::audio::Aggregate::AllActive,
        sessions: 1,
        error: None,
    };
    assert!(ui.layout_context().current_app_audio_available);
}

#[test]
fn snapshot_publication_defers_uia_delivery_past_settings_borrow() {
    use std::cell::{Cell, RefCell};
    use windows::Win32::UI::Accessibility::UIA_ToggleToggleStatePropertyId;

    let hwnd = HWND(std::ptr::dangling_mut());
    let cell = RefCell::new(empty_settings_ui());
    let automation = {
        let mut ui = cell.borrow_mut();
        ui.install_automation(hwnd);
        ui.draft.overlay.enabled = !ui.draft.overlay.enabled;
        ui.publish_automation_snapshot(hwnd);
        ui.automation.clone().expect("automation")
    };
    let delivery_count = Cell::new(0);
    automation.flush_pending_events_for_test(|automation| {
        delivery_count.set(delivery_count.get() + 1);
        assert!(cell.try_borrow().is_ok());
        let provider = automation
            .provider_for(ElementId::OverlayEnabled)
            .expect("test provider initialization");
        let _ = unsafe {
            provider
                .GetPropertyValue(UIA_ToggleToggleStatePropertyId)
                .expect("provider re-query")
        };
    });
    assert!(delivery_count.get() > 0);
}

#[test]
fn button_activation_queues_one_deferred_invoke_event() {
    use std::cell::Cell;

    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = empty_settings_ui();
    ui.page = Page::Audio;
    ui.rebuild_layout(hwnd);
    ui.install_automation(hwnd);
    ui.activate(hwnd, ElementId::InputDevice);
    let automation = ui.automation.clone().expect("automation");
    let delivered = Cell::new(0);
    automation.flush_pending_events_for_test(|automation| {
        delivered.set(delivered.get() + 1);
        let provider = automation
            .provider_for(ElementId::InputDevice)
            .expect("test provider initialization");
        let _ = unsafe {
            provider
                .GetPropertyValue(
                    windows::Win32::UI::Accessibility::UIA_IsInvokePatternAvailablePropertyId,
                )
                .expect("invoke provider re-query")
        };
    });
    assert_eq!(delivered.get(), 1);
}

#[test]
fn cancel_after_toggle_change_restores_visual_toggle_source() {
    let id = ElementId::OverlayEnabled;
    let mut ui = empty_settings_ui();
    ui.motion
        .animate_to(id, MotionChannel::ToggleState, 1.0, 160);
    let mut live = Config::default();
    live.overlay.enabled = false;
    ui.replace_draft(live);
    assert_eq!(ui.motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
}

#[test]
fn reset_after_multiple_toggle_changes_matches_default_values() {
    let ids = [
        ElementId::StartHotkeysEnabled,
        ElementId::DesktopsEnabled,
        ElementId::WinNumberEnabled,
        ElementId::OverlayEnabled,
        ElementId::OverlayMicrophone,
        ElementId::OverlaySpeaker,
        ElementId::OverlayCurrentAppAudio,
        ElementId::OverlayWorkspace,
        ElementId::OverlayDisplayProfile,
    ];
    let mut ui = empty_settings_ui();
    for id in ids {
        ui.motion
            .animate_to(id, MotionChannel::ToggleState, 0.0, 160);
    }
    ui.replace_draft(Config::default());
    for id in ids {
        assert_eq!(ui.motion.value(id, MotionChannel::ToggleState, 1.0), 1.0);
    }
}

#[test]
fn active_toggle_animation_cannot_survive_model_replacement() {
    let id = ElementId::OverlayEnabled;
    let mut ui = empty_settings_ui();
    ui.motion
        .animate_to(id, MotionChannel::ToggleState, 1.0, 10_000);
    let mut replacement = Config::default();
    replacement.overlay.enabled = false;
    ui.replace_draft(replacement);
    assert_eq!(ui.motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
}

#[test]
fn fixed_titlebar_exposes_only_close_and_no_resize_actions() {
    let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
    let close = layout
        .element(ElementId::WindowClose)
        .expect("close button");
    assert_eq!(close.kind, ElementKind::ButtonSecondary);
    assert!(node_has_invoke(close.kind));
    assert!(layout.focus_order().contains(&close.id));
    assert_eq!(
        layout
            .elements
            .iter()
            .filter(|element| { !element.scrolls && element.kind == ElementKind::ButtonSecondary })
            .map(|element| element.id)
            .collect::<Vec<_>>(),
        vec![ElementId::WindowClose]
    );

    let forbidden = windows::Win32::UI::WindowsAndMessaging::WS_THICKFRAME.0
        | windows::Win32::UI::WindowsAndMessaging::WS_MINIMIZEBOX.0
        | windows::Win32::UI::WindowsAndMessaging::WS_MAXIMIZEBOX.0;
    assert_eq!(CONTROL_CENTER_STYLE.0 & forbidden, 0);
    assert_eq!(
        CONTROL_CENTER_STYLE.0 & windows::Win32::UI::WindowsAndMessaging::WS_POPUP.0,
        windows::Win32::UI::WindowsAndMessaging::WS_POPUP.0
    );
    assert!(blocked_fixed_window_command(0xF000));
    assert!(blocked_fixed_window_command(0xF020));
    assert!(blocked_fixed_window_command(0xF030));
    assert!(blocked_fixed_window_command(0xF120));
    assert!(!blocked_fixed_window_command(0xF060));

    let chrome = crate::ui::layout::top_chrome_geometry(layout.width, layout.nav_width);
    assert_eq!(layout.search_rect.y, chrome.close.y);
    assert_eq!(layout.search_rect.h, chrome.close.h);
    assert_eq!(
        chrome_hit_test_dip(
            chrome,
            chrome.search.x + chrome.search.w * 0.5,
            chrome.search.y + chrome.search.h * 0.5,
        ),
        HTCLIENT
    );
    assert_eq!(
        chrome_hit_test_dip(
            chrome,
            chrome.caption.x + chrome.caption.w * 0.5,
            chrome.caption.y + chrome.caption.h * 0.5,
        ),
        HTCAPTION
    );
    assert_eq!(
        chrome_hit_test_dip(chrome, chrome.row.x + 1.0, chrome.row.y + 1.0),
        HTCLIENT
    );
    assert_eq!(
        chrome_hit_test_dip(chrome, 1.0, layout.height - 1.0),
        HTCLIENT
    );
}

#[test]
fn preview_work_area_aspect_uses_real_bounds_and_16_9_fallback() {
    assert_eq!(work_area_aspect(None), (16, 9));
    assert_eq!(
        work_area_aspect(Some(RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1200,
        })),
        (1920, 1200)
    );
}

#[test]
fn normal_special_workspace_summary_uses_available_without_ready_badge() {
    let mut ui = empty_settings_ui();
    ui.runtime.desktop.native = crate::desktop::BackendAvailability::Available;

    let (status, detail, action) = ui.special_workspace_summary();

    assert_eq!(status, "Available");
    assert!(!detail.is_empty());
    assert_eq!(action, "Open");
}

#[test]
fn reenable_validates_disabled_chord_against_active_bindings() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = empty_settings_ui();
    let conflict = ui.draft.hotkeys.toggle_microphone.unwrap();
    ui.draft
        .hotkeys
        .set_disabled_hotkey("cycle_output_device".into(), conflict);

    ui.toggle_hotkey_enabled(hwnd, HotkeySlot::CycleOutput);

    assert!(ui.draft.hotkeys.cycle_output_device.is_none());
    assert_eq!(
        ui.draft.hotkeys.disabled_hotkey("cycle_output_device"),
        Some(conflict)
    );
    assert!(ui
        .validation
        .iter()
        .any(|violation| violation.message.contains("conflicts")));
}

#[test]
fn home_navigation_copy_uses_coherent_open_actions() {
    let ui = empty_settings_ui();
    assert_eq!(ui.special_workspace_summary().2, "Open");
    assert_eq!(ui.shortcut_health_copy().2, "Open");
    match ui.value_for(ElementId::HomeDiagnostics) {
        ControlValue::Action(value) => assert_eq!(value, "Open"),
        _ => panic!("expected Home diagnostics action"),
    }
    match ui.value_for(ElementId::DiagnosticsStatus) {
        ControlValue::Action(value) => assert_eq!(value, "Open"),
        _ => panic!("expected diagnostics action"),
    }
}

#[test]
fn special_workspace_off_state_exposes_enable_recovery_action() {
    let mut ui = empty_settings_ui();
    ui.draft.virtual_desktops.enabled = false;
    assert_eq!(
        ui.special_workspace_summary(),
        (
            "Off".to_string(),
            "Workspaces are off".to_string(),
            "Enable".to_string(),
        )
    );
}

#[test]
fn special_workspace_distinguishes_disabled_from_backend_unavailable() {
    let mut ui = empty_settings_ui();
    ui.draft.virtual_desktops.enabled = true;
    let (status, detail, action) = ui.special_workspace_summary();
    assert_eq!(status, "Unavailable");
    assert!(detail.contains("service"));
    assert_eq!(action, "Open");
    ui.draft.virtual_desktops.enabled = false;
    assert_eq!(ui.special_workspace_summary().0, "Off");
}

#[test]
fn pause_toggle_reports_paused_state_not_enabled_state() {
    let mut ui = empty_settings_ui();
    ui.draft.general.start_hotkeys_enabled = false;
    assert!(matches!(
        ui.value_for(ElementId::StartHotkeysEnabled),
        ControlValue::Toggle(true)
    ));
    ui.draft.general.start_hotkeys_enabled = true;
    assert!(matches!(
        ui.value_for(ElementId::StartHotkeysEnabled),
        ControlValue::Toggle(false)
    ));
}

#[test]
fn applied_status_is_generic() {
    let status = APPLIED_STATUS.to_ascii_lowercase();
    assert!(status.contains("applied"));
    assert!(!status.contains("hotkey"));
}
