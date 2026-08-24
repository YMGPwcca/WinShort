//! Monitor enumeration / work area helpers (spec §33).

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO,
    MONITORINFOEXW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};

/// Geometry of one monitor relevant to the overlay.
#[derive(Debug, Clone)]
pub struct MonitorGeometry {
    pub handle: HMONITOR,
    /// Stable identity from MONITORINFOEXW.szDevice, e.g. "\\\\.\\DISPLAY1" (#26).
    pub device_name: String,
    /// Full monitor rect in virtual screen coordinates.
    pub rect: RECT,
    /// Work area (excludes taskbar), virtual screen coordinates.
    pub work: RECT,
    pub dpi: u32,
    pub primary: bool,
}

/// Monitor nearest to the given window.
pub fn from_window(hwnd: HWND) -> HMONITOR {
    unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) }
}

pub fn primary() -> Option<MonitorGeometry> {
    let hmon = unsafe { MonitorFromWindow(HWND::default(), MONITOR_DEFAULTTOPRIMARY) };
    if hmon.is_invalid() {
        None
    } else {
        info_for(hmon)
    }
}

pub fn info_for(hmon: HMONITOR) -> Option<MonitorGeometry> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: correctly sized MONITORINFOEXW passed via pointer cast.
    let ok = unsafe { GetMonitorInfoW(hmon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) };
    if ok.as_bool() {
        Some(MonitorGeometry {
            handle: hmon,
            // SAFETY-adjacent: szDevice is NUL-terminated by the OS.
            device_name: String::from_utf16_lossy(
                &info.szDevice[..info.szDevice.iter().position(|&c| c == 0).unwrap_or(0)],
            ),
            rect: info.monitorInfo.rcMonitor,
            work: info.monitorInfo.rcWork,
            dpi: crate::platform::dpi::monitor_dpi(hmon),
            primary: (info.monitorInfo.dwFlags & 1) != 0, // MONITORINFOF_PRIMARY
        })
    } else {
        None
    }
}

/// All active monitors. Allocation is fine here: called on demand, never in a hot path.
pub fn all() -> Vec<MonitorGeometry> {
    let mut out = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            Some(HDC(std::ptr::null_mut())),
            None,
            Some(enum_proc),
            LPARAM(&mut out as *mut _ as isize),
        );
    }
    out
}

unsafe extern "system" fn enum_proc(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> windows::core::BOOL {
    // SAFETY: lparam carries the caller-owned Vec reference for this callback.
    let list = unsafe { &mut *(lparam.0 as *mut Vec<MonitorGeometry>) };
    if let Some(g) = info_for(hmon) {
        list.push(g);
    }
    true.into()
}
