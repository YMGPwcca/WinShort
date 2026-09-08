//! Capability classification shared by immutable UIA snapshots and native providers.

use crate::ui::layout::ElementKind;

pub(super) fn is_keyboard_focusable_kind(kind: ElementKind) -> bool {
    !matches!(kind, ElementKind::Card | ElementKind::Info)
}

pub(crate) fn node_has_invoke(kind: ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::Value
            | ElementKind::Hotkey
            | ElementKind::Action
            | ElementKind::ButtonSecondary
            | ElementKind::ButtonPrimary
            | ElementKind::ButtonDanger
            | ElementKind::Navigation
    )
}

pub(super) fn node_has_value(kind: ElementKind) -> bool {
    // Picker triggers and hotkey actions expose their current display value;
    // Search is the one editable ValuePattern in the custom shell.
    matches!(
        kind,
        ElementKind::Value | ElementKind::Hotkey | ElementKind::Search
    )
}
