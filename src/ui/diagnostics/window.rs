//! Window for the diagnostics.

use super::messages::diagnostics_wndproc;
use super::native::invalidate;
use super::state::DiagnosticsUi;
use crate::diagnostics::snapshot::{DiagnosticsSnapshot, SelfTestReport};
use crate::error::{Error, Result};
use crate::platform::window as win;
use crate::ui::layout::TitlebarGeometry;
use std::sync::OnceLock;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, ShowWindow, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTRIGHT,
    HTTOP, HTTOPLEFT, HTTOPRIGHT, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CLIPCHILDREN,
    WS_POPUP, WS_THICKFRAME,
};

pub(crate) const CLASS_NAME: &str = "WinShort.Diagnostics";

const DESIGN_WIDTH: f32 = 860.0;

const DESIGN_HEIGHT: f32 = 760.0;

pub(super) const WM_MOUSELEAVE: u32 = 0x02A3;

pub(super) const FOOTER_HEIGHT: f32 = 104.0;

pub(super) const HEADER_HEIGHT: f32 = 124.0;

pub(super) const ROW_HEIGHT: f32 = 30.0;

const RESIZE_BORDER: f32 = 6.0;

pub(super) const MIN_WIDTH: f32 = 720.0;

pub(super) const MIN_HEIGHT: f32 = 560.0;

static REGISTERED: OnceLock<u16> = OnceLock::new();

pub(crate) struct DiagnosticsWindow {
    pub hwnd: HWND,
}

pub(super) fn diagnostics_hit_test_dip(
    chrome: TitlebarGeometry,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
) -> u32 {
    if chrome.close.contains(x, y) {
        return HTCLIENT;
    }

    let left = x < RESIZE_BORDER;
    let right = x >= width - RESIZE_BORDER;
    let top = y < RESIZE_BORDER;
    let bottom = y >= height - RESIZE_BORDER;
    match (left, right, top, bottom) {
        (true, false, true, false) => HTTOPLEFT,
        (false, true, true, false) => HTTOPRIGHT,
        (true, false, false, true) => HTBOTTOMLEFT,
        (false, true, false, true) => HTBOTTOMRIGHT,
        (true, false, false, false) => HTLEFT,
        (false, true, false, false) => HTRIGHT,
        (false, false, true, false) => HTTOP,
        (false, false, false, true) => HTBOTTOM,
        _ => crate::ui::chrome::titlebar_hit_test_dip(chrome, x, y),
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
                WINDOW_STYLE(WS_POPUP.0 | WS_THICKFRAME.0 | WS_CLIPCHILDREN.0),
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
        win::set_application_icon(hwnd)?;
        win::apply_chrome(hwnd, crate::ui::theme::Theme::current());
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

#[cfg(test)]
mod tests {
    use super::{diagnostics_hit_test_dip, MIN_HEIGHT, MIN_WIDTH};
    use crate::ui::layout::titlebar_geometry;
    use windows::Win32::UI::WindowsAndMessaging::{
        HTBOTTOM, HTCAPTION, HTCLIENT, HTLEFT, HTTOP, HTTOPRIGHT,
    };

    #[test]
    fn diagnostics_chrome_preserves_close_caption_and_resize_hit_targets() {
        let chrome = titlebar_geometry(MIN_WIDTH, 0.0);
        assert_eq!(
            diagnostics_hit_test_dip(
                chrome,
                MIN_WIDTH,
                MIN_HEIGHT,
                chrome.close.x + chrome.close.w * 0.5,
                chrome.close.y + chrome.close.h * 0.5,
            ),
            HTCLIENT
        );
        assert_eq!(
            diagnostics_hit_test_dip(chrome, MIN_WIDTH, MIN_HEIGHT, 120.0, 20.0),
            HTCAPTION
        );
        assert_eq!(
            diagnostics_hit_test_dip(chrome, MIN_WIDTH, MIN_HEIGHT, 1.0, 240.0),
            HTLEFT
        );
        assert_eq!(
            diagnostics_hit_test_dip(chrome, MIN_WIDTH, MIN_HEIGHT, 240.0, MIN_HEIGHT - 1.0),
            HTBOTTOM
        );
        assert_eq!(
            diagnostics_hit_test_dip(chrome, MIN_WIDTH, MIN_HEIGHT, MIN_WIDTH - 1.0, 1.0),
            HTTOPRIGHT
        );
        assert_eq!(
            diagnostics_hit_test_dip(chrome, MIN_WIDTH, MIN_HEIGHT, 240.0, 1.0),
            HTTOP
        );
    }
}
