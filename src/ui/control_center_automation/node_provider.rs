//! Node provider for the control center automation.

#![allow(non_upper_case_globals)]
// Preserve Windows SDK names in ABI-facing constant patterns.

use super::abi::{
    element_not_enabled, install_node_vtables, invalid_argument, require_nullable_result,
    unsupported,
};
use super::capabilities::is_keyboard_focusable_kind;
use super::capabilities::node_has_invoke;
use super::capabilities::node_has_value;
use super::model::{SettingsAutomationNode, SettingsAutomationSnapshot};
use super::properties::element_unavailable_error;
use super::properties::node_property_value;
use super::properties::runtime_id_variant;
use super::properties::unsupported_error;
use super::providers::SettingsAutomationNodeProvider;
use super::providers::SettingsAutomationNodeProvider_Impl;
use super::session::{upgrade_state, SettingsAutomation, SettingsAutomationAction};
use crate::ui::layout::{ElementId, ElementKind};
use windows::core::{IUnknown, Interface, BSTR};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    IInvokeProvider_Impl, IRangeValueProvider_Impl, IRawElementProviderFragment,
    IRawElementProviderFragmentRoot, IRawElementProviderFragment_Impl, IRawElementProviderSimple,
    IRawElementProviderSimple_Impl, ISelectionItemProvider_Impl, IToggleProvider_Impl,
    IValueProvider_Impl, NavigateDirection, NavigateDirection_NextSibling,
    NavigateDirection_Parent, NavigateDirection_PreviousSibling, ProviderOptions,
    ProviderOptions_ProviderOwnsSetFocus, ProviderOptions_ServerSideProvider, ToggleState,
    ToggleState_Off, ToggleState_On, UIA_InvokePatternId, UIA_RangeValuePatternId,
    UIA_SelectionItemPatternId, UIA_TogglePatternId, UIA_ValuePatternId, UiaAppendRuntimeId,
    UiaRect, UIA_E_INVALIDOPERATION, UIA_PATTERN_ID,
};

impl IRawElementProviderSimple_Impl for SettingsAutomationNodeProvider_Impl {
    fn ProviderOptions(&self) -> windows::core::Result<ProviderOptions> {
        let _ = self.node()?;
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_ProviderOwnsSetFocus)
    }

    fn GetPatternProvider(&self, patternid: UIA_PATTERN_ID) -> windows::core::Result<IUnknown> {
        require_nullable_result(self.pattern_provider_result(patternid))
    }

    fn GetPropertyValue(
        &self,
        propertyid: windows::Win32::UI::Accessibility::UIA_PROPERTY_ID,
    ) -> windows::core::Result<VARIANT> {
        let node = self.node()?;
        node_property_value(&node, propertyid)
    }

    fn HostRawElementProvider(&self) -> windows::core::Result<IRawElementProviderSimple> {
        require_nullable_result(self.host_raw_element_provider_result())
    }
}

impl IRawElementProviderFragment_Impl for SettingsAutomationNodeProvider_Impl {
    fn Navigate(
        &self,
        direction: NavigateDirection,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        require_nullable_result(self.navigate_result(direction))
    }

    fn GetRuntimeId(&self) -> windows::core::Result<*mut SAFEARRAY> {
        let index = self.node_index()?;
        runtime_id_variant(&[UiaAppendRuntimeId as i32, index as i32 + 1])
    }

    fn BoundingRectangle(&self) -> windows::core::Result<UiaRect> {
        let node = self.node()?;
        Ok(UiaRect {
            left: node.bounds.left,
            top: node.bounds.top,
            width: node.bounds.width,
            height: node.bounds.height,
        })
    }

