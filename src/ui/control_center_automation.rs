//! Custom UI Automation surface for the owner-drawn Control Center window.
//!
//! The provider never reaches into Control Center UI state. Reads use the last
//! published immutable snapshot; actions are queued back to the Control Center
//! HWND. Snapshot changes queue typed notifications, and only a later Control
//! Center flush crosses the external UI Automation boundary.

#![allow(non_upper_case_globals)]

use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};

use windows::core::{implement, IUnknown, IUnknownImpl, IUnknown_Vtbl, Interface, BSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Variant::{
    InitVariantFromDoubleArray, InitVariantFromInt32Array, VariantClear, VARIANT,
};
use windows::Win32::UI::Accessibility::{
    IInvokeProvider, IInvokeProvider_Impl, IRangeValueProvider, IRangeValueProvider_Impl,
    IRawElementProviderFragment, IRawElementProviderFragmentRoot,
    IRawElementProviderFragmentRoot_Impl, IRawElementProviderFragmentRoot_Vtbl,
    IRawElementProviderFragment_Impl, IRawElementProviderFragment_Vtbl, IRawElementProviderSimple,
    IRawElementProviderSimple_Impl, IRawElementProviderSimple_Vtbl, ISelectionItemProvider,
    ISelectionItemProvider_Impl, IToggleProvider, IToggleProvider_Impl, IValueProvider,
    IValueProvider_Impl, NavigateDirection, NavigateDirection_FirstChild,
    NavigateDirection_LastChild, NavigateDirection_NextSibling, NavigateDirection_Parent,
    NavigateDirection_PreviousSibling, ProviderOptions, ProviderOptions_ProviderOwnsSetFocus,
    ProviderOptions_ServerSideProvider, ToggleState, ToggleState_Off, ToggleState_On,
    UIA_AutomationFocusChangedEventId, UIA_AutomationIdPropertyId, UIA_BoundingRectanglePropertyId,
    UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_HasKeyboardFocusPropertyId,
    UIA_HelpTextPropertyId, UIA_InvokePatternId, UIA_Invoke_InvokedEventId,
    UIA_IsContentElementPropertyId, UIA_IsControlElementPropertyId, UIA_IsEnabledPropertyId,
    UIA_IsExpandCollapsePatternAvailablePropertyId, UIA_IsInvokePatternAvailablePropertyId,
    UIA_IsKeyboardFocusablePropertyId, UIA_IsOffscreenPropertyId,
    UIA_IsRangeValuePatternAvailablePropertyId, UIA_IsSelectionItemPatternAvailablePropertyId,
    UIA_IsTogglePatternAvailablePropertyId, UIA_IsValuePatternAvailablePropertyId,
    UIA_NamePropertyId, UIA_ProviderDescriptionPropertyId, UIA_RadioButtonControlTypeId,
    UIA_RangeValueLargeChangePropertyId, UIA_RangeValueMaximumPropertyId,
    UIA_RangeValueMinimumPropertyId, UIA_RangeValuePatternId, UIA_RangeValueSmallChangePropertyId,
    UIA_RangeValueValuePropertyId, UIA_SelectionItemIsSelectedPropertyId,
    UIA_SelectionItemPatternId, UIA_SliderControlTypeId, UIA_TogglePatternId,
    UIA_ToggleToggleStatePropertyId, UIA_ValueIsReadOnlyPropertyId, UIA_ValuePatternId,
    UIA_ValueValuePropertyId, UiaAppendRuntimeId, UiaRaiseAutomationEvent,
    UiaRaiseAutomationPropertyChangedEvent, UiaRect, UiaReturnRawElementProvider, UiaRootObjectId,
    UIA_E_ELEMENTNOTAVAILABLE, UIA_E_ELEMENTNOTENABLED, UIA_E_INVALIDOPERATION, UIA_E_NOTSUPPORTED,
    UIA_PATTERN_ID, UIA_PROPERTY_ID,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::ui::layout::{ElementId, ElementKind, Rect as UiRect, SettingsLayout};

pub(crate) const WM_APP_SETTINGS_AUTOMATION: u32 = WM_APP + 6;
pub(crate) const WM_APP_SETTINGS_AUTOMATION_EVENTS: u32 = WM_APP + 7;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AutomationRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for AutomationRect {
    fn default() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

impl AutomationRect {
    fn from_ui(rect: UiRect, scale: f64, origin: POINT) -> Self {
        Self {
            left: origin.x as f64 + rect.x as f64 * scale,
            top: origin.y as f64 + rect.y as f64 * scale,
            width: rect.w as f64 * scale,
            height: rect.h as f64 * scale,
        }
    }

    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.left && y >= self.top && x < self.left + self.width && y < self.top + self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AutomationRange {
    pub value: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub small_change: f64,
    pub large_change: f64,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SettingsAutomationNode {
    pub id: ElementId,
    pub name: String,
    pub help_text: String,
    pub enabled: bool,
    pub focused: bool,
    pub offscreen: bool,
    pub bounds: AutomationRect,
    pub kind: ElementKind,
    pub value: String,
    pub toggle: Option<bool>,
    pub range: Option<AutomationRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AutomationFocusOwner {
    Settings,
    Picker,
    #[default]
    Outside,
}

impl SettingsAutomationSnapshot {
    pub(crate) fn set_focus_state(
        &mut self,
        focus_owner: AutomationFocusOwner,
        picker_open_for: Option<ElementId>,
    ) {
        self.focus_owner = focus_owner;
        self.picker_open_for = picker_open_for;
        self.focused = self
            .focused
            .filter(|id| self.nodes.iter().any(|node| node.id == *id && node.enabled));
        for node in &mut self.nodes {
            node.focused =
                focus_owner == AutomationFocusOwner::Settings && self.focused == Some(node.id);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SettingsAutomationSnapshot {
    pub window: AutomationRect,
    pub nodes: Vec<SettingsAutomationNode>,
    pub focused: Option<ElementId>,
    pub focus_owner: AutomationFocusOwner,
    pub picker_open_for: Option<ElementId>,
    pub page: crate::ui::navigation::Page,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutomationTarget {
    Root,
    Node(ElementId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutomationNotificationKind {
    FocusChanged,
    Invoked,
    Property(i32),
}

#[derive(Debug, Clone, PartialEq)]
enum AutomationValue {
    Empty,
    Bool(bool),
    I32(i32),
    F64(f64),
    String(String),
    Rect(AutomationRect),
}

#[derive(Debug, Clone, PartialEq)]
struct AutomationNotification {
    target: AutomationTarget,
    kind: AutomationNotificationKind,
    old_value: AutomationValue,
    new_value: AutomationValue,
}

impl AutomationNotification {
    fn focus(target: AutomationTarget) -> Self {
        Self {
            target,
            kind: AutomationNotificationKind::FocusChanged,
            old_value: AutomationValue::Empty,
            new_value: AutomationValue::Empty,
        }
    }

    fn invoked(id: ElementId) -> Self {
        Self {
            target: AutomationTarget::Node(id),
            kind: AutomationNotificationKind::Invoked,
            old_value: AutomationValue::Empty,
            new_value: AutomationValue::Empty,
        }
    }

    fn property(
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

#[derive(Default)]
struct PendingAutomationEvents {
    notifications: Vec<AutomationNotification>,
    flush_posted: bool,
}

struct AutomationState {
    snapshot: RwLock<SettingsAutomationSnapshot>,
    actions: Mutex<Vec<SettingsAutomationAction>>,
    pending: Mutex<PendingAutomationEvents>,
    hwnd: isize,
    root: OnceLock<usize>,
}

impl Drop for AutomationState {
    fn drop(&mut self) {
        if let Some(raw) = self.root.get().copied() {
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
    state: Arc<AutomationState>,
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

    fn queue_notifications<I>(&self, notifications: I)
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
            if result.is_err() {
                let mut pending = self
                    .state
                    .pending
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                pending.flush_posted = false;
            }
        }
    }

    fn take_pending_notifications(&self) -> Vec<AutomationNotification> {
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
    fn flush_pending_events_with<F>(&self, mut sink: F)
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

    fn emit_notification(&self, notification: AutomationNotification) {
        let provider = match notification.target {
            AutomationTarget::Root => self.root_provider(),
            AutomationTarget::Node(id) => self.provider_for(id),
        };
        match notification.kind {
            AutomationNotificationKind::FocusChanged => unsafe {
                let _ = UiaRaiseAutomationEvent(&provider, UIA_AutomationFocusChangedEventId);
            },
            AutomationNotificationKind::Invoked => unsafe {
                let _ = UiaRaiseAutomationEvent(&provider, UIA_Invoke_InvokedEventId);
            },
            AutomationNotificationKind::Property(property) => {
                let mut old_value = match notification.old_value.to_variant() {
                    Ok(value) => value,
                    Err(_) => return,
                };
                let mut new_value = match notification.new_value.to_variant() {
                    Ok(value) => value,
                    Err(_) => {
                        unsafe {
                            let _ = VariantClear(&mut old_value);
                        }
                        return;
                    }
                };
                unsafe {
                    let _ = UiaRaiseAutomationPropertyChangedEvent(
                        &provider,
                        UIA_PROPERTY_ID(property),
                        &old_value,
                        &new_value,
                    );
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

    fn hwnd(&self) -> HWND {
        HWND(self.state.hwnd as *mut _)
    }

    pub(crate) fn root_provider(&self) -> IRawElementProviderSimple {
        self.root_fragment()
            .cast()
            .expect("root provider implements IRawElementProviderSimple")
    }

    fn root_identity(&self) -> usize {
        *self.state.root.get_or_init(|| {
            let root: IRawElementProviderFragmentRoot = SettingsAutomationRootProvider {
                state: Arc::downgrade(&self.state),
            }
            .into();
            install_root_vtables(&root);
            let raw = root.as_raw() as usize;
            std::mem::forget(root);
            raw
        })
    }

    fn root_fragment(&self) -> IRawElementProviderFragmentRoot {
        let raw = self.root_identity() as *mut core::ffi::c_void;
        let cached = unsafe { IRawElementProviderFragmentRoot::from_raw(raw) };
        let result = cached.clone();
        std::mem::forget(cached);
        result
    }

    pub(crate) fn provider_for(&self, id: ElementId) -> IRawElementProviderSimple {
        let provider: IRawElementProviderSimple = SettingsAutomationNodeProvider {
            state: Arc::downgrade(&self.state),
            node: id,
        }
        .into();
        install_node_vtables(&provider);
        provider
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
        let provider = self.root_provider();
        Some(unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &provider) })
    }
}

pub(crate) fn snapshot_from_settings(
    hwnd: HWND,
    layout: &SettingsLayout,
    values: &[(ElementId, String, bool, f32)],
    focused: Option<ElementId>,
    dpi: u32,
) -> SettingsAutomationSnapshot {
    let scale = dpi.max(96) as f64 / 96.0;
    let mut origin = POINT::default();
    unsafe {
        let _ = ClientToScreen(hwnd, &mut origin);
    }
    let window = AutomationRect {
        left: origin.x as f64,
        top: origin.y as f64,
        width: layout.width as f64 * scale,
        height: layout.height as f64 * scale,
    };
    let nodes = layout
        .elements
        .iter()
        .filter_map(|element| {
            let (_, value, enabled, ratio) =
                values.iter().find(|(id, _, _, _)| *id == element.id)?;
            let clipped = clip_rect(element.rect, layout.content_clip, element.scrolls);
            let offscreen = clipped.is_none();
            let bounds = AutomationRect::from_ui(clipped.unwrap_or(element.rect), scale, origin);
            let toggle = matches!(
                element.kind,
                ElementKind::Toggle | ElementKind::Checkbox | ElementKind::Choice
            )
            .then(|| value == "On" || value == "Selected");
            let range = slider_range(element.id, element.kind, *ratio);
            Some(SettingsAutomationNode {
                id: element.id,
                name: element.label.to_string(),
                help_text: element.description.to_string(),
                enabled: *enabled,
                focused: focused == Some(element.id) && *enabled,
                offscreen,
                bounds,
                kind: element.kind,
                value: value.clone(),
                toggle,
                range,
            })
        })
        .collect();
    SettingsAutomationSnapshot {
        window,
        nodes,
        focused,
        focus_owner: if focused.is_some() {
            AutomationFocusOwner::Settings
        } else {
            AutomationFocusOwner::Outside
        },
        picker_open_for: None,
        page: layout.page,
    }
}

pub(crate) fn clip_rect(rect: UiRect, viewport: UiRect, scrolls: bool) -> Option<UiRect> {
    if !scrolls {
        return Some(rect);
    }
    let left = rect.x.max(viewport.x);
    let top = rect.y.max(viewport.y);
    let right = rect.right().min(viewport.right());
    let bottom = rect.bottom().min(viewport.bottom());
    (right > left && bottom > top).then_some(UiRect::new(left, top, right - left, bottom - top))
}

fn slider_range(id: ElementId, kind: ElementKind, ratio: f32) -> Option<AutomationRange> {
    if kind != ElementKind::Slider {
        return None;
    }
    let (minimum, maximum, small_change, large_change) = match id {
        ElementId::OverlayDuration => (500.0, 10_000.0, 100.0, 500.0),
        ElementId::OverlayOpacity => (0.3, 1.0, 0.05, 0.25),
        ElementId::OverlayScale => (0.7, 1.6, 0.1, 0.5),
        _ => return None,
    };
    Some(AutomationRange {
        value: minimum + (maximum - minimum) * ratio.clamp(0.0, 1.0) as f64,
        minimum,
        maximum,
        small_change,
        large_change,
        read_only: false,
    })
}

fn invalid_argument<T>() -> windows::core::Result<T> {
    Err(windows::core::Error::from_hresult(windows::core::HRESULT(
        0x80070057u32 as i32,
    )))
}
fn element_not_enabled<T>() -> windows::core::Result<T> {
    Err(element_not_enabled_error())
}

fn unsupported<T>() -> windows::core::Result<T> {
    Err(unsupported_error())
}

fn no_nullable_result_error() -> windows::core::Error {
    // Internal sentinel for the generated high-level trait; raw nullable
    // thunks consume None directly and never expose this HRESULT.
    windows::core::Error::from_hresult(windows::core::HRESULT(0x80004002u32 as i32))
}

fn require_nullable_result<T>(
    result: windows::core::Result<Option<T>>,
) -> windows::core::Result<T> {
    result.and_then(|value| value.ok_or_else(no_nullable_result_error))
}

unsafe fn write_nullable_interface<T, F>(
    pretval: *mut *mut core::ffi::c_void,
    produce: F,
) -> windows::core::HRESULT
where
    T: Interface,
    F: FnOnce() -> windows::core::Result<Option<T>>,
{
    if pretval.is_null() {
        return windows::core::HRESULT(0x80004003u32 as i32);
    }
    unsafe {
        pretval.write(std::ptr::null_mut());
    }
    match produce() {
        Ok(Some(value)) => {
            unsafe {
                pretval.write(value.into_raw());
            }
            windows::core::HRESULT(0)
        }
        Ok(None) => windows::core::HRESULT(0),
        Err(error) => error.into(),
    }
}

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
impl AutomationValue {
    fn to_variant(&self) -> windows::core::Result<VARIANT> {
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

fn runtime_id_variant(values: &[i32]) -> windows::core::Result<*mut SAFEARRAY> {
    let variant = unsafe { InitVariantFromInt32Array(values)? };
    let variant = ManuallyDrop::new(variant);
    let array = unsafe { variant.Anonymous.Anonymous.Anonymous.parray };
    Ok(array)
}
fn optional_i32_value(value: Option<i32>) -> AutomationValue {
    value.map_or(AutomationValue::Empty, AutomationValue::I32)
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

fn snapshot_notifications(
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
fn node_has_value(kind: ElementKind) -> bool {
    // Picker triggers and hotkey actions expose their current display value;
    // Search is the one editable ValuePattern in the custom shell.
    matches!(
        kind,
        ElementKind::Value | ElementKind::Hotkey | ElementKind::Search
    )
}

fn control_type(kind: ElementKind) -> i32 {
    match kind {
        ElementKind::Toggle | ElementKind::Checkbox => UIA_CheckBoxControlTypeId.0,
        ElementKind::Choice => UIA_RadioButtonControlTypeId.0,
        ElementKind::Slider => UIA_SliderControlTypeId.0,
        ElementKind::Search => UIA_EditControlTypeId.0,
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
    format!("WinShort.ControlCenter.{id:?}")
}

fn element_unavailable_error() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}

fn element_not_enabled_error() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(UIA_E_ELEMENTNOTENABLED as i32))
}

fn unsupported_error() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(UIA_E_NOTSUPPORTED as i32))
}

fn upgrade_state(state: &Weak<AutomationState>) -> windows::core::Result<Arc<AutomationState>> {
    state.upgrade().ok_or_else(element_unavailable_error)
}
fn root_property_value(
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

fn node_property_value(
    node: &SettingsAutomationNode,
    propertyid: UIA_PROPERTY_ID,
) -> windows::core::Result<VARIANT> {
    match propertyid {
        UIA_NamePropertyId => Ok(string_variant(&node.name)),
        UIA_HelpTextPropertyId => Ok(string_variant(&node.help_text)),
        UIA_ControlTypePropertyId => Ok(i32_variant(control_type(node.kind))),
        UIA_IsEnabledPropertyId => Ok(bool_variant(node.enabled)),
        UIA_IsKeyboardFocusablePropertyId => Ok(bool_variant(node.enabled)),
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

impl SettingsAutomationNode {
    fn is_value_pattern_available(&self) -> bool {
        node_has_value(self.kind)
    }
}

#[derive(Clone)]
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot
)]
struct SettingsAutomationRootProvider {
    state: Weak<AutomationState>,
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
struct SettingsAutomationNodeProvider {
    state: Weak<AutomationState>,
    node: ElementId,
}
static ROOT_SIMPLE_VTABLE: OnceLock<IRawElementProviderSimple_Vtbl> = OnceLock::new();
static ROOT_FRAGMENT_VTABLE: OnceLock<IRawElementProviderFragment_Vtbl> = OnceLock::new();
static ROOT_FRAGMENT_ROOT_VTABLE: OnceLock<IRawElementProviderFragmentRoot_Vtbl> = OnceLock::new();
static NODE_SIMPLE_VTABLE: OnceLock<IRawElementProviderSimple_Vtbl> = OnceLock::new();
static NODE_FRAGMENT_VTABLE: OnceLock<IRawElementProviderFragment_Vtbl> = OnceLock::new();

fn copy_iunknown_vtable(source: &IUnknown_Vtbl) -> IUnknown_Vtbl {
    IUnknown_Vtbl {
        QueryInterface: source.QueryInterface,
        AddRef: source.AddRef,
        Release: source.Release,
    }
}

unsafe fn replace_vtable<I: Interface>(interface: &I, replacement: &'static I::Vtable) {
    unsafe {
        (interface.as_raw() as *mut *const I::Vtable).write(replacement);
    }
}

unsafe extern "system" fn root_simple_get_pattern_provider(
    this: *mut core::ffi::c_void,
    patternid: UIA_PATTERN_ID,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-1) as *const SettingsAutomationRootProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.pattern_provider_result(patternid)) }
}

unsafe extern "system" fn root_fragment_navigate(
    this: *mut core::ffi::c_void,
    direction: NavigateDirection,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-2) as *const SettingsAutomationRootProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.navigate_result(direction)) }
}

unsafe extern "system" fn root_element_provider_from_point(
    this: *mut core::ffi::c_void,
    x: f64,
    y: f64,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-3) as *const SettingsAutomationRootProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.element_provider_from_point_result(x, y)) }
}

unsafe extern "system" fn root_get_focus(
    this: *mut core::ffi::c_void,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-3) as *const SettingsAutomationRootProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.focus_result()) }
}

unsafe extern "system" fn node_simple_get_pattern_provider(
    this: *mut core::ffi::c_void,
    patternid: UIA_PATTERN_ID,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-1) as *const SettingsAutomationNodeProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.pattern_provider_result(patternid)) }
}

unsafe extern "system" fn node_simple_host_raw_element_provider(
    this: *mut core::ffi::c_void,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-1) as *const SettingsAutomationNodeProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.host_raw_element_provider_result()) }
}

unsafe extern "system" fn node_fragment_navigate(
    this: *mut core::ffi::c_void,
    direction: NavigateDirection,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-2) as *const SettingsAutomationNodeProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.navigate_result(direction)) }
}

