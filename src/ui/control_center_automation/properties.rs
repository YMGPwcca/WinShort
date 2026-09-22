//! Properties for the control center automation.

#![allow(non_upper_case_globals)]
use super::capabilities::{is_keyboard_focusable_kind, node_has_invoke, node_has_value};
// Preserve Windows SDK names in ABI-facing constant patterns.

use super::model::{
    AutomationFocusOwner, AutomationRect, SettingsAutomationNode, SettingsAutomationSnapshot,
};
use super::notifications::AutomationValue;
use crate::ui::layout::{ElementId, ElementKind};
use std::mem::ManuallyDrop;
use windows::core::BSTR;
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Variant::{
    InitVariantFromDoubleArray, InitVariantFromInt32Array, VARIANT,
};
use windows::Win32::UI::Accessibility::{
    ToggleState_Off, ToggleState_On, UIA_AutomationIdPropertyId, UIA_BoundingRectanglePropertyId,
    UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_HasKeyboardFocusPropertyId,
    UIA_HelpTextPropertyId, UIA_IsContentElementPropertyId, UIA_IsControlElementPropertyId,
    UIA_IsEnabledPropertyId, UIA_IsExpandCollapsePatternAvailablePropertyId,
    UIA_IsInvokePatternAvailablePropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_IsOffscreenPropertyId, UIA_IsRangeValuePatternAvailablePropertyId,
    UIA_IsSelectionItemPatternAvailablePropertyId, UIA_IsTogglePatternAvailablePropertyId,
    UIA_IsValuePatternAvailablePropertyId, UIA_NamePropertyId, UIA_ProviderDescriptionPropertyId,
    UIA_RadioButtonControlTypeId, UIA_RangeValueLargeChangePropertyId,
    UIA_RangeValueMaximumPropertyId, UIA_RangeValueMinimumPropertyId,
    UIA_RangeValueSmallChangePropertyId, UIA_RangeValueValuePropertyId,
    UIA_SelectionItemIsSelectedPropertyId, UIA_SliderControlTypeId, UIA_TextControlTypeId,
    UIA_ToggleToggleStatePropertyId, UIA_ValueIsReadOnlyPropertyId, UIA_ValueValuePropertyId,
    UIA_E_ELEMENTNOTAVAILABLE, UIA_E_ELEMENTNOTENABLED, UIA_E_NOTSUPPORTED, UIA_PROPERTY_ID,
};

fn string_variant(value: &str) -> VARIANT {
    BSTR::from(value).into()
}

fn bool_variant(value: bool) -> VARIANT {
    value.into()
}

fn i32_variant(value: i32) -> VARIANT {
    value.into()
}

fn f64_variant(value: f64) -> VARIANT {
    value.into()
}

fn rect_variant(rect: AutomationRect) -> windows::core::Result<VARIANT> {
    unsafe { InitVariantFromDoubleArray(&[rect.left, rect.top, rect.width, rect.height]) }
}

pub(super) fn runtime_id_variant(values: &[i32]) -> windows::core::Result<*mut SAFEARRAY> {
    let variant = unsafe { InitVariantFromInt32Array(values)? };
    let variant = ManuallyDrop::new(variant);
    let array = unsafe { variant.Anonymous.Anonymous.Anonymous.parray };
    Ok(array)
}

pub(super) fn optional_i32_value(value: Option<i32>) -> AutomationValue {
    value.map_or(AutomationValue::Empty, AutomationValue::I32)
}

pub(super) fn control_type(kind: ElementKind) -> i32 {
    match kind {
        ElementKind::Toggle | ElementKind::Checkbox => UIA_CheckBoxControlTypeId.0,
        ElementKind::Choice => UIA_RadioButtonControlTypeId.0,
        ElementKind::Slider => UIA_SliderControlTypeId.0,
        ElementKind::Search => UIA_EditControlTypeId.0,
        ElementKind::Info => UIA_TextControlTypeId.0,
        ElementKind::Value
        | ElementKind::Hotkey
        | ElementKind::Action
        | ElementKind::ButtonSecondary
        | ElementKind::ButtonPrimary
        | ElementKind::ButtonDanger
        | ElementKind::Navigation
        | ElementKind::Card => UIA_ButtonControlTypeId.0,
    }
}

fn automation_id(id: ElementId) -> String {
    let name = match id {
        // OverlayBlur replaced the former opacity control internally, but
        // this AutomationId is an external accessibility contract.
        ElementId::OverlayBlur => "OverlayOpacity".to_owned(),
        _ => format!("{id:?}"),
    };
    format!("WinShort.ControlCenter.{name}")
}

pub(super) fn element_unavailable_error() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}

pub(super) fn element_not_enabled_error() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(UIA_E_ELEMENTNOTENABLED as i32))
}

pub(super) fn unsupported_error() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(UIA_E_NOTSUPPORTED as i32))
}

