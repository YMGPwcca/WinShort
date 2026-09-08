//! Sliders for the control center.

use super::native::invalidate;
use super::state::SettingsUi;
use crate::config::model::{
    OverlayBlur, OVERLAY_DURATION_MAX_MS, OVERLAY_DURATION_MIN_MS, OVERLAY_DURATION_STEP_MS,
};
use crate::ui::controls;
use crate::ui::layout::{ElementId, Rect};
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn set_slider_from_ratio(&mut self, id: ElementId, ratio: f32) {
        let ratio = ratio.clamp(0.0, 1.0);
        match id {
            ElementId::OverlayDuration => {
                self.draft.overlay.duration_ms = ((OVERLAY_DURATION_MIN_MS as f32
                    + ratio * (OVERLAY_DURATION_MAX_MS - OVERLAY_DURATION_MIN_MS) as f32)
                    / OVERLAY_DURATION_STEP_MS as f32)
                    .round() as u32
                    * OVERLAY_DURATION_STEP_MS;
            }
            ElementId::OverlayBlur => {
                self.draft.overlay.blur = OverlayBlur::from_index((ratio * 4.0).round() as usize);
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
            ElementId::OverlayDuration => {
                (value - OVERLAY_DURATION_MIN_MS as f64)
                    / (OVERLAY_DURATION_MAX_MS - OVERLAY_DURATION_MIN_MS) as f64
            }
            ElementId::OverlayBlur => value / 4.0,
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
        self.set_slider_from_ratio(id, Self::slider_ratio_from_x(element.rect, x));
    }

    pub(super) fn slider_ratio_from_x(row: Rect, x: f32) -> f32 {
        let track = controls::slider_track_rect(row);
        (x - track.x) / track.w
    }

    pub(super) fn slider_value(id: ElementId, current: f32, step: f32) -> f32 {
        if step.is_infinite() {
            return match (id, step.is_sign_negative()) {
                (ElementId::OverlayDuration, true) => OVERLAY_DURATION_MIN_MS as f32,
                (ElementId::OverlayDuration, false) => OVERLAY_DURATION_MAX_MS as f32,
                (ElementId::OverlayBlur, true) => 0.0,
                (ElementId::OverlayBlur, false) => 4.0,
                (ElementId::OverlayScale, true) => 0.7,
                (ElementId::OverlayScale, false) => 1.6,
                _ => current,
            };
        }
        match id {
            ElementId::OverlayDuration => (current + step * OVERLAY_DURATION_STEP_MS as f32)
                .round()
                .clamp(
                    OVERLAY_DURATION_MIN_MS as f32,
                    OVERLAY_DURATION_MAX_MS as f32,
                ),
            ElementId::OverlayBlur => {
                let step = if step.abs() > 1.0 {
                    step.signum() * 2.0
                } else {
                    step
                };
                (current + step).round().clamp(0.0, 4.0)
            }
            ElementId::OverlayScale => (current + step * 0.1).clamp(0.7, 1.6),
            _ => current,
        }
    }

    pub(super) fn adjust_focused_slider(&mut self, hwnd: HWND, vk: u16) -> bool {
        let Some(
            id @ (ElementId::OverlayDuration | ElementId::OverlayBlur | ElementId::OverlayScale),
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
            ElementId::OverlayBlur => {
                self.draft.overlay.blur = OverlayBlur::from_index(Self::slider_value(
                    id,
                    self.draft.overlay.blur.index() as f32,
                    step,
                ) as usize);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_mapping_uses_the_shared_slider_track_geometry() {
        let row = Rect::new(248.0, 100.0, 680.0, 56.0);
        let geometry = controls::slider_cluster_geometry(row);

        assert!((SettingsUi::slider_ratio_from_x(row, geometry.track.x)).abs() < f32::EPSILON);
        assert!(
            (SettingsUi::slider_ratio_from_x(row, geometry.track.right()) - 1.0).abs()
                < f32::EPSILON
        );
        assert_eq!(
            SettingsUi::slider_ratio_from_x(row, geometry.track.x + geometry.track.w * 0.25),
            0.25
        );
    }
}
