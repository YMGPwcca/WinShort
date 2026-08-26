//! Custom UI Automation surface for the owner-drawn Settings window.
//!
//! The provider never reaches into SettingsUi. Reads use the last published
//! immutable snapshot; actions are queued back to the Settings HWND.

#![allow(non_upper_case_globals)]

use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, RwLock};

use windows::core::{implement, IUnknown, Interface, BSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Variant::{
    InitVariantFromDoubleArray, InitVariantFromInt32Array, VariantClear, VARIANT,
};
use windows::Win32::UI::Accessibility::{
    IInvokeProvider, IInvokeProvider_Impl, IRangeValueProvider, IRangeValueProvider_Impl,
    IRawElementProviderFragment, IRawElementProviderFragmentRoot,
    IRawElementProviderFragmentRoot_Impl, IRawElementProviderFragment_Impl,
    IRawElementProviderSimple, IRawElementProviderSimple_Impl, IToggleProvider,
    IToggleProvider_Impl, IValueProvider, IValueProvider_Impl, NavigateDirection,
    NavigateDirection_FirstChild, NavigateDirection_LastChild, NavigateDirection_NextSibling,
    NavigateDirection_Parent, NavigateDirection_PreviousSibling, ProviderOptions,
    ProviderOptions_ProviderOwnsSetFocus, ProviderOptions_ServerSideProvider, ToggleState,
    ToggleState_Off, ToggleState_On, UIA_AutomationFocusChangedEventId, UIA_AutomationIdPropertyId,
    UIA_BoundingRectanglePropertyId, UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId,
    UIA_ClassNamePropertyId, UIA_ControlTypePropertyId, UIA_HasKeyboardFocusPropertyId,
    UIA_HelpTextPropertyId, UIA_InvokePatternId, UIA_IsContentElementPropertyId,
    UIA_IsControlElementPropertyId, UIA_IsEnabledPropertyId,
    UIA_IsExpandCollapsePatternAvailablePropertyId, UIA_IsInvokePatternAvailablePropertyId,
    UIA_IsKeyboardFocusablePropertyId, UIA_IsOffscreenPropertyId,
    UIA_IsRangeValuePatternAvailablePropertyId, UIA_IsTogglePatternAvailablePropertyId,
    UIA_IsValuePatternAvailablePropertyId, UIA_NamePropertyId, UIA_ProviderDescriptionPropertyId,
    UIA_RangeValueLargeChangePropertyId, UIA_RangeValueMaximumPropertyId,
    UIA_RangeValueMinimumPropertyId, UIA_RangeValuePatternId, UIA_RangeValueSmallChangePropertyId,
    UIA_RangeValueValuePropertyId, UIA_SliderControlTypeId, UIA_TogglePatternId,
    UIA_ToggleToggleStatePropertyId, UIA_ValueIsReadOnlyPropertyId, UIA_ValuePatternId,
    UIA_ValueValuePropertyId, UiaAppendRuntimeId, UiaRaiseAutomationEvent,
    UiaRaiseAutomationPropertyChangedEvent, UiaRect, UiaReturnRawElementProvider, UiaRootObjectId,
    UIA_E_INVALIDOPERATION, UIA_PROPERTY_ID,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::ui::layout::{ElementId, ElementKind, Rect as UiRect, SettingsLayout};

pub(crate) const WM_APP_SETTINGS_AUTOMATION: u32 = WM_APP + 6;

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
}

#[derive(Debug, Clone)]
pub(crate) enum SettingsAutomationAction {
    Invoke(ElementId),
    Toggle(ElementId),
    SetSlider { id: ElementId, value: f64 },
    SetFocus(ElementId),
    SetWindowFocus,
}

#[derive(Clone)]
pub(crate) struct SettingsAutomation {
    snapshot: Arc<RwLock<SettingsAutomationSnapshot>>,
    actions: Arc<Mutex<Vec<SettingsAutomationAction>>>,
    hwnd: isize,
}

impl SettingsAutomation {
    pub(crate) fn new(hwnd: HWND) -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(SettingsAutomationSnapshot::default())),
            actions: Arc::new(Mutex::new(Vec::new())),
            hwnd: hwnd.0 as isize,
        }
    }

    pub(crate) fn publish(&self, snapshot: SettingsAutomationSnapshot) {
        let previous = {
            let mut current = self
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
        // UIA callbacks may synchronously query this provider; never hold the
        // snapshot lock across the UIA boundary.
        self.raise_snapshot_changes(&previous, &snapshot);
    }
    fn raise_property_changed(
        &self,
        provider: &IRawElementProviderSimple,
        property: UIA_PROPERTY_ID,
        old_value: VARIANT,
        new_value: VARIANT,
    ) {
        let mut old_value = old_value;
        let mut new_value = new_value;
        unsafe {
            let _ =
                UiaRaiseAutomationPropertyChangedEvent(provider, property, &old_value, &new_value);
            let _ = VariantClear(&mut old_value);
            let _ = VariantClear(&mut new_value);
        }
    }

    fn raise_snapshot_changes(
        &self,
        previous: &SettingsAutomationSnapshot,
        current: &SettingsAutomationSnapshot,
    ) {
        if (previous.focused != current.focused || previous.focus_owner != current.focus_owner)
            && current.focus_owner == AutomationFocusOwner::Settings
        {
            let provider = current
                .focused
                .map_or_else(|| self.root_provider(), |id| self.provider_for(id));
            unsafe {
                let _ = UiaRaiseAutomationEvent(&provider, UIA_AutomationFocusChangedEventId);
            }
        }
        let previous_root_focus =
            previous.focus_owner == AutomationFocusOwner::Settings && previous.focused.is_none();
        let current_root_focus =
            current.focus_owner == AutomationFocusOwner::Settings && current.focused.is_none();
        if previous_root_focus != current_root_focus {
            let provider = self.root_provider();
            let old_value = bool_variant(previous_root_focus);
            let new_value = bool_variant(current_root_focus);
            self.raise_property_changed(
                &provider,
                UIA_HasKeyboardFocusPropertyId,
                old_value,
                new_value,
            );
        }

        if previous.window != current.window {
            if let (Ok(old_value), Ok(new_value)) =
                (rect_variant(previous.window), rect_variant(current.window))
            {
                let provider = self.root_provider();
                self.raise_property_changed(
                    &provider,
                    UIA_BoundingRectanglePropertyId,
                    old_value,
                    new_value,
                );
            }
        }

        for node in &current.nodes {
            let Some(previous_node) = previous.nodes.iter().find(|item| item.id == node.id) else {
                continue;
            };
            for property in changed_property_ids(previous_node, node) {
                let Some((old_value, new_value)) = property_values(property, previous_node, node)
                else {
                    continue;
                };
                let provider = self.provider_for(node.id);
                self.raise_property_changed(&provider, property, old_value, new_value);
            }
        }
    }

    pub(crate) fn snapshot(&self) -> SettingsAutomationSnapshot {
        self.snapshot
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn drain_actions(&self) -> Vec<SettingsAutomationAction> {
        let mut actions = self
            .actions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        std::mem::take(&mut *actions)
    }

    pub(crate) fn enqueue(&self, action: SettingsAutomationAction) -> windows::core::Result<()> {
        self.actions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(action);
        unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as *mut _)),
                WM_APP_SETTINGS_AUTOMATION,
                WPARAM(0),
                LPARAM(0),
            )
        }
    }

    pub(crate) fn root_provider(&self) -> IRawElementProviderSimple {
        SettingsAutomationProvider {
            automation: self.clone(),
            node: None,
        }
        .into()
    }

    pub(crate) fn provider_for(&self, id: ElementId) -> IRawElementProviderSimple {
        SettingsAutomationProvider {
            automation: self.clone(),
            node: Some(id),
        }
        .into()
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
            let toggle = (element.kind == ElementKind::Toggle).then(|| value == "On");
            let range = slider_range(element.id, element.kind, *ratio);
            Some(SettingsAutomationNode {
                id: element.id,
                name: element.label.to_string(),
                help_text: element.description.to_string(),
                enabled: *enabled,
                focused: focused == Some(element.id),
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

fn no_interface<T>() -> windows::core::Result<T> {
    Err(windows::core::Error::from_hresult(windows::core::HRESULT(
        0x80004002u32 as i32,
    )))
}

fn invalid_argument<T>() -> windows::core::Result<T> {
    Err(windows::core::Error::from_hresult(windows::core::HRESULT(
        0x80070057u32 as i32,
    )))
}

fn null_interface<T: Interface>() -> T {
    // The generated UIA vtable moves successful interface return values into
    // the ABI out-parameter. It therefore preserves a NULL COM result without
    // dropping a null smart pointer on this Rust side.
    unsafe { T::from_raw(std::ptr::null_mut()) }
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

fn runtime_id_variant(values: &[i32]) -> windows::core::Result<*mut SAFEARRAY> {
    let variant = unsafe { InitVariantFromInt32Array(values)? };
    let variant = ManuallyDrop::new(variant);
    let array = unsafe { variant.Anonymous.Anonymous.Anonymous.parray };
    Ok(array)
}
fn optional_i32_variant(value: Option<i32>) -> VARIANT {
    value.map_or_else(VARIANT::default, i32_variant)
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
        changed.push(UIA_ToggleToggleStatePropertyId);
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

fn property_values(
    property: UIA_PROPERTY_ID,
    previous: &SettingsAutomationNode,
    current: &SettingsAutomationNode,
) -> Option<(VARIANT, VARIANT)> {
    match property {
        UIA_NamePropertyId => Some((
            string_variant(&previous.name),
            string_variant(&current.name),
        )),
        UIA_IsEnabledPropertyId | UIA_IsKeyboardFocusablePropertyId => Some((
            bool_variant(previous.enabled),
            bool_variant(current.enabled),
        )),
        UIA_HasKeyboardFocusPropertyId => Some((
            bool_variant(previous.focused),
            bool_variant(current.focused),
        )),
        UIA_IsOffscreenPropertyId => Some((
            bool_variant(previous.offscreen),
            bool_variant(current.offscreen),
        )),
        UIA_BoundingRectanglePropertyId => Some((
            rect_variant(previous.bounds).ok()?,
            rect_variant(current.bounds).ok()?,
        )),
        UIA_ToggleToggleStatePropertyId => Some((
            optional_i32_variant(previous.toggle.map(|value| {
                if value {
                    ToggleState_On.0
                } else {
                    ToggleState_Off.0
                }
            })),
            optional_i32_variant(current.toggle.map(|value| {
                if value {
                    ToggleState_On.0
                } else {
                    ToggleState_Off.0
                }
            })),
        )),
        UIA_RangeValueValuePropertyId => Some((
            previous
                .range
                .map_or_else(VARIANT::default, |range| f64_variant(range.value)),
            current
                .range
                .map_or_else(VARIANT::default, |range| f64_variant(range.value)),
        )),
        UIA_ValueValuePropertyId => Some((
            if node_has_value(previous.kind) {
                string_variant(&previous.value)
            } else {
                VARIANT::default()
            },
            if node_has_value(current.kind) {
                string_variant(&current.value)
            } else {
                VARIANT::default()
            },
        )),
        _ => None,
    }
}

fn node_has_invoke(kind: ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::Value
            | ElementKind::Hotkey
            | ElementKind::Action
            | ElementKind::ButtonSecondary
            | ElementKind::ButtonPrimary
    )
}

fn node_has_value(kind: ElementKind) -> bool {
    // Picker triggers and hotkey actions expose their current display value,
    // but neither accepts arbitrary text through ValuePattern.
    matches!(kind, ElementKind::Value | ElementKind::Hotkey)
}

fn control_type(kind: ElementKind) -> i32 {
    match kind {
        ElementKind::Toggle => UIA_CheckBoxControlTypeId.0,
        ElementKind::Slider => UIA_SliderControlTypeId.0,
        ElementKind::Value
        | ElementKind::Hotkey
        | ElementKind::Action
        | ElementKind::ButtonSecondary
        | ElementKind::ButtonPrimary => UIA_ButtonControlTypeId.0,
    }
}

fn automation_id(id: ElementId) -> String {
    format!("WinShort.Settings.{id:?}")
}

#[derive(Clone)]
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot,
    IInvokeProvider,
    IToggleProvider,
    IRangeValueProvider,
    IValueProvider
)]
struct SettingsAutomationProvider {
    automation: SettingsAutomation,
    node: Option<ElementId>,
}

impl SettingsAutomationProvider {
    fn snapshot(&self) -> SettingsAutomationSnapshot {
        self.automation.snapshot()
    }

    fn node(&self) -> Option<SettingsAutomationNode> {
        let id = self.node?;
        self.snapshot().nodes.into_iter().find(|node| node.id == id)
    }

    fn node_index(&self) -> Option<usize> {
        let id = self.node?;
        self.snapshot().nodes.iter().position(|node| node.id == id)
    }

    fn hwnd(&self) -> HWND {
        HWND(self.automation.hwnd as *mut _)
    }

    fn self_simple(&self) -> IRawElementProviderSimple {
        Self {
            automation: self.automation.clone(),
            node: self.node,
        }
        .into()
    }

    fn child_fragment(&self, id: ElementId) -> IRawElementProviderFragment {
        Self {
            automation: self.automation.clone(),
            node: Some(id),
        }
        .into()
    }

    fn root_fragment(&self) -> IRawElementProviderFragmentRoot {
        Self {
            automation: self.automation.clone(),
            node: None,
        }
        .into()
    }
    fn root_fragment_node(&self) -> IRawElementProviderFragment {
        Self {
            automation: self.automation.clone(),
            node: None,
        }
        .into()
    }

    fn enqueue(&self, action: SettingsAutomationAction) -> windows::core::Result<()> {
        self.automation.enqueue(action)
    }
}

impl IRawElementProviderSimple_Impl for SettingsAutomationProvider_Impl {
    fn ProviderOptions(&self) -> windows::core::Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_ProviderOwnsSetFocus)
    }

    fn GetPatternProvider(
        &self,
        patternid: windows::Win32::UI::Accessibility::UIA_PATTERN_ID,
    ) -> windows::core::Result<IUnknown> {
        let Some(node) = self.node() else {
            return Ok(null_interface());
        };
        let available = match patternid {
            UIA_InvokePatternId => node_has_invoke(node.kind),
            UIA_TogglePatternId => node.toggle.is_some(),
            UIA_RangeValuePatternId => node.range.is_some(),
            UIA_ValuePatternId => node_has_value(node.kind),
            _ => false,
        };
        if !available {
            return Ok(null_interface());
        }
        self.self_simple().cast()
    }

    fn GetPropertyValue(
        &self,
        propertyid: windows::Win32::UI::Accessibility::UIA_PROPERTY_ID,
    ) -> windows::core::Result<VARIANT> {
        let snapshot = self.snapshot();
        let Some(node) = self.node() else {
            return match propertyid {
                UIA_NamePropertyId => Ok(string_variant("WinShort Settings")),
                UIA_HelpTextPropertyId => Ok(string_variant("WinShort Settings")),
                UIA_ControlTypePropertyId => Ok(i32_variant(
                    windows::Win32::UI::Accessibility::UIA_WindowControlTypeId.0,
                )),
                UIA_IsEnabledPropertyId
                | UIA_IsKeyboardFocusablePropertyId
                | UIA_IsContentElementPropertyId
                | UIA_IsControlElementPropertyId => Ok(bool_variant(true)),
                UIA_HasKeyboardFocusPropertyId => Ok(bool_variant(
                    snapshot.focus_owner == AutomationFocusOwner::Settings
                        && snapshot.focused.is_none(),
                )),
                UIA_IsOffscreenPropertyId => Ok(bool_variant(false)),
                UIA_BoundingRectanglePropertyId => rect_variant(snapshot.window),
                UIA_AutomationIdPropertyId => Ok(string_variant("WinShort.Settings")),
                UIA_ClassNamePropertyId => Ok(string_variant("WinShort.Settings")),
                UIA_ProviderDescriptionPropertyId => Ok(string_variant(
                    "WinShort custom Settings UI Automation provider",
                )),
                _ => Ok(VARIANT::default()),
            };
        };
        match propertyid {
            UIA_NamePropertyId => Ok(string_variant(&node.name)),
            UIA_HelpTextPropertyId => Ok(string_variant(&node.help_text)),
            UIA_ControlTypePropertyId => Ok(i32_variant(control_type(node.kind))),
            UIA_IsEnabledPropertyId => Ok(bool_variant(node.enabled)),
            UIA_IsKeyboardFocusablePropertyId => Ok(bool_variant(node.enabled)),
            UIA_HasKeyboardFocusPropertyId => Ok(bool_variant(node.focused)),
            UIA_IsOffscreenPropertyId => Ok(bool_variant(node.offscreen)),
            UIA_IsContentElementPropertyId | UIA_IsControlElementPropertyId => {
                Ok(bool_variant(true))
            }
            UIA_BoundingRectanglePropertyId => rect_variant(node.bounds),
            UIA_AutomationIdPropertyId => Ok(string_variant(&automation_id(node.id))),
            UIA_ClassNamePropertyId => Ok(string_variant("WinShort.Settings.Item")),
            UIA_ProviderDescriptionPropertyId => Ok(string_variant(
                "WinShort custom Settings UI Automation provider",
            )),
            UIA_IsInvokePatternAvailablePropertyId => Ok(bool_variant(node_has_invoke(node.kind))),
            UIA_IsTogglePatternAvailablePropertyId => Ok(bool_variant(node.toggle.is_some())),
            UIA_IsRangeValuePatternAvailablePropertyId => Ok(bool_variant(node.range.is_some())),
            UIA_IsValuePatternAvailablePropertyId => Ok(bool_variant(node_has_value(node.kind))),
            UIA_IsExpandCollapsePatternAvailablePropertyId => Ok(bool_variant(false)),
            UIA_ToggleToggleStatePropertyId => node
                .toggle
                .map(|value| {
                    i32_variant(if value {
                        ToggleState_On.0
                    } else {
                        ToggleState_Off.0
                    })
                })
                .ok_or_else(|| {
                    windows::core::Error::from_hresult(windows::core::HRESULT(0x80004002u32 as i32))
                }),
            UIA_RangeValueValuePropertyId => node
                .range
                .map(|range| f64_variant(range.value))
                .ok_or_else(|| {
                    windows::core::Error::from_hresult(windows::core::HRESULT(0x80004002u32 as i32))
                }),
            UIA_RangeValueMinimumPropertyId => node
                .range
                .map(|range| f64_variant(range.minimum))
                .ok_or_else(|| {
                    windows::core::Error::from_hresult(windows::core::HRESULT(0x80004002u32 as i32))
                }),
            UIA_RangeValueMaximumPropertyId => node
                .range
                .map(|range| f64_variant(range.maximum))
                .ok_or_else(|| {
                    windows::core::Error::from_hresult(windows::core::HRESULT(0x80004002u32 as i32))
                }),
            UIA_RangeValueSmallChangePropertyId => node
                .range
                .map(|range| f64_variant(range.small_change))
                .ok_or_else(|| {
                    windows::core::Error::from_hresult(windows::core::HRESULT(0x80004002u32 as i32))
                }),
            UIA_RangeValueLargeChangePropertyId => node
                .range
                .map(|range| f64_variant(range.large_change))
                .ok_or_else(|| {
                    windows::core::Error::from_hresult(windows::core::HRESULT(0x80004002u32 as i32))
                }),
            UIA_ValueValuePropertyId => {
                if node_has_value(node.kind) {
                    Ok(string_variant(&node.value))
                } else {
                    Ok(VARIANT::default())
                }
            }
            UIA_ValueIsReadOnlyPropertyId => Ok(bool_variant(node_has_value(node.kind))),
            _ => Ok(VARIANT::default()),
        }
    }

    fn HostRawElementProvider(&self) -> windows::core::Result<IRawElementProviderSimple> {
        if self.node.is_some() {
            return Ok(null_interface());
        }
        unsafe { windows::Win32::UI::Accessibility::UiaHostProviderFromHwnd(self.hwnd()) }
    }
}

