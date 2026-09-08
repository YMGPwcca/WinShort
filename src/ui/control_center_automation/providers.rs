//! Providers for the control center automation.

use super::session::AutomationState;
use crate::ui::layout::ElementId;
use std::sync::Weak;
use windows::core::implement;
use windows::Win32::UI::Accessibility::{
    IInvokeProvider, IRangeValueProvider, IRawElementProviderFragment,
    IRawElementProviderFragmentRoot, IRawElementProviderSimple, ISelectionItemProvider,
    IToggleProvider, IValueProvider,
};

#[derive(Clone)]
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot
)]
pub(super) struct SettingsAutomationRootProvider {
    pub(super) state: Weak<AutomationState>,
}

#[derive(Clone)]
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IInvokeProvider,
    IToggleProvider,
    ISelectionItemProvider,
    IRangeValueProvider,
    IValueProvider
)]
pub(super) struct SettingsAutomationNodeProvider {
    pub(super) state: Weak<AutomationState>,
    pub(super) node: ElementId,
}
