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
    InitVariantFromDoubleArray, InitVariantFromInt32Array, VARIANT,
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
    UIA_ClassNamePropertyId, UIA_ComboBoxControlTypeId, UIA_ControlTypePropertyId,
    UIA_EditControlTypeId, UIA_HasKeyboardFocusPropertyId, UIA_HelpTextPropertyId,
    UIA_InvokePatternId, UIA_IsContentElementPropertyId, UIA_IsControlElementPropertyId,
    UIA_IsEnabledPropertyId, UIA_IsExpandCollapsePatternAvailablePropertyId,
    UIA_IsInvokePatternAvailablePropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_IsOffscreenPropertyId, UIA_IsRangeValuePatternAvailablePropertyId,
    UIA_IsTogglePatternAvailablePropertyId, UIA_IsValuePatternAvailablePropertyId,
    UIA_NamePropertyId, UIA_ProviderDescriptionPropertyId, UIA_RangeValueLargeChangePropertyId,
    UIA_RangeValueMaximumPropertyId, UIA_RangeValueMinimumPropertyId, UIA_RangeValuePatternId,
    UIA_RangeValueSmallChangePropertyId, UIA_RangeValueValuePropertyId, UIA_SliderControlTypeId,
    UIA_TogglePatternId, UIA_ToggleToggleStatePropertyId, UIA_ValueIsReadOnlyPropertyId,
    UIA_ValuePatternId, UIA_ValueValuePropertyId, UiaRaiseAutomationEvent, UiaRect,
    UiaReturnRawElementProvider, UiaRootObjectId,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SettingsAutomationSnapshot {
    pub window: AutomationRect,
    pub nodes: Vec<SettingsAutomationNode>,
    pub focused: Option<ElementId>,
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
        let focus_changed = {
            let mut current = self
                .snapshot
                .write()
                .unwrap_or_else(|error| error.into_inner());
            let changed = current.focused != snapshot.focused;
            *current = snapshot;
            changed
        };
        if focus_changed {
            if let Some(id) = self.snapshot().focused {
                self.raise_focus(id);
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

    pub(crate) fn raise_focus(&self, id: ElementId) {
        let provider = self.provider_for(id);
        unsafe {
            let _ = UiaRaiseAutomationEvent(&provider, UIA_AutomationFocusChangedEventId);
        }
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

fn not_implemented<T>() -> windows::core::Result<T> {
    Err(windows::core::Error::from_hresult(windows::core::HRESULT(
        0x80004001u32 as i32,
    )))
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
    matches!(kind, ElementKind::Value | ElementKind::Hotkey)
}

fn control_type(kind: ElementKind) -> i32 {
    match kind {
        ElementKind::Toggle => UIA_CheckBoxControlTypeId.0,
        ElementKind::Slider => UIA_SliderControlTypeId.0,
        ElementKind::Value => UIA_ComboBoxControlTypeId.0,
        ElementKind::Hotkey => UIA_EditControlTypeId.0,
        ElementKind::Action | ElementKind::ButtonSecondary | ElementKind::ButtonPrimary => {
            UIA_ButtonControlTypeId.0
        }
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
            return no_interface();
        };
        let available = match patternid {
            UIA_InvokePatternId => node_has_invoke(node.kind),
            UIA_TogglePatternId => node.toggle.is_some(),
            UIA_RangeValuePatternId => node.range.is_some(),
            UIA_ValuePatternId => node_has_value(node.kind),
            _ => false,
        };
        if !available {
            return no_interface();
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
                UIA_HasKeyboardFocusPropertyId => Ok(bool_variant(snapshot.focused.is_none())),
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
        next.map_or_else(no_interface, |id| Ok(self.child_fragment(id)))
    }

    fn GetRuntimeId(&self) -> windows::core::Result<*mut SAFEARRAY> {
        let suffix = self.node_index().map_or(0, |index| index as i32 + 1);
        runtime_id_variant(&[42, suffix])
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
            no_interface()
        }
    }

    fn GetFocus(&self) -> windows::core::Result<IRawElementProviderFragment> {
        self.snapshot().focused.map_or_else(
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
        not_implemented()
    }

    fn Value(&self) -> windows::core::Result<BSTR> {
        self.node()
            .filter(|node| node_has_value(node.kind))
            .map_or_else(no_interface, |node| Ok(BSTR::from(node.value.as_str())))
    }

    fn IsReadOnly(&self) -> windows::core::Result<windows::core::BOOL> {
        Ok(self
            .node()
            .is_none_or(|node| !node_has_value(node.kind))
            .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> SettingsLayout {
        SettingsLayout::build(610.0, 720.0, 64.0)
    }

    fn values() -> Vec<(ElementId, String, bool, f32)> {
        ElementId::FOCUS_ORDER
            .into_iter()
            .map(|id| (id, String::from("Off"), true, 0.5))
            .collect()
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
}