fn root_simple_vtable(
    original: &IRawElementProviderSimple_Vtbl,
) -> &'static IRawElementProviderSimple_Vtbl {
    ROOT_SIMPLE_VTABLE.get_or_init(|| IRawElementProviderSimple_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        ProviderOptions: original.ProviderOptions,
        GetPatternProvider: root_simple_get_pattern_provider,
        GetPropertyValue: original.GetPropertyValue,
        HostRawElementProvider: original.HostRawElementProvider,
    })
}

fn root_fragment_vtable(
    original: &IRawElementProviderFragment_Vtbl,
) -> &'static IRawElementProviderFragment_Vtbl {
    ROOT_FRAGMENT_VTABLE.get_or_init(|| IRawElementProviderFragment_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        Navigate: root_fragment_navigate,
        GetRuntimeId: original.GetRuntimeId,
        BoundingRectangle: original.BoundingRectangle,
        GetEmbeddedFragmentRoots: original.GetEmbeddedFragmentRoots,
        SetFocus: original.SetFocus,
        FragmentRoot: original.FragmentRoot,
    })
}

fn root_fragment_root_vtable(
    original: &IRawElementProviderFragmentRoot_Vtbl,
) -> &'static IRawElementProviderFragmentRoot_Vtbl {
    ROOT_FRAGMENT_ROOT_VTABLE.get_or_init(|| IRawElementProviderFragmentRoot_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        ElementProviderFromPoint: root_element_provider_from_point,
        GetFocus: root_get_focus,
    })
}

