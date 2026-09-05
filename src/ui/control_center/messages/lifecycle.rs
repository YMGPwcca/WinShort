//! Lifecycle message handling for the control center.

use super::super::appearance::settings_theme;
use super::super::chrome::apply_chrome;
use super::super::native::invalidate;
use super::super::placement::fixed_window_size;

use super::super::state::SettingsUi;
use super::super::window::UI_TIMER;

use std::time::Instant;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};

use windows::Win32::UI::WindowsAndMessaging::{
    KillTimer, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
};

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_killfocus(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
    wparam: WPARAM,
) -> LRESULT {
    {
        let next = HWND(wparam.0 as *mut _);
        let picker_to_close = {
            let mut ui = cell.borrow_mut();
            ui.on_window_focus(hwnd, false, next);
            if ui
                .focus
                .picker_window()
                .is_some_and(|picker| picker != next)
                && ui.focus.picker_list_window() != Some(next)
            {
                ui.focus.picker_window()
            } else {
                None
            }
        };
        if let Some(popup_hwnd) = picker_to_close {
            crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
                popup_hwnd: popup_hwnd.0 as isize,
                restore_focus: false,
            });
        }
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_close(cell: &std::cell::RefCell<SettingsUi>) -> LRESULT {
    {
        let (should_close, capture_armed) = {
            let mut ui = cell.borrow_mut();
            if !ui.begin_close() {
                (false, false)
            } else {
                let capture_armed = ui.recording.is_some() || ui.capture_armed;
                ui.capture_armed = false;
                ui.recording = None;
                (true, capture_armed)
            }
        };
        if should_close {
            crate::event::post_main(crate::event::AppEvent::ControlCenterWindowClosed);
            if capture_armed {
                crate::keyboard::hook::end_capture();
            }
        }
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_paint(cell: &std::cell::RefCell<SettingsUi>, hwnd: HWND) -> LRESULT {
    {
        let _paint = crate::platform::window::PaintSession::begin(hwnd);
        if let Err(e) = cell.borrow_mut().paint(hwnd) {
            crate::error_!("settings paint failed: {e}");
        }
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_size(cell: &std::cell::RefCell<SettingsUi>, hwnd: HWND) -> LRESULT {
    {
        let picker_hwnd = cell.borrow().focus.picker_window();
        if let Some(popup_hwnd) = picker_hwnd {
            crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
                popup_hwnd: popup_hwnd.0 as isize,
                restore_focus: false,
            });
        }
        let mut ui = cell.borrow_mut();
        if let Some(renderer) = ui.renderer.as_mut() {
            let _ = renderer.resize();
        }
        ui.rebuild_layout(hwnd);
        ui.publish_automation_snapshot(hwnd);
        invalidate(hwnd);
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_dpichanged(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    {
        let picker_hwnd = cell.borrow().focus.picker_window();
        if let Some(popup_hwnd) = picker_hwnd {
            crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
                popup_hwnd: popup_hwnd.0 as isize,
                restore_focus: false,
            });
        }
        // SetWindowPos below re-enters this proc with WM_SIZE; never hold
        // the state borrow across it.
        let new_dpi = ((wparam.0 >> 16) as u32).max(96);
        {
            let mut ui = cell.borrow_mut();
            ui.dpi = new_dpi;
            if let Some(renderer) = ui.renderer.as_mut() {
                let _ = renderer.set_dpi(new_dpi);
            }
        }
        let suggested = unsafe { &*(lparam.0 as *const RECT) };
        let (fixed_width, fixed_height) = fixed_window_size(new_dpi);
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                suggested.left,
                suggested.top,
                fixed_width,
                fixed_height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        let mut ui = cell.borrow_mut();
        ui.rebuild_layout(hwnd);
        ui.publish_automation_snapshot(hwnd);
        invalidate(hwnd);
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_settingchange(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
) -> LRESULT {
    {
        let theme = settings_theme();
        let theme_applied = {
            let mut ui = cell.borrow_mut();
            if let Some(renderer) = ui.renderer.as_mut() {
                match renderer.set_theme(theme) {
                    Ok(()) => true,
                    Err(error) => {
                        crate::error_!("settings theme change failed: {error}");
                        false
                    }
                }
            } else {
                true
            }
        };
        if theme_applied {
            apply_chrome(hwnd, theme);
        }
        invalidate(hwnd);
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_timer(cell: &std::cell::RefCell<SettingsUi>, hwnd: HWND) -> LRESULT {
    {
        let mut ui = cell.borrow_mut();
        if ui.recording.is_some() {
            while let Some(chord) = crate::keyboard::hook::take_captured_chord() {
                let before = ui.draft.clone();
                ui.finish_recording(chord);
                if ui.recording.is_none()
                    && ui.validation.is_empty()
                    && ui.draft != before
                    && !ui.display_draft_dirty
                {
                    ui.commit_local_change(hwnd, before);
                }
            }
            if ui.recording.is_none() {
                crate::keyboard::hook::end_capture();
                ui.capture_armed = false;
            }
        }
        let active = ui.motion.tick();
        let caret_active = ui.tick_search_caret(Instant::now());
        let applied = ui.applied_until.is_some_and(|until| Instant::now() < until);
        invalidate(hwnd);
        if !active && !caret_active && !applied && ui.recording.is_none() {
            let _ = unsafe { KillTimer(Some(hwnd), UI_TIMER) };
        }
        LRESULT(0)
    }
}
