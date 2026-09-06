//! Keyboard message handling for the control center.

use super::super::native::invalidate;

use super::super::scroll::page_scroll_target;
use super::super::shortcuts::key_down;
use super::super::state::SettingsUi;

use crate::platform::window as win;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_keydown(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let vk = wparam.0 as u16;
        {
            let mut ui = cell.borrow_mut();
            {
                let value = true;
                ui.focus.set_indicator_visible(value);
            };
            if ui.handle_search_key(hwnd, vk) {
                return LRESULT(0);
            }
            if ui.record_key(hwnd, vk, true) {
                ui.publish_automation_snapshot(hwnd);
                return LRESULT(0);
            }
            if ui.adjust_focused_slider(hwnd, vk) {
                ui.publish_automation_snapshot(hwnd);
                return LRESULT(0);
            }
        }
        match vk {
            0x09 => {
                let reverse = unsafe { key_down(0x10) };
                cell.borrow_mut().focus_next(hwnd, reverse);
                let _ = unsafe { SetFocus(Some(hwnd)) };
                let actual = unsafe { GetFocus() };
                cell.borrow_mut().sync_focus_after_set_focus(hwnd, actual);
                invalidate(hwnd);
                LRESULT(0)
            }
            0x0D | 0x20 => {
                let focused = cell.borrow_mut().focus.target();
                if let Some(id) = focused {
                    cell.borrow_mut().activate(hwnd, id);
                }
                LRESULT(0)
            }
            0x21 | 0x22 => {
                let mut ui = cell.borrow_mut();
                let page = ui.layout.content_clip.h.max(64.0);
                let target = page_scroll_target(ui.scroll, page, ui.layout.max_scroll, vk == 0x22);
                ui.set_scroll(target, hwnd);
                ui.publish_automation_snapshot(hwnd);
                invalidate(hwnd);
                LRESULT(0)
            }
            0x1B => {
                let confirmation_cleared =
                    cell.borrow_mut().interaction.confirmations_mut().clear();
                if confirmation_cleared {
                    invalidate(hwnd);
                } else if cell.borrow_mut().begin_close() {
                    crate::event::post_main(crate::event::AppEvent::ControlCenterWindowClosed);
                }
                LRESULT(0)
            }
            _ => win::def_proc(hwnd, msg, wparam, lparam),
        }
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_keyup(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let consumed = {
            let mut ui = cell.borrow_mut();
            ui.record_key(hwnd, wparam.0 as u16, false)
        };
        if consumed {
            return LRESULT(0);
        }
        win::def_proc(hwnd, msg, wparam, lparam)
    }
}
