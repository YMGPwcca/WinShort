//! Native operations for the Control Center.

use super::window::{UI_TIMER, UI_TIMER_MS};
use crate::error::{Error, Result};
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

pub(super) fn open_config_folder() -> Result<()> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let folder = crate::config::data_dir();
    std::fs::create_dir_all(&folder)
        .map_err(|error| Error::config(format!("create config directory: {error}")))?;
    let operation = HSTRING::from("open");
    let target = HSTRING::from(folder.to_string_lossy().as_ref());
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as usize;
    if !shell_execute_succeeded(code) {
        return Err(Error::config(format!(
            "open config directory failed ({code})"
        )));
    }
    Ok(())
}

fn shell_execute_succeeded(code: usize) -> bool {
    code > 32
}

#[cfg(test)]
mod tests {
    use super::shell_execute_succeeded;

    #[test]
    fn shell_execute_uses_documented_success_boundary() {
        assert!(!shell_execute_succeeded(0));
        assert!(!shell_execute_succeeded(32));
        assert!(shell_execute_succeeded(33));
    }
}