    fn GetEmbeddedFragmentRoots(&self) -> windows::core::Result<*mut SAFEARRAY> {
        let _ = self.node()?;
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> windows::core::Result<()> {
        let node = self.node()?;
        if !node.enabled {
            return element_not_enabled();
        }
        if !is_keyboard_focusable_kind(node.kind) {
            return unsupported();
        }
        self.automation()?
            .enqueue(SettingsAutomationAction::SetFocus(self.node))
    }

    fn FragmentRoot(&self) -> windows::core::Result<IRawElementProviderFragmentRoot> {
        let _ = self.node()?;
        self.automation()?.root_fragment()
    }
}

impl IInvokeProvider_Impl for SettingsAutomationNodeProvider_Impl {
    fn Invoke(&self) -> windows::core::Result<()> {
        let node = self.node()?;
        if !node_has_invoke(node.kind) {
            return unsupported();
        }
        if !node.enabled {
            return element_not_enabled();
        }
        self.automation()?
            .enqueue(SettingsAutomationAction::Invoke(node.id))
    }
}

impl IToggleProvider_Impl for SettingsAutomationNodeProvider_Impl {
    fn Toggle(&self) -> windows::core::Result<()> {
        let node = self.node()?;
        if node.toggle.is_none() {
            return unsupported();
        }
        if !node.enabled {
            return element_not_enabled();
        }
        self.automation()?
            .enqueue(SettingsAutomationAction::Toggle(node.id))
    }

    fn ToggleState(&self) -> windows::core::Result<ToggleState> {
        let node = self.node()?;
        node.toggle.ok_or_else(unsupported_error).map(|value| {
            if value {
                ToggleState_On
            } else {
                ToggleState_Off
            }
        })
    }
}

impl ISelectionItemProvider_Impl for SettingsAutomationNodeProvider_Impl {
    fn Select(&self) -> windows::core::Result<()> {
        let node = self.node()?;
        if node.kind != ElementKind::Choice {
            return unsupported();
        }
        if !node.enabled {
            return element_not_enabled();
        }
        self.automation()?
            .enqueue(SettingsAutomationAction::Toggle(node.id))
    }

    fn AddToSelection(&self) -> windows::core::Result<()> {
        self.Select()
    }

    fn RemoveFromSelection(&self) -> windows::core::Result<()> {
        let node = self.node()?;
        if node.kind != ElementKind::Choice {
            return unsupported();
        }
        unsupported()
    }

    fn IsSelected(&self) -> windows::core::Result<windows::core::BOOL> {
        let node = self.node()?;
        if node.kind != ElementKind::Choice {
            return unsupported();
        }
        node.toggle.ok_or_else(unsupported_error).map(Into::into)
    }

    fn SelectionContainer(&self) -> windows::core::Result<IRawElementProviderSimple> {
        let node = self.node()?;
        if node.kind != ElementKind::Choice {
            return unsupported();
        }
        self.automation()?.root_provider()
    }
}

impl IRangeValueProvider_Impl for SettingsAutomationNodeProvider_Impl {
    fn SetValue(&self, val: f64) -> windows::core::Result<()> {
        let node = self.node()?;
        let range = node.range.ok_or_else(unsupported_error)?;
        if !node.enabled {
            return element_not_enabled();
        }
        if !val.is_finite() || val < range.minimum || val > range.maximum {
            return invalid_argument();
        }
        self.automation()?
            .enqueue(SettingsAutomationAction::SetSlider {
                id: node.id,
                value: val,
            })
    }

    fn Value(&self) -> windows::core::Result<f64> {
        let node = self.node()?;
        node.range
            .ok_or_else(unsupported_error)
            .map(|range| range.value)
    }

    fn IsReadOnly(&self) -> windows::core::Result<windows::core::BOOL> {
        let node = self.node()?;
        node.range
            .ok_or_else(unsupported_error)
            .map(|range| range.read_only.into())
    }

    fn Maximum(&self) -> windows::core::Result<f64> {
        let node = self.node()?;
        node.range
            .ok_or_else(unsupported_error)
            .map(|range| range.maximum)
    }

    fn Minimum(&self) -> windows::core::Result<f64> {
        let node = self.node()?;
        node.range
            .ok_or_else(unsupported_error)
            .map(|range| range.minimum)
    }

    fn LargeChange(&self) -> windows::core::Result<f64> {
        let node = self.node()?;
        node.range
            .ok_or_else(unsupported_error)
            .map(|range| range.large_change)
    }

