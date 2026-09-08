use super::*;

#[test]
fn snapshot_contains_logical_nodes_without_child_windows() {
    let snapshot = test_snapshot_from_settings(
        HWND(std::ptr::null_mut()),
        &layout(),
        &values(),
        Some(ElementId::OverlayEnabled),
        96,
    );
    assert_eq!(snapshot.nodes.len(), focus_order().len());
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .find(|node| node.id == ElementId::OverlayEnabled)
            .and_then(|node| node.toggle),
        Some(false)
    );
    assert!(snapshot
        .nodes
        .iter()
        .all(|node| node.bounds.width > 0.0 && node.bounds.height > 0.0));
}
#[test]
fn blur_control_retains_legacy_opacity_automation_id() {
    let automation = published_automation();
    let provider = automation
        .provider_for(ElementId::OverlayBlur)
        .expect("blur control provider");
    let automation_id = unsafe {
        provider
            .GetPropertyValue(UIA_AutomationIdPropertyId)
            .expect("automation id")
    };
    assert_eq!(
        automation_id.to_string(),
        "WinShort.ControlCenter.OverlayOpacity"
    );
}

#[test]
fn phase_one_hotkey_nodes_expose_names_and_help_text() {
    let snapshot =
        test_snapshot_from_settings(HWND(std::ptr::null_mut()), &layout(), &values(), None, 96);
    let expected = [
        (
            ElementId::CycleInputHotkey,
            "Next microphone",
            "Switch to the next selected microphone",
        ),
        (
            ElementId::CycleOutputHotkey,
            "Next speaker",
            "Switch to the next selected speaker",
        ),
        (
            ElementId::ForegroundVolumeUpHotkey,
            "Current app volume up",
            "Raise the current app by five percent",
        ),
        (
            ElementId::ForegroundVolumeDownHotkey,
            "Current app volume down",
            "Lower the current app by five percent",
        ),
    ];
    for (id, name, help_text) in expected {
        let node = snapshot
            .nodes
            .iter()
            .find(|node| node.id == id)
            .expect("node");
        assert_eq!(node.name, name);
        assert_eq!(node.help_text, help_text);
        assert!(node.enabled);
    }
}

#[test]
fn snapshot_clips_scrolled_nodes_and_marks_them_offscreen() {
    let snapshot =
        test_snapshot_from_settings(HWND(std::ptr::null_mut()), &layout(), &values(), None, 144);
    assert!(snapshot.nodes.iter().any(|node| node.offscreen));
    assert!(snapshot.nodes.iter().any(|node| !node.offscreen));
}

#[test]
fn control_types_and_ranges_are_truthful() {
    assert_eq!(
        control_type(ElementKind::Toggle),
        UIA_CheckBoxControlTypeId.0
    );
    assert_eq!(control_type(ElementKind::Slider), UIA_SliderControlTypeId.0);
    assert!(node_has_invoke(ElementKind::Value));
    assert!(!node_has_invoke(ElementKind::Toggle));
    assert!(!node_has_invoke(ElementKind::Slider));
    let ranges = values();
    let snapshot =
        test_snapshot_from_settings(HWND(std::ptr::null_mut()), &layout(), &ranges, None, 96);
    assert!(snapshot
        .nodes
        .iter()
        .filter(|node| node.kind == ElementKind::Slider)
        .all(|node| node.range.is_some()));
}

#[test]
fn pointer_snapshot_uses_screen_space_origin_when_available() {
    let snapshot =
        test_snapshot_from_settings(HWND(std::ptr::null_mut()), &layout(), &values(), None, 96);
    assert_eq!(snapshot.window.left, 0.0);
    assert_eq!(snapshot.window.top, 0.0);
}

#[test]
fn automation_rects_are_screen_space_and_dpi_scaled() {
    let rect = AutomationRect::from_ui(
        UiRect::new(10.0, 20.0, 30.0, 40.0),
        1.5,
        POINT { x: 100, y: -20 },
    );
    assert_eq!(rect.left, 115.0);
    assert_eq!(rect.top, 10.0);
    assert_eq!(rect.width, 45.0);
    assert_eq!(rect.height, 60.0);
}