fn node_simple_vtable(
    original: &IRawElementProviderSimple_Vtbl,
) -> &'static IRawElementProviderSimple_Vtbl {
    NODE_SIMPLE_VTABLE.get_or_init(|| IRawElementProviderSimple_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        ProviderOptions: original.ProviderOptions,
        GetPatternProvider: node_simple_get_pattern_provider,
        GetPropertyValue: original.GetPropertyValue,
        HostRawElementProvider: node_simple_host_raw_element_provider,
    })
}

fn node_fragment_vtable(
    original: &IRawElementProviderFragment_Vtbl,
) -> &'static IRawElementProviderFragment_Vtbl {
    NODE_FRAGMENT_VTABLE.get_or_init(|| IRawElementProviderFragment_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        Navigate: node_fragment_navigate,
        GetRuntimeId: original.GetRuntimeId,
        BoundingRectangle: original.BoundingRectangle,
        GetEmbeddedFragmentRoots: original.GetEmbeddedFragmentRoots,
        SetFocus: original.SetFocus,
        FragmentRoot: original.FragmentRoot,
    })
}

fn install_root_vtables(root: &IRawElementProviderFragmentRoot) {
    let simple: IRawElementProviderSimple = root.cast().expect("root simple provider");
    let fragment: IRawElementProviderFragment = root.cast().expect("root fragment provider");
    let simple_vtable = root_simple_vtable(simple.vtable());
    let fragment_vtable = root_fragment_vtable(fragment.vtable());
    let root_vtable = root_fragment_root_vtable(root.vtable());
    unsafe {
        replace_vtable(&simple, simple_vtable);
        replace_vtable(&fragment, fragment_vtable);
        replace_vtable(root, root_vtable);
    }
}

