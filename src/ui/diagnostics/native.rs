//! Native for the diagnostics.

use crate::error::{Error, Result};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ScreenToClient};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SHIFT};

pub(super) fn post_main(event: crate::event::AppEvent) {
    crate::event::post_main(event);
}

pub(super) fn invalidate(hwnd: HWND) {
    // A closing diagnostics HWND needs no retry if invalidation is rejected.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

pub(super) fn client_size_dip(hwnd: HWND, dpi: u32) -> Result<(f32, f32)> {
    let mut rect = RECT::default();
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect) }
        .map_err(|error| Error::win("GetClientRect(diagnostics)", &error))?;
    let scale = 96.0 / dpi.max(96) as f32;
    Ok((
        (rect.right - rect.left).max(0) as f32 * scale,
        (rect.bottom - rect.top).max(0) as f32 * scale,
    ))
}

pub(super) fn mouse_point(lparam: LPARAM, dpi: u32) -> (f32, f32) {
    let x = (lparam.0 as u32 & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam.0 as u32 >> 16) & 0xFFFF) as u16 as i16 as f32;
    let scale = 96.0 / dpi.max(96) as f32;
    (x * scale, y * scale)
}

pub(super) fn screen_point_dip(hwnd: HWND, lparam: LPARAM, dpi: u32) -> Option<(f32, f32)> {
    let mut point = windows::Win32::Foundation::POINT {
        x: (lparam.0 as u32 & 0xFFFF) as u16 as i16 as i32,
        y: ((lparam.0 as u32 >> 16) & 0xFFFF) as u16 as i16 as i32,
    };
    if !unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
        return None;
    }
    let scale = 96.0 / dpi.max(96) as f32;
    Some((point.x as f32 * scale, point.y as f32 * scale))
}

pub(super) fn is_shift_down() -> bool {
    unsafe { (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 }
}
