use super::*;

#[test]
fn property_notifications_preserve_first_old_and_latest_new_values() {
    let automation = published_automation();
    let mut first = automation.snapshot();
    first
        .nodes
        .iter_mut()
        .find(|node| node.id == ElementId::OverlayDuration)
        .expect("duration slider")
        .range
        .as_mut()
        .expect("duration range")
        .value = 700.0;
    automation.publish(first);

    let mut latest = automation.snapshot();
    latest
        .nodes
        .iter_mut()
        .find(|node| node.id == ElementId::OverlayDuration)
        .expect("duration slider")
        .range
        .as_mut()
        .expect("duration range")
        .value = 800.0;
    automation.publish(latest);

    let mut delivered = Vec::new();
    automation.flush_pending_events_with(|_, notification| delivered.push(notification.clone()));
    assert_eq!(delivered.len(), 1);
    assert_eq!(
        delivered[0].kind,
        AutomationNotificationKind::Property(UIA_RangeValueValuePropertyId.0)
    );
    match &delivered[0].old_value {
        AutomationValue::F64(value) => assert!((*value - 5250.0).abs() < 1e-12),
        value => panic!("unexpected old value: {value:?}"),
    }
    match &delivered[0].new_value {
        AutomationValue::F64(value) => assert!((*value - 800.0).abs() < 1e-12),
        value => panic!("unexpected new value: {value:?}"),
    }
}