impl IRawElementProviderFragment_Impl for SettingsAutomationProvider_Impl {
    fn Navigate(
        &self,
        direction: NavigateDirection,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        let snapshot = self.snapshot();
        let next = match (self.node, direction) {
            (None, NavigateDirection_FirstChild) => snapshot.nodes.first().map(|node| node.id),
            (None, NavigateDirection_LastChild) => snapshot.nodes.last().map(|node| node.id),
            (Some(_), NavigateDirection_Parent) => return Ok(self.root_fragment_node()),
            (Some(_), NavigateDirection_NextSibling) => self
                .node_index()
                .and_then(|index| snapshot.nodes.get(index + 1))
                .map(|node| node.id),
            (Some(_), NavigateDirection_PreviousSibling) => self
                .node_index()
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| snapshot.nodes.get(index))
                .map(|node| node.id),
            _ => None,
        };
        match next {
            Some(id) => Ok(self.child_fragment(id)),
            None => Ok(null_interface()),
        }
    }

    fn GetRuntimeId(&self) -> windows::core::Result<*mut SAFEARRAY> {
        let Some(index) = self.node_index() else {
            return Ok(std::ptr::null_mut());
        };
        runtime_id_variant(&[UiaAppendRuntimeId as i32, index as i32 + 1])
    }

    fn BoundingRectangle(&self) -> windows::core::Result<UiaRect> {
        let bounds = self
            .node()
            .map_or_else(|| self.snapshot().window, |node| node.bounds);
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
        let action = self.node.map_or(
            SettingsAutomationAction::SetWindowFocus,
            SettingsAutomationAction::SetFocus,
        );
        self.enqueue(action)
    }

    fn FragmentRoot(&self) -> windows::core::Result<IRawElementProviderFragmentRoot> {
        Ok(self.root_fragment())
    }
}

