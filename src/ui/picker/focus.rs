//! Focus for the picker.

use super::selection::selected_value;
use super::window::{PickerCloseAction, PickerUi, LB_ITEMFROMPOINT, WM_APP_PICKER_FOCUS_LOST};
use crate::platform::window as win;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::WindowsAndMessaging::{IsChild, PostMessageW};

pub(super) fn picker_focus_snapshot(cell: &std::cell::RefCell<PickerUi>) -> (HWND, u64) {
    let ui = cell.borrow();
    (ui.list, ui.generation)
}

pub(super) fn defer_focus_loss(parent: HWND, next: HWND) {
    let Some(cell) = (unsafe { win::state_cell::<PickerUi>(parent) }) else {
        return;
    };
    let (list, generation) = picker_focus_snapshot(cell);
    let inside = next == parent || next == list || unsafe { IsChild(parent, next).as_bool() };
    if inside {
        return;
    }
    unsafe {
        let _ = PostMessageW(
            Some(parent),
            WM_APP_PICKER_FOCUS_LOST,
            WPARAM(next.0 as usize),
            LPARAM(generation as isize),
        );
    }
}

pub(super) fn should_close_after_focus_loss(
    current_generation: u64,
    message_generation: u64,
    focus_is_internal: bool,
) -> bool {
    current_generation == message_generation && !focus_is_internal
}

pub(super) fn picker_item_from_point(hwnd: HWND, lparam: LPARAM) -> Option<usize> {
    let result = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            hwnd,
            LB_ITEMFROMPOINT,
            Some(WPARAM(0)),
            Some(lparam),
        )
    };

    let packed = result.0 as usize;
    if ((packed >> 16) & 0xFFFF) != 0 {
        None
    } else {
        Some(packed & 0xFFFF)
    }
}

pub(super) fn set_picker_hover(parent: HWND, list: HWND, hovered: Option<usize>) {
    let Some(cell) = (unsafe { win::state_cell::<PickerUi>(parent) }) else {
        return;
    };
    let changed = {
        let mut ui = cell.borrow_mut();
        let next = hovered.filter(|index| *index < ui.choices.len());
        if ui.hovered_index == next {
            false
        } else {
            ui.hovered_index = next;
            true
        }
    };
    if changed {
        unsafe {
            let _ = InvalidateRect(Some(list), None, false);
        }
    }
}

pub(super) fn claim_close(cell: &std::cell::RefCell<PickerUi>, action: PickerCloseAction) -> bool {
    let mut ui = cell.borrow_mut();
    if ui.close_action.is_some() {
        return false;
    }
    ui.close_action = Some(action);
    true
}

pub(super) fn commit_selected(parent: HWND, list: HWND) {
    let Some((kind, value)) = selected_value(parent, list) else {
        return;
    };
    let Some(cell) = (unsafe { win::state_cell::<PickerUi>(parent) }) else {
        return;
    };
    if !claim_close(cell, PickerCloseAction::Commit) {
        return;
    }
    crate::event::post_main(crate::event::AppEvent::CommitSettingsPicker { kind, value });
}

pub(super) fn cancel_picker(hwnd: HWND, restore_focus: bool) {
    let Some(cell) = (unsafe { win::state_cell::<PickerUi>(hwnd) }) else {
        return;
    };
    if !claim_close(cell, PickerCloseAction::Cancel) {
        return;
    }
    crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
        popup_hwnd: hwnd.0 as isize,
        restore_focus,
    });
}
