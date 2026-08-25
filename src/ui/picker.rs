//! Native, keyboard-accessible picker popups used by Settings.
//!
//! A real LISTBOX is hosted in a small owner window rather than cycling values
//! in the painted Settings row. The native list supplies selection semantics to
//! UI Automation/Narrator while the surrounding Settings surface remains D2D.

use std::sync::OnceLock;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, IsChild, SetForegroundWindow, ShowWindow, CREATESTRUCTW,
    MA_ACTIVATE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_KEYDOWN,
    WM_KILLFOCUS, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY, WS_BORDER, WS_CHILD,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

use crate::config::model::{DeviceSelection, EndpointRole, MonitorChoice, OverlayPosition};
use crate::error::{Error, Result};
use crate::platform::window as win;

const CLASS_NAME: &str = "WinShort.SettingsPicker";
const LB_ADDSTRING: u32 = 0x0180;
const LB_SETCURSEL: u32 = 0x0186;
const LB_GETCURSEL: u32 = 0x0188;
const LBN_SELCHANGE: u16 = 1;
const LBN_DBLCLK: u16 = 2;
const SUBCLASS_ID: usize = 1;
static REGISTERED: OnceLock<u16> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    InputDevice,
    OutputDevice,
    InputRole,
    OutputRole,
    OverlayPosition,
    OverlayMonitor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerValue {
    Device(DeviceSelection),
    Role(EndpointRole),
    Position(OverlayPosition),
    Monitor(MonitorChoice),
}

#[derive(Debug, Clone)]
pub struct PickerChoice {
    pub label: String,
    pub value: PickerValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PopupRect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// Place a popup below its anchor when possible, otherwise above it, then clamp
/// the complete rectangle to the monitor work area. Pure geometry is kept out
/// of Win32 calls so DPI/negative-coordinate behavior is testable.
pub fn place_popup(anchor: PopupRect, work: PopupRect, width: i32, height: i32) -> PopupRect {
    let width = width.min(work.width()).max(1);
    let height = height.min(work.height()).max(1);
    let mut left = anchor.left;
    let mut top = if work.bottom - anchor.bottom >= height {
        anchor.bottom
    } else {
        anchor.top - height
    };
    left = left.clamp(work.left, work.right - width);
    top = top.clamp(work.top, work.bottom - height);
    PopupRect::new(left, top, left + width, top + height)
}

pub struct PickerPopup {
    pub hwnd: HWND,
}

struct PickerUi {
    kind: PickerKind,
    choices: Vec<PickerChoice>,
    list: HWND,
}

impl PickerPopup {
    pub fn create(
        parent: HWND,
        kind: PickerKind,
        choices: Vec<PickerChoice>,
        current: usize,
        geometry: PopupRect,
    ) -> Result<Self> {
        let _atom = *REGISTERED.get_or_init(|| {
            win::register_class(CLASS_NAME, Some(picker_wndproc)).expect("register picker class")
        });
        let state = Box::new(PickerUi {
            kind,
            choices,
            list: HWND::default(),
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0),
                PCWSTR(HSTRING::from(CLASS_NAME).as_ptr()),
                PCWSTR(HSTRING::from("").as_ptr()),
                WINDOW_STYLE(WS_POPUP.0 | WS_BORDER.0),
                geometry.left,
                geometry.top,
                geometry.width(),
                geometry.height(),
                Some(parent),
                None,
                None,
                Some(Box::into_raw(state).cast()),
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(settings picker)", &error))?;

        let list_style =
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | WS_VSCROLL.0 | 0x0001 | 0x0040);
        let list = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(HSTRING::from("LISTBOX").as_ptr()),
                PCWSTR(HSTRING::from("").as_ptr()),
                list_style,
                4,
                4,
                geometry.width() - 8,
                geometry.height() - 8,
                Some(hwnd),
                None,
                None,
                None,
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(settings picker list)", &error))?;

        if let Some(cell) = unsafe { win::state_cell::<PickerUi>(hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.list = list;
            for choice in &ui.choices {
                let text = HSTRING::from(choice.label.as_str());
                unsafe {
                    windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        list,
                        LB_ADDSTRING,
                        Some(WPARAM(0)),
                        Some(LPARAM(text.as_ptr() as isize)),
                    );
                }
            }
            let selected = current.min(ui.choices.len().saturating_sub(1));
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_SETCURSEL,
                    Some(WPARAM(selected)),
                    Some(LPARAM(0)),
                );
                let _ = SetWindowSubclass(
                    list,
                    Some(picker_list_subclass),
                    SUBCLASS_ID,
                    hwnd.0 as usize,
                );
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(list));
            }
        } else {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return Err(Error::internal("settings picker state missing"));
        }
        Ok(Self { hwnd })
    }
}

