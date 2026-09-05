use super::*;

#[test]
fn overlay_monitor_picker_exposes_only_primary_and_cursor_position() {
    let mut config = Config::default();
    let (choices, current) = picker_choices(
        PickerKind::OverlayMonitor,
        &config,
        &Default::default(),
        &[],
        &[],
        0,
    );

    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Primary", "Cursor position"]
    );
    assert_eq!(current, 1);
    assert_eq!(
        choices[1].value,
        PickerValue::Monitor(MonitorChoice::Cursor)
    );

    config.overlay.monitor = MonitorChoice::Primary;
    let (_, current) = picker_choices(
        PickerKind::OverlayMonitor,
        &config,
        &Default::default(),
        &[],
        &[],
        0,
    );
    assert_eq!(current, 0);
}

#[test]
fn device_picker_exposes_only_real_endpoints_and_marks_system_default() {
    let current = crate::audio::DeviceId {
        endpoint: "current-endpoint".into(),
        name: "Current microphone".into(),
    };
    let devices = crate::audio::devices::DeviceLists {
        inputs: vec![current.clone()],
        outputs: Vec::new(),
        input_defaults: crate::audio::devices::DefaultDevices {
            console: Some(current),
            ..Default::default()
        },
        output_defaults: Default::default(),
        warnings: Vec::new(),
    };

    let config = Config::default();
    let (choices, selected) =
        picker_choices(PickerKind::InputDevice, &config, &devices, &[], &[], 0);

    assert_eq!(choices.len(), 1);
    assert_eq!(selected, 0);
    assert_eq!(choices[0].label, "Current microphone");
    assert_eq!(
        choices[0].value,
        PickerValue::Device(DeviceSelection::Endpoint("current-endpoint".into()))
    );
    assert_ne!(
        choices[0].value,
        PickerValue::Device(DeviceSelection::Default)
    );
}

#[test]
fn allowlist_picker_exposes_clear_controls_and_offline_selections() {
    let mut config = Config::default();
    config.audio.cycle_input_allowlist =
        Some(vec!["second-endpoint".into(), "missing-endpoint".into()]);
    let devices = crate::audio::devices::DeviceLists {
        inputs: vec![
            crate::audio::DeviceId {
                endpoint: "first-endpoint".into(),
                name: "First microphone".into(),
            },
            crate::audio::DeviceId {
                endpoint: "second-endpoint".into(),
                name: "Second microphone".into(),
            },
        ],
        outputs: Vec::new(),
        input_defaults: Default::default(),
        output_defaults: Default::default(),
        warnings: Vec::new(),
    };
    let (choices, current) =
        picker_choices(PickerKind::InputAllowlist, &config, &devices, &[], &[], 0);
    assert_eq!(current, 1);
    assert_eq!(choices[0].label, "All available microphones");
    assert_eq!(choices[1].label, "Selected microphones");
    assert_eq!(choices[2].label, "Don't cycle microphones");
    assert_eq!(
        picker_selection_indices(PickerKind::InputAllowlist, &config, &choices),
        vec![1, 4, 5]
    );
    assert!(choices[5].label.starts_with("Saved device unavailable"));

    config.audio.cycle_input_allowlist = None;
    assert_eq!(
        picker_selection_indices(PickerKind::InputAllowlist, &config, &choices),
        vec![0]
    );
    config.audio.cycle_input_allowlist = Some(Vec::new());
    assert_eq!(
        picker_selection_indices(PickerKind::InputAllowlist, &config, &choices),
        vec![2]
    );
}

#[test]
fn picker_anchor_is_converted_to_control_center_client_pixels() {
    let row = UiRect::new(24.0, 300.0, 560.0, 58.0);
    let control = controls::value_control_rect(row, crate::ui::layout::ElementKind::Value);
    let anchor = client_rect_from_dip(control, 144);
    assert_eq!(anchor.width(), 309);
    assert_eq!(anchor.height(), 51);
    let work = PopupRect::new(0, 0, 900, 600);
    let popup = crate::ui::picker::place_popup(anchor, work, 400, 180);
    assert_eq!(popup.right, anchor.right);
    assert_eq!(popup.bottom, anchor.top);
    assert!(popup.right <= work.right);
    assert!(popup.bottom <= work.bottom);
}

