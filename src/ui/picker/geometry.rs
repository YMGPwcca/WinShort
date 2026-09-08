//! Geometry for the picker.

use super::model::PopupRect;
use crate::ui::theme::UiTokens;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ};

/// Place a child picker below its anchor when possible, otherwise above it,
/// then clamp the complete rectangle to the Control Center client viewport.
/// Pure geometry is kept out of Win32 calls so DPI and edge behavior are testable.
pub(crate) fn place_popup(
    anchor: PopupRect,
    work: PopupRect,
    width: i32,
    height: i32,
) -> PopupRect {
    let width = width.min(work.width()).max(1);
    let height = height.min(work.height()).max(1);
    // Keep the attachment edge on the control when a wider popup cannot fit
    // to its right; final clamping still handles monitor edges.
    let mut left = if anchor.left + width <= work.right {
        anchor.left
    } else {
        anchor.right - width
    };
    let mut top = if work.bottom - anchor.bottom >= height {
        anchor.bottom
    } else {
        anchor.top - height
    };
    left = left.clamp(work.left, work.right - width);
    top = top.clamp(work.top, work.bottom - height);
    PopupRect::new(left, top, left + width, top + height)
}

fn scaled_dip_px(value: f32, dpi: u32) -> i32 {
    (value * dpi.max(96) as f32 / 96.0).round().max(1.0) as i32
}

pub(super) fn picker_inset_px(dpi: u32) -> i32 {
    scaled_dip_px(UiTokens::PICKER_INSET, dpi)
}

pub(super) fn picker_corner_diameter_px(dpi: u32) -> i32 {
    scaled_dip_px(UiTokens::PICKER_RADIUS * 2.0, dpi)
}

pub(super) fn picker_list_rect(geometry: PopupRect, dpi: u32) -> PopupRect {
    let inset = picker_inset_px(dpi);
    let width = (geometry.width() - inset * 2).max(1);
    let height = (geometry.height() - inset * 2).max(1);
    PopupRect::new(inset, inset, inset + width, inset + height)
}

pub(super) unsafe fn clip_window_to_round_rect(hwnd: HWND, width: i32, height: i32, diameter: i32) {
    let region =
        unsafe { CreateRoundRectRgn(0, 0, width.max(1), height.max(1), diameter, diameter) };
    if region.is_invalid() {
        return;
    }
    if unsafe { SetWindowRgn(hwnd, Some(region), true) } == 0 {
        let _ = unsafe { DeleteObject(HGDIOBJ(region.0)) };
    }
}
