//! Lifecycle message handling for the diagnostics.

use super::super::native::invalidate;
use super::super::state::DiagnosticsUi;
use super::super::window::apply_chrome;

use crate::ui::theme::Theme;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};

use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_paint(cell: &std::cell::RefCell<DiagnosticsUi>, hwnd: HWND) -> LRESULT {
    {
        let _paint = crate::platform::window::PaintSession::begin(hwnd);
        if let Err(error) = cell.borrow_mut().paint(hwnd) {
            crate::error_!("diagnostics paint failed: {error}");
        }
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_dpichanged(
    cell: &std::cell::RefCell<DiagnosticsUi>,
    hwnd: HWND,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let new_dpi = ((wparam.0 >> 16) as u32).max(96);
        {
            let mut ui = cell.borrow_mut();
            ui.dpi = new_dpi;
            if let Some(renderer) = ui.renderer.as_mut() {
                let _ = renderer.set_dpi(new_dpi);
            }
        }
        let suggested = unsafe { &*(lparam.0 as *const RECT) };
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        invalidate(hwnd);
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_settingchange(
    cell: &std::cell::RefCell<DiagnosticsUi>,
    hwnd: HWND,
) -> LRESULT {
    {
        let theme = Theme::current();
        if let Some(renderer) = cell.borrow_mut().renderer.as_mut() {
            let _ = renderer.set_theme(theme);
        }
        apply_chrome(hwnd, theme);
        invalidate(hwnd);
        LRESULT(0)
    }
}