pub(super) fn root_property_value(
    snapshot: &SettingsAutomationSnapshot,
    propertyid: UIA_PROPERTY_ID,
) -> windows::core::Result<VARIANT> {
    match propertyid {
        UIA_IsInvokePatternAvailablePropertyId
        | UIA_IsTogglePatternAvailablePropertyId
        | UIA_IsRangeValuePatternAvailablePropertyId
        | UIA_IsValuePatternAvailablePropertyId
        | UIA_IsExpandCollapsePatternAvailablePropertyId
        | UIA_IsSelectionItemPatternAvailablePropertyId => Ok(bool_variant(false)),
        UIA_NamePropertyId => Ok(string_variant(&format!(
            "WinShort Control Center — {}",
            snapshot.page.label()
        ))),
        UIA_HelpTextPropertyId => Ok(string_variant("WinShort Control Center")),
        UIA_ControlTypePropertyId => Ok(i32_variant(
            windows::Win32::UI::Accessibility::UIA_WindowControlTypeId.0,
        )),
        UIA_IsEnabledPropertyId
        | UIA_IsKeyboardFocusablePropertyId
        | UIA_IsContentElementPropertyId
        | UIA_IsControlElementPropertyId => Ok(bool_variant(true)),
        UIA_HasKeyboardFocusPropertyId => Ok(bool_variant(
            snapshot.focus_owner == AutomationFocusOwner::Settings && snapshot.focused.is_none(),
        )),
        UIA_IsOffscreenPropertyId => Ok(bool_variant(false)),
        UIA_BoundingRectanglePropertyId => rect_variant(snapshot.window),
        UIA_AutomationIdPropertyId => Ok(string_variant("WinShort.ControlCenter")),
        UIA_ClassNamePropertyId => Ok(string_variant("WinShort.ControlCenter")),
        UIA_ProviderDescriptionPropertyId => Ok(string_variant(
            "WinShort Control Center UI Automation provider",
        )),
        _ => Ok(VARIANT::default()),
    }
}

pub(super) fn node_property_value(
    node: &SettingsAutomationNode,
    propertyid: UIA_PROPERTY_ID,
) -> windows::core::Result<VARIANT> {
    match propertyid {
        UIA_NamePropertyId => Ok(string_variant(&node.name)),
        UIA_HelpTextPropertyId => Ok(string_variant(&node.help_text)),
        UIA_ControlTypePropertyId => Ok(i32_variant(control_type(node.kind))),
        UIA_IsEnabledPropertyId => Ok(bool_variant(node.enabled)),
        UIA_IsKeyboardFocusablePropertyId => Ok(bool_variant(
            node.enabled && is_keyboard_focusable_kind(node.kind),
        )),
        UIA_HasKeyboardFocusPropertyId => Ok(bool_variant(node.focused)),
        UIA_IsOffscreenPropertyId => Ok(bool_variant(node.offscreen)),
        UIA_IsContentElementPropertyId | UIA_IsControlElementPropertyId => Ok(bool_variant(true)),
        UIA_BoundingRectanglePropertyId => rect_variant(node.bounds),
        UIA_AutomationIdPropertyId => Ok(string_variant(&automation_id(node.id))),
        UIA_ClassNamePropertyId => Ok(string_variant("WinShort.ControlCenter.Item")),
        UIA_ProviderDescriptionPropertyId => Ok(string_variant(
            "WinShort Control Center UI Automation provider",
        )),
        UIA_IsInvokePatternAvailablePropertyId => Ok(bool_variant(node_has_invoke(node.kind))),
        UIA_IsTogglePatternAvailablePropertyId => Ok(bool_variant(
            node.kind != ElementKind::Choice && node.toggle.is_some(),
        )),
        UIA_IsSelectionItemPatternAvailablePropertyId => Ok(bool_variant(
            node.kind == ElementKind::Choice && node.toggle.is_some(),
        )),
        UIA_IsRangeValuePatternAvailablePropertyId => Ok(bool_variant(node.range.is_some())),
        UIA_IsValuePatternAvailablePropertyId => Ok(bool_variant(node_has_value(node.kind))),
        UIA_IsExpandCollapsePatternAvailablePropertyId => Ok(bool_variant(false)),
        UIA_SelectionItemIsSelectedPropertyId => Ok(if node.kind == ElementKind::Choice {
            node.toggle.map_or_else(VARIANT::default, bool_variant)
        } else {
            VARIANT::default()
        }),
        UIA_ToggleToggleStatePropertyId => Ok(if node.kind == ElementKind::Choice {
            VARIANT::default()
        } else {
            node.toggle.map_or_else(VARIANT::default, |value| {
                i32_variant(if value {
                    ToggleState_On.0
                } else {
                    ToggleState_Off.0
                })
            })
        }),
        UIA_RangeValueValuePropertyId
        | UIA_RangeValueMinimumPropertyId
        | UIA_RangeValueMaximumPropertyId
        | UIA_RangeValueSmallChangePropertyId
        | UIA_RangeValueLargeChangePropertyId => {
            Ok(node
                .range
                .map_or_else(VARIANT::default, |range| match propertyid {
                    UIA_RangeValueValuePropertyId => f64_variant(range.value),
                    UIA_RangeValueMinimumPropertyId => f64_variant(range.minimum),
                    UIA_RangeValueMaximumPropertyId => f64_variant(range.maximum),
                    UIA_RangeValueSmallChangePropertyId => f64_variant(range.small_change),
                    UIA_RangeValueLargeChangePropertyId => f64_variant(range.large_change),
                    _ => VARIANT::default(),
                }))
        }
        UIA_ValueValuePropertyId => Ok(if node.is_value_pattern_available() {
            string_variant(&node.value)
        } else {
            VARIANT::default()
        }),
        UIA_ValueIsReadOnlyPropertyId => Ok(if node.is_value_pattern_available() {
            bool_variant(node.kind != ElementKind::Search)
        } else {
            VARIANT::default()
        }),
        _ => Ok(VARIANT::default()),
    }
}

impl AutomationValue {
    pub(super) fn to_variant(&self) -> windows::core::Result<VARIANT> {
        match self {
            Self::Empty => Ok(VARIANT::default()),
            Self::Bool(value) => Ok(bool_variant(*value)),
            Self::I32(value) => Ok(i32_variant(*value)),
            Self::F64(value) => Ok(f64_variant(*value)),
            Self::String(value) => Ok(string_variant(value)),
            Self::Rect(value) => rect_variant(*value),
        }
    }
}
