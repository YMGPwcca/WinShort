use super::*;

#[test]
fn review_summary_is_exposed_as_non_focusable_text() {
    let layout = SettingsLayout::build_shell_with_context(
        1000.0,
        800.0,
        0.0,
        crate::ui::navigation::Page::Displays,
        "",
        crate::ui::layout::LayoutContext {
            display_editor_step: Some(crate::ui::presentation::DisplayWizardStep::Review),
            display_route_count: 2,
            ..Default::default()
        },
        None,
    );
    let values = layout
        .elements
        .iter()
        .filter(|element| element.kind != ElementKind::Card)
        .map(|element| (element.id, "Off".into(), true, 0.0))
        .collect::<Vec<_>>();
    let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
    automation.publish(test_snapshot_from_settings(
        HWND(std::ptr::null_mut()),
        &layout,
        &values,
        None,
        96,
    ));
    let node = automation
        .snapshot()
        .nodes
        .into_iter()
        .find(|node| node.id == ElementId::DisplayWizardSummary)
        .expect("review summary node");
    assert_eq!(node.kind, ElementKind::Info);
    let provider = automation
        .provider_for(ElementId::DisplayWizardSummary)
        .expect("test provider initialization");
    let control_type = unsafe {
        provider
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("control type")
    };
    assert_eq!(
        i32::try_from(&control_type).expect("I4"),
        UIA_TextControlTypeId.0
    );
    let keyboard_focusable = unsafe {
        provider
            .GetPropertyValue(UIA_IsKeyboardFocusablePropertyId)
            .expect("focusability")
    };
    assert!(!bool::try_from(&keyboard_focusable).expect("BOOL variant"));
}

#[test]
fn root_focus_returns_logical_child_fragment() {
    let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
    let mut snapshot = test_snapshot_from_settings(
        HWND(std::ptr::null_mut()),
        &layout(),
        &values(),
        Some(ElementId::OverlayEnabled),
        96,
    );
    snapshot.focused = Some(ElementId::OverlayEnabled);
    automation.publish(snapshot);
    let root_simple = automation
        .root_provider()
        .expect("test provider initialization");
    let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment");
    let focused = unsafe { root.GetFocus().expect("logical focus") };
    let focused_simple: IRawElementProviderSimple =
        focused.cast().expect("focused simple provider");
    let has_focus = unsafe {
        focused_simple
            .GetPropertyValue(UIA_HasKeyboardFocusPropertyId)
            .expect("focus property")
    };
    assert!(bool::try_from(&has_focus).expect("BOOL variant"));
}

#[test]
fn focus_policy_distinguishes_settings_picker_and_outside() {
    let automation = published_automation();
    let root_simple = automation
        .root_provider()
        .expect("test provider initialization");
    let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment root");
    let mut snapshot = automation.snapshot();
    snapshot.focused = Some(ElementId::OverlayEnabled);
    snapshot.set_focus_state(AutomationFocusOwner::Settings, None);
    automation.publish(snapshot.clone());
    let focused = unsafe { root.GetFocus().expect("settings focus") };
    let focused_simple: IRawElementProviderSimple = focused.cast().expect("focused child");
    let focused_value = unsafe {
        focused_simple
            .GetPropertyValue(UIA_HasKeyboardFocusPropertyId)
            .expect("focus property")
    };
    assert!(bool::try_from(&focused_value).expect("BOOL variant"));

    snapshot.set_focus_state(
        AutomationFocusOwner::Picker,
        Some(ElementId::OverlayEnabled),
    );
    automation.publish(snapshot.clone());
    assert_raw_null_focus(&root);
    let picker_value = unsafe {
        automation
            .provider_for(ElementId::OverlayEnabled)
            .expect("test provider initialization")
            .GetPropertyValue(UIA_HasKeyboardFocusPropertyId)
            .expect("picker focus property")
    };
    assert!(!bool::try_from(&picker_value).expect("BOOL variant"));

    snapshot.set_focus_state(AutomationFocusOwner::Outside, None);
    automation.publish(snapshot);
    assert_raw_null_focus(&root);
}

#[test]
fn ui_automation_set_focus_is_queued_for_window_dispatch() {
    let automation = published_automation_for(unsafe { GetDesktopWindow() });
    let provider = automation
        .provider_for(ElementId::OverlayEnabled)
        .expect("test provider initialization");
    let fragment: IRawElementProviderFragment = provider.cast().expect("fragment provider");
    unsafe { fragment.SetFocus().expect("queue focus") };
    assert!(matches!(
        automation.drain_actions().as_slice(),
        [SettingsAutomationAction::SetFocus(
            ElementId::OverlayEnabled
        )]
    ));
}

#[test]
fn disabled_set_focus_returns_element_not_enabled() {
    let automation = automation_with_disabled(ElementId::OverlayEnabled);
    let provider = automation
        .provider_for(ElementId::OverlayEnabled)
        .expect("test provider initialization");
    let fragment: IRawElementProviderFragment = provider.cast().expect("fragment interface");
    assert_hresult(
        unsafe { fragment.SetFocus() }.expect_err("disabled SetFocus"),
        UIA_E_ELEMENTNOTENABLED,
    );
}

#[test]
fn valid_set_focus_queues_exactly_one_action() {
    let automation = SettingsAutomation::new(unsafe { GetDesktopWindow() });
    automation.publish(test_snapshot_from_settings(
        HWND(std::ptr::null_mut()),
        &layout(),
        &values(),
        None,
        96,
    ));
    let provider = automation
        .provider_for(ElementId::OverlayEnabled)
        .expect("test provider initialization");
    let fragment: IRawElementProviderFragment = provider.cast().expect("fragment interface");
    unsafe { fragment.SetFocus().expect("valid SetFocus") };
    assert!(matches!(
        automation.drain_actions().as_slice(),
        [SettingsAutomationAction::SetFocus(
            ElementId::OverlayEnabled
        )]
    ));
}