#[test]
fn slider_ranges_expose_model_units() {
    let range =
        slider_range(ElementId::OverlayDuration, ElementKind::Slider, 0.5).expect("duration range");
    assert_eq!(range.minimum, 500.0);
    assert_eq!(range.maximum, 10_000.0);
    assert!((range.value - 5250.0).abs() < f64::EPSILON);
    assert_eq!(range.small_change, 100.0);
    assert_eq!(range.large_change, 500.0);
    let blur = slider_range(ElementId::OverlayBlur, ElementKind::Slider, 0.5).expect("blur range");
    assert_eq!(blur.minimum, 0.0);
    assert_eq!(blur.maximum, 4.0);
    assert_eq!(blur.value, 2.0);
    assert_eq!(blur.small_change, 1.0);
    assert_eq!(blur.large_change, 2.0);
}

#[test]
fn automation_actions_are_queued_for_later_window_dispatch() {
    let automation = SettingsAutomation::new(unsafe { GetDesktopWindow() });
    automation
        .enqueue(SettingsAutomationAction::Toggle(ElementId::OverlayEnabled))
        .expect("post action");
    let actions = automation.drain_actions();
    assert!(matches!(
        actions.as_slice(),
        [SettingsAutomationAction::Toggle(ElementId::OverlayEnabled)]
    ));
}

#[test]
fn published_provider_exposes_selection_item_for_choice() {
    let context = crate::ui::layout::LayoutContext {
        input_device_count: 1,
        output_device_count: 1,
        ..Default::default()
    };
    let layout = SettingsLayout::build_shell_with_context(
        1000.0,
        800.0,
        0.0,
        crate::ui::navigation::Page::Audio,
        "",
        context,
        None,
    );
    let values = layout
        .focus_order()
        .into_iter()
        .map(|id| {
            let value = if id == ElementId::OutputCycleMode(0) {
                "On"
            } else {
                "Off"
            }
            .into();
            (id, value, true, 0.0)
        })
        .collect::<Vec<_>>();
    let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
    automation.publish(test_snapshot_from_settings(
        HWND(std::ptr::null_mut()),
        &layout,
        &values,
        None,
        96,
    ));
    let provider = automation
        .provider_for(ElementId::OutputCycleMode(0))
        .expect("test provider initialization");
    let control_type = unsafe {
        provider
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("control type")
    };
    assert_eq!(
        i32::try_from(&control_type).expect("I4"),
        UIA_RadioButtonControlTypeId.0
    );
    assert_raw_empty_property(&provider, UIA_ToggleToggleStatePropertyId);
    let selection_unknown = unsafe {
        provider
            .GetPatternProvider(UIA_SelectionItemPatternId)
            .expect("selection item pattern")
    };
    let selection: ISelectionItemProvider =
        selection_unknown.cast().expect("selection item interface");
    assert!(unsafe { selection.IsSelected().expect("selection state") }.as_bool());
    let (hr, toggle_available) = raw_property(&provider, UIA_IsTogglePatternAvailablePropertyId);
    assert_eq!(hr, windows::core::HRESULT(0));
    assert!(!bool::try_from(&toggle_available).expect("toggle availability"));
}

#[test]
fn navigation_boundaries_return_s_ok_and_null() {
    let automation = published_automation();
    let root_simple = automation
        .root_provider()
        .expect("test provider initialization");
    let root: IRawElementProviderFragment = root_simple.cast().expect("root fragment");
    assert_raw_null_navigation(&root, NavigateDirection_Parent);

    let first_simple = automation
        .provider_for(focus_order()[0])
        .expect("test provider initialization");
    let first: IRawElementProviderFragment = first_simple.cast().expect("first fragment");
    assert_raw_null_navigation(&first, NavigateDirection_PreviousSibling);

    let last_simple = automation
        .provider_for(*focus_order().last().expect("last id"))
        .expect("test provider initialization");
    let last: IRawElementProviderFragment = last_simple.cast().expect("last fragment");
    assert_raw_null_navigation(&last, NavigateDirection_NextSibling);
}

