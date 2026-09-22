//! Native operations for the Control Center.

use super::window::{UI_TIMER, UI_TIMER_MS};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::WindowsAndMessaging::SetTimer;

#[cfg(test)]
pub(super) fn close_picker_before_settings_hide<Cancel, Discard, Hide>(
    cancel_picker: Cancel,
    discard_draft: Discard,
    hide_settings: Hide,
) where
    Cancel: FnOnce(),
    Discard: FnOnce(),
    Hide: FnOnce(),
{
    cancel_picker();
    discard_draft();
    hide_settings();
}

pub(super) fn invalidate(hwnd: HWND) {
    // Repaint invalidation is best-effort: a destroyed/closing HWND needs no retry.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

pub(super) fn start_timer(hwnd: HWND) {
    if unsafe { SetTimer(Some(hwnd), UI_TIMER, UI_TIMER_MS, None) } == 0 {
        crate::warn_!("Control Center UI timer could not be started");
    }
}

pub(super) fn post_main(event: crate::event::AppEvent) {
    crate::event::post_main(event);
}
