//! Native message dispatch. Reentrant calls must run outside SettingsUi borrows.

use super::chrome::{blocked_fixed_window_command, chrome_hit_test};
use super::native::invalidate;
use super::placement::fixed_window_size;

use super::state::SettingsUi;
use super::window::{UI_TIMER, WM_MOUSELEAVE};
use crate::platform::window as win;
use crate::ui::control_center_automation::{
    WM_APP_SETTINGS_AUTOMATION, WM_APP_SETTINGS_AUTOMATION_EVENTS,
};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, WINDOWPOS, WM_CHAR, WM_CLOSE, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND,
    WM_GETOBJECT, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_SETFOCUS,
    WM_SETTINGCHANGE, WM_SIZE, WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
    WM_WINDOWPOSCHANGING,
};

pub(super) unsafe extern "system" fn settings_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: settings window is main-thread owned; state via WindowState cell.
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            let Some(ui) = win::WindowCreation::<SettingsUi>::take_from(cs.lpCreateParams) else {
                return LRESULT(0);
            };
            win::store_state_ptr(hwnd, win::WindowState::new(ui));
            if let Some(cell) = win::state_cell::<SettingsUi>(hwnd) {
                cell.borrow_mut().install_automation(hwnd);
            }
            return win::def_proc(hwnd, msg, wparam, lparam);
        }

        let cell = match win::state_cell::<SettingsUi>(hwnd) {
            Some(cell) => cell,
            None => return win::def_proc(hwnd, msg, wparam, lparam),
        };

        if msg == WM_NCDESTROY {
            drop(win::take_state::<SettingsUi>(hwnd));
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_NCHITTEST => {
                let ui = cell.borrow();
                LRESULT(chrome_hit_test(&ui, hwnd, lparam) as isize)
            }
            WM_SYSCOMMAND if blocked_fixed_window_command(wparam.0) => LRESULT(0),
            WM_GETOBJECT => {
                // Clone only; UiaReturnRawElementProvider must run without a
                // live SettingsUi RefCell borrow.
                let automation = { cell.borrow().automation.clone() };
                automation
                    .and_then(|automation| automation.handle_get_object(hwnd, wparam, lparam))
                    .unwrap_or_else(|| win::def_proc(hwnd, msg, wparam, lparam))
            }
            WM_APP_SETTINGS_AUTOMATION => accessibility::handle_app_settings_automation(cell, hwnd),
            WM_APP_SETTINGS_AUTOMATION_EVENTS => {
                // The borrow ends before flush_pending_events crosses into UIA.
                let automation = { cell.borrow().automation.clone() };
                if let Some(automation) = automation {
                    automation.flush_pending_events();
                }
                LRESULT(0)
            }
            WM_SETFOCUS => {
                cell.borrow_mut()
                    .on_window_focus(hwnd, true, HWND::default());
                LRESULT(0)
            }
            WM_KILLFOCUS => lifecycle::handle_killfocus(cell, hwnd, wparam),
            WM_CLOSE => lifecycle::handle_close(cell),
            WM_PAINT => lifecycle::handle_paint(cell, hwnd),
            WM_ERASEBKGND => LRESULT(1),
            WM_WINDOWPOSCHANGING => {
                let position = &mut *(lparam.0 as *mut WINDOWPOS);
                let (width, height) = fixed_window_size(cell.borrow().dpi);
                position.cx = width;
                position.cy = height;
                LRESULT(0)
            }
            WM_SIZE => lifecycle::handle_size(cell, hwnd),
            WM_DPICHANGED => lifecycle::handle_dpichanged(cell, hwnd, wparam, lparam),
            WM_DISPLAYCHANGE => {
                let mut ui = cell.borrow_mut();
                ui.refresh_overlay_preview_aspect();
                ui.rebuild_layout(hwnd);
                ui.publish_automation_snapshot(hwnd);
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_SETTINGCHANGE => lifecycle::handle_settingchange(cell, hwnd),
            WM_MOUSEMOVE => pointer::handle_mousemove(cell, hwnd, lparam),
            WM_MOUSELEAVE => {
                let mut ui = cell.borrow_mut();
                ui.interaction.set_mouse_tracking(false);
                ui.set_hover(hwnd, None);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => pointer::handle_lbuttondown(cell, hwnd, lparam),
            WM_LBUTTONUP => pointer::handle_lbuttonup(cell, hwnd, lparam),
            WM_MOUSEWHEEL => pointer::handle_mousewheel(cell, hwnd, wparam),
            WM_KEYDOWN | WM_SYSKEYDOWN => keyboard::handle_keydown(cell, hwnd, msg, wparam, lparam),
            WM_KEYUP | WM_SYSKEYUP => keyboard::handle_keyup(cell, hwnd, msg, wparam, lparam),
            WM_TIMER if wparam.0 == UI_TIMER => lifecycle::handle_timer(cell, hwnd),
            WM_CHAR => {
                let mut ui = cell.borrow_mut();
                if ui.handle_search_char(hwnd, wparam.0 as u16) {
                    return LRESULT(0);
                }
                LRESULT(0)
            }
            _ => win::def_proc(hwnd, msg, wparam, lparam),
        }
    }
}

mod accessibility;
mod keyboard;
mod lifecycle;
mod pointer;