impl IRawElementProviderFragmentRoot_Impl for SettingsAutomationProvider_Impl {
    fn ElementProviderFromPoint(
        &self,
        x: f64,
        y: f64,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        let snapshot = self.snapshot();
        if let Some(node) = snapshot
            .nodes
            .into_iter()
            .find(|node| !node.offscreen && node.bounds.contains(x, y))
        {
            return Ok(self.child_fragment(node.id));
        }
        if snapshot.window.contains(x, y) {
            Ok(self.root_fragment_node())
        } else {
            Ok(null_interface())
        }
    }

    fn GetFocus(&self) -> windows::core::Result<IRawElementProviderFragment> {
        let snapshot = self.snapshot();
        if snapshot.focus_owner != AutomationFocusOwner::Settings {
            return Ok(null_interface());
        }
        snapshot.focused.map_or_else(
            || Ok(self.root_fragment_node()),
            |id| Ok(self.child_fragment(id)),
        )
    }
}

impl IInvokeProvider_Impl for SettingsAutomationProvider_Impl {
    fn Invoke(&self) -> windows::core::Result<()> {
        let Some(node) = self.node() else {
            return no_interface();
        };
        if !node.enabled || !node_has_invoke(node.kind) {
            return invalid_argument();
        }
        self.enqueue(SettingsAutomationAction::Invoke(node.id))
    }
}