#[test]
fn runtime_ids_use_uia_append_runtime_id_and_unique_child_values() {
    let automation = published_automation();
    let root_simple = automation
        .root_provider()
        .expect("test provider initialization");
    let root: IRawElementProviderFragment = root_simple.cast().expect("root fragment");
    assert!(unsafe { root.GetRuntimeId().expect("root runtime id") }.is_null());

    let first_simple = automation
        .provider_for(focus_order()[0])
        .expect("test provider initialization");
    let first: IRawElementProviderFragment = first_simple.cast().expect("first fragment");
    let second_simple = automation
        .provider_for(focus_order()[1])
        .expect("test provider initialization");
    let second: IRawElementProviderFragment = second_simple.cast().expect("second fragment");
    let first_id = read_runtime_id(&first);
    let second_id = read_runtime_id(&second);
    assert_eq!(first_id, vec![UiaAppendRuntimeId as i32, 1]);
    assert_eq!(second_id, vec![UiaAppendRuntimeId as i32, 2]);
    assert_ne!(first_id, second_id);
}

#[test]
fn host_provider_is_only_returned_for_fragment_root() {
    let automation = published_automation_for(unsafe { GetDesktopWindow() });
    let root = automation
        .root_provider()
        .expect("test provider initialization");
    let host = unsafe { root.HostRawElementProvider() }.expect("root host provider");
    assert!(!host.as_raw().is_null());

    let child = automation
        .provider_for(ElementId::OverlayEnabled)
        .expect("test provider initialization");
    assert_raw_null_host(&child);
}

#[test]
fn picker_and_hotkey_nodes_are_button_actions() {
    let automation = published_automation();
    for id in [ElementId::InputDevice, ElementId::MicHotkey] {
        let provider = automation
            .provider_for(id)
            .expect("test provider initialization");
        let control_type = unsafe {
            provider
                .GetPropertyValue(UIA_ControlTypePropertyId)
                .expect("control type")
        };
        assert_eq!(
            i32::try_from(&control_type).expect("I4"),
            UIA_ButtonControlTypeId.0
        );
        let invoke = unsafe {
            provider
                .GetPatternProvider(UIA_InvokePatternId)
                .expect("invoke pattern")
        };
        let _: IInvokeProvider = invoke.cast().expect("invoke interface");
        assert_raw_null_pattern(
            &provider,
            windows::Win32::UI::Accessibility::UIA_ExpandCollapsePatternId,
        );
    }
}

#[test]
fn point_queries_return_child_root_or_null() {
    let automation = published_automation();
    let root_simple = automation
        .root_provider()
        .expect("test provider initialization");
    let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment root");
    assert_raw_null_point(&root, -1.0, -1.0);

    let background = unsafe {
        root.ElementProviderFromPoint(1.0, 1.0)
            .expect("background fragment")
    };
    let _: IRawElementProviderFragmentRoot =
        background.cast().expect("background is root fragment");

    let node = automation
        .snapshot()
        .nodes
        .into_iter()
        .find(|node| !node.offscreen)
        .expect("visible node");
    let child = unsafe {
        root.ElementProviderFromPoint(
            node.bounds.left + node.bounds.width / 2.0,
            node.bounds.top + node.bounds.height / 2.0,
        )
        .expect("child fragment")
    };
    let child: IRawElementProviderSimple = child.cast().expect("child simple");
    assert_eq!(
        unsafe {
            child
                .GetPropertyValue(UIA_AutomationIdPropertyId)
                .expect("child automation id")
        }
        .to_string(),
        "WinShort.ControlCenter.StartWithWindows"
    );
}

