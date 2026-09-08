use super::*;

#[test]
fn published_provider_exposes_toggle_pattern_and_state() {
    let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
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
    let control_type = unsafe {
        provider
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("control type")
    };
    assert_eq!(
        i32::try_from(&control_type).expect("I4"),
        UIA_CheckBoxControlTypeId.0
    );
    let unknown = unsafe {
        provider
            .GetPatternProvider(UIA_TogglePatternId)
            .expect("toggle pattern")
    };
    let toggle: IToggleProvider = unknown.cast().expect("toggle interface");
    assert_eq!(
        unsafe { toggle.ToggleState().expect("toggle state") },
        ToggleState_Off
    );
}

#[test]
fn published_provider_exposes_slider_range_pattern() {
    let mut input = values();
    if let Some((_, value, _, ratio)) = input
        .iter_mut()
        .find(|(id, _, _, _)| *id == ElementId::OverlayDuration)
    {
        *value = "2.0s".into();
        *ratio = 0.5;
    }
    let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
    automation.publish(test_snapshot_from_settings(
        HWND(std::ptr::null_mut()),
        &layout(),
        &input,
        None,
        96,
    ));
    let provider = automation
        .provider_for(ElementId::OverlayDuration)
        .expect("test provider initialization");
    let unknown = unsafe {
        provider
            .GetPatternProvider(UIA_RangeValuePatternId)
            .expect("range pattern")
    };
    let range: IRangeValueProvider = unknown.cast().expect("range interface");
    assert_eq!(unsafe { range.Minimum().expect("minimum") }, 500.0);
    assert_eq!(unsafe { range.Maximum().expect("maximum") }, 10_000.0);
    assert_eq!(unsafe { range.Value().expect("value") }, 5_250.0);
}

#[test]
fn unsupported_patterns_return_s_ok_and_null() {
    let automation = published_automation();
    let root = automation
        .root_provider()
        .expect("test provider initialization");
    assert_raw_null_pattern(&root, UIA_InvokePatternId);

    let slider = automation
        .provider_for(ElementId::OverlayDuration)
        .expect("test provider initialization");
    assert_raw_null_pattern(&slider, UIA_TogglePatternId);
    assert_raw_null_pattern(&slider, UIA_PATTERN_ID(99_999));
}

#[test]
fn read_only_value_pattern_rejects_set_value() {
    let automation = published_automation();
    let provider = automation
        .provider_for(ElementId::InputDevice)
        .expect("test provider initialization");
    let unknown = unsafe {
        provider
            .GetPatternProvider(UIA_ValuePatternId)
            .expect("value pattern")
    };
    let value: IValueProvider = unknown.cast().expect("value interface");
    assert!(unsafe { value.IsReadOnly().expect("read-only state") }.as_bool());

    let text = HSTRING::from("must not be accepted");
    let text = PCWSTR(text.as_ptr());
    let error = unsafe { value.SetValue(text) }.expect_err("read-only SetValue");
    assert_eq!(
        error.code(),
        windows::core::HRESULT(UIA_E_INVALIDOPERATION as i32)
    );
}

#[test]
fn value_pattern_properties_use_bstr_and_bool_variants() {
    let automation = published_automation();
    let provider = automation
        .provider_for(ElementId::InputDevice)
        .expect("test provider initialization");
    let mut value = assert_raw_variant_type(&provider, UIA_ValueValuePropertyId, VT_BSTR);
    unsafe {
        VariantClear(&mut value).expect("clear BSTR value");
    }
    let mut read_only = assert_raw_variant_type(&provider, UIA_ValueIsReadOnlyPropertyId, VT_BOOL);
    assert!(bool::try_from(&read_only).expect("BOOL variant"));
    unsafe {
        VariantClear(&mut read_only).expect("clear BOOL value");
    }
}

#[test]
fn directly_called_unsupported_patterns_return_uia_not_supported() {
    let automation = published_automation();

    let slider = automation
        .provider_for(ElementId::OverlayDuration)
        .expect("test provider initialization");
    let invoke: IInvokeProvider = slider.cast().expect("Invoke interface");
    assert_hresult(
        unsafe { invoke.Invoke() }.expect_err("unsupported Invoke"),
        UIA_E_NOTSUPPORTED,
    );

    let button = automation
        .provider_for(ElementId::OpenConfigFolder)
        .expect("test provider initialization");
    let toggle: IToggleProvider = button.cast().expect("Toggle interface");
    assert_hresult(
        unsafe { toggle.Toggle() }.expect_err("unsupported Toggle"),
        UIA_E_NOTSUPPORTED,
    );
    let range: IRangeValueProvider = button.cast().expect("RangeValue interface");
    assert_hresult(
        unsafe { range.SetValue(1.0) }.expect_err("unsupported RangeValue"),
        UIA_E_NOTSUPPORTED,
    );
}
