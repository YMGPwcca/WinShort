//! Messages for the picker.

use super::appearance::{picker_colors, to_colorref};
use super::focus::{
    cancel_picker, commit_selected, defer_focus_loss, picker_item_from_point, set_picker_hover,
};

use super::painting::draw_picker_surface;
use super::selection::normalize_multi_selection;
use super::window::{PickerUi, SUBCLASS_ID, WM_APP_PICKER_FOCUS_LOST, WM_MOUSELEAVE};
use crate::platform::window as win;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FillRect, HDC, HGDIOBJ};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, MA_NOACTIVATE, WM_CLOSE, WM_COMMAND, WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND,
    WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONUP, WM_MEASUREITEM, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
    WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
};

pub(super) unsafe extern "system" fn picker_list_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    ref_data: usize,
) -> LRESULT {
    let parent = HWND(ref_data as *mut _);
    match msg {
        WM_ERASEBKGND => {
            let hdc = HDC(wparam.0 as *mut _);
            let mut rect = RECT::default();
            if unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect) }
                .is_err()
            {
                return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            }
            let colors = picker_colors(false);
            let brush = unsafe { CreateSolidBrush(to_colorref(colors.background)) };
            let _ = unsafe { FillRect(hdc, &rect, brush) };
            let _ = unsafe { DeleteObject(HGDIOBJ(brush.0)) };
            LRESULT(1)
        }
        WM_MOUSEMOVE => {
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut track);
            }
            set_picker_hover(parent, hwnd, picker_item_from_point(hwnd, lparam));
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSELEAVE => {
            set_picker_hover(parent, hwnd, None);
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_KEYDOWN if wparam.0 as u16 == 0x1B => {
            cancel_picker(parent, true);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 as u16 == 0x09 => {
            let reverse = unsafe { (GetKeyState(0x10) as u16 & 0x8000) != 0 };
            cancel_picker(parent, true);
            crate::event::post_main(crate::event::AppEvent::FocusSettingsFromPicker { reverse });
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 as u16 == 0x0D => {
            commit_selected(parent, hwnd);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            let kind =
                unsafe { win::state_cell::<PickerUi>(parent) }.map(|cell| cell.borrow().kind);
            if let Some(kind) = kind.filter(|kind| kind.is_multi_select()) {
                normalize_multi_selection(kind, hwnd);
            } else {
                commit_selected(parent, hwnd);
            }
            result
        }
        WM_KILLFOCUS => {
            let next = HWND(wparam.0 as *mut _);
            defer_focus_loss(parent, next);
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(picker_list_subclass), SUBCLASS_ID);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        },
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

pub(super) unsafe extern "system" fn picker_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let Some(state) = win::WindowCreation::<PickerUi>::take_from(create.lpCreateParams)
            else {
                return LRESULT(0);
            };
            win::store_state_ptr(hwnd, win::WindowState::new(state));
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        let cell = match win::state_cell::<PickerUi>(hwnd) {
            Some(cell) => cell,
            None => return win::def_proc(hwnd, msg, wparam, lparam),
        };
        if msg == WM_NCDESTROY {
            drop(win::take_state::<PickerUi>(hwnd));
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_DPICHANGED => lifecycle::handle_dpichanged(cell, hwnd, msg, wparam, lparam),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => {
                let hdc = HDC(wparam.0 as *mut _);
                draw_picker_surface(hwnd, hdc);
                LRESULT(1)
            }
            WM_PAINT => {
                let _paint = crate::platform::window::PaintSession::begin(hwnd);
                draw_picker_surface(hwnd, _paint.dc());
                LRESULT(0)
            }
            WM_MEASUREITEM => lifecycle::handle_measureitem(hwnd, msg, wparam, lparam),
            WM_DRAWITEM => lifecycle::handle_drawitem(cell, hwnd, msg, wparam, lparam),
            WM_COMMAND => lifecycle::handle_command(cell, hwnd, msg, wparam, lparam),
            WM_APP_PICKER_FOCUS_LOST => {
                lifecycle::handle_app_picker_focus_lost(cell, hwnd, wparam, lparam)
            }
            WM_KILLFOCUS => {
                let next = HWND(wparam.0 as *mut _);
                defer_focus_loss(hwnd, next);
                LRESULT(0)
            }
            WM_CLOSE => {
                cancel_picker(hwnd, true);
                LRESULT(0)
            }
            _ => win::def_proc(hwnd, msg, wparam, lparam),
        }
    }
}

mod lifecycle;
