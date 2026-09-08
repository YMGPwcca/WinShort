//! Chrome for the control center.

use super::state::SettingsUi;

use crate::ui::theme::{Theme, ThemeMode};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, HTCLIENT};

pub(super) fn chrome_hit_test_dip(
    chrome: crate::ui::layout::TopChromeGeometry,
    x: f32,
    y: f32,
) -> u32 {
    if chrome.search.contains(x, y) || chrome.close.contains(x, y) {
        HTCLIENT
    } else if chrome.caption.contains(x, y) {
        HTCAPTION
    } else {
        HTCLIENT
    }
}

pub(super) fn blocked_fixed_window_command(command: usize) -> bool {
    matches!(command & 0xFFF0, 0xF000 | 0xF020 | 0xF030 | 0xF120)
}

pub(super) fn chrome_hit_test(ui: &SettingsUi, hwnd: HWND, lparam: LPARAM) -> u32 {
    let mut point = POINT {
        x: (lparam.0 as u32 & 0xFFFF) as u16 as i16 as i32,
        y: ((lparam.0 as u32 >> 16) & 0xFFFF) as u16 as i16 as i32,
    };
    if !unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
        return HTCLIENT;
    }
    let scale = 96.0 / unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96) as f32;
    let x = point.x as f32 * scale;
    let y = point.y as f32 * scale;
    let chrome = crate::ui::layout::top_chrome_geometry(ui.layout.width, ui.layout.nav_width);
    chrome_hit_test_dip(chrome, x, y)
}

pub(super) fn apply_chrome(hwnd: HWND, theme: Theme) {
    unsafe {
        let dark: u32 = if theme.mode == ThemeMode::Dark { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let pref = DWMWCP_ROUND.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&pref as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        let c = theme.bg;
        let caption = COLORREF(c.r as u32 | ((c.g as u32) << 8) | ((c.b as u32) << 16));
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            (&caption as *const COLORREF).cast(),
            std::mem::size_of::<COLORREF>() as u32,
        );
    }
}
