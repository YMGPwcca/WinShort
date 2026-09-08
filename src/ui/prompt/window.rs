//! Native window ownership for the text prompt.

use super::messages::prompt_wndproc;
use super::model::{PromptAction, PromptState};
use crate::error::{Error, Result};
use crate::platform::window as win;
use std::sync::OnceLock;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SetWindowTextW, ShowWindow, CW_USEDEFAULT, SW_SHOW, WS_CAPTION,
    WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU,
};

const CLASS_NAME: &str = "WinShort.TextPrompt";
static REGISTERED: OnceLock<u16> = OnceLock::new();

pub(crate) struct TextPrompt {
    hwnd: HWND,
}

impl TextPrompt {
    pub(crate) fn create(
        owner: HWND,
        action: PromptAction,
        title: &str,
        submit_text: &str,
        initial: &str,
    ) -> Result<Self> {
        let _atom = win::register_class_once(&REGISTERED, CLASS_NAME, Some(prompt_wndproc))?;
        let mut state = win::WindowCreation::new(PromptState {
            action,
            controls: None,
            submit_text: submit_text.into(),
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                PCWSTR(HSTRING::from(CLASS_NAME).as_ptr()),
                PCWSTR(HSTRING::from(title).as_ptr()),
                WS_POPUP | WS_CAPTION | WS_SYSMENU,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                520,
                168,
                Some(owner),
                None,
                None,
                Some(state.parameter()),
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(text prompt)", &error))?;
        // SAFETY: this constructor exclusively owns the newly created HWND.
        let construction = unsafe { win::WindowConstructionGuard::new(hwnd) };
        let Some(cell) = (unsafe { win::state_cell::<PromptState>(hwnd) }) else {
            return Err(Error::internal("text prompt state missing"));
        };
        let controls = cell
            .borrow()
            .controls
            .ok_or_else(|| Error::internal("text prompt controls missing after creation"))?;
        let initial = HSTRING::from(initial);
        unsafe {
            if SetWindowTextW(controls.edit, PCWSTR(initial.as_ptr())).is_err() {
                crate::warn_!("could not initialize text prompt contents");
            }
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetFocus(Some(controls.edit));
        }
        let hwnd = construction.complete();
        Ok(Self { hwnd })
    }

    pub(crate) fn close(&mut self) {
        if !self.hwnd.0.is_null() {
            if unsafe { DestroyWindow(self.hwnd) }.is_err() {
                crate::warn_!("could not destroy text prompt window");
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
