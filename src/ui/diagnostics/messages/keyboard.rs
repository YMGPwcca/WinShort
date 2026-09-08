//! Keyboard message handling for the diagnostics.

use super::super::native::{invalidate, is_shift_down};
use super::super::state::DiagnosticsUi;

use crate::platform::window as win;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_keydown(
    cell: &std::cell::RefCell<DiagnosticsUi>,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let vk = wparam.0 as u16;
        let mut ui = cell.borrow_mut();
        match vk {
            0x09 => {
                ui.focus_next(is_shift_down());
                invalidate(hwnd);
                LRESULT(0)
            }
            0x0D | 0x20 => {
                if let Some(action) = ui.focused {
                    ui.activate(hwnd, action);
                }
                LRESULT(0)
            }
            0x1B => {
                let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
                LRESULT(0)
            }
            _ => win::def_proc(hwnd, msg, wparam, lparam),
        }
    }
}