fn install_node_vtables(simple: &IRawElementProviderSimple) {
    let fragment: IRawElementProviderFragment = simple.cast().expect("node fragment provider");
    let simple_vtable = node_simple_vtable(simple.vtable());
    let fragment_vtable = node_fragment_vtable(fragment.vtable());
    unsafe {
        replace_vtable(simple, simple_vtable);
        replace_vtable(&fragment, fragment_vtable);
    }
}

impl SettingsAutomationRootProvider_Impl {
    fn snapshot(&self) -> windows::core::Result<SettingsAutomationSnapshot> {
        Ok(upgrade_state(&self.state)?
            .snapshot
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone())
    }

    fn automation(&self) -> windows::core::Result<SettingsAutomation> {
        Ok(SettingsAutomation {
            state: upgrade_state(&self.state)?,
        })
    }

    fn hwnd(&self) -> windows::core::Result<HWND> {
        Ok(HWND(upgrade_state(&self.state)?.hwnd as *mut _))
    }

    fn child_fragment(&self, id: ElementId) -> windows::core::Result<IRawElementProviderFragment> {
        let _ = upgrade_state(&self.state)?;
        let provider: IRawElementProviderFragment = SettingsAutomationNodeProvider {
            state: self.state.clone(),
            node: id,
        }
        .into();
        let simple: IRawElementProviderSimple = provider.cast().expect("node simple provider");
        install_node_vtables(&simple);
        Ok(provider)
    }

    fn pattern_provider_result(
        &self,
        _patternid: UIA_PATTERN_ID,
    ) -> windows::core::Result<Option<IUnknown>> {
        let _ = self.snapshot()?;
        Ok(None)
    }

