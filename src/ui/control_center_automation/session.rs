//! Accessibility lifetime, short-lived locks and deferred command/event queues.

use super::abi::{install_node_vtables, install_root_vtables};
use super::model::SettingsAutomationSnapshot;
use super::notifications::{
    snapshot_notifications, AutomationNotification, AutomationNotificationKind, AutomationTarget,
};
use super::properties::element_unavailable_error;
use super::providers::{SettingsAutomationNodeProvider, SettingsAutomationRootProvider};
use crate::ui::layout::ElementId;
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};
use windows::core::Interface;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Variant::VariantClear;
use windows::Win32::UI::Accessibility::{
    IRawElementProviderFragmentRoot, IRawElementProviderSimple, UIA_AutomationFocusChangedEventId,
    UIA_Invoke_InvokedEventId, UiaRaiseAutomationEvent, UiaRaiseAutomationPropertyChangedEvent,
    UiaReturnRawElementProvider, UiaRootObjectId, UIA_PROPERTY_ID,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub(crate) const WM_APP_SETTINGS_AUTOMATION: u32 = WM_APP + 6;

pub(crate) const WM_APP_SETTINGS_AUTOMATION_EVENTS: u32 = WM_APP + 7;

#[derive(Default)]
struct PendingAutomationEvents {
    notifications: Vec<AutomationNotification>,
    flush_posted: bool,
}

pub(super) struct AutomationState {
    pub(super) snapshot: RwLock<SettingsAutomationSnapshot>,
    pub(super) actions: Mutex<Vec<SettingsAutomationAction>>,
    pending: Mutex<PendingAutomationEvents>,
    pub(super) hwnd: isize,
    pub(super) root: OnceLock<std::result::Result<usize, windows::core::HRESULT>>,
}

impl Drop for AutomationState {
    fn drop(&mut self) {
        if let Some(Ok(raw)) = self.root.get().copied() {
            // Keep the cached root identity alive for the state's lifetime,
            // then release that one retained COM reference during teardown.
            unsafe {
                drop(IRawElementProviderFragmentRoot::from_raw(
                    raw as *mut core::ffi::c_void,
                ));
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum SettingsAutomationAction {
    Invoke(ElementId),
    Toggle(ElementId),
    SetSlider { id: ElementId, value: f64 },
    SetSearch(String),
    SetFocus(ElementId),
    SetWindowFocus,
}

#[derive(Clone)]
pub(crate) struct SettingsAutomation {
    pub(super) state: Arc<AutomationState>,
}

pub(super) fn upgrade_state(
    state: &Weak<AutomationState>,
) -> windows::core::Result<Arc<AutomationState>> {
    state.upgrade().ok_or_else(element_unavailable_error)
}

impl SettingsAutomation {
    pub(crate) fn new(hwnd: HWND) -> Self {
        Self {
            state: Arc::new(AutomationState {
                snapshot: RwLock::new(SettingsAutomationSnapshot::default()),
                actions: Mutex::new(Vec::new()),
                pending: Mutex::new(PendingAutomationEvents::default()),
                hwnd: hwnd.0 as isize,
                root: OnceLock::new(),
            }),
        }
    }

    pub(crate) fn publish(&self, snapshot: SettingsAutomationSnapshot) {
        // Phase 1: commit state and queue owned, typed notifications only.
        // UIA calls belong exclusively to flush_pending_events below.
        let previous = {
            let mut current = self
                .state
                .snapshot
                .write()
                .unwrap_or_else(|error| error.into_inner());
            if *current == snapshot {
                return;
            }
            let previous = current.clone();
            *current = snapshot.clone();
            previous
        };
        let first_snapshot = previous.nodes.is_empty() && !snapshot.nodes.is_empty();
        self.queue_notifications(snapshot_notifications(&previous, &snapshot, first_snapshot));
    }

    pub(crate) fn queue_invoked(&self, id: ElementId) {
        self.queue_notifications([AutomationNotification::invoked(id)]);
    }

    pub(super) fn queue_notifications<I>(&self, notifications: I)
    where
        I: IntoIterator<Item = AutomationNotification>,
    {
        let mut should_post = false;
        {
            let mut pending = self
                .state
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            for notification in notifications {
                if matches!(notification.kind, AutomationNotificationKind::Property(_)) {
                    if let Some(index) = pending.notifications.iter().position(|existing| {
                        existing.target == notification.target && existing.kind == notification.kind
                    }) {
                        pending.notifications[index].new_value = notification.new_value;
                        if pending.notifications[index].old_value
                            == pending.notifications[index].new_value
                        {
                            pending.notifications.remove(index);
                        }
                        continue;
                    }
                }
                pending.notifications.push(notification);
            }
            if !pending.notifications.is_empty() && !pending.flush_posted {
                pending.flush_posted = true;
                should_post = true;
            }
        }
        if should_post {
            let result = unsafe {
                PostMessageW(
                    Some(self.hwnd()),
                    WM_APP_SETTINGS_AUTOMATION_EVENTS,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
            if let Err(error) = result {
                crate::warn_!("could not schedule accessibility notification flush: {error}");
                let mut pending = self
                    .state
                    .pending
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                pending.flush_posted = false;
            }
        }
    }

    pub(super) fn take_pending_notifications(&self) -> Vec<AutomationNotification> {
        let mut pending = self
            .state
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        pending.flush_posted = false;
        std::mem::take(&mut pending.notifications)
    }

    /// Phase 2: deliver notifications after the owning Control Center UI borrow is gone.
    pub(crate) fn flush_pending_events(&self) {
        for notification in self.take_pending_notifications() {
            self.emit_notification(notification);
        }
    }

    #[cfg(test)]
    pub(super) fn flush_pending_events_with<F>(&self, mut sink: F)
    where
        F: FnMut(&Self, &AutomationNotification),
    {
        for notification in self.take_pending_notifications() {
            sink(self, &notification);
        }
    }

    #[cfg(test)]
    pub(crate) fn flush_pending_events_for_test<F>(&self, mut sink: F)
    where
        F: FnMut(&Self),
    {
        for _notification in self.take_pending_notifications() {
            sink(self);
        }
    }

    pub(super) fn emit_notification(&self, notification: AutomationNotification) {
        let provider = match notification.target {
            AutomationTarget::Root => self.root_provider(),
            AutomationTarget::Node(id) => self.provider_for(id),
        };
        let provider = match provider {
            Ok(provider) => provider,
            Err(error) => {
                crate::warn_!("accessibility provider unavailable: {error}");
                return;
            }
        };
        match notification.kind {
            AutomationNotificationKind::FocusChanged => unsafe {
                if let Err(error) =
                    UiaRaiseAutomationEvent(&provider, UIA_AutomationFocusChangedEventId)
                {
                    crate::warn_!("accessibility focus event delivery failed: {error}");
                }
            },
            AutomationNotificationKind::Invoked => unsafe {
                if let Err(error) = UiaRaiseAutomationEvent(&provider, UIA_Invoke_InvokedEventId) {
                    crate::warn_!("accessibility invoke event delivery failed: {error}");
                }
            },
            AutomationNotificationKind::Property(property) => {
                let mut old_value = match notification.old_value.to_variant() {
                    Ok(value) => value,
                    Err(error) => {
                        crate::warn_!(
                            "accessibility old property value conversion failed: {error}"
                        );
                        return;
                    }
                };
                let mut new_value = match notification.new_value.to_variant() {
                    Ok(value) => value,
                    Err(error) => {
                        crate::warn_!(
                            "accessibility new property value conversion failed: {error}"
                        );
                        unsafe {
                            // Cleanup cannot change notification semantics and has
                            // no meaningful recovery path if COM rejects it.
                            let _ = VariantClear(&mut old_value);
                        }
                        return;
                    }
                };
                unsafe {
                    if let Err(error) = UiaRaiseAutomationPropertyChangedEvent(
                        &provider,
                        UIA_PROPERTY_ID(property),
                        &old_value,
                        &new_value,
                    ) {
                        crate::warn_!("accessibility property event delivery failed: {error}");
                    }
                    // These variants are local owned payloads. Clearing them is
                    // teardown; retrying or surfacing cleanup failure is not useful.
                    let _ = VariantClear(&mut old_value);
                    let _ = VariantClear(&mut new_value);
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> SettingsAutomationSnapshot {
        self.state
            .snapshot
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn drain_actions(&self) -> Vec<SettingsAutomationAction> {
        let mut actions = self
            .state
            .actions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        std::mem::take(&mut *actions)
    }

    pub(crate) fn enqueue(&self, action: SettingsAutomationAction) -> windows::core::Result<()> {
        self.state
            .actions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(action);
        unsafe {
            PostMessageW(
                Some(self.hwnd()),
                WM_APP_SETTINGS_AUTOMATION,
                WPARAM(0),
                LPARAM(0),
            )
        }
    }

    pub(super) fn hwnd(&self) -> HWND {
        HWND(self.state.hwnd as *mut _)
    }

    pub(crate) fn root_provider(&self) -> windows::core::Result<IRawElementProviderSimple> {
        self.root_fragment()?.cast()
    }

    pub(super) fn root_identity(&self) -> windows::core::Result<usize> {
        // Cache a fully initialized identity, or its initialization error. Never
        // publish an interface whose nullable ABI bridge is only half installed.
        self.state
            .root
            .get_or_init(|| {
                let root: IRawElementProviderFragmentRoot = SettingsAutomationRootProvider {
                    state: Arc::downgrade(&self.state),
                }
                .into();
                install_root_vtables(&root).map_err(|error| error.code())?;
                Ok(root.into_raw() as usize)
            })
            .map_err(windows::core::Error::from_hresult)
    }

    pub(super) fn root_fragment(&self) -> windows::core::Result<IRawElementProviderFragmentRoot> {
        let raw = self.root_identity()? as *mut core::ffi::c_void;
        // SAFETY: AutomationState owns this retained reference until its Drop;
        // `self` keeps that state alive. Clone obtains the caller's own reference.
        unsafe { IRawElementProviderFragmentRoot::from_raw_borrowed(&raw) }
            .cloned()
            .ok_or_else(element_unavailable_error)
    }

    pub(crate) fn provider_for(
        &self,
        id: ElementId,
    ) -> windows::core::Result<IRawElementProviderSimple> {
        let provider: IRawElementProviderSimple = SettingsAutomationNodeProvider {
            state: Arc::downgrade(&self.state),
            node: id,
        }
        .into();
        install_node_vtables(&provider)?;
        Ok(provider)
    }

    pub(crate) fn handle_get_object(
        &self,
        hwnd: HWND,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<windows::Win32::Foundation::LRESULT> {
        if lparam.0 != UiaRootObjectId as isize {
            return None;
        }
        let provider = match self.root_provider() {
            Ok(provider) => provider,
            Err(error) => {
                crate::warn_!("accessibility root provider unavailable: {error}");
                return None;
            }
        };
        Some(unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &provider) })
    }
}
