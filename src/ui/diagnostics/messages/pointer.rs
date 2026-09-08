//! Pointer message handling for the diagnostics.

use super::super::model::Action;
use super::super::native::{invalidate, mouse_point};
use super::super::state::DiagnosticsUi;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_mousemove(
    cell: &std::cell::RefCell<DiagnosticsUi>,
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    {
        let mut ui = cell.borrow_mut();
        let (x, y) = mouse_point(lparam, ui.dpi);
        if !ui.mouse_tracking {
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = unsafe { TrackMouseEvent(&mut track) };
            ui.mouse_tracking = true;
        }
        let next = ui.action_at(hwnd, x, y);
        if next != ui.hovered {
            ui.hovered = next;
            invalidate(hwnd);
        }
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_lbuttondown(
    cell: &std::cell::RefCell<DiagnosticsUi>,
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    {
        let mut ui = cell.borrow_mut();
        let (x, y) = mouse_point(lparam, ui.dpi);
        if let Some(action) = ui.action_at(hwnd, x, y) {
            if !(action == Action::Bundle && ui.bundle_running) {
                ui.pressed = Some(action);
                ui.focused = Some(action);
                let _ = unsafe { SetCapture(hwnd) };
                invalidate(hwnd);
            }
        }
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_lbuttonup(
    cell: &std::cell::RefCell<DiagnosticsUi>,
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    {
        let mut ui = cell.borrow_mut();
        let (x, y) = mouse_point(lparam, ui.dpi);
        let pressed = ui.pressed.take();
        let _ = unsafe { ReleaseCapture() };
        if let Some(action) = pressed {
            if ui.action_at(hwnd, x, y) == Some(action) {
                ui.activate(hwnd, action);
            }
        }
        invalidate(hwnd);
        LRESULT(0)
    }
}
