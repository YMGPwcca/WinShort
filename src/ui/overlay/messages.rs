//! Messages for the overlay.

use super::backend::resize_surface;

use super::state::OverlayState;
use super::timeline::Phase;
use super::window::{apply_hide_window, TIMER_ID};

use crate::platform::window as win;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, CREATESTRUCTW, HTTRANSPARENT, MA_NOACTIVATE, WM_DPICHANGED, WM_ERASEBKGND,
    WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_SETTINGCHANGE, WM_SIZE,
    WM_SYSCOLORCHANGE, WM_THEMECHANGED, WM_TIMER,
};

pub(super) unsafe extern "system" fn overlay_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let Some(state) = win::WindowCreation::<OverlayState>::take_from(create.lpCreateParams)
            else {
                return LRESULT(0);
            };
            win::store_state_ptr(hwnd, win::WindowState::new(state));
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        if msg == WM_NCDESTROY {
            drop(win::take_state::<OverlayState>(hwnd));
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let Some(cell) = win::state_cell::<OverlayState>(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            crate::event::WM_APP_UI_ACCEPTANCE_HIDE_OVERLAY
                if std::env::var_os("WINSHORT_UI_ACCEPTANCE").is_some() =>
            {
                {
                    cell.borrow_mut().phase = Phase::Hidden;
                }
                apply_hide_window(hwnd);
                LRESULT(0)
            }
            WM_SETTINGCHANGE | WM_SYSCOLORCHANGE | WM_THEMECHANGED => {
                lifecycle::handle_settingchange(cell, hwnd)
            }
            WM_TIMER if wparam.0 == TIMER_ID => lifecycle::handle_timer(cell, hwnd),
            WM_PAINT => lifecycle::handle_paint(cell, hwnd),
            WM_SIZE => {
                if let Err(error) = resize_surface(cell, hwnd) {
                    crate::warn_!("overlay resize failed: {error}");
                }
                LRESULT(0)
            }
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_DPICHANGED => lifecycle::handle_dpichanged(cell, hwnd, wparam),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

mod lifecycle;
