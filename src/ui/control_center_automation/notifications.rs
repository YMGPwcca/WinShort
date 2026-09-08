//! Notifications for the control center automation.

#![allow(non_upper_case_globals)]
// Preserve Windows SDK names in ABI-facing constant patterns.

use super::capabilities::node_has_value;
use super::model::{
    AutomationFocusOwner, AutomationRect, SettingsAutomationNode, SettingsAutomationSnapshot,
};
use super::properties::optional_i32_value;
use crate::ui::layout::{ElementId, ElementKind};
use windows::Win32::UI::Accessibility::{
    ToggleState_Off, ToggleState_On, UIA_BoundingRectanglePropertyId,
    UIA_HasKeyboardFocusPropertyId, UIA_IsEnabledPropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_IsOffscreenPropertyId, UIA_NamePropertyId, UIA_RangeValueValuePropertyId,
    UIA_SelectionItemIsSelectedPropertyId, UIA_ToggleToggleStatePropertyId,
    UIA_ValueValuePropertyId, UIA_PROPERTY_ID,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutomationTarget {
    Root,
    Node(ElementId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutomationNotificationKind {
    FocusChanged,
    Invoked,
    Property(i32),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AutomationValue {
    Empty,
    Bool(bool),
    I32(i32),
    F64(f64),
    String(String),
    Rect(AutomationRect),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct AutomationNotification {
    pub(super) target: AutomationTarget,
    pub(super) kind: AutomationNotificationKind,
    pub(super) old_value: AutomationValue,
    pub(super) new_value: AutomationValue,
}

pub(crate) fn changed_property_ids(
    previous: &SettingsAutomationNode,
    current: &SettingsAutomationNode,
) -> Vec<UIA_PROPERTY_ID> {
    let mut changed = Vec::new();
    if previous.name != current.name {
        changed.push(UIA_NamePropertyId);
    }
    if previous.enabled != current.enabled {
        changed.push(UIA_IsEnabledPropertyId);
        changed.push(UIA_IsKeyboardFocusablePropertyId);
    }
    if previous.focused != current.focused {
        changed.push(UIA_HasKeyboardFocusPropertyId);
    }
    if previous.offscreen != current.offscreen {
        changed.push(UIA_IsOffscreenPropertyId);
    }
    if previous.bounds != current.bounds {
        changed.push(UIA_BoundingRectanglePropertyId);
    }
    if previous.toggle != current.toggle {
        if previous.kind == ElementKind::Choice || current.kind == ElementKind::Choice {
            changed.push(UIA_SelectionItemIsSelectedPropertyId);
        } else {
            changed.push(UIA_ToggleToggleStatePropertyId);
        }
    }
    if previous.range.map(|range| range.value) != current.range.map(|range| range.value) {
        changed.push(UIA_RangeValueValuePropertyId);
    }
    if previous.value != current.value
        && (node_has_value(previous.kind) || node_has_value(current.kind))
    {
        changed.push(UIA_ValueValuePropertyId);
    }
    changed
}

fn typed_property_values(
    property: UIA_PROPERTY_ID,
    previous: &SettingsAutomationNode,
    current: &SettingsAutomationNode,
) -> Option<(AutomationValue, AutomationValue)> {
    match property {
        UIA_NamePropertyId => Some((
            AutomationValue::String(previous.name.clone()),
            AutomationValue::String(current.name.clone()),
        )),
        UIA_IsEnabledPropertyId | UIA_IsKeyboardFocusablePropertyId => Some((
            AutomationValue::Bool(previous.enabled),
            AutomationValue::Bool(current.enabled),
        )),
        UIA_HasKeyboardFocusPropertyId => Some((
            AutomationValue::Bool(previous.focused),
            AutomationValue::Bool(current.focused),
        )),
        UIA_IsOffscreenPropertyId => Some((
            AutomationValue::Bool(previous.offscreen),
            AutomationValue::Bool(current.offscreen),
        )),
        UIA_SelectionItemIsSelectedPropertyId => Some((
            AutomationValue::Bool(previous.toggle.unwrap_or(false)),
            AutomationValue::Bool(current.toggle.unwrap_or(false)),
        )),
        UIA_ToggleToggleStatePropertyId => Some((
            optional_i32_value(
                (previous.kind != ElementKind::Choice)
                    .then_some(previous.toggle)
                    .flatten()
                    .map(|value| {
                        if value {
                            ToggleState_On.0
                        } else {
                            ToggleState_Off.0
                        }
                    }),
            ),
            optional_i32_value(
                (current.kind != ElementKind::Choice)
                    .then_some(current.toggle)
                    .flatten()
                    .map(|value| {
                        if value {
                            ToggleState_On.0
                        } else {
                            ToggleState_Off.0
                        }
                    }),
            ),
        )),
        UIA_RangeValueValuePropertyId => Some((
            previous.range.map_or(AutomationValue::Empty, |range| {
                AutomationValue::F64(range.value)
            }),
            current.range.map_or(AutomationValue::Empty, |range| {
                AutomationValue::F64(range.value)
            }),
        )),
        UIA_ValueValuePropertyId => Some((
            if node_has_value(previous.kind) {
                AutomationValue::String(previous.value.clone())
            } else {
                AutomationValue::Empty
            },
            if node_has_value(current.kind) {
                AutomationValue::String(current.value.clone())
            } else {
                AutomationValue::Empty
            },
        )),
        _ => None,
    }
}

pub(super) fn snapshot_notifications(
    previous: &SettingsAutomationSnapshot,
    current: &SettingsAutomationSnapshot,
    suppress_initial: bool,
) -> Vec<AutomationNotification> {
    let mut notifications = Vec::new();
    if !suppress_initial
        && (previous.focused != current.focused || previous.focus_owner != current.focus_owner)
        && current.focus_owner == AutomationFocusOwner::Settings
    {
        let target = current
            .focused
            .map_or(AutomationTarget::Root, AutomationTarget::Node);
        notifications.push(AutomationNotification::focus(target));
    }

    if !suppress_initial {
        let previous_root_focus =
            previous.focus_owner == AutomationFocusOwner::Settings && previous.focused.is_none();
        let current_root_focus =
            current.focus_owner == AutomationFocusOwner::Settings && current.focused.is_none();
        if previous_root_focus != current_root_focus {
            notifications.push(AutomationNotification::property(
                AutomationTarget::Root,
                UIA_HasKeyboardFocusPropertyId,
                AutomationValue::Bool(previous_root_focus),
                AutomationValue::Bool(current_root_focus),
            ));
        }
        if previous.window != current.window {
            notifications.push(AutomationNotification::property(
                AutomationTarget::Root,
                UIA_BoundingRectanglePropertyId,
                AutomationValue::Rect(previous.window),
                AutomationValue::Rect(current.window),
            ));
        }
        if previous.page != current.page {
            notifications.push(AutomationNotification::property(
                AutomationTarget::Root,
                UIA_NamePropertyId,
                AutomationValue::String(format!(
                    "WinShort Control Center — {}",
                    previous.page.label()
                )),
                AutomationValue::String(format!(
                    "WinShort Control Center — {}",
                    current.page.label()
                )),
            ));
        }
    }

    for node in &current.nodes {
        let Some(previous_node) = previous.nodes.iter().find(|item| item.id == node.id) else {
            continue;
        };
        for property in changed_property_ids(previous_node, node) {
            let Some((old_value, new_value)) = typed_property_values(property, previous_node, node)
            else {
                continue;
            };
            notifications.push(AutomationNotification::property(
                AutomationTarget::Node(node.id),
                property,
                old_value,
                new_value,
            ));
        }
    }
    notifications
}

impl AutomationNotification {
    pub(super) fn focus(target: AutomationTarget) -> Self {
        Self {
            target,
            kind: AutomationNotificationKind::FocusChanged,
            old_value: AutomationValue::Empty,
            new_value: AutomationValue::Empty,
        }
    }

    pub(super) fn invoked(id: ElementId) -> Self {
        Self {
            target: AutomationTarget::Node(id),
            kind: AutomationNotificationKind::Invoked,
            old_value: AutomationValue::Empty,
            new_value: AutomationValue::Empty,
        }
    }

    pub(super) fn property(
        target: AutomationTarget,
        property: UIA_PROPERTY_ID,
        old_value: AutomationValue,
        new_value: AutomationValue,
    ) -> Self {
        Self {
            target,
            kind: AutomationNotificationKind::Property(property.0),
            old_value,
            new_value,
        }
    }
}
