//! Per-Monitor V2 DPI awareness and scale helpers.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, GetDpiForWindow, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};

/// Must run before any window is created. Idempotent; failure (already set by
/// manifest or a prior call) is non-fatal.
pub fn set_process_awareness() {
    // SAFETY: documented process-wide setting, no lifetime constraints.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// Effective DPI for a window; falls back to 96.
pub fn dpi_for_window(hwnd: HWND) -> u32 {
    // SAFETY: hwnd is owned by the caller on this thread.
    unsafe { GetDpiForWindow(hwnd).max(96) }
}

/// Effective DPI of the monitor nearest to `hwnd`'s rect.
pub fn dpi_for_monitor_of(hwnd: HWND) -> u32 {
    monitor_dpi(unsafe {
        windows::Win32::Graphics::Gdi::MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
    })
}

pub fn monitor_dpi(hmon: HMONITOR) -> u32 {
    // SAFETY: hmon from MonitorFromWindow; MDT_EFFECTIVE_DPI matches PMv2 scaling.
    unsafe {
        let mut dx = 0u32;
        let mut dy = 0u32;
        if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() && dx > 0 {
            dx
        } else {
            96
        }
    }
}

pub fn scale(dpi: u32) -> f32 {
    dpi as f32 / 96.0
}