    fn SmallChange(&self) -> windows::core::Result<f64> {
        let node = self.node()?;
        node.range
            .ok_or_else(unsupported_error)
            .map(|range| range.small_change)
    }
}

impl IValueProvider_Impl for SettingsAutomationNodeProvider_Impl {
    fn SetValue(&self, val: &windows::core::PCWSTR) -> windows::core::Result<()> {
        let node = self.node()?;
        if node.kind != ElementKind::Search {
            return if node_has_value(node.kind) {
                Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    UIA_E_INVALIDOPERATION as i32,
                )))
            } else {
                unsupported()
            };
        }
        if !node.enabled {
            return element_not_enabled();
        }
        let value = unsafe { val.to_string() }.or_else(|_| invalid_argument())?;
        if value.chars().count() > 256 {
            return invalid_argument();
        }
        self.automation()?
            .enqueue(SettingsAutomationAction::SetSearch(value))
    }

    fn Value(&self) -> windows::core::Result<BSTR> {
        let node = self.node()?;
        if !node_has_value(node.kind) {
            return unsupported();
        }
        Ok(BSTR::from(node.value.as_str()))
    }

    fn IsReadOnly(&self) -> windows::core::Result<windows::core::BOOL> {
        let node = self.node()?;
        if !node_has_value(node.kind) {
            return unsupported();
        }
        Ok((node.kind != ElementKind::Search).into())
    }
}

impl SettingsAutomationNodeProvider_Impl {
    pub(super) fn snapshot(&self) -> windows::core::Result<SettingsAutomationSnapshot> {
        Ok(upgrade_state(&self.state)?
            .snapshot
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone())
    }

    pub(super) fn automation(&self) -> windows::core::Result<SettingsAutomation> {
        Ok(SettingsAutomation {
            state: upgrade_state(&self.state)?,
        })
    }

    pub(super) fn node(&self) -> windows::core::Result<SettingsAutomationNode> {
        self.snapshot()?
            .nodes
            .into_iter()
            .find(|node| node.id == self.node)
            .ok_or_else(element_unavailable_error)
    }

    pub(super) fn node_index(&self) -> windows::core::Result<usize> {
        self.snapshot()?
            .nodes
            .iter()
            .position(|node| node.id == self.node)
            .ok_or_else(element_unavailable_error)
    }

    pub(super) fn self_simple(&self) -> windows::core::Result<IRawElementProviderSimple> {
        let provider: IRawElementProviderSimple = SettingsAutomationNodeProvider {
            state: self.state.clone(),
            node: self.node,
        }
        .into();
        install_node_vtables(&provider)?;
        Ok(provider)
    }

    pub(super) fn child_fragment(
        &self,
        id: ElementId,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        let provider: IRawElementProviderFragment = SettingsAutomationNodeProvider {
            state: self.state.clone(),
            node: id,
        }
        .into();
        let simple: IRawElementProviderSimple = provider.cast()?;
        install_node_vtables(&simple)?;
        Ok(provider)
    }

    pub(super) fn pattern_provider_result(
        &self,
        patternid: UIA_PATTERN_ID,
    ) -> windows::core::Result<Option<IUnknown>> {
        let node = self.node()?;
        let available = match patternid {
            UIA_InvokePatternId => node_has_invoke(node.kind),
            UIA_TogglePatternId => node.kind != ElementKind::Choice && node.toggle.is_some(),
            UIA_SelectionItemPatternId => node.kind == ElementKind::Choice && node.toggle.is_some(),
            UIA_RangeValuePatternId => node.range.is_some(),
            UIA_ValuePatternId => node_has_value(node.kind),
            _ => false,
        };
        if !available {
            return Ok(None);
        }
        self.self_simple()?.cast().map(Some)
    }

    pub(super) fn host_raw_element_provider_result(
        &self,
    ) -> windows::core::Result<Option<IRawElementProviderSimple>> {
        let _ = self.node()?;
        Ok(None)
    }

    pub(super) fn navigate_result(
        &self,
        direction: NavigateDirection,
    ) -> windows::core::Result<Option<IRawElementProviderFragment>> {
        let _ = self.node()?;
        let snapshot = self.snapshot()?;
        let index = self.node_index()?;
        let next = match direction {
            NavigateDirection_Parent => {
                return self.automation()?.root_fragment()?.cast().map(Some);
            }
            NavigateDirection_NextSibling => snapshot.nodes.get(index + 1).map(|node| node.id),
            NavigateDirection_PreviousSibling => index
                .checked_sub(1)
                .and_then(|index| snapshot.nodes.get(index))
                .map(|node| node.id),
            _ => None,
        };
        next.map(|id| self.child_fragment(id)).transpose()
    }
}
