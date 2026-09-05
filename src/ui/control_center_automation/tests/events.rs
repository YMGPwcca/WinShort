use super::*;

#[test]
fn provider_events_are_filtered_to_meaningful_changes() {
    let snapshot =
        snapshot_from_settings(HWND(std::ptr::null_mut()), &layout(), &values(), None, 96);
    let toggle = snapshot
        .nodes
        .iter()
        .find(|node| node.id == ElementId::OverlayEnabled)
        .expect("toggle node")
        .clone();
    assert!(changed_property_ids(&toggle, &toggle).is_empty());

    let mut changed = toggle.clone();
    changed.enabled = false;
    let changed_properties = changed_property_ids(&toggle, &changed);
    assert!(changed_properties.contains(&UIA_IsEnabledPropertyId));
    assert!(changed_properties.contains(&UIA_IsKeyboardFocusablePropertyId));

    changed = toggle.clone();
    changed.focused = true;
    assert_eq!(
        changed_property_ids(&toggle, &changed),
        vec![UIA_HasKeyboardFocusPropertyId]
    );

    changed = toggle.clone();
    changed.toggle = Some(true);
    assert_eq!(
        changed_property_ids(&toggle, &changed),
        vec![UIA_ToggleToggleStatePropertyId]
    );

    let slider = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == ElementKind::Slider)
        .expect("slider node")
        .clone();
    changed = slider.clone();
    changed.range.as_mut().expect("slider range").value += 1.0;
    assert_eq!(
        changed_property_ids(&slider, &changed),
        vec![UIA_RangeValueValuePropertyId]
    );

    changed = slider.clone();
    changed.offscreen = !changed.offscreen;
    changed.bounds.left += 1.0;
    let changed_properties = changed_property_ids(&slider, &changed);
    assert!(changed_properties.contains(&UIA_IsOffscreenPropertyId));
    assert!(changed_properties.contains(&UIA_BoundingRectanglePropertyId));
}

#[test]
fn snapshot_commit_defers_requerying_event_delivery_past_refcell_borrow() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let automation = published_automation();
    let mut changed_snapshot = automation.snapshot();
    changed_snapshot
        .nodes
        .iter_mut()
        .find(|node| node.id == ElementId::OverlayEnabled)
        .expect("toggle node")
        .toggle = Some(true);

    let settings_borrow = RefCell::new(());
    let borrowed = settings_borrow.borrow_mut();
    let delivered = Rc::new(RefCell::new(Vec::new()));
    automation.publish(changed_snapshot);
    assert!(delivered.borrow().is_empty());
    drop(borrowed);

    let delivered_for_sink = Rc::clone(&delivered);
    automation.flush_pending_events_with(|automation, notification| {
        let provider = automation
            .provider_for(ElementId::OverlayEnabled)
            .expect("test provider initialization");
        let _ = unsafe {
            provider
                .GetPropertyValue(UIA_ToggleToggleStatePropertyId)
                .expect("re-query provider during delivery")
        };
        delivered_for_sink.borrow_mut().push(notification.clone());
    });
    let delivered = delivered.borrow();
    assert_eq!(delivered.len(), 1);
    assert!(matches!(
        delivered[0].kind,
        AutomationNotificationKind::Property(property)
            if property == UIA_ToggleToggleStatePropertyId.0
    ));
    assert_eq!(
        delivered[0].old_value,
        AutomationValue::I32(ToggleState_Off.0)
    );
    assert_eq!(
        delivered[0].new_value,
        AutomationValue::I32(ToggleState_On.0)
    );
}

#[test]
fn invoke_notifications_are_deferred_once_per_accepted_invocation() {
    let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
    automation.queue_invoked(ElementId::InputDevice);
    let mut delivered = Vec::new();
    automation.flush_pending_events_with(|_, notification| delivered.push(notification.clone()));
    assert_eq!(delivered.len(), 1);
    assert_eq!(
        delivered[0].target,
        AutomationTarget::Node(ElementId::InputDevice)
    );
    assert_eq!(delivered[0].kind, AutomationNotificationKind::Invoked);
}

#[test]
fn initial_snapshot_does_not_queue_synthetic_notifications() {
    let automation = published_automation();
    let mut delivered = Vec::new();
    automation.flush_pending_events_with(|_, notification| delivered.push(notification.clone()));
    assert!(delivered.is_empty());
}
