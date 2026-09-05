//! Snapshot for the control center automation.

use super::capabilities::is_keyboard_focusable_kind;
use super::model::{
    AutomationFocusOwner, AutomationRange, AutomationRect, SettingsAutomationNode,
    SettingsAutomationSnapshot,
};
use crate::ui::layout::{ElementId, ElementKind, Rect as UiRect, SettingsLayout};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::ClientToScreen;

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
                enabled: *enabled,
                name: element.label.to_string(),
                help_text: element.description.to_string(),
                focused: focused == Some(element.id)
                    && *enabled
                    && is_keyboard_focusable_kind(element.kind),
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

pub(super) fn slider_range(
    id: ElementId,
    kind: ElementKind,
    ratio: f32,
) -> Option<AutomationRange> {
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