#[test]
fn picker_width_is_compact_and_bounded() {
    let short = vec![PickerChoice {
        label: "Top Left".into(),
        value: PickerValue::Position(OverlayPosition::TopLeft),
    }];
    let long = vec![PickerChoice {
        label: "A deliberately long endpoint name for the default device".into(),
        value: PickerValue::Position(OverlayPosition::TopLeft),
    }];
    assert_eq!(picker_width_dip(190.0, &short), 320.0);
    assert_eq!(picker_width_dip(190.0, &long), 400.0);
    assert_eq!(picker_width_dip(900.0, &long), 400.0);
}

#[test]
fn picker_height_includes_shared_host_inset() {
    assert_eq!(picker_height_px(0, 1.0), 10);
    assert_eq!(picker_height_px(3, 1.0), 106);
    assert_eq!(picker_height_px(12, 1.0), 330);
    assert_eq!(picker_height_px(3, 1.5), 158);
}

#[test]
fn picker_focus_state_suppresses_logical_child_focus() {
    let hwnd = HWND(2usize as *mut _);
    let mut ui = empty_settings_ui();
    ui.install_automation(hwnd);
    {
        let value = Some(ElementId::InputDevice);
        ui.focus.set_target(value);
    };
    let picker_hwnd = HWND(3usize as *mut _);
    let picker_list_hwnd = HWND(4usize as *mut _);
    ui.set_picker_open(
        hwnd,
        ElementId::InputDevice,
        picker_hwnd,
        picker_list_hwnd,
        hwnd,
    );
    let registered = ui.automation.as_ref().expect("automation").snapshot();
    assert_eq!(registered.focus_owner, AutomationFocusOwner::Settings);
    assert_eq!(registered.picker_open_for, Some(ElementId::InputDevice));
    assert!(
        registered
            .nodes
            .iter()
            .find(|node| node.id == ElementId::InputDevice)
            .expect("picker node")
            .focused
    );
    ui.on_window_focus(hwnd, false, picker_list_hwnd);
    let snapshot = ui.automation.as_ref().expect("automation").snapshot();
    assert_eq!(snapshot.focus_owner, AutomationFocusOwner::Picker);
    assert_eq!(snapshot.picker_open_for, Some(ElementId::InputDevice));
    assert!(snapshot.nodes.iter().all(|node| !node.focused));
    ui.set_picker_closed(hwnd, Some(ElementId::InputDevice), HWND::default());
    let snapshot = ui.automation.as_ref().expect("automation").snapshot();
    assert_eq!(snapshot.focus_owner, AutomationFocusOwner::Outside);
    assert_eq!(snapshot.picker_open_for, None);
    assert_eq!(snapshot.focused, Some(ElementId::InputDevice));
}

#[test]
fn parent_scroll_closes_picker_before_scrolling() {
    let popup = HWND(7usize as *mut _);
    assert!(matches!(
        settings_wheel_action(Some(popup), 128.0, 120.0, 512.0),
        SettingsWheelAction::ClosePicker(hwnd) if hwnd == popup
    ));
    assert!(matches!(
        settings_wheel_action(None, 128.0, 120.0, 512.0),
        SettingsWheelAction::Scroll(scroll) if (scroll - 48.0).abs() < f32::EPSILON
    ));
}

#[test]
fn close_request_blocks_pending_picker_activation() {
    let mut ui = empty_settings_ui();
    assert!(ui.picker_activation_allowed());
    assert!(ui.begin_close());
    assert!(!ui.picker_activation_allowed());
    assert!(!ui.begin_close());
}

#[test]
fn settings_hide_cancels_picker_before_hiding_parent() {
    use std::cell::RefCell;

    let steps = RefCell::new(Vec::new());
    close_picker_before_settings_hide(
        || steps.borrow_mut().push("picker"),
        || steps.borrow_mut().push("discard"),
        || steps.borrow_mut().push("settings"),
    );
    assert_eq!(&*steps.borrow(), &["picker", "discard", "settings"]);
}
