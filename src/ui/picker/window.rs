//! Window for the picker.

use super::font::{create_picker_font, PickerFont};
use super::geometry::{
    clip_window_to_round_rect, picker_corner_diameter_px, picker_inset_px, picker_list_rect,
};
use super::messages::{picker_list_subclass, picker_wndproc};
use super::model::{PickerChoice, PickerKind, PickerModel, PopupRect};
use crate::error::{Error, Result};
use crate::platform::visual::SystemVisualPreferences;
use crate::platform::window as win;
use crate::ui::theme::Theme;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};

use windows::Win32::UI::Controls::SetWindowTheme;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::SetWindowSubclass;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_SHOWNA, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WS_CHILD,
    WS_CLIPCHILDREN, WS_TABSTOP, WS_VSCROLL,
};

pub(super) const PICKER_HOST_STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_CHILD.0 | WS_CLIPCHILDREN.0);

const CLASS_NAME: &str = "WinShort.ControlCenterPicker";

const LB_ADDSTRING: u32 = 0x0180;

const LB_SETCURSEL: u32 = 0x0186;

pub(super) const LB_GETCURSEL: u32 = 0x0188;

pub(super) const LBN_SELCHANGE: u16 = 1;

pub(super) const LB_SETSEL: u32 = 0x0185;

pub(super) const LB_GETSEL: u32 = 0x0187;

pub(super) const LB_GETCOUNT: u32 = 0x018A;

pub(super) const LB_GETSELCOUNT: u32 = 0x0190;

pub(super) const LB_GETSELITEMS: u32 = 0x0191;

pub(crate) const fn loword(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

pub(crate) const fn hiword(value: usize) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}

pub(super) const LBN_DBLCLK: u16 = 2;

pub(super) const SUBCLASS_ID: usize = 1;

pub(super) const WM_APP_PICKER_FOCUS_LOST: u32 = WM_APP + 4;

pub(super) const LB_ITEMFROMPOINT: u32 = 0x01A9;

pub(super) const WM_MOUSELEAVE: u32 = 0x02A3;

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

static REGISTERED: OnceLock<u16> = OnceLock::new();

pub(crate) struct PickerPopup {
    /// Child host HWND used for lifecycle and deferred close routing.
    pub hwnd: HWND,
    /// Native LISTBOX HWND that owns keyboard focus while open.
    pub list: HWND,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PickerCloseAction {
    Commit,
    Cancel,
}

pub(super) struct PickerUi {
    pub(super) generation: u64,
    pub(super) kind: PickerKind,
    pub(super) choices: Vec<PickerChoice>,
    pub(super) list: HWND,
    pub(super) close_action: Option<PickerCloseAction>,
    pub(super) hovered_index: Option<usize>,
    pub(super) font: Option<PickerFont>,
}

pub(crate) const ITEM_HEIGHT_DIP: f32 = 32.0;

pub(super) fn picker_item_height_px(dpi: u32) -> u32 {
    (ITEM_HEIGHT_DIP * dpi.max(96) as f32 / 96.0)
        .ceil()
        .max(1.0) as u32
}

impl PickerPopup {
    pub(crate) fn create(parent: HWND, model: PickerModel, geometry: PopupRect) -> Result<Self> {
        let _atom = win::register_class_once(&REGISTERED, CLASS_NAME, Some(picker_wndproc))?;
        let (kind, choices, current, selected_indices) = model.into_parts();
        let mut state = win::WindowCreation::new(PickerUi {
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
            kind,
            choices,
            list: HWND::default(),
            close_action: None,
            hovered_index: None,
            font: None,
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(HSTRING::from(CLASS_NAME).as_ptr()),
                PCWSTR(HSTRING::from("").as_ptr()),
                PICKER_HOST_STYLE,
                geometry.left,
                geometry.top,
                geometry.width(),
                geometry.height(),
                Some(parent),
                None,
                None,
                Some(state.parameter()),
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(settings picker)", &error))?;
        // SAFETY: this constructor exclusively owns the newly created HWND.
        let construction = unsafe { win::WindowConstructionGuard::new(hwnd) };
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let list_rect = picker_list_rect(geometry, dpi);
        unsafe {
            clip_window_to_round_rect(
                hwnd,
                geometry.width(),
                geometry.height(),
                picker_corner_diameter_px(dpi),
            );
        }

        let list_style = WINDOW_STYLE(
            WS_CHILD.0
                | WS_TABSTOP.0
                | WS_VSCROLL.0
                | 0x0001 // LBS_NOTIFY
                | 0x0010 // LBS_OWNERDRAWFIXED
                | 0x0040 // LBS_HASSTRINGS
                | if kind.is_multi_select() { 0x0008 } else { 0 }, // LBS_MULTIPLESEL
        );
        let list = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(HSTRING::from("LISTBOX").as_ptr()),
                PCWSTR(HSTRING::from("").as_ptr()),
                list_style,
                list_rect.left,
                list_rect.top,
                list_rect.width(),
                list_rect.height(),
                Some(hwnd),
                None,
                None,
                None,
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(settings picker list)", &error))?;
        unsafe {
            clip_window_to_round_rect(
                list,
                list_rect.width(),
                list_rect.height(),
                (picker_corner_diameter_px(dpi) - picker_inset_px(dpi) * 2).max(2),
            );
        }

        if !SystemVisualPreferences::query().high_contrast {
            let theme_name = if Theme::current().mode == crate::ui::theme::ThemeMode::Dark {
                HSTRING::from("DarkMode_Explorer")
            } else {
                HSTRING::from("Explorer")
            };
            let _ = unsafe { SetWindowTheme(list, PCWSTR(theme_name.as_ptr()), PCWSTR::null()) };
        }
        let font = create_picker_font(dpi);
        let labels = {
            let Some(cell) = (unsafe { win::state_cell::<PickerUi>(hwnd) }) else {
                return Err(Error::internal("settings picker state missing"));
            };
            let mut ui = cell.borrow_mut();
            ui.list = list;
            ui.font = font;
            ui.choices
                .iter()
                .map(|choice| choice.label.clone())
                .collect::<Vec<_>>()
        };

        for label in labels {
            let text = HSTRING::from(label);
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_ADDSTRING,
                    Some(WPARAM(0)),
                    Some(LPARAM(text.as_ptr() as isize)),
                );
            }
        }
        unsafe {
            if kind.is_multi_select() {
                for index in selected_indices {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        list,
                        LB_SETSEL,
                        Some(WPARAM(1)),
                        Some(LPARAM(index as isize)),
                    );
                }
            } else if let Some(selected) = current {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_SETCURSEL,
                    Some(WPARAM(selected)),
                    Some(LPARAM(0)),
                );
            }
            let _ = SetWindowSubclass(
                list,
                Some(picker_list_subclass),
                SUBCLASS_ID,
                hwnd.0 as usize,
            );
        }
        let hwnd = construction.complete();
        Ok(Self { hwnd, list })
    }

    pub(crate) fn activate(&self) {
        unsafe {
            // Keep the picker in the Control Center's child z-order. The
            // owner remains the active top-level window while the real
            // LISTBOX receives keyboard focus.
            if let Err(error) = SetWindowPos(
                self.hwnd,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            ) {
                crate::warn_!("picker activation placement failed: {error}");
            }
            let _ = ShowWindow(self.list, SW_SHOWNA);
            let _ = ShowWindow(self.hwnd, SW_SHOWNA);
            let _ = SetFocus(Some(self.list));
        }
    }
}