#[test]
fn unsupported_properties_return_s_ok_and_vt_empty() {
    let automation = published_automation();
    let root = automation
        .root_provider()
        .expect("test provider initialization");
    let mut availability =
        assert_raw_variant_type(&root, UIA_IsInvokePatternAvailablePropertyId, VT_BOOL);
    assert!(!bool::try_from(&availability).expect("BOOL variant"));
    unsafe {
        VariantClear(&mut availability).expect("clear availability value");
    }
    let checkbox = automation
        .provider_for(ElementId::OverlayEnabled)
        .expect("test provider initialization");
    assert_raw_empty_property(&checkbox, UIA_RangeValueValuePropertyId);

    let slider = automation
        .provider_for(ElementId::OverlayDuration)
        .expect("test provider initialization");
    assert_raw_empty_property(&slider, UIA_ToggleToggleStatePropertyId);

    let button = automation
        .provider_for(ElementId::OpenConfigFolder)
        .expect("test provider initialization");
    assert_raw_empty_property(&button, UIA_ToggleToggleStatePropertyId);
    assert_raw_empty_property(&button, UIA_RangeValueValuePropertyId);
    assert_raw_empty_property(&button, UIA_ValueValuePropertyId);
    assert_raw_empty_property(&button, UIA_ValueIsReadOnlyPropertyId);
    assert_raw_empty_property(&button, UIA_PROPERTY_ID(99_998));
}

#[test]
fn root_and_child_provider_com_identities_are_distinct_and_stable() {
    let automation = published_automation();
    let root_simple = automation
        .root_provider()
        .expect("test provider initialization");
    let root_fragment: IRawElementProviderFragment = root_simple.cast().expect("root fragment");
    let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment root");
    let queried_root: IRawElementProviderFragmentRoot =
        root_simple.cast().expect("root QueryInterface");
    assert_eq!(iunknown_identity(&root), iunknown_identity(&queried_root));
    let returned_root = unsafe { root_fragment.FragmentRoot().expect("root FragmentRoot") };
    assert_eq!(iunknown_identity(&root), iunknown_identity(&returned_root));

    let child_simple = automation
        .provider_for(ElementId::OverlayEnabled)
        .expect("test provider initialization");
    let error = child_simple
        .cast::<IRawElementProviderFragmentRoot>()
        .expect_err("child must not expose FragmentRoot");
    assert_eq!(error.code(), windows::core::HRESULT(0x80004002u32 as i32));
    let child: IRawElementProviderFragment = child_simple.cast().expect("child fragment");
    let returned_root = unsafe { child.FragmentRoot().expect("child FragmentRoot") };
    assert_eq!(iunknown_identity(&root), iunknown_identity(&returned_root));

    let first = unsafe {
        root_fragment
            .Navigate(NavigateDirection_FirstChild)
            .expect("first child")
    };
    let next = unsafe {
        first
            .Navigate(NavigateDirection_NextSibling)
            .expect("next sibling")
    };
    let previous = unsafe {
        next.Navigate(NavigateDirection_PreviousSibling)
            .expect("previous sibling")
    };
    assert_eq!(read_runtime_id(&first), read_runtime_id(&previous));
}

#[test]
fn disabled_invoke_returns_element_not_enabled() {
    let automation = automation_with_disabled(ElementId::OpenConfigFolder);
    let provider = automation
        .provider_for(ElementId::OpenConfigFolder)
        .expect("test provider initialization");
    let unknown = unsafe {
        provider
            .GetPatternProvider(UIA_InvokePatternId)
            .expect("Invoke pattern")
    };
    let invoke: IInvokeProvider = unknown.cast().expect("Invoke interface");
    assert_hresult(
        unsafe { invoke.Invoke() }.expect_err("disabled Invoke"),
        UIA_E_ELEMENTNOTENABLED,
    );
}

