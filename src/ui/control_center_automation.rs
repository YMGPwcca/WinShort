//! UI Automation boundary. Snapshot data is separate from COM providers and the nullable ABI bridge.

mod abi;
mod capabilities;
mod model;
mod node_provider;
mod notifications;
mod properties;
mod providers;
mod root_provider;
mod session;
mod snapshot;

pub(crate) use capabilities::node_has_invoke;
#[cfg(test)]
pub(crate) use model::AutomationRect;
pub(crate) use model::{AutomationFocusOwner, SettingsAutomationNode};
#[cfg(test)]
pub(crate) use notifications::changed_property_ids;
pub(crate) use session::{
    SettingsAutomation, SettingsAutomationAction, WM_APP_SETTINGS_AUTOMATION,
    WM_APP_SETTINGS_AUTOMATION_EVENTS,
};
pub(crate) use snapshot::snapshot_from_settings;

#[cfg(test)]
use self::notifications::{AutomationNotificationKind, AutomationTarget, AutomationValue};
#[cfg(test)]
use self::properties::control_type;
#[cfg(test)]
use self::snapshot::slider_range;
#[cfg(test)]
use crate::ui::layout::{ElementId, ElementKind, Rect as UiRect, SettingsLayout};
#[cfg(test)]
use windows::core::{IUnknown, Interface};
#[cfg(test)]
use windows::Win32::Foundation::{HWND, POINT};
#[cfg(test)]
use windows::Win32::System::Variant::{VariantClear, VARIANT};
#[cfg(test)]
use windows::Win32::UI::Accessibility::{
    IInvokeProvider, IRangeValueProvider, IRawElementProviderFragment,
    IRawElementProviderFragmentRoot, IRawElementProviderSimple, ISelectionItemProvider,
    IToggleProvider, IValueProvider, NavigateDirection, NavigateDirection_FirstChild,
    NavigateDirection_NextSibling, NavigateDirection_Parent, NavigateDirection_PreviousSibling,
    ToggleState_Off, ToggleState_On, UIA_AutomationIdPropertyId, UIA_BoundingRectanglePropertyId,
    UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId, UIA_ControlTypePropertyId,
    UIA_HasKeyboardFocusPropertyId, UIA_InvokePatternId, UIA_IsEnabledPropertyId,
    UIA_IsInvokePatternAvailablePropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_IsOffscreenPropertyId, UIA_IsTogglePatternAvailablePropertyId, UIA_NamePropertyId,
    UIA_RadioButtonControlTypeId, UIA_RangeValuePatternId, UIA_RangeValueValuePropertyId,
    UIA_SelectionItemPatternId, UIA_SliderControlTypeId, UIA_TextControlTypeId,
    UIA_TogglePatternId, UIA_ToggleToggleStatePropertyId, UIA_ValueIsReadOnlyPropertyId,
    UIA_ValuePatternId, UIA_ValueValuePropertyId, UiaAppendRuntimeId, UIA_E_ELEMENTNOTAVAILABLE,
    UIA_E_ELEMENTNOTENABLED, UIA_E_INVALIDOPERATION, UIA_E_NOTSUPPORTED, UIA_PROPERTY_ID,
};

#[cfg(test)]
mod tests;