    fn navigate_result(
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

    fn element_provider_from_point_result(
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

    fn focus_result(&self) -> windows::core::Result<Option<IRawElementProviderFragment>> {
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

impl SettingsAutomationNodeProvider_Impl {
    fn snapshot(&self) -> windows::core::Result<SettingsAutomationSnapshot> {
        Ok(upgrade_state(&self.state)?
            .snapshot
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone())
    }

    fn automation(&self) -> windows::core::Result<SettingsAutomation> {
        Ok(SettingsAutomation {
            state: upgrade_state(&self.state)?,
        })
    }
    fn node(&self) -> windows::core::Result<SettingsAutomationNode> {
        self.snapshot()?
            .nodes
            .into_iter()
            .find(|node| node.id == self.node)
            .ok_or_else(element_unavailable_error)
    }

    fn node_index(&self) -> windows::core::Result<usize> {
        self.snapshot()?
            .nodes
            .iter()
            .position(|node| node.id == self.node)
            .ok_or_else(element_unavailable_error)
    }

    fn self_simple(&self) -> IRawElementProviderSimple {
        let provider: IRawElementProviderSimple = SettingsAutomationNodeProvider {
            state: self.state.clone(),
            node: self.node,
        }
        .into();
        install_node_vtables(&provider);
        provider
    }

    fn child_fragment(&self, id: ElementId) -> IRawElementProviderFragment {
        let provider: IRawElementProviderFragment = SettingsAutomationNodeProvider {
            state: self.state.clone(),
            node: id,
        }
        .into();
        let simple: IRawElementProviderSimple = provider.cast().expect("node simple provider");
        install_node_vtables(&simple);
        provider
    }

    fn pattern_provider_result(
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
        self.self_simple().cast().map(Some)
    }

    fn host_raw_element_provider_result(
        &self,
    ) -> windows::core::Result<Option<IRawElementProviderSimple>> {
        let _ = self.node()?;
        Ok(None)
    }

    fn navigate_result(
        &self,
        direction: NavigateDirection,
    ) -> windows::core::Result<Option<IRawElementProviderFragment>> {
        let _ = self.node()?;
        let snapshot = self.snapshot()?;
        let index = self.node_index()?;
        let next = match direction {
            NavigateDirection_Parent => {
                return self.automation()?.root_fragment().cast().map(Some);
            }
            NavigateDirection_NextSibling => snapshot.nodes.get(index + 1).map(|node| node.id),
            NavigateDirection_PreviousSibling => index
                .checked_sub(1)
                .and_then(|index| snapshot.nodes.get(index))
                .map(|node| node.id),
            _ => None,
        };
        Ok(next.map(|id| self.child_fragment(id)))
    }
}

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
        self.automation()?
            .enqueue(SettingsAutomationAction::SetFocus(self.node))
    }

    fn FragmentRoot(&self) -> windows::core::Result<IRawElementProviderFragmentRoot> {
        let _ = self.node()?;
        Ok(self.automation()?.root_fragment())
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
        Ok(self.automation()?.root_provider())
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Ole::{
        SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetLBound,
        SafeArrayGetUBound,
    };
    use windows::Win32::System::Variant::{VT_BOOL, VT_BSTR, VT_EMPTY};
    use windows::Win32::UI::Accessibility::UIA_PATTERN_ID;
    use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

    fn assert_raw_null_pattern(provider: &IRawElementProviderSimple, pattern: UIA_PATTERN_ID) {
        let mut returned = std::ptr::dangling_mut();
        let hr = unsafe {
            (provider.vtable().GetPatternProvider)(provider.as_raw(), pattern, &mut returned)
        };
        assert_eq!(hr, windows::core::HRESULT(0));
        assert!(returned.is_null());
    }

    fn assert_raw_null_navigation(
        provider: &IRawElementProviderFragment,
        direction: NavigateDirection,
    ) {
        let mut returned = std::ptr::dangling_mut();
        let hr =
            unsafe { (provider.vtable().Navigate)(provider.as_raw(), direction, &mut returned) };
        assert_eq!(hr, windows::core::HRESULT(0));
        assert!(returned.is_null());
    }

    fn assert_raw_null_host(provider: &IRawElementProviderSimple) {
        let mut returned = std::ptr::dangling_mut();
        let hr =
            unsafe { (provider.vtable().HostRawElementProvider)(provider.as_raw(), &mut returned) };
        assert_eq!(hr, windows::core::HRESULT(0));
        assert!(returned.is_null());
    }

    fn assert_raw_null_point(provider: &IRawElementProviderFragmentRoot, x: f64, y: f64) {
        let mut returned = std::ptr::dangling_mut();
        let hr = unsafe {
            (provider.vtable().ElementProviderFromPoint)(provider.as_raw(), x, y, &mut returned)
        };
        assert_eq!(hr, windows::core::HRESULT(0));
        assert!(returned.is_null());
    }

    fn assert_raw_null_focus(provider: &IRawElementProviderFragmentRoot) {
        let mut returned = std::ptr::dangling_mut();
        let hr = unsafe { (provider.vtable().GetFocus)(provider.as_raw(), &mut returned) };
        assert_eq!(hr, windows::core::HRESULT(0));
        assert!(returned.is_null());
    }

    fn read_runtime_id(fragment: &IRawElementProviderFragment) -> Vec<i32> {
        let array = unsafe { fragment.GetRuntimeId().expect("runtime id") };
        assert!(!array.is_null());
        assert_eq!(unsafe { SafeArrayGetDim(array) }, 1);
        let lower = unsafe { SafeArrayGetLBound(array, 1).expect("runtime lower bound") };
        let upper = unsafe { SafeArrayGetUBound(array, 1).expect("runtime upper bound") };
        let mut values = Vec::new();
        for index in lower..=upper {
            let mut value = 0i32;
            unsafe {
                SafeArrayGetElement(array, &index, (&mut value as *mut i32).cast())
                    .expect("runtime element");
            }
            values.push(value);
        }
        unsafe { SafeArrayDestroy(array).expect("runtime array destroy") };
        values
    }
    fn raw_property(
        provider: &IRawElementProviderSimple,
        property: UIA_PROPERTY_ID,
    ) -> (windows::core::HRESULT, VARIANT) {
        let mut value = VARIANT::default();
        let hr = unsafe {
            (provider.vtable().GetPropertyValue)(provider.as_raw(), property, &mut value)
        };
        (hr, value)
    }

    fn assert_raw_empty_property(provider: &IRawElementProviderSimple, property: UIA_PROPERTY_ID) {
        let (hr, value) = raw_property(provider, property);
        assert_eq!(hr, windows::core::HRESULT(0));
        assert_eq!(unsafe { value.Anonymous.Anonymous.vt }, VT_EMPTY);
    }

    fn assert_raw_variant_type(
        provider: &IRawElementProviderSimple,
        property: UIA_PROPERTY_ID,
        expected: windows::Win32::System::Variant::VARENUM,
    ) -> VARIANT {
        let (hr, value) = raw_property(provider, property);
        assert_eq!(hr, windows::core::HRESULT(0));
        assert_eq!(unsafe { value.Anonymous.Anonymous.vt }, expected);
        value
    }

    fn iunknown_identity<T: Interface>(interface: &T) -> *mut core::ffi::c_void {
        let unknown: IUnknown = interface.cast().expect("IUnknown");
        unknown.as_raw()
    }

    fn layout() -> SettingsLayout {
        SettingsLayout::build(610.0, 720.0, 64.0)
    }

    fn values() -> Vec<(ElementId, String, bool, f32)> {
        ElementId::FOCUS_ORDER
            .into_iter()
            .map(|id| (id, String::from("Off"), true, 0.5))
            .collect()
    }

    fn automation_with_disabled(id: ElementId) -> SettingsAutomation {
        let automation = published_automation();
        let mut snapshot = automation.snapshot();
        snapshot
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
            .expect("node")
            .enabled = false;
        automation.publish(snapshot);
        automation
    }

    fn stale_provider(id: ElementId) -> IRawElementProviderSimple {
        let automation = published_automation();
        let provider = automation.provider_for(id);
        drop(automation);
        provider
    }

    fn assert_hresult(error: windows::core::Error, expected: u32) {
        assert_eq!(
            error.code(),
            windows::core::HRESULT(expected as i32),
            "unexpected HRESULT"
        );
    }

    fn published_automation() -> SettingsAutomation {
        published_automation_for(HWND(std::ptr::null_mut()))
    }

    fn published_automation_for(hwnd: HWND) -> SettingsAutomation {
        let automation = SettingsAutomation::new(hwnd);
        automation.publish(snapshot_from_settings(hwnd, &layout(), &values(), None, 96));
        automation
    }

    #[test]
    fn snapshot_contains_logical_nodes_without_child_windows() {
        let snapshot = snapshot_from_settings(
            HWND(std::ptr::null_mut()),
            &layout(),
            &values(),
            Some(ElementId::OverlayEnabled),
            96,
        );
        assert_eq!(snapshot.nodes.len(), ElementId::FOCUS_ORDER.len());
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .find(|node| node.id == ElementId::OverlayEnabled)
                .and_then(|node| node.toggle),
            Some(false)
        );
        assert!(snapshot
            .nodes
            .iter()
            .all(|node| node.bounds.width > 0.0 && node.bounds.height > 0.0));
    }

    #[test]
    fn phase_one_hotkey_nodes_expose_names_and_help_text() {
        let snapshot =
            snapshot_from_settings(HWND(std::ptr::null_mut()), &layout(), &values(), None, 96);
        let expected = [
            (
                ElementId::CycleInputHotkey,
                "Next microphone",
                "Switch to the next selected microphone",
            ),
            (
                ElementId::CycleOutputHotkey,
                "Next speaker",
                "Switch to the next selected speaker",
            ),
            (
                ElementId::ForegroundVolumeUpHotkey,
                "Current app volume up",
                "Raise the current app by five percent",
            ),
            (
                ElementId::ForegroundVolumeDownHotkey,
                "Current app volume down",
                "Lower the current app by five percent",
            ),
        ];
        for (id, name, help_text) in expected {
            let node = snapshot
                .nodes
                .iter()
                .find(|node| node.id == id)
                .expect("node");
            assert_eq!(node.name, name);
            assert_eq!(node.help_text, help_text);
            assert!(node.enabled);
        }
    }
    #[test]
    fn snapshot_clips_scrolled_nodes_and_marks_them_offscreen() {
        let snapshot =
            snapshot_from_settings(HWND(std::ptr::null_mut()), &layout(), &values(), None, 144);
        assert!(snapshot.nodes.iter().any(|node| node.offscreen));
        assert!(snapshot.nodes.iter().any(|node| !node.offscreen));
    }

    #[test]
    fn control_types_and_ranges_are_truthful() {
        assert_eq!(
            control_type(ElementKind::Toggle),
            UIA_CheckBoxControlTypeId.0
        );
        assert_eq!(control_type(ElementKind::Slider), UIA_SliderControlTypeId.0);
        assert!(node_has_invoke(ElementKind::Value));
        assert!(!node_has_invoke(ElementKind::Toggle));
        assert!(!node_has_invoke(ElementKind::Slider));
        let ranges = values();
        let snapshot =
            snapshot_from_settings(HWND(std::ptr::null_mut()), &layout(), &ranges, None, 96);
        assert!(snapshot
            .nodes
            .iter()
            .filter(|node| node.kind == ElementKind::Slider)
            .all(|node| node.range.is_some()));
    }

    #[test]
    fn pointer_snapshot_uses_screen_space_origin_when_available() {
        let snapshot =
            snapshot_from_settings(HWND(std::ptr::null_mut()), &layout(), &values(), None, 96);
        assert_eq!(snapshot.window.left, 0.0);
        assert_eq!(snapshot.window.top, 0.0);
    }
    #[test]
    fn automation_rects_are_screen_space_and_dpi_scaled() {
        let rect = AutomationRect::from_ui(
            UiRect::new(10.0, 20.0, 30.0, 40.0),
            1.5,
            POINT { x: 100, y: -20 },
        );
        assert_eq!(rect.left, 115.0);
        assert_eq!(rect.top, 10.0);
        assert_eq!(rect.width, 45.0);
        assert_eq!(rect.height, 60.0);
    }

    #[test]
    fn slider_ranges_expose_model_units() {
        let range = slider_range(ElementId::OverlayOpacity, ElementKind::Slider, 0.5)
            .expect("opacity range");
        assert_eq!(range.minimum, 0.3);
        assert_eq!(range.maximum, 1.0);
        assert!((range.value - 0.65).abs() < f64::EPSILON);
        assert_eq!(range.small_change, 0.05);
        assert_eq!(range.large_change, 0.25);
    }

    #[test]
    fn automation_actions_are_queued_for_later_window_dispatch() {
        let automation = SettingsAutomation::new(unsafe { GetDesktopWindow() });
        automation
            .enqueue(SettingsAutomationAction::Toggle(ElementId::OverlayEnabled))
            .expect("post action");
        let actions = automation.drain_actions();
        assert!(matches!(
            actions.as_slice(),
            [SettingsAutomationAction::Toggle(ElementId::OverlayEnabled)]
        ));
    }
    #[test]
    fn published_provider_exposes_toggle_pattern_and_state() {
        let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
        automation.publish(snapshot_from_settings(
            HWND(std::ptr::null_mut()),
            &layout(),
            &values(),
            None,
            96,
        ));
        let provider = automation.provider_for(ElementId::OverlayEnabled);
        let control_type = unsafe {
            provider
                .GetPropertyValue(UIA_ControlTypePropertyId)
                .expect("control type")
        };
        assert_eq!(
            i32::try_from(&control_type).expect("I4"),
            UIA_CheckBoxControlTypeId.0
        );
        let unknown = unsafe {
            provider
                .GetPatternProvider(UIA_TogglePatternId)
                .expect("toggle pattern")
        };
        let toggle: IToggleProvider = unknown.cast().expect("toggle interface");
        assert_eq!(
            unsafe { toggle.ToggleState().expect("toggle state") },
            ToggleState_Off
        );
    }

    #[test]
    fn published_provider_exposes_selection_item_for_choice() {
        let context = crate::ui::layout::LayoutContext {
            input_device_count: 1,
            output_device_count: 1,
            ..Default::default()
        };
        let layout = SettingsLayout::build_shell_with_context(
            1000.0,
            800.0,
            0.0,
            crate::ui::navigation::Page::Audio,
            "",
            context,
            None,
        );
        let values = layout
            .focus_order()
            .into_iter()
            .map(|id| {
                let value = if id == ElementId::OutputCycleMode(0) {
                    "On"
                } else {
                    "Off"
                }
                .into();
                (id, value, true, 0.0)
            })
            .collect::<Vec<_>>();
        let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
        automation.publish(snapshot_from_settings(
            HWND(std::ptr::null_mut()),
            &layout,
            &values,
            None,
            96,
        ));
        let provider = automation.provider_for(ElementId::OutputCycleMode(0));
        let control_type = unsafe {
            provider
                .GetPropertyValue(UIA_ControlTypePropertyId)
                .expect("control type")
        };
        assert_eq!(
            i32::try_from(&control_type).expect("I4"),
            UIA_RadioButtonControlTypeId.0
        );
        assert_raw_empty_property(&provider, UIA_ToggleToggleStatePropertyId);
        let selection_unknown = unsafe {
            provider
                .GetPatternProvider(UIA_SelectionItemPatternId)
                .expect("selection item pattern")
        };
        let selection: ISelectionItemProvider =
            selection_unknown.cast().expect("selection item interface");
        assert!(unsafe { selection.IsSelected().expect("selection state") }.as_bool());
        let (hr, toggle_available) =
            raw_property(&provider, UIA_IsTogglePatternAvailablePropertyId);
        assert_eq!(hr, windows::core::HRESULT(0));
        assert!(!bool::try_from(&toggle_available).expect("toggle availability"));
    }

    #[test]
    fn published_provider_exposes_slider_range_pattern() {
        let mut input = values();
        if let Some((_, value, _, ratio)) = input
            .iter_mut()
            .find(|(id, _, _, _)| *id == ElementId::OverlayDuration)
        {
            *value = "2.0s".into();
            *ratio = 0.5;
        }
        let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
        automation.publish(snapshot_from_settings(
            HWND(std::ptr::null_mut()),
            &layout(),
            &input,
            None,
            96,
        ));
        let provider = automation.provider_for(ElementId::OverlayDuration);
        let unknown = unsafe {
            provider
                .GetPatternProvider(UIA_RangeValuePatternId)
                .expect("range pattern")
        };
        let range: IRangeValueProvider = unknown.cast().expect("range interface");
        assert_eq!(unsafe { range.Minimum().expect("minimum") }, 500.0);
        assert_eq!(unsafe { range.Maximum().expect("maximum") }, 10_000.0);
        assert_eq!(unsafe { range.Value().expect("value") }, 5_250.0);
    }
    #[test]
    fn root_focus_returns_logical_child_fragment() {
        let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
        let mut snapshot = snapshot_from_settings(
            HWND(std::ptr::null_mut()),
            &layout(),
            &values(),
            Some(ElementId::OverlayEnabled),
            96,
        );
        snapshot.focused = Some(ElementId::OverlayEnabled);
        automation.publish(snapshot);
        let root_simple = automation.root_provider();
        let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment");
        let focused = unsafe { root.GetFocus().expect("logical focus") };
        let focused_simple: IRawElementProviderSimple =
            focused.cast().expect("focused simple provider");
        let has_focus = unsafe {
            focused_simple
                .GetPropertyValue(UIA_HasKeyboardFocusPropertyId)
                .expect("focus property")
        };
        assert!(bool::try_from(&has_focus).expect("BOOL variant"));
    }
    #[test]
    fn unsupported_patterns_return_s_ok_and_null() {
        let automation = published_automation();
        let root = automation.root_provider();
        assert_raw_null_pattern(&root, UIA_InvokePatternId);

        let slider = automation.provider_for(ElementId::OverlayDuration);
        assert_raw_null_pattern(&slider, UIA_TogglePatternId);
        assert_raw_null_pattern(&slider, UIA_PATTERN_ID(99_999));
    }

    #[test]
    fn navigation_boundaries_return_s_ok_and_null() {
        let automation = published_automation();
        let root_simple = automation.root_provider();
        let root: IRawElementProviderFragment = root_simple.cast().expect("root fragment");
        assert_raw_null_navigation(&root, NavigateDirection_Parent);

        let first_simple = automation.provider_for(ElementId::FOCUS_ORDER[0]);
        let first: IRawElementProviderFragment = first_simple.cast().expect("first fragment");
        assert_raw_null_navigation(&first, NavigateDirection_PreviousSibling);

        let last_simple = automation.provider_for(*ElementId::FOCUS_ORDER.last().expect("last id"));
        let last: IRawElementProviderFragment = last_simple.cast().expect("last fragment");
        assert_raw_null_navigation(&last, NavigateDirection_NextSibling);
    }

    #[test]
    fn runtime_ids_use_uia_append_runtime_id_and_unique_child_values() {
        let automation = published_automation();
        let root_simple = automation.root_provider();
        let root: IRawElementProviderFragment = root_simple.cast().expect("root fragment");
        assert!(unsafe { root.GetRuntimeId().expect("root runtime id") }.is_null());

        let first_simple = automation.provider_for(ElementId::FOCUS_ORDER[0]);
        let first: IRawElementProviderFragment = first_simple.cast().expect("first fragment");
        let second_simple = automation.provider_for(ElementId::FOCUS_ORDER[1]);
        let second: IRawElementProviderFragment = second_simple.cast().expect("second fragment");
        let first_id = read_runtime_id(&first);
        let second_id = read_runtime_id(&second);
        assert_eq!(first_id, vec![UiaAppendRuntimeId as i32, 1]);
        assert_eq!(second_id, vec![UiaAppendRuntimeId as i32, 2]);
        assert_ne!(first_id, second_id);
    }

    #[test]
    fn host_provider_is_only_returned_for_fragment_root() {
        let automation = published_automation_for(unsafe { GetDesktopWindow() });
        let root = automation.root_provider();
        let host = unsafe { root.HostRawElementProvider() }.expect("root host provider");
        assert!(!host.as_raw().is_null());

        let child = automation.provider_for(ElementId::OverlayEnabled);
        assert_raw_null_host(&child);
    }

    #[test]
    fn nullable_provider_abi_regression() {
        let automation = published_automation_for(unsafe { GetDesktopWindow() });
        let root_simple = automation.root_provider();
        let root_fragment: IRawElementProviderFragment = root_simple.cast().expect("root fragment");
        let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment root");

        // These assertions call the raw ABI directly. The output starts as a
        // dangling sentinel and must be cleared to NULL without constructing
        // or releasing a Rust Interface value.
        assert_raw_null_pattern(&root_simple, UIA_InvokePatternId);
        assert_raw_null_navigation(&root_fragment, NavigateDirection_Parent);
        let child = automation.provider_for(ElementId::OverlayEnabled);
        assert_raw_null_host(&child);
        assert_raw_null_point(&root, -1.0, -1.0);
        assert_raw_null_focus(&root);

        let first = unsafe {
            root_fragment
                .Navigate(NavigateDirection_FirstChild)
                .expect("supported Navigate")
        };
        assert!(!first.as_raw().is_null());
        let host = unsafe {
            root_simple
                .HostRawElementProvider()
                .expect("supported host provider")
        };
        assert!(!host.as_raw().is_null());
        let supported = unsafe {
            child
                .GetPatternProvider(UIA_TogglePatternId)
                .expect("supported Toggle pattern")
        };
        assert!(!supported.as_raw().is_null());
    }

    #[test]
    fn picker_and_hotkey_nodes_are_button_actions() {
        let automation = published_automation();
        for id in [ElementId::InputDevice, ElementId::MicHotkey] {
            let provider = automation.provider_for(id);
            let control_type = unsafe {
                provider
                    .GetPropertyValue(UIA_ControlTypePropertyId)
                    .expect("control type")
            };
            assert_eq!(
                i32::try_from(&control_type).expect("I4"),
                UIA_ButtonControlTypeId.0
            );
            let invoke = unsafe {
                provider
                    .GetPatternProvider(UIA_InvokePatternId)
                    .expect("invoke pattern")
            };
            let _: IInvokeProvider = invoke.cast().expect("invoke interface");
            assert_raw_null_pattern(
                &provider,
                windows::Win32::UI::Accessibility::UIA_ExpandCollapsePatternId,
            );
        }
    }

    #[test]
    fn read_only_value_pattern_rejects_set_value() {
        let automation = published_automation();
        let provider = automation.provider_for(ElementId::InputDevice);
        let unknown = unsafe {
            provider
                .GetPatternProvider(UIA_ValuePatternId)
                .expect("value pattern")
        };
        let value: IValueProvider = unknown.cast().expect("value interface");
        assert!(unsafe { value.IsReadOnly().expect("read-only state") }.as_bool());

        let text = HSTRING::from("must not be accepted");
        let text = PCWSTR(text.as_ptr());
        let error = unsafe { value.SetValue(text) }.expect_err("read-only SetValue");
        assert_eq!(
            error.code(),
            windows::core::HRESULT(UIA_E_INVALIDOPERATION as i32)
        );
    }

    #[test]
    fn point_queries_return_child_root_or_null() {
        let automation = published_automation();
        let root_simple = automation.root_provider();
        let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment root");
        assert_raw_null_point(&root, -1.0, -1.0);

        let background = unsafe {
            root.ElementProviderFromPoint(1.0, 1.0)
                .expect("background fragment")
        };
        let _: IRawElementProviderFragmentRoot =
            background.cast().expect("background is root fragment");

        let node = automation
            .snapshot()
            .nodes
            .into_iter()
            .find(|node| !node.offscreen)
            .expect("visible node");
        let child = unsafe {
            root.ElementProviderFromPoint(
                node.bounds.left + node.bounds.width / 2.0,
                node.bounds.top + node.bounds.height / 2.0,
            )
            .expect("child fragment")
        };
        let child: IRawElementProviderSimple = child.cast().expect("child simple");
        assert_eq!(
            unsafe {
                child
                    .GetPropertyValue(UIA_AutomationIdPropertyId)
                    .expect("child automation id")
            }
            .to_string(),
            "WinShort.ControlCenter.StartWithWindows"
        );
    }

    #[test]
    fn focus_policy_distinguishes_settings_picker_and_outside() {
        let automation = published_automation();
        let root_simple = automation.root_provider();
        let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment root");
        let mut snapshot = automation.snapshot();
        snapshot.focused = Some(ElementId::OverlayEnabled);
        snapshot.set_focus_state(AutomationFocusOwner::Settings, None);
        automation.publish(snapshot.clone());
        let focused = unsafe { root.GetFocus().expect("settings focus") };
        let focused_simple: IRawElementProviderSimple = focused.cast().expect("focused child");
        let focused_value = unsafe {
            focused_simple
                .GetPropertyValue(UIA_HasKeyboardFocusPropertyId)
                .expect("focus property")
        };
        assert!(bool::try_from(&focused_value).expect("BOOL variant"));

        snapshot.set_focus_state(
            AutomationFocusOwner::Picker,
            Some(ElementId::OverlayEnabled),
        );
        automation.publish(snapshot.clone());
        assert_raw_null_focus(&root);
        let picker_value = unsafe {
            automation
                .provider_for(ElementId::OverlayEnabled)
                .GetPropertyValue(UIA_HasKeyboardFocusPropertyId)
                .expect("picker focus property")
        };
        assert!(!bool::try_from(&picker_value).expect("BOOL variant"));

        snapshot.set_focus_state(AutomationFocusOwner::Outside, None);
        automation.publish(snapshot);
        assert_raw_null_focus(&root);
    }

    #[test]
    fn ui_automation_set_focus_is_queued_for_window_dispatch() {
        let automation = published_automation_for(unsafe { GetDesktopWindow() });
        let provider = automation.provider_for(ElementId::OverlayEnabled);
        let fragment: IRawElementProviderFragment = provider.cast().expect("fragment provider");
        unsafe { fragment.SetFocus().expect("queue focus") };
        assert!(matches!(
            automation.drain_actions().as_slice(),
            [SettingsAutomationAction::SetFocus(
                ElementId::OverlayEnabled
            )]
        ));
    }

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
    fn unsupported_properties_return_s_ok_and_vt_empty() {
        let automation = published_automation();
        let root = automation.root_provider();
        let mut availability =
            assert_raw_variant_type(&root, UIA_IsInvokePatternAvailablePropertyId, VT_BOOL);
        assert!(!bool::try_from(&availability).expect("BOOL variant"));
        unsafe {
            VariantClear(&mut availability).expect("clear availability value");
        }
        let checkbox = automation.provider_for(ElementId::OverlayEnabled);
        assert_raw_empty_property(&checkbox, UIA_RangeValueValuePropertyId);

        let slider = automation.provider_for(ElementId::OverlayDuration);
        assert_raw_empty_property(&slider, UIA_ToggleToggleStatePropertyId);

        let button = automation.provider_for(ElementId::OpenConfigFolder);
        assert_raw_empty_property(&button, UIA_ToggleToggleStatePropertyId);
        assert_raw_empty_property(&button, UIA_RangeValueValuePropertyId);
        assert_raw_empty_property(&button, UIA_ValueValuePropertyId);
        assert_raw_empty_property(&button, UIA_ValueIsReadOnlyPropertyId);
        assert_raw_empty_property(&button, UIA_PROPERTY_ID(99_998));
    }

    #[test]
    fn value_pattern_properties_use_bstr_and_bool_variants() {
        let automation = published_automation();
        let provider = automation.provider_for(ElementId::InputDevice);
        let mut value = assert_raw_variant_type(&provider, UIA_ValueValuePropertyId, VT_BSTR);
        unsafe {
            VariantClear(&mut value).expect("clear BSTR value");
        }
        let mut read_only =
            assert_raw_variant_type(&provider, UIA_ValueIsReadOnlyPropertyId, VT_BOOL);
        assert!(bool::try_from(&read_only).expect("BOOL variant"));
        unsafe {
            VariantClear(&mut read_only).expect("clear BOOL value");
        }
    }

    #[test]
    fn root_and_child_provider_com_identities_are_distinct_and_stable() {
        let automation = published_automation();
        let root_simple = automation.root_provider();
        let root_fragment: IRawElementProviderFragment = root_simple.cast().expect("root fragment");
        let root: IRawElementProviderFragmentRoot = root_simple.cast().expect("root fragment root");
        let queried_root: IRawElementProviderFragmentRoot =
            root_simple.cast().expect("root QueryInterface");
        assert_eq!(iunknown_identity(&root), iunknown_identity(&queried_root));
        let returned_root = unsafe { root_fragment.FragmentRoot().expect("root FragmentRoot") };
        assert_eq!(iunknown_identity(&root), iunknown_identity(&returned_root));

        let child_simple = automation.provider_for(ElementId::OverlayEnabled);
        let error = child_simple
            .cast::<IRawElementProviderFragmentRoot>()
            .expect_err("child must not expose FragmentRoot");
        assert_eq!(error.code(), windows::core::HRESULT(0x80004002u32 as i32));
        let child: IRawElementProviderFragment = child_simple.cast().expect("child fragment");
        let returned_root = unsafe { child.FragmentRoot().expect("child FragmentRoot") };
        assert_eq!(iunknown_identity(&root), iunknown_identity(&returned_root));

        let first = unsafe {
            root_fragment
                .Navigate(NavigateDirection_FirstChild)
                .expect("first child")
        };
        let next = unsafe {
            first
                .Navigate(NavigateDirection_NextSibling)
                .expect("next sibling")
        };
        let previous = unsafe {
            next.Navigate(NavigateDirection_PreviousSibling)
                .expect("previous sibling")
        };
        assert_eq!(read_runtime_id(&first), read_runtime_id(&previous));
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
            let provider = automation.provider_for(ElementId::OverlayEnabled);
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
        automation
            .flush_pending_events_with(|_, notification| delivered.push(notification.clone()));
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
        automation
            .flush_pending_events_with(|_, notification| delivered.push(notification.clone()));
        assert!(delivered.is_empty());
    }

    #[test]
    fn property_notifications_preserve_first_old_and_latest_new_values() {
        let automation = published_automation();
        let mut first = automation.snapshot();
        first
            .nodes
            .iter_mut()
            .find(|node| node.id == ElementId::OverlayOpacity)
            .expect("opacity slider")
            .range
            .as_mut()
            .expect("opacity range")
            .value = 0.7;
        automation.publish(first);

        let mut latest = automation.snapshot();
        latest
            .nodes
            .iter_mut()
            .find(|node| node.id == ElementId::OverlayOpacity)
            .expect("opacity slider")
            .range
            .as_mut()
            .expect("opacity range")
            .value = 0.8;
        automation.publish(latest);

        let mut delivered = Vec::new();
        automation
            .flush_pending_events_with(|_, notification| delivered.push(notification.clone()));
        assert_eq!(delivered.len(), 1);
        assert_eq!(
            delivered[0].kind,
            AutomationNotificationKind::Property(UIA_RangeValueValuePropertyId.0)
        );
        match &delivered[0].old_value {
            AutomationValue::F64(value) => assert!((*value - 0.65).abs() < 1e-12),
            value => panic!("unexpected old value: {value:?}"),
        }
        match &delivered[0].new_value {
            AutomationValue::F64(value) => assert!((*value - 0.8).abs() < 1e-12),
            value => panic!("unexpected new value: {value:?}"),
        }
    }
    #[test]
    fn disabled_invoke_returns_element_not_enabled() {
        let automation = automation_with_disabled(ElementId::OpenConfigFolder);
        let provider = automation.provider_for(ElementId::OpenConfigFolder);
        let unknown = unsafe {
            provider
                .GetPatternProvider(UIA_InvokePatternId)
                .expect("Invoke pattern")
        };
        let invoke: IInvokeProvider = unknown.cast().expect("Invoke interface");
        assert_hresult(
            unsafe { invoke.Invoke() }.expect_err("disabled Invoke"),
            UIA_E_ELEMENTNOTENABLED,
        );
    }

    #[test]
    fn disabled_toggle_returns_element_not_enabled() {
        let automation = automation_with_disabled(ElementId::OverlayEnabled);
        let provider = automation.provider_for(ElementId::OverlayEnabled);
        let unknown = unsafe {
            provider
                .GetPatternProvider(UIA_TogglePatternId)
                .expect("Toggle pattern")
        };
        let toggle: IToggleProvider = unknown.cast().expect("Toggle interface");
        assert_hresult(
            unsafe { toggle.Toggle() }.expect_err("disabled Toggle"),
            UIA_E_ELEMENTNOTENABLED,
        );
    }

    #[test]
    fn disabled_range_value_returns_element_not_enabled() {
        let automation = automation_with_disabled(ElementId::OverlayOpacity);
        let provider = automation.provider_for(ElementId::OverlayOpacity);
        let unknown = unsafe {
            provider
                .GetPatternProvider(UIA_RangeValuePatternId)
                .expect("RangeValue pattern")
        };
        let range: IRangeValueProvider = unknown.cast().expect("RangeValue interface");
        assert_hresult(
            unsafe { range.SetValue(0.7) }.expect_err("disabled RangeValue"),
            UIA_E_ELEMENTNOTENABLED,
        );
    }

    #[test]
    fn invalid_range_value_returns_invalid_argument() {
        let automation = published_automation();
        let provider = automation.provider_for(ElementId::OverlayOpacity);
        let unknown = unsafe {
            provider
                .GetPatternProvider(UIA_RangeValuePatternId)
                .expect("RangeValue pattern")
        };
        let range: IRangeValueProvider = unknown.cast().expect("RangeValue interface");
        assert_hresult(
            unsafe { range.SetValue(1.1) }.expect_err("out-of-range RangeValue"),
            0x80070057,
        );
    }

    #[test]
    fn disabled_set_focus_returns_element_not_enabled() {
        let automation = automation_with_disabled(ElementId::OverlayEnabled);
        let provider = automation.provider_for(ElementId::OverlayEnabled);
        let fragment: IRawElementProviderFragment = provider.cast().expect("fragment interface");
        assert_hresult(
            unsafe { fragment.SetFocus() }.expect_err("disabled SetFocus"),
            UIA_E_ELEMENTNOTENABLED,
        );
    }

    #[test]
    fn valid_set_focus_queues_exactly_one_action() {
        let automation = SettingsAutomation::new(unsafe { GetDesktopWindow() });
        automation.publish(snapshot_from_settings(
            HWND(std::ptr::null_mut()),
            &layout(),
            &values(),
            None,
            96,
        ));
        let provider = automation.provider_for(ElementId::OverlayEnabled);
        let fragment: IRawElementProviderFragment = provider.cast().expect("fragment interface");
        unsafe { fragment.SetFocus().expect("valid SetFocus") };
        assert!(matches!(
            automation.drain_actions().as_slice(),
            [SettingsAutomationAction::SetFocus(
                ElementId::OverlayEnabled
            )]
        ));
    }

    #[test]
    fn stale_provider_operations_return_element_not_available() {
        let provider = stale_provider(ElementId::OpenConfigFolder);
        let property_error = unsafe {
            provider
                .GetPropertyValue(UIA_NamePropertyId)
                .expect_err("stale property")
        };
        assert_hresult(property_error, UIA_E_ELEMENTNOTAVAILABLE);
        let pattern_error = unsafe {
            provider
                .GetPatternProvider(UIA_InvokePatternId)
                .expect_err("stale pattern")
        };
        assert_hresult(pattern_error, UIA_E_ELEMENTNOTAVAILABLE);

        let invoke: IInvokeProvider = provider.cast().expect("stale Invoke interface");
        assert_hresult(
            unsafe { invoke.Invoke() }.expect_err("stale Invoke"),
            UIA_E_ELEMENTNOTAVAILABLE,
        );
        let fragment: IRawElementProviderFragment =
            provider.cast().expect("stale fragment interface");
        assert_hresult(
            unsafe { fragment.SetFocus() }.expect_err("stale SetFocus"),
            UIA_E_ELEMENTNOTAVAILABLE,
        );
        assert_hresult(
            unsafe { fragment.BoundingRectangle() }.expect_err("stale bounds"),
            UIA_E_ELEMENTNOTAVAILABLE,
        );
        assert_hresult(
            unsafe { fragment.GetEmbeddedFragmentRoots() }.expect_err("stale embedded roots"),
            UIA_E_ELEMENTNOTAVAILABLE,
        );
        assert_hresult(
            unsafe { fragment.FragmentRoot() }.expect_err("stale fragment root"),
            UIA_E_ELEMENTNOTAVAILABLE,
        );
        assert_hresult(
            unsafe { provider.ProviderOptions() }.expect_err("stale provider options"),
            UIA_E_ELEMENTNOTAVAILABLE,
        );

        let range_provider = stale_provider(ElementId::OverlayOpacity);
        let range: IRangeValueProvider = range_provider.cast().expect("stale RangeValue");
        assert_hresult(
            unsafe { range.SetValue(0.7) }.expect_err("stale RangeValue"),
            UIA_E_ELEMENTNOTAVAILABLE,
        );
    }

    #[test]
    fn directly_called_unsupported_patterns_return_uia_not_supported() {
        let automation = published_automation();

        let slider = automation.provider_for(ElementId::OverlayOpacity);
        let invoke: IInvokeProvider = slider.cast().expect("Invoke interface");
        assert_hresult(
            unsafe { invoke.Invoke() }.expect_err("unsupported Invoke"),
            UIA_E_NOTSUPPORTED,
        );

        let button = automation.provider_for(ElementId::OpenConfigFolder);
        let toggle: IToggleProvider = button.cast().expect("Toggle interface");
        assert_hresult(
            unsafe { toggle.Toggle() }.expect_err("unsupported Toggle"),
            UIA_E_NOTSUPPORTED,
        );
        let range: IRangeValueProvider = button.cast().expect("RangeValue interface");
        assert_hresult(
            unsafe { range.SetValue(1.0) }.expect_err("unsupported RangeValue"),
            UIA_E_NOTSUPPORTED,
        );
    }
}