#[test]
fn disabled_toggle_returns_element_not_enabled() {
    let automation = automation_with_disabled(ElementId::OverlayEnabled);
    let provider = automation
        .provider_for(ElementId::OverlayEnabled)
        .expect("test provider initialization");
    let unknown = unsafe {
        provider
            .GetPatternProvider(UIA_TogglePatternId)
            .expect("Toggle pattern")
    };
    let toggle: IToggleProvider = unknown.cast().expect("Toggle interface");
    assert_hresult(
        unsafe { toggle.Toggle() }.expect_err("disabled Toggle"),
        UIA_E_ELEMENTNOTENABLED,
    );
}

#[test]
fn disabled_range_value_returns_element_not_enabled() {
    let automation = automation_with_disabled(ElementId::OverlayDuration);
    let provider = automation
        .provider_for(ElementId::OverlayDuration)
        .expect("test provider initialization");
    let unknown = unsafe {
        provider
            .GetPatternProvider(UIA_RangeValuePatternId)
            .expect("RangeValue pattern")
    };
    let range: IRangeValueProvider = unknown.cast().expect("RangeValue interface");
    assert_hresult(
        unsafe { range.SetValue(0.7) }.expect_err("disabled RangeValue"),
        UIA_E_ELEMENTNOTENABLED,
    );
}

#[test]
fn invalid_range_value_returns_invalid_argument() {
    let automation = published_automation();
    let provider = automation
        .provider_for(ElementId::OverlayDuration)
        .expect("test provider initialization");
    let unknown = unsafe {
        provider
            .GetPatternProvider(UIA_RangeValuePatternId)
            .expect("RangeValue pattern")
    };
    let range: IRangeValueProvider = unknown.cast().expect("RangeValue interface");
    assert_hresult(
        unsafe { range.SetValue(1.1) }.expect_err("out-of-range RangeValue"),
        0x80070057,
    );
}

#[test]
fn stale_provider_operations_return_element_not_available() {
    let provider = stale_provider(ElementId::OpenConfigFolder);
    let property_error = unsafe {
        provider
            .GetPropertyValue(UIA_NamePropertyId)
            .expect_err("stale property")
    };
    assert_hresult(property_error, UIA_E_ELEMENTNOTAVAILABLE);
    let pattern_error = unsafe {
        provider
            .GetPatternProvider(UIA_InvokePatternId)
            .expect_err("stale pattern")
    };
    assert_hresult(pattern_error, UIA_E_ELEMENTNOTAVAILABLE);

    let invoke: IInvokeProvider = provider.cast().expect("stale Invoke interface");
    assert_hresult(
        unsafe { invoke.Invoke() }.expect_err("stale Invoke"),
        UIA_E_ELEMENTNOTAVAILABLE,
    );
    let fragment: IRawElementProviderFragment = provider.cast().expect("stale fragment interface");
    assert_hresult(
        unsafe { fragment.SetFocus() }.expect_err("stale SetFocus"),
        UIA_E_ELEMENTNOTAVAILABLE,
    );
    assert_hresult(
        unsafe { fragment.BoundingRectangle() }.expect_err("stale bounds"),
        UIA_E_ELEMENTNOTAVAILABLE,
    );
    assert_hresult(
        unsafe { fragment.GetEmbeddedFragmentRoots() }.expect_err("stale embedded roots"),
        UIA_E_ELEMENTNOTAVAILABLE,
    );
    assert_hresult(
        unsafe { fragment.FragmentRoot() }.expect_err("stale fragment root"),
        UIA_E_ELEMENTNOTAVAILABLE,
    );
    assert_hresult(
        unsafe { provider.ProviderOptions() }.expect_err("stale provider options"),
        UIA_E_ELEMENTNOTAVAILABLE,
    );

    let range_provider = stale_provider(ElementId::OverlayDuration);
    let range: IRangeValueProvider = range_provider.cast().expect("stale RangeValue");
    assert_hresult(
        unsafe { range.SetValue(0.7) }.expect_err("stale RangeValue"),
        UIA_E_ELEMENTNOTAVAILABLE,
    );
}
