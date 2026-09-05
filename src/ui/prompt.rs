//! Small accessible native text prompts used by Display Profile workflows.

use std::sync::OnceLock;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow, CREATESTRUCTW, HMENU,
    SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
    WM_CREATE, WM_KEYDOWN, WM_NCDESTROY, WM_SETFOCUS, WM_SIZE, WS_BORDER, WS_CAPTION, WS_CHILD,
    WS_EX_DLGMODALFRAME, WS_EX_TOOLWINDOW, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::platform::window as win;

const CLASS_NAME: &str = "WinShort.DisplayProfilePrompt";
const EDIT_ID: usize = 1;
const OK_ID: usize = 2;
const CANCEL_ID: usize = 3;
const WIDTH: i32 = 440;
const HEIGHT: i32 = 150;
const EDIT_SUBCLASS_ID: usize = 1;
#[derive(Debug, Clone)]
pub enum PromptAction {
    RenameProfile {
        profile_id: String,
    },
    EditRoute {
        profile_id: String,
        route_index: usize,
    },
}
static REGISTERED: OnceLock<u16> = OnceLock::new();

struct PromptUi {
    action: PromptAction,
    accept_label: String,
    edit: HWND,
    closing: bool,
}

pub struct TextPrompt {
    pub hwnd: HWND,
}

impl TextPrompt {
    pub fn create(
        parent: HWND,
        action: PromptAction,
        title: &str,
        accept_label: &str,
        initial_name: &str,
    ) -> crate::error::Result<Self> {
        let _atom = win::register_class_once(&REGISTERED, CLASS_NAME, Some(prompt_wndproc))?;
        let mut parent_rect = RECT::default();
        unsafe {
            let _ = GetWindowRect(parent, &mut parent_rect);
        }
        let x = parent_rect.left + ((parent_rect.right - parent_rect.left) - WIDTH) / 2;
        let y = parent_rect.top + ((parent_rect.bottom - parent_rect.top) - HEIGHT) / 2;
        let mut state = win::WindowCreation::new(PromptUi {
            action,
            accept_label: accept_label.into(),
            edit: HWND::default(),
            closing: false,
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_DLGMODALFRAME.0 | WS_EX_TOOLWINDOW.0),
                PCWSTR(HSTRING::from(CLASS_NAME).as_ptr()),
                PCWSTR(HSTRING::from(title).as_ptr()),
                WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0),
                x,
                y,
                WIDTH,
                HEIGHT,
                Some(parent),
                None,
                None,
                Some(state.parameter()),
            )
        }
        .map_err(|error| {
            crate::error::Error::win("CreateWindowExW(display profile prompt)", &error)
        })?;
        // SAFETY: this constructor exclusively owns the newly created HWND.
        let construction = unsafe { win::WindowConstructionGuard::new(hwnd) };
        let prompt = Self {
            hwnd: construction.complete(),
        };
        if let Some(cell) = unsafe { win::state_cell::<PromptUi>(prompt.hwnd) } {
            let edit = cell.borrow().edit;
            let initial = HSTRING::from(initial_name);
            unsafe {
                let _ = SetWindowTextW(edit, PCWSTR(initial.as_ptr()));
            }
        }
        unsafe {
            let _ = ShowWindow(prompt.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(prompt.hwnd);
        }
        Ok(prompt)
    }

    pub fn close(&mut self) {
        if !self.hwnd.0.is_null() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
            self.hwnd = HWND::default();
        }
    }
}

impl Drop for TextPrompt {
    fn drop(&mut self) {
        self.close();
    }
}