impl Drop for PickerPopup {
    fn drop(&mut self) {
        if !self.hwnd.0.is_null() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

unsafe extern "system" fn picker_list_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    ref_data: usize,
) -> LRESULT {
    let parent = HWND(ref_data as *mut _);
    match msg {
        WM_KEYDOWN if wparam.0 as u16 == 0x1B => {
            cancel_picker(parent);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 as u16 == 0x0D => {
            commit_selected(parent, hwnd);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            commit_selected(parent, hwnd);
            result
        }
        WM_KILLFOCUS => {
            let next = HWND(lparam.0 as *mut _);
            if next != parent && next != hwnd && !unsafe { IsChild(parent, next).as_bool() } {
                cancel_picker(parent);
            }
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(picker_list_subclass), SUBCLASS_ID);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        },
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

unsafe extern "system" fn picker_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let state = Box::from_raw(create.lpCreateParams as *mut PickerUi);
            win::store_state_ptr(hwnd, win::WindowState::new(*state));
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
            WM_MOUSEACTIVATE => LRESULT(MA_ACTIVATE as isize),
            WM_COMMAND => {
                let notification = (wparam.0 & 0xFFFF) as u16;
                let source = HWND(lparam.0 as *mut _);
                if source == cell.borrow().list && notification == LBN_DBLCLK {
                    commit_selected(hwnd, source);
                    LRESULT(0)
                } else if source == cell.borrow().list && notification == LBN_SELCHANGE {
                    LRESULT(0)
                } else {
                    win::def_proc(hwnd, msg, wparam, lparam)
                }
            }
            WM_KILLFOCUS => {
                let next = HWND(lparam.0 as *mut _);
                let list = cell.borrow().list;
                if next != list && !IsChild(hwnd, next).as_bool() {
                    cancel_picker(hwnd);
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                cancel_picker(hwnd);
                LRESULT(0)
            }
            _ => win::def_proc(hwnd, msg, wparam, lparam),
        }
    }
}

fn selected_value(parent: HWND, list: HWND) -> Option<(PickerKind, PickerValue)> {
    let cell = unsafe { win::state_cell::<PickerUi>(parent) }?;
    let ui = cell.borrow();
    let index = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            list,
            LB_GETCURSEL,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
        .0 as usize
    };
    ui.choices
        .get(index)
        .map(|choice| (ui.kind, choice.value.clone()))
}

fn commit_selected(parent: HWND, list: HWND) {
    if let Some((kind, value)) = selected_value(parent, list) {
        crate::app::with_app(|app| app.commit_settings_picker(kind, value));
    }
}

fn cancel_picker(hwnd: HWND) {
    crate::app::with_app(|app| app.cancel_settings_picker(hwnd));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_prefers_below_then_flips_above() {
        let work = PopupRect::new(0, 0, 1000, 800);
        let below = place_popup(PopupRect::new(100, 100, 300, 140), work, 240, 200);
        assert_eq!(below.top, 140);
        let above = place_popup(PopupRect::new(100, 700, 300, 740), work, 240, 200);
        assert_eq!(above.bottom, 700);
    }

    #[test]
    fn popup_clamps_negative_and_right_edges() {
        let work = PopupRect::new(-500, -200, 500, 600);
        let rect = place_popup(PopupRect::new(450, 100, 480, 140), work, 300, 200);
        assert_eq!(rect.right, 500);
        assert!(rect.left >= work.left);
        assert!(rect.top >= work.top);
    }
}
