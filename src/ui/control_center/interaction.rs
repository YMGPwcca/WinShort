//! Interaction for the control center.

use super::config_toggle::ConfigToggle;
use super::native::{invalidate, start_timer};
use super::state::SettingsUi;
use crate::platform::visual::SystemVisualPreferences;
use crate::ui::animation::MotionChannel;
use crate::ui::controls::Interaction;
use crate::ui::layout::ElementId;
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn interaction(&self, id: ElementId, disabled: bool) -> Interaction {
        let toggle_value = ConfigToggle::from_element(id).map_or_else(
            || match id {
                ElementId::StartWithWindows => self.startup_enabled,

                ElementId::DebugLogging => crate::diagnostics::logging::debug_logging_enabled(),
                _ => false,
            },
            |toggle| toggle.selected(&self.draft),
        );
        Interaction {
            hovered: self.hovered == Some(id),
            pressed: self.pressed == Some(id),
            focused: self.visual_focus(id),
            disabled,
            hover_t: self.motion.value(
                id,
                MotionChannel::Hover,
                if self.hovered == Some(id) { 1.0 } else { 0.0 },
            ),
            state_t: self.motion.value(
                id,
                MotionChannel::ToggleState,
                if toggle_value { 1.0 } else { 0.0 },
            ),
        }
    }

    pub(super) fn set_hover(&mut self, hwnd: HWND, next: Option<ElementId>) {
        if next == self.hovered {
            return;
        }
        if !SystemVisualPreferences::query().animations_enabled {
            self.hovered = next;
            self.motion.clear_channel(MotionChannel::Hover);
            invalidate(hwnd);
            return;
        }
        if let Some(old) = self.hovered {
            self.motion.animate_to(old, MotionChannel::Hover, 0.0, 140);
        }
        if let Some(new) = next {
            self.motion.animate_to(new, MotionChannel::Hover, 1.0, 140);
        }
        self.hovered = next;
        start_timer(hwnd);
        invalidate(hwnd);
    }

    pub(super) fn update_hover(&mut self, hwnd: HWND, x: f32, y: f32) {
        self.set_hover(hwnd, self.layout.hit_test(x, y));
    }

    pub(super) fn animate_toggle(&mut self, hwnd: HWND, id: ElementId, value: bool) {
        if !SystemVisualPreferences::query().animations_enabled {
            self.motion.clear_channel(MotionChannel::ToggleState);
            return;
        }
        self.motion.animate_to(
            id,
            MotionChannel::ToggleState,
            if value { 1.0 } else { 0.0 },
            160,
        );
        start_timer(hwnd);
    }
}
