use super::model::SettingsAutomationSnapshot;
use super::*;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::Ole::{
    SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
};
use windows::Win32::System::Variant::{VT_BOOL, VT_BSTR, VT_EMPTY};
use windows::Win32::UI::Accessibility::UIA_PATTERN_ID;
use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

fn assert_raw_null_pattern(provider: &IRawElementProviderSimple, pattern: UIA_PATTERN_ID) {
    let mut returned = std::ptr::dangling_mut();
    let hr = unsafe {
        (provider.vtable().GetPatternProvider)(provider.as_raw(), pattern, &mut returned)
    };
    assert_eq!(hr, windows::core::HRESULT(0));
    assert!(returned.is_null());
}

fn assert_raw_null_navigation(
    provider: &IRawElementProviderFragment,
    direction: NavigateDirection,
) {
    let mut returned = std::ptr::dangling_mut();
    let hr = unsafe { (provider.vtable().Navigate)(provider.as_raw(), direction, &mut returned) };
    assert_eq!(hr, windows::core::HRESULT(0));
    assert!(returned.is_null());
}

fn assert_raw_null_host(provider: &IRawElementProviderSimple) {
    let mut returned = std::ptr::dangling_mut();
    let hr =
        unsafe { (provider.vtable().HostRawElementProvider)(provider.as_raw(), &mut returned) };
    assert_eq!(hr, windows::core::HRESULT(0));
    assert!(returned.is_null());
}

fn assert_raw_null_point(provider: &IRawElementProviderFragmentRoot, x: f64, y: f64) {
    let mut returned = std::ptr::dangling_mut();
    let hr = unsafe {
        (provider.vtable().ElementProviderFromPoint)(provider.as_raw(), x, y, &mut returned)
    };
    assert_eq!(hr, windows::core::HRESULT(0));
    assert!(returned.is_null());
}

fn assert_raw_null_focus(provider: &IRawElementProviderFragmentRoot) {
    let mut returned = std::ptr::dangling_mut();
    let hr = unsafe { (provider.vtable().GetFocus)(provider.as_raw(), &mut returned) };
    assert_eq!(hr, windows::core::HRESULT(0));
    assert!(returned.is_null());
}

fn read_runtime_id(fragment: &IRawElementProviderFragment) -> Vec<i32> {
    let array = unsafe { fragment.GetRuntimeId().expect("runtime id") };
    assert!(!array.is_null());
    assert_eq!(unsafe { SafeArrayGetDim(array) }, 1);
    let lower = unsafe { SafeArrayGetLBound(array, 1).expect("runtime lower bound") };
    let upper = unsafe { SafeArrayGetUBound(array, 1).expect("runtime upper bound") };
    let mut values = Vec::new();
    for index in lower..=upper {
        let mut value = 0i32;
        unsafe {
            SafeArrayGetElement(array, &index, (&mut value as *mut i32).cast())
                .expect("runtime element");
        }
        values.push(value);
    }
    unsafe { SafeArrayDestroy(array).expect("runtime array destroy") };
    values
}
fn raw_property(
    provider: &IRawElementProviderSimple,
    property: UIA_PROPERTY_ID,
) -> (windows::core::HRESULT, VARIANT) {
    let mut value = VARIANT::default();
    let hr =
        unsafe { (provider.vtable().GetPropertyValue)(provider.as_raw(), property, &mut value) };
    (hr, value)
}

fn assert_raw_empty_property(provider: &IRawElementProviderSimple, property: UIA_PROPERTY_ID) {
    let (hr, value) = raw_property(provider, property);
    assert_eq!(hr, windows::core::HRESULT(0));
    assert_eq!(unsafe { value.Anonymous.Anonymous.vt }, VT_EMPTY);
}

fn assert_raw_variant_type(
    provider: &IRawElementProviderSimple,
    property: UIA_PROPERTY_ID,
    expected: windows::Win32::System::Variant::VARENUM,
) -> VARIANT {
    let (hr, value) = raw_property(provider, property);
    assert_eq!(hr, windows::core::HRESULT(0));
    assert_eq!(unsafe { value.Anonymous.Anonymous.vt }, expected);
    value
}

fn iunknown_identity<T: Interface>(interface: &T) -> *mut core::ffi::c_void {
    let unknown: IUnknown = interface.cast().expect("IUnknown");
    unknown.as_raw()
}

fn layout() -> SettingsLayout {
    SettingsLayout::build(610.0, 720.0, 64.0)
}

fn values() -> Vec<(ElementId, String, bool, f32)> {
    layout()
        .focus_order()
        .into_iter()
        .map(|id| (id, String::from("Off"), true, 0.5))
        .collect()
}

fn focus_order() -> Vec<ElementId> {
    layout().focus_order()
}

fn test_snapshot_from_settings(
    _hwnd: HWND,
    layout: &SettingsLayout,
    values: &[(ElementId, String, bool, f32)],
    focused: Option<ElementId>,
    dpi: u32,
) -> SettingsAutomationSnapshot {
    super::snapshot::snapshot_from_settings_at_origin(
        layout,
        values,
        focused,
        dpi,
        POINT::default(),
    )
}

fn automation_with_disabled(id: ElementId) -> SettingsAutomation {
    let automation = published_automation();
    let mut snapshot = automation.snapshot();
    snapshot
        .nodes
        .iter_mut()
        .find(|node| node.id == id)
        .expect("node")
        .enabled = false;
    automation.publish(snapshot);
    automation
}

fn stale_provider(id: ElementId) -> IRawElementProviderSimple {
    let automation = published_automation();
    let provider = automation
        .provider_for(id)
        .expect("test provider initialization");
    drop(automation);
    provider
}

fn assert_hresult(error: windows::core::Error, expected: u32) {
    assert_eq!(
        error.code(),
        windows::core::HRESULT(expected as i32),
        "unexpected HRESULT"
    );
}

fn published_automation() -> SettingsAutomation {
    published_automation_for(HWND(std::ptr::null_mut()))
}

fn published_automation_for(hwnd: HWND) -> SettingsAutomation {
    let automation = SettingsAutomation::new(hwnd);
    automation.publish(test_snapshot_from_settings(
        hwnd,
        &layout(),
        &values(),
        None,
        96,
    ));
    automation
}

mod abi_values;
mod events;
mod focus;
mod nullable_abi;
mod patterns;
mod session;
