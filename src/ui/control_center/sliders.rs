//! Sliders for the control center.

use super::native::invalidate;
use super::state::SettingsUi;
use crate::ui::controls;
use crate::ui::layout::ElementId;
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn set_slider_from_ratio(&mut self, id: ElementId, ratio: f32) {
        let ratio = ratio.clamp(0.0, 1.0);
        match id {
            ElementId::OverlayDuration => {
                self.draft.overlay.duration_ms =
                    ((500.0 + ratio * 9500.0) / 100.0).round() as u32 * 100;
            }
            ElementId::OverlayOpacity => {
                self.draft.overlay.opacity = ((0.3 + ratio * 0.7) * 20.0).round() / 20.0;
            }
            ElementId::OverlayScale => {
                self.draft.overlay.scale = ((0.7 + ratio * 0.9) * 10.0).round() / 10.0;
            }
            _ => return,
        }
        self.validation.clear();
    }

    pub(super) fn set_slider_from_value(&mut self, id: ElementId, value: f64) -> bool {
        let ratio = match id {
            ElementId::OverlayDuration => (value - 500.0) / 9500.0,
            ElementId::OverlayOpacity => (value - 0.3) / 0.7,
            ElementId::OverlayScale => (value - 0.7) / 0.9,
            _ => return false,
        };
        self.set_slider_from_ratio(id, ratio as f32);
        true
    }

    pub(super) fn set_slider_from_x(&mut self, id: ElementId, x: f32) {
        let Some(element) = self.layout.element(id) else {
            return;
        };
        let track = controls::slider_track_rect(element.rect);
        let ratio = (x - track.x) / track.w;
        self.set_slider_from_ratio(id, ratio);
    }

    pub(super) fn slider_value(id: ElementId, current: f32, step: f32) -> f32 {
        if step.is_infinite() {
            return match (id, step.is_sign_negative()) {
                (ElementId::OverlayDuration, true) => 500.0,
                (ElementId::OverlayDuration, false) => 10_000.0,
                (ElementId::OverlayOpacity, true) => 0.3,
                (ElementId::OverlayOpacity, false) => 1.0,
                (ElementId::OverlayScale, true) => 0.7,
                (ElementId::OverlayScale, false) => 1.6,
                _ => current,
            };
        }
        match id {
            ElementId::OverlayDuration => (current + step * 100.0).round().clamp(500.0, 10_000.0),
            ElementId::OverlayOpacity => (current + step * 0.05).clamp(0.3, 1.0),
            ElementId::OverlayScale => (current + step * 0.1).clamp(0.7, 1.6),
            _ => current,
        }
    }

    pub(super) fn adjust_focused_slider(&mut self, hwnd: HWND, vk: u16) -> bool {
        let Some(
            id @ (ElementId::OverlayDuration | ElementId::OverlayOpacity | ElementId::OverlayScale),
        ) = self.focus.target()
        else {
            return false;
        };
        let before = self.draft.clone();
        let step = match vk {
            0x25 | 0x28 => -1.0,
            0x27 | 0x26 => 1.0,
            0x21 => 5.0,
            0x22 => -5.0,
            0x24 => f32::NEG_INFINITY,
            0x23 => f32::INFINITY,
            _ => return false,
        };
        match id {
            ElementId::OverlayDuration => {
                self.draft.overlay.duration_ms =
                    Self::slider_value(id, self.draft.overlay.duration_ms as f32, step) as u32;
            }
            ElementId::OverlayOpacity => {
                self.draft.overlay.opacity =
                    Self::slider_value(id, self.draft.overlay.opacity, step);
            }
            ElementId::OverlayScale => {
                self.draft.overlay.scale = Self::slider_value(id, self.draft.overlay.scale, step);
            }
            _ => return false,
        }
        if self.draft != before {
            self.commit_local_change(hwnd, before);
        }
        invalidate(hwnd);
        true
    }
}