impl IToggleProvider_Impl for SettingsAutomationProvider_Impl {
    fn Toggle(&self) -> windows::core::Result<()> {
        let Some(node) = self.node() else {
            return no_interface();
        };
        if !node.enabled || node.toggle.is_none() {
            return invalid_argument();
        }
        self.enqueue(SettingsAutomationAction::Toggle(node.id))
    }

    fn ToggleState(&self) -> windows::core::Result<ToggleState> {
        self.node()
            .and_then(|node| node.toggle)
            .map_or_else(no_interface, |value| {
                Ok(if value {
                    ToggleState_On
                } else {
                    ToggleState_Off
                })
            })
    }
}

impl IRangeValueProvider_Impl for SettingsAutomationProvider_Impl {
    fn SetValue(&self, val: f64) -> windows::core::Result<()> {
        let Some(node) = self.node() else {
            return no_interface();
        };
        let Some(range) = node.range else {
            return no_interface();
        };
        if !node.enabled || !val.is_finite() || val < range.minimum || val > range.maximum {
            return invalid_argument();
        }
        self.enqueue(SettingsAutomationAction::SetSlider {
            id: node.id,
            value: val,
        })
    }

    fn Value(&self) -> windows::core::Result<f64> {
        self.node()
            .and_then(|node| node.range)
            .map_or_else(no_interface, |range| Ok(range.value))
    }

