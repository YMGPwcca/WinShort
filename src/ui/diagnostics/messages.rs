//! Messages for the diagnostics.

use super::native::{client_size_dip, invalidate, screen_point_dip};
use super::state::DiagnosticsUi;
use super::window::{diagnostics_hit_test_dip, MIN_HEIGHT, MIN_WIDTH, WM_MOUSELEAVE};
use crate::platform::window as win;
use crate::ui::layout::titlebar_geometry;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use windows::Win32::UI::WindowsAndMessaging::{
    ShowWindow, CREATESTRUCTW, SW_HIDE, WM_CLOSE, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE,
    WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

pub(super) unsafe extern "system" fn diagnostics_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let Some(ui) = win::WindowCreation::<DiagnosticsUi>::take_from(create.lpCreateParams)
            else {
                return LRESULT(0);
            };
            win::store_state_ptr(hwnd, win::WindowState::new(ui));
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        let cell = match win::state_cell::<DiagnosticsUi>(hwnd) {
            Some(cell) => cell,
            None => return win::def_proc(hwnd, msg, wparam, lparam),
        };
        if msg == WM_NCDESTROY {
            drop(win::take_state::<DiagnosticsUi>(hwnd));
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let dpi = cell.borrow().dpi;
                let Some((width, height)) = client_size_dip(hwnd, dpi).ok() else {
                    return LRESULT(windows::Win32::UI::WindowsAndMessaging::HTCLIENT as isize);
                };
                let Some((x, y)) = screen_point_dip(hwnd, lparam, dpi) else {
                    return LRESULT(windows::Win32::UI::WindowsAndMessaging::HTCLIENT as isize);
                };
                LRESULT(
                    diagnostics_hit_test_dip(titlebar_geometry(width, 0.0), width, height, x, y)
                        as isize,
                )
            }
            WM_NCCALCSIZE => LRESULT(0),
            WM_PAINT => lifecycle::handle_paint(cell, hwnd),
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                let mut ui = cell.borrow_mut();
                if let Some(renderer) = ui.renderer.as_mut() {
                    if let Err(error) = renderer.resize() {
                        crate::error_!("diagnostics renderer resize failed: {error}");
                    }
                }
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_DPICHANGED => lifecycle::handle_dpichanged(cell, hwnd, wparam, lparam),
            WM_SETTINGCHANGE => lifecycle::handle_settingchange(cell, hwnd),
            WM_MOUSEMOVE => pointer::handle_mousemove(cell, hwnd, lparam),
            WM_MOUSELEAVE => {
                let mut ui = cell.borrow_mut();
                ui.mouse_tracking = false;
                ui.hovered = None;
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => pointer::handle_lbuttondown(cell, hwnd, lparam),
            WM_LBUTTONUP => pointer::handle_lbuttonup(cell, hwnd, lparam),
            WM_MOUSEWHEEL => {
                let mut ui = cell.borrow_mut();
                let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
                match ui.layout(hwnd) {
                    Ok(layout) => {
                        ui.scroll =
                            (ui.scroll - delta / 120.0 * 56.0).clamp(0.0, layout.max_scroll);
                    }
                    Err(error) => crate::warn_!("diagnostics layout unavailable: {error}"),
                }
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => keyboard::handle_keydown(cell, hwnd, msg, wparam, lparam),
            WM_KEYUP | WM_SYSKEYUP => win::def_proc(hwnd, msg, wparam, lparam),
            WM_GETMINMAXINFO => {
                let info =
                    &mut *(lparam.0 as *mut windows::Win32::UI::WindowsAndMessaging::MINMAXINFO);
                let scale = cell.borrow().dpi as f32 / 96.0;
                info.ptMinTrackSize.x = (MIN_WIDTH * scale) as i32;
                info.ptMinTrackSize.y = (MIN_HEIGHT * scale) as i32;
                LRESULT(0)
            }
            _ => win::def_proc(hwnd, msg, wparam, lparam),
        }
    }
}

mod keyboard;
mod lifecycle;
mod pointer;
