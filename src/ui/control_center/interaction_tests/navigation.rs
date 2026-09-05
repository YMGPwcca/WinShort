use super::*;

#[test]
fn wheel_scroll_policy_accumulates_native_sized_deltas() {
    assert_eq!(scroll_after_wheel(128.0, 120.0, 512.0), 48.0);
    assert_eq!(scroll_after_wheel(48.0, 120.0, 512.0), 0.0);
    assert_eq!(scroll_after_wheel(128.0, 240.0, 512.0), 0.0);
    assert_eq!(scroll_after_wheel(0.0, 120.0, 512.0), 0.0);
    assert_eq!(scroll_after_wheel(512.0, -120.0, 512.0), 512.0);
}

#[test]
fn wheel_and_page_scroll_are_immediate_and_accumulate_without_tween() {
    let mut scroll = 128.0;
    scroll = scroll_after_wheel(scroll, 120.0, 512.0);
    assert_eq!(scroll, 48.0);
    scroll = scroll_after_wheel(scroll, 240.0, 512.0);
    assert_eq!(scroll, 0.0);
    assert_eq!(page_scroll_target(128.0, 300.0, 512.0, true), 428.0);
    assert_eq!(page_scroll_target(128.0, 300.0, 512.0, false), 0.0);
    assert!(!Motion::default().has_active());
}
