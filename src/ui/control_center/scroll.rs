//! Scroll for the control center.

use super::state::SettingsUi;
use windows::Win32::Foundation::HWND;

const WHEEL_SCROLL_DIP: f32 = 80.0;

pub(super) fn scroll_after_wheel(scroll: f32, delta: f32, max_scroll: f32) -> f32 {
    (scroll - delta / 120.0 * WHEEL_SCROLL_DIP).clamp(0.0, max_scroll)
}

pub(super) fn page_scroll_target(scroll: f32, page: f32, max_scroll: f32, forward: bool) -> f32 {
    if forward {
        (scroll + page).min(max_scroll)
    } else {
        (scroll - page).max(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum SettingsWheelAction {
    ClosePicker(HWND),
    Scroll(f32),
}

pub(super) fn settings_wheel_action(
    picker_hwnd: Option<HWND>,
    scroll: f32,
    delta: f32,
    max_scroll: f32,
) -> SettingsWheelAction {
    picker_hwnd.map_or_else(
        || SettingsWheelAction::Scroll(scroll_after_wheel(scroll, delta, max_scroll)),
        SettingsWheelAction::ClosePicker,
    )
}

impl SettingsUi {
    pub(super) fn reset_scroll(&mut self) {
        self.scroll = 0.0;
    }

    pub(super) fn set_scroll(&mut self, target: f32, hwnd: HWND) {
        self.scroll = target.clamp(0.0, self.layout.max_scroll);
        self.rebuild_layout(hwnd);
    }
}