unsafe extern "system" fn prompt_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        // SAFETY: WM_NCCREATE supplies the live creation slot passed to CreateWindowExW.
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        // SAFETY: ownership transfers to the window state slot exactly once.
        let Some(state) =
            (unsafe { win::WindowCreation::<PromptUi>::take_from(create.lpCreateParams) })
        else {
            return LRESULT(0);
        };
        win::store_state_ptr(hwnd, win::WindowState::new(state));
        return win::def_proc(hwnd, msg, wparam, lparam);
    }
    let Some(cell) = (unsafe { win::state_cell::<PromptUi>(hwnd) }) else {
        return win::def_proc(hwnd, msg, wparam, lparam);
    };
    if msg == WM_NCDESTROY {
        // SAFETY: this is the matching exactly-once state teardown.
        unsafe {
            drop(win::take_state::<PromptUi>(hwnd));
        }
        return win::def_proc(hwnd, msg, wparam, lparam);
    }
    match msg {
        WM_CREATE => {
            let edit = match unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    PCWSTR(HSTRING::from("EDIT").as_ptr()),
                    PCWSTR(HSTRING::from("").as_ptr()),
                    WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | WS_TABSTOP.0 | 0x0080),
                    12,
                    14,
                    WIDTH - 36,
                    28,
                    Some(hwnd),
                    Some(HMENU(EDIT_ID as *mut _)),
                    None,
                    None,
                )
            } {
                Ok(edit) => edit,
                Err(_) => return LRESULT(-1),
            };
            cell.borrow_mut().edit = edit;
            let accept_label = cell.borrow().accept_label.clone();
            let ok = match unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    PCWSTR(HSTRING::from("BUTTON").as_ptr()),
                    PCWSTR(HSTRING::from(accept_label.as_str()).as_ptr()),
                    WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | 0x0001),
                    WIDTH - 220,
                    66,
                    92,
                    30,
                    Some(hwnd),
                    Some(HMENU(OK_ID as *mut _)),
                    None,
                    None,
                )
            } {
                Ok(ok) => ok,
                Err(_) => return LRESULT(-1),
            };
            let cancel = match unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    PCWSTR(HSTRING::from("BUTTON").as_ptr()),
                    PCWSTR(HSTRING::from("Cancel").as_ptr()),
                    WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0),
                    WIDTH - 118,
                    66,
                    92,
                    30,
                    Some(hwnd),
                    Some(HMENU(CANCEL_ID as *mut _)),
                    None,
                    None,
                )
            } {
                Ok(cancel) => cancel,
                Err(_) => return LRESULT(-1),
            };
            if edit.0.is_null() || ok.0.is_null() || cancel.0.is_null() {
                return LRESULT(-1);
            }
            unsafe {
                let _ = SetWindowSubclass(
                    edit,
                    Some(prompt_edit_subclass),
                    EDIT_SUBCLASS_ID,
                    hwnd.0 as usize,
                );
            }
            unsafe {
                let _ = SetFocus(Some(edit));
            }
            LRESULT(0)
        }
        WM_SETFOCUS => {
            let edit = cell.borrow().edit;
            unsafe {
                let _ = SetFocus(Some(edit));
            }
            LRESULT(0)
        }
        WM_SIZE => {
            let width = (lparam.0 as u32 & 0xFFFF) as i32;
            let edit = cell.borrow().edit;
            unsafe {
                let _ = SetWindowPos(
                    edit,
                    None,
                    12,
                    14,
                    width.saturating_sub(36),
                    28,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let command = wparam.0 & 0xFFFF;
            if command == OK_ID {
                submit(hwnd, cell);
                LRESULT(0)
            } else if command == CANCEL_ID {
                cancel(hwnd, cell);
                LRESULT(0)
            } else {
                win::def_proc(hwnd, msg, wparam, lparam)
            }
        }
        WM_CLOSE => {
            cancel(hwnd, cell);
            LRESULT(0)
        }
        _ => win::def_proc(hwnd, msg, wparam, lparam),
    }
}
unsafe extern "system" fn prompt_edit_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    ref_data: usize,
) -> LRESULT {
    let parent = HWND(ref_data as *mut _);
    match msg {
        WM_KEYDOWN if wparam.0 as u16 == 0x0D => {
            if let Some(cell) = unsafe { win::state_cell::<PromptUi>(parent) } {
                submit(parent, cell);
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 as u16 == 0x1B => {
            if let Some(cell) = unsafe { win::state_cell::<PromptUi>(parent) } {
                cancel(parent, cell);
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(prompt_edit_subclass), EDIT_SUBCLASS_ID);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        },
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

fn submit(hwnd: HWND, cell: &std::cell::RefCell<PromptUi>) {
    let (edit, action, closing) = {
        let state = cell.borrow();
        (state.edit, state.action.clone(), state.closing)
    };
    if closing {
        return;
    }
    let length = unsafe { GetWindowTextLengthW(edit) }.max(0) as usize;
    let mut buffer = vec![0u16; length + 1];
    unsafe {
        let _ = GetWindowTextW(edit, &mut buffer);
    }
    let value = String::from_utf16_lossy(&buffer[..length]);
    if value.trim().is_empty() {
        return;
    }
    cell.borrow_mut().closing = true;
    match action {
        PromptAction::RenameProfile { profile_id } => {
            crate::event::post_main(crate::event::AppEvent::DisplayProfileRenameSubmitted {
                profile_id,
                name: value,
            });
        }
        PromptAction::EditRoute {
            profile_id,
            route_index,
        } => {
            crate::event::post_main(crate::event::AppEvent::DisplayProfileRouteEditSubmitted {
                profile_id,
                route_index,
                value,
            });
        }
    }
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

fn cancel(hwnd: HWND, cell: &std::cell::RefCell<PromptUi>) {
    if cell.borrow().closing {
        return;
    }
    cell.borrow_mut().closing = true;
    crate::event::post_main(crate::event::AppEvent::DisplayProfileRenameCancelled);
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}
