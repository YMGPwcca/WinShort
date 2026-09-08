//! Root provider for the control center automation.

#![allow(non_upper_case_globals)]
// Preserve Windows SDK names in ABI-facing constant patterns.

use super::abi::{install_node_vtables, require_nullable_result};
use super::model::{AutomationFocusOwner, SettingsAutomationSnapshot};
use super::properties::root_property_value;
use super::providers::SettingsAutomationNodeProvider;
use super::providers::SettingsAutomationRootProvider_Impl;
use super::session::{upgrade_state, SettingsAutomation, SettingsAutomationAction};
use crate::ui::layout::ElementId;
use windows::core::IUnknownImpl;
use windows::core::{IUnknown, Interface};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    IRawElementProviderFragment, IRawElementProviderFragmentRoot,
    IRawElementProviderFragmentRoot_Impl, IRawElementProviderFragment_Impl,
    IRawElementProviderSimple, IRawElementProviderSimple_Impl, NavigateDirection,
    NavigateDirection_FirstChild, NavigateDirection_LastChild, ProviderOptions,
    ProviderOptions_ProviderOwnsSetFocus, ProviderOptions_ServerSideProvider, UiaRect,
    UIA_PATTERN_ID,
};

impl IRawElementProviderSimple_Impl for SettingsAutomationRootProvider_Impl {
    fn ProviderOptions(&self) -> windows::core::Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_ProviderOwnsSetFocus)
    }

    fn GetPatternProvider(&self, patternid: UIA_PATTERN_ID) -> windows::core::Result<IUnknown> {
        require_nullable_result(self.pattern_provider_result(patternid))
    }

    fn GetPropertyValue(
        &self,
        propertyid: windows::Win32::UI::Accessibility::UIA_PROPERTY_ID,
    ) -> windows::core::Result<VARIANT> {
        root_property_value(&self.snapshot()?, propertyid)
    }

    fn HostRawElementProvider(&self) -> windows::core::Result<IRawElementProviderSimple> {
        unsafe { windows::Win32::UI::Accessibility::UiaHostProviderFromHwnd(self.hwnd()?) }
    }
}

impl IRawElementProviderFragment_Impl for SettingsAutomationRootProvider_Impl {
    fn Navigate(
        &self,
        direction: NavigateDirection,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        require_nullable_result(self.navigate_result(direction))
    }

    fn GetRuntimeId(&self) -> windows::core::Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn BoundingRectangle(&self) -> windows::core::Result<UiaRect> {
        let bounds = self.snapshot()?.window;
        Ok(UiaRect {
            left: bounds.left,
            top: bounds.top,
            width: bounds.width,
            height: bounds.height,
        })
    }

    fn GetEmbeddedFragmentRoots(&self) -> windows::core::Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> windows::core::Result<()> {
        self.automation()?
            .enqueue(SettingsAutomationAction::SetWindowFocus)
    }

    fn FragmentRoot(&self) -> windows::core::Result<IRawElementProviderFragmentRoot> {
        Ok(self.to_interface())
    }
}

impl IRawElementProviderFragmentRoot_Impl for SettingsAutomationRootProvider_Impl {
    fn ElementProviderFromPoint(
        &self,
        x: f64,
        y: f64,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        require_nullable_result(self.element_provider_from_point_result(x, y))
    }

    fn GetFocus(&self) -> windows::core::Result<IRawElementProviderFragment> {
        require_nullable_result(self.focus_result())
    }
}

impl SettingsAutomationRootProvider_Impl {
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

    pub(super) fn hwnd(&self) -> windows::core::Result<HWND> {
        Ok(HWND(upgrade_state(&self.state)?.hwnd as *mut _))
    }

    pub(super) fn child_fragment(
        &self,
        id: ElementId,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        let _ = upgrade_state(&self.state)?;
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
        _patternid: UIA_PATTERN_ID,
    ) -> windows::core::Result<Option<IUnknown>> {
        let _ = self.snapshot()?;
        Ok(None)
    }

    pub(super) fn navigate_result(
        &self,
        direction: NavigateDirection,
    ) -> windows::core::Result<Option<IRawElementProviderFragment>> {
        let snapshot = self.snapshot()?;
        let next = match direction {
            NavigateDirection_FirstChild => snapshot.nodes.first().map(|node| node.id),
            NavigateDirection_LastChild => snapshot.nodes.last().map(|node| node.id),
            _ => None,
        };
        next.map_or_else(|| Ok(None), |id| self.child_fragment(id).map(Some))
    }

    pub(super) fn element_provider_from_point_result(
        &self,
        x: f64,
        y: f64,
    ) -> windows::core::Result<Option<IRawElementProviderFragment>> {
        let snapshot = self.snapshot()?;
        if let Some(node) = snapshot
            .nodes
            .into_iter()
            .find(|node| !node.offscreen && node.bounds.contains(x, y))
        {
            return self.child_fragment(node.id).map(Some);
        }
        if snapshot.window.contains(x, y) {
            return Ok(Some(self.to_interface()));
        }
        Ok(None)
    }

    pub(super) fn focus_result(
        &self,
    ) -> windows::core::Result<Option<IRawElementProviderFragment>> {
        let snapshot = self.snapshot()?;
        if snapshot.focus_owner != AutomationFocusOwner::Settings {
            return Ok(None);
        }
        snapshot.focused.map_or_else(
            || Ok(Some(self.to_interface())),
            |id| self.child_fragment(id).map(Some),
        )
    }
}
