use super::*;

#[test]
fn nullable_provider_abi_regression() {
    let automation = published_automation_for(unsafe { GetDesktopWindow() });
    let root_simple = automation
        .root_provider()
        .expect("test provider initialization");
    let root_fragment: IRawElementProviderFragment = root_simple.cast().expect("root fragment");
    let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment root");

    // These assertions call the raw ABI directly. The output starts as a
    // dangling sentinel and must be cleared to NULL without constructing
    // or releasing a Rust Interface value.
    assert_raw_null_pattern(&root_simple, UIA_InvokePatternId);
    assert_raw_null_navigation(&root_fragment, NavigateDirection_Parent);
    let child = automation
        .provider_for(ElementId::OverlayEnabled)
        .expect("test provider initialization");
    assert_raw_null_host(&child);
    assert_raw_null_point(&root, -1.0, -1.0);
    assert_raw_null_focus(&root);

    let first = unsafe {
        root_fragment
            .Navigate(NavigateDirection_FirstChild)
            .expect("supported Navigate")
    };
    assert!(!first.as_raw().is_null());
    let host = unsafe {
        root_simple
            .HostRawElementProvider()
            .expect("supported host provider")
    };
    assert!(!host.as_raw().is_null());
    let supported = unsafe {
        child
            .GetPatternProvider(UIA_TogglePatternId)
            .expect("supported Toggle pattern")
    };
    assert!(!supported.as_raw().is_null());
}
