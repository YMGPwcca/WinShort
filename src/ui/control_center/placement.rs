//! Placement for the control center.

use super::window::{SavedSettingsRect, DESIGN_HEIGHT, DESIGN_WIDTH};
use crate::error::{Error, Result};
use crate::ui::layout::Rect as UiRect;
use crate::ui::picker::{PickerChoice, PopupRect};
use crate::ui::theme::UiTokens;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};

pub(super) fn load_settings_rect() -> Option<SavedSettingsRect> {
    let path = crate::config::data_dir().join("settings-window.txt");
    let text = std::fs::read_to_string(path).ok()?;
    let mut left = None;
    let mut top = None;
    let mut right = None;
    let mut bottom = None;
    let mut dpi: Option<u32> = None;
    for line in text.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "left" => left = value.parse().ok(),
            "top" => top = value.parse().ok(),
            "right" => right = value.parse().ok(),
            "bottom" => bottom = value.parse().ok(),
            "dpi" => dpi = value.parse().ok(),
            _ => {}
        }
    }
    Some(SavedSettingsRect {
        rect: RECT {
            left: left?,
            top: top?,
            right: right?,
            bottom: bottom?,
        },
        dpi: dpi?.max(96),
    })
}

pub(super) fn fixed_window_size(dpi: u32) -> (i32, i32) {
    let scale = dpi.max(96) as f32 / 96.0;
    (
        (DESIGN_WIDTH * scale).round() as i32,
        (DESIGN_HEIGHT * scale).round() as i32,
    )
}

pub(super) fn window_geometry(
    primary_work: Option<RECT>,
    primary_dpi: u32,
    saved: Option<SavedSettingsRect>,
    default_width: i32,
    default_height: i32,
) -> (i32, i32, i32, i32, u32) {
    let fallback = primary_work.unwrap_or(RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    });
    if let Some(saved) = saved {
        let target = monitor_for_rect(saved.rect);
        let work = target
            .as_ref()
            .map(|monitor| monitor.work)
            .unwrap_or(fallback);
        let target_dpi = target
            .as_ref()
            .map(|monitor| monitor.dpi)
            .unwrap_or(primary_dpi)
            .max(96);
        let (width, height) = fixed_window_size(target_dpi);
        let rect = clamp_window_rect(
            RECT {
                left: saved.rect.left,
                top: saved.rect.top,
                right: saved.rect.left + width,
                bottom: saved.rect.top + height,
            },
            work,
        );
        return (
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            target_dpi,
        );
    }
    (
        fallback.left + ((fallback.right - fallback.left) - default_width) / 2,
        fallback.top + ((fallback.bottom - fallback.top) - default_height) / 2,
        default_width,
        default_height,
        primary_dpi.max(96),
    )
}

fn monitor_for_rect(rect: RECT) -> Option<crate::platform::monitor::MonitorGeometry> {
    let monitor = unsafe {
        windows::Win32::Graphics::Gdi::MonitorFromRect(
            &rect,
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        )
    };
    if monitor.is_invalid() {
        None
    } else {
        crate::platform::monitor::info_for(monitor)
    }
}

pub(crate) fn clamp_window_rect(saved: RECT, work: RECT) -> RECT {
    let width = (saved.right - saved.left)
        .max(320)
        .min((work.right - work.left).max(1));
    let height = (saved.bottom - saved.top)
        .max(260)
        .min((work.bottom - work.top).max(1));
    let left = saved.left.clamp(work.left, work.right - width);
    let top = saved.top.clamp(work.top, work.bottom - height);
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

pub(super) fn client_rect_from_dip(rect: UiRect, dpi: u32) -> PopupRect {
    let scale = dpi.max(96) as f32 / 96.0;
    PopupRect::new(
        (rect.x * scale).round() as i32,
        (rect.y * scale).round() as i32,
        (rect.right() * scale).round() as i32,
        (rect.bottom() * scale).round() as i32,
    )
}

pub(super) fn client_work_rect(hwnd: HWND) -> Result<PopupRect> {
    let mut client = RECT::default();
    unsafe {
        if windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client).is_err() {
            return Err(Error::config("settings picker client area is unavailable"));
        }
    }
    let work = PopupRect::new(client.left, client.top, client.right, client.bottom);
    if work.width() <= 0 || work.height() <= 0 {
        return Err(Error::config("settings picker client area is empty"));
    }
    Ok(work)
}

const PICKER_MIN_WIDTH_DIP: f32 = 320.0;

const PICKER_MAX_WIDTH_DIP: f32 = 400.0;

pub(super) fn picker_height_px(choice_count: usize, scale: f32) -> i32 {
    let items =
        (choice_count.min(10) as f32 * crate::ui::picker::ITEM_HEIGHT_DIP * scale).round() as i32;
    let inset = (UiTokens::PICKER_INSET * scale).round().max(1.0) as i32;
    items + inset * 2 + 2
}

pub(super) fn picker_width_dip(control_width: f32, choices: &[PickerChoice]) -> f32 {
    // The native list uses a single-line GDI item renderer. Reserve a
    // conservative text envelope for the longest concise label, then keep
    // the popup compact enough to remain a bounded child surface.
    let longest = choices
        .iter()
        .map(|choice| choice.label().encode_utf16().count() as f32)
        .fold(0.0, f32::max);
    let content_width = 64.0 + longest * 7.2;
    control_width
        .max(PICKER_MIN_WIDTH_DIP)
        .max(content_width)
        .min(PICKER_MAX_WIDTH_DIP)
}

pub(super) fn client_size_dip(hwnd: HWND, dpi: u32) -> (f32, f32) {
    let mut rect = RECT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
    }
    let scale = 96.0 / dpi.max(96) as f32;
    (
        (rect.right - rect.left).max(0) as f32 * scale,
        (rect.bottom - rect.top).max(0) as f32 * scale,
    )
}

pub(super) fn mouse_point(lparam: LPARAM, dpi: u32) -> (f32, f32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
    let scale = 96.0 / dpi.max(96) as f32;
    (x * scale, y * scale)
}
