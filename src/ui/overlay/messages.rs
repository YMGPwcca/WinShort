//! Messages for the overlay.

use super::backend::resize_surface;

use super::state::OverlayState;

use crate::platform::window as win;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, CREATESTRUCTW, HTTRANSPARENT, MA_NOACTIVATE, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT,
    WM_SETTINGCHANGE, WM_SIZE, WM_SYSCOLORCHANGE, WM_THEMECHANGED,
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
            if let Some(cell) = win::state_cell::<OverlayState>(hwnd) {
                let state = cell.borrow();
                let _ = state.graphics.clock.arm(hwnd, state.timer_id, None);
            }
            super::hover::unregister(hwnd);
            drop(win::take_state::<OverlayState>(hwnd));
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let Some(cell) = win::state_cell::<OverlayState>(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            super::hover::POINTER_MESSAGE => {
                let point = super::hover::take_pointer(hwnd);
                if let Err(error) = super::window::update_hover_pointer(cell, hwnd, point) {
                    crate::warn_!("overlay hover update failed: {error}");
                }
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_SYSCOLORCHANGE | WM_THEMECHANGED => {
                lifecycle::handle_settingchange()
            }
            super::frame_clock::FRAME_MESSAGE => {
                let current = {
                    let state = cell.borrow();
                    state.timer_id == wparam.0 && state.graphics.clock.acknowledge(hwnd, wparam.0)
                };
                if current {
                    lifecycle::handle_timer(cell, hwnd, wparam.0)
                } else {
                    LRESULT(0)
                }
            }
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
            WM_DPICHANGED => lifecycle::handle_dpichanged(),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

mod lifecycle;
