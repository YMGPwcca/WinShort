//! Lifecycle message handling for the picker.

use super::super::focus::{
    cancel_picker, commit_selected, picker_focus_snapshot, should_close_after_focus_loss,
};
use super::super::font::{create_picker_font, PickerFont};

use super::super::painting::draw_picker_item;
use super::super::selection::normalize_multi_selection;
use super::super::window::{
    hiword, loword, picker_item_height_px, PickerUi, LBN_DBLCLK, LBN_SELCHANGE,
};
use crate::platform::window as win;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODT_LISTBOX};

use windows::Win32::UI::WindowsAndMessaging::IsChild;

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_dpichanged(
    cell: &std::cell::RefCell<PickerUi>,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let Some(font) =
            create_picker_font(unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96))
        else {
            return win::def_proc(hwnd, msg, wparam, lparam);
        };
        let (old, list) = {
            let mut ui = cell.borrow_mut();
            let old = ui.font.replace(font);
            (old, ui.list)
        };
        drop(old);
        let _ = unsafe { InvalidateRect(Some(list), None, false) };
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_measureitem(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let measure = unsafe { &mut *(lparam.0 as *mut MEASUREITEMSTRUCT) };
        if measure.CtlType == ODT_LISTBOX {
            let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
            measure.itemHeight = picker_item_height_px(dpi);
            LRESULT(1)
        } else {
            win::def_proc(hwnd, msg, wparam, lparam)
        }
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_drawitem(
    cell: &std::cell::RefCell<PickerUi>,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
        if item.CtlType != ODT_LISTBOX {
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        let (list, hovered, label, font, multi_select, allowlist_mode) = {
            let ui = cell.borrow();
            (
                ui.list,
                ui.hovered_index == Some(item.itemID as usize),
                ui.choices
                    .get(item.itemID as usize)
                    .map(|choice| choice.label.clone()),
                ui.font.as_ref().map(PickerFont::handle).unwrap_or_default(),
                ui.kind.is_multi_select(),
                ui.kind.is_allowlist(),
            )
        };
        if item.hwndItem != list {
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        label.map_or(LRESULT(1), |value| unsafe {
            draw_picker_item(item, &value, hovered, font, multi_select, allowlist_mode)
        })
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_command(
    cell: &std::cell::RefCell<PickerUi>,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let _control_id = loword(wparam.0);
        let notification = hiword(wparam.0);
        let source = HWND(lparam.0 as *mut _);
        let (list, kind) = {
            let ui = cell.borrow();
            (ui.list, ui.kind)
        };
        if source == list && notification == LBN_DBLCLK && !kind.is_multi_select() {
            commit_selected(hwnd, source);
            LRESULT(0)
        } else if source == list && notification == LBN_SELCHANGE {
            if kind.is_multi_select() {
                normalize_multi_selection(kind, source);
            }
            LRESULT(0)
        } else {
            win::def_proc(hwnd, msg, wparam, lparam)
        }
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_app_picker_focus_lost(
    cell: &std::cell::RefCell<PickerUi>,
    hwnd: HWND,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let generation = lparam.0 as u64;
        let next = HWND(wparam.0 as *mut _);
        let (list, current_generation) = picker_focus_snapshot(cell);
        let focus_is_internal =
            next == hwnd || next == list || unsafe { IsChild(hwnd, next) }.as_bool();
        if should_close_after_focus_loss(current_generation, generation, focus_is_internal) {
            cancel_picker(hwnd, false);
        }
        LRESULT(0)
    }
}
