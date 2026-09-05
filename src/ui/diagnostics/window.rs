//! Window for the diagnostics.

use super::messages::diagnostics_wndproc;
use super::native::invalidate;
use super::state::DiagnosticsUi;
use crate::diagnostics::snapshot::{DiagnosticsSnapshot, SelfTestReport};
use crate::error::{Error, Result};
use crate::platform::window as win;
use crate::ui::theme::{Theme, ThemeMode};
use std::sync::OnceLock;
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, ShowWindow, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WS_OVERLAPPEDWINDOW,
};

pub(crate) const CLASS_NAME: &str = "WinShort.Diagnostics";

const DESIGN_WIDTH: f32 = 860.0;

const DESIGN_HEIGHT: f32 = 760.0;

pub(super) const WM_MOUSELEAVE: u32 = 0x02A3;

pub(super) const FOOTER_HEIGHT: f32 = 104.0;

pub(super) const HEADER_HEIGHT: f32 = 84.0;

pub(super) const ROW_HEIGHT: f32 = 28.0;

static REGISTERED: OnceLock<u16> = OnceLock::new();

pub(crate) struct DiagnosticsWindow {
    pub hwnd: HWND,
}

pub(super) fn apply_chrome(hwnd: HWND, theme: Theme) {
    unsafe {
        let dark: u32 = if theme.mode == ThemeMode::Dark { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let preference = DWMWCP_ROUND.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&preference as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        let color = theme.bg;
        let caption = COLORREF(color.r as u32 | ((color.g as u32) << 8) | ((color.b as u32) << 16));
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            (&caption as *const COLORREF).cast(),
            std::mem::size_of::<COLORREF>() as u32,
        );
    }
}

impl DiagnosticsWindow {
    pub(crate) fn create(snapshot: DiagnosticsSnapshot) -> Result<Self> {
        let _atom = win::register_class_once(&REGISTERED, CLASS_NAME, Some(diagnostics_wndproc))?;
        let primary = crate::platform::monitor::primary();
        let dpi = primary.as_ref().map_or(96, |monitor| monitor.dpi);
        let scale = dpi as f32 / 96.0;
        let width = (DESIGN_WIDTH * scale) as i32;
        let height = (DESIGN_HEIGHT * scale) as i32;
        let (x, y) = match &primary {
            Some(monitor) => (
                monitor.work.left + ((monitor.work.right - monitor.work.left) - width) / 2,
                monitor.work.top + ((monitor.work.bottom - monitor.work.top) - height) / 2,
            ),
            None => (0, 0),
        };
        let mut state = win::WindowCreation::new(DiagnosticsUi::new(dpi, snapshot));
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::PCWSTR(windows::core::HSTRING::from(CLASS_NAME).as_ptr()),
                windows::core::PCWSTR(
                    windows::core::HSTRING::from("WinShort Diagnostics & Support").as_ptr(),
                ),
                WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0),
                x,
                y,
                width,
                height,
                None,
                None,
                None,
                Some(state.parameter()),
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(diagnostics)", &error))?;
        // SAFETY: this constructor exclusively owns the newly created HWND.
        let construction = unsafe { win::WindowConstructionGuard::new(hwnd) };
        apply_chrome(hwnd, Theme::current());
        let hwnd = construction.complete();
        Ok(Self { hwnd })
    }

    pub(crate) fn show(&mut self) -> Result<()> {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(self.hwnd);
        }
        Ok(())
    }

    pub(crate) fn set_snapshot(
        &mut self,
        snapshot: DiagnosticsSnapshot,
        self_test: Option<SelfTestReport>,
    ) {
        if let Some(cell) = unsafe { win::state_cell::<DiagnosticsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.set_snapshot(snapshot, self_test);
            invalidate(self.hwnd);
        }
    }

    pub(crate) fn set_action_status(&mut self, status: String) {
        if let Some(cell) = unsafe { win::state_cell::<DiagnosticsUi>(self.hwnd) } {
            cell.borrow_mut().action_status = Some(status);
            invalidate(self.hwnd);
        }
    }

    pub(crate) fn set_bundle_running(&mut self, running: bool) {
        if let Some(cell) = unsafe { win::state_cell::<DiagnosticsUi>(self.hwnd) } {
            cell.borrow_mut().bundle_running = running;
            invalidate(self.hwnd);
        }
    }
}
