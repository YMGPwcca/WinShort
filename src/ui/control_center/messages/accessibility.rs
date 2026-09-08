//! Accessibility message handling for the control center.

use super::super::state::SettingsUi;

use windows::Win32::Foundation::{HWND, LRESULT};

use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_app_settings_automation(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
) -> LRESULT {
    {
        let focus_requested = cell.borrow_mut().drain_automation_actions(hwnd);
        if focus_requested {
            let _ = unsafe { SetFocus(Some(hwnd)) };
            let actual = unsafe { GetFocus() };
            cell.borrow_mut().sync_focus_after_set_focus(hwnd, actual);
        }
        LRESULT(0)
    }
}