    fn IsReadOnly(&self) -> windows::core::Result<windows::core::BOOL> {
        Ok(self
            .node()
            .and_then(|node| node.range)
            .is_none_or(|range| range.read_only)
            .into())
    }

    fn Maximum(&self) -> windows::core::Result<f64> {
        self.node()
            .and_then(|node| node.range)
            .map_or_else(no_interface, |range| Ok(range.maximum))
    }

    fn Minimum(&self) -> windows::core::Result<f64> {
        self.node()
            .and_then(|node| node.range)
            .map_or_else(no_interface, |range| Ok(range.minimum))
    }

    fn LargeChange(&self) -> windows::core::Result<f64> {
        self.node()
            .and_then(|node| node.range)
            .map_or_else(no_interface, |range| Ok(range.large_change))
    }

    fn SmallChange(&self) -> windows::core::Result<f64> {
        self.node()
            .and_then(|node| node.range)
            .map_or_else(no_interface, |range| Ok(range.small_change))
    }
}

impl IValueProvider_Impl for SettingsAutomationProvider_Impl {
    fn SetValue(&self, _val: &windows::core::PCWSTR) -> windows::core::Result<()> {
        let Some(node) = self.node() else {
            return no_interface();
        };
        if !node_has_value(node.kind) {
            return no_interface();
        }
        Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            UIA_E_INVALIDOPERATION as i32,
        )))
    }

    fn Value(&self) -> windows::core::Result<BSTR> {
        self.node()
            .filter(|node| node_has_value(node.kind))
            .map_or_else(no_interface, |node| Ok(BSTR::from(node.value.as_str())))
    }

    fn IsReadOnly(&self) -> windows::core::Result<windows::core::BOOL> {
        self.node()
            .filter(|node| node_has_value(node.kind))
            .map_or_else(no_interface, |_| Ok(true.into()))
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

    fn layout() -> SettingsLayout {
        SettingsLayout::build(610.0, 720.0, 64.0)
    }

    fn values() -> Vec<(ElementId, String, bool, f32)> {
        ElementId::FOCUS_ORDER
            .into_iter()
            .map(|id| (id, String::from("Off"), true, 0.5))
            .collect()
    }

    fn published_automation() -> SettingsAutomation {
        let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
        automation.publish(snapshot_from_settings(
            HWND(std::ptr::null_mut()),
            &layout(),
            &values(),
            None,
            96,
        ));
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
        let automation = SettingsAutomation::new(HWND(std::ptr::null_mut()));
        automation
            .actions
            .lock()
            .expect("action queue")
            .push(SettingsAutomationAction::Toggle(ElementId::OverlayEnabled));
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
        automation
            .snapshot
            .write()
            .expect("snapshot")
            .clone_from(&snapshot);
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
        let automation = SettingsAutomation::new(unsafe { GetDesktopWindow() });
        let root = automation.root_provider();
        let host = unsafe { root.HostRawElementProvider() }.expect("root host provider");
        assert!(!host.as_raw().is_null());

        let child = automation.provider_for(ElementId::OverlayEnabled);
        assert_raw_null_host(&child);
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
            "WinShort.Settings.StartWithWindows"
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
        automation
            .snapshot
            .write()
            .expect("snapshot")
            .clone_from(&snapshot);
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
        automation
            .snapshot
            .write()
            .expect("snapshot")
            .clone_from(&snapshot);
        assert_raw_null_focus(&root);
        let picker_value = unsafe {
            automation
                .provider_for(ElementId::OverlayEnabled)
                .GetPropertyValue(UIA_HasKeyboardFocusPropertyId)
                .expect("picker focus property")
        };
        assert!(!bool::try_from(&picker_value).expect("BOOL variant"));

        snapshot.set_focus_state(AutomationFocusOwner::Outside, None);
        automation
            .snapshot
            .write()
            .expect("snapshot")
            .clone_from(&snapshot);
        assert_raw_null_focus(&root);
    }

    #[test]
    fn ui_automation_set_focus_is_queued_for_window_dispatch() {
        let automation = SettingsAutomation::new(unsafe { GetDesktopWindow() });
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
}
