//! Overlay preview for the control center.

use super::state::SettingsUi;
use crate::config::model::{MonitorChoice, OverlayAppearance, OverlayBlur, OverlayPosition};

use crate::ui::layout::{overlay_placement_geometry, overlay_preview_canvas_rect, Rect as UiRect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};
use windows::Win32::Foundation::{HWND, RECT};

pub(super) fn overlay_position(index: usize) -> OverlayPosition {
    OverlayPosition::ALL[index.min(OverlayPosition::ALL.len() - 1)]
}

pub(super) fn work_area_aspect(work: Option<RECT>) -> (u32, u32) {
    work.map_or((16, 9), |work| {
        let width = work.right.saturating_sub(work.left).max(1) as u32;
        let height = work.bottom.saturating_sub(work.top).max(1) as u32;
        (width, height)
    })
}

pub(super) fn overlay_preview_aspect(choice: &MonitorChoice) -> (u32, u32) {
    let work = match choice {
        MonitorChoice::Primary => crate::platform::monitor::primary().map(|monitor| monitor.work),
        MonitorChoice::Cursor => crate::platform::monitor::cursor().map(|monitor| monitor.work),
        MonitorChoice::Device(name) => crate::platform::monitor::all()
            .into_iter()
            .find(|monitor| monitor.device_name.eq_ignore_ascii_case(name))
            .map(|monitor| monitor.work),
    };
    work_area_aspect(work)
}

pub(super) fn overlay_preview_card_rect(
    canvas: UiRect,
    position: OverlayPosition,
    scale: f32,
) -> UiRect {
    let scale = scale.clamp(0.7, 1.6);
    let margin_x = (canvas.w * 0.04).clamp(8.0, 48.0);
    let margin_y = (canvas.h * 0.04).clamp(8.0, 32.0);
    let max_width = (canvas.w - margin_x * 2.0).max(1.0);
    let max_height = (canvas.h - margin_y * 2.0).max(1.0);
    let width = (canvas.w * 0.36 * scale).min(max_width).max(1.0);
    let height = (canvas.h * 0.34 * scale).min(max_height).max(1.0);
    let x = match position {
        OverlayPosition::TopLeft | OverlayPosition::CenterLeft | OverlayPosition::BottomLeft => {
            canvas.x + margin_x
        }
        OverlayPosition::TopCenter | OverlayPosition::Center | OverlayPosition::BottomCenter => {
            canvas.x + (canvas.w - width) * 0.5
        }
        OverlayPosition::TopRight | OverlayPosition::CenterRight | OverlayPosition::BottomRight => {
            canvas.right() - margin_x - width
        }
    };
    let y = match position {
        OverlayPosition::TopLeft | OverlayPosition::TopCenter | OverlayPosition::TopRight => {
            canvas.y + margin_y
        }
        OverlayPosition::CenterLeft | OverlayPosition::Center | OverlayPosition::CenterRight => {
            canvas.y + (canvas.h - height) * 0.5
        }
        OverlayPosition::BottomLeft
        | OverlayPosition::BottomCenter
        | OverlayPosition::BottomRight => canvas.bottom() - margin_y - height,
    };
    UiRect::new(x, y, width, height)
}

pub(super) fn overlay_position_label(index: usize) -> &'static str {
    overlay_position(index).label()
}

pub(super) fn overlay_duration_label(milliseconds: u32) -> String {
    if milliseconds % 1000 == 0 {
        format!("{} s", milliseconds / 1000)
    } else {
        format!("{:.1} s", milliseconds as f32 / 1000.0)
    }
}

pub(super) fn overlay_scale_label(scale: f32) -> String {
    if (scale.fract()).abs() < f32::EPSILON {
        format!("{scale:.0}×")
    } else {
        format!("{scale:.1}×")
    }
}

pub(super) fn preview_treatment(
    appearance: OverlayAppearance,
    blur: OverlayBlur,
) -> Option<(BrushRole, f32)> {
    let material_role = match appearance {
        OverlayAppearance::Dark => BrushRole::CardPressed,
        OverlayAppearance::Light => BrushRole::BackgroundSubtle,
        OverlayAppearance::System => BrushRole::Background,
    };
    match blur {
        OverlayBlur::Transparent => None,
        OverlayBlur::BlurLight => Some((material_role, 0.25)),
        OverlayBlur::BlurMedium => Some((material_role, 0.50)),
        OverlayBlur::BlurHeavy => Some((material_role, 0.75)),
        OverlayBlur::Solid => Some((BrushRole::CardPressed, 1.0)),
    }
}

fn stroke_rounded_inside(
    renderer: &Renderer,
    rect: UiRect,
    radius: f32,
    role: BrushRole,
    width: f32,
) {
    let inset = width * 0.5;
    renderer.stroke_rounded(
        rect.inset(inset).d2d(),
        (radius - inset).max(0.0),
        role,
        width,
    );
}

impl SettingsUi {
    pub(super) fn draw_overlay_preview(&self, renderer: &Renderer, rect: UiRect) {
        let placement = overlay_placement_geometry(rect, self.overlay_preview_aspect);
        let preview = placement.preview;
        renderer.text(
            "Preview",
            UiRect::new(preview.x, preview.y + 10.0, preview.w, 24.0).d2d(),
            TextStyle::Section,
            BrushRole::Text,
        );
        let controls = placement.controls;
        renderer.text(
            "Position",
            UiRect::new(controls.x, controls.y + 8.0, controls.w, 24.0).d2d(),
            TextStyle::Section,
            BrushRole::Text,
        );
        let canvas = overlay_preview_canvas_rect(preview, self.overlay_preview_aspect);
        renderer.fill_rounded(canvas.translated_y(2.0).d2d(), 8.0, BrushRole::Shadow);
        renderer.fill_rounded(canvas.d2d(), 8.0, BrushRole::Card);
        stroke_rounded_inside(renderer, canvas, 8.0, BrushRole::BorderStrong, 1.0);
        let sample = overlay_preview_card_rect(
            canvas,
            self.draft.overlay.position,
            self.draft.overlay.scale,
        );
        if let Some((sample_surface, intensity)) =
            preview_treatment(self.draft.overlay.appearance, self.draft.overlay.blur)
        {
            renderer.fill_rounded_with_alpha(
                sample.translated_y(1.0).d2d(),
                8.0,
                BrushRole::Shadow,
                intensity,
            );
            renderer.fill_rounded_with_alpha(sample.d2d(), 8.0, sample_surface, intensity);
            stroke_rounded_inside(renderer, sample, 8.0, BrushRole::Accent, 1.0);
        }
    }

    pub(super) fn set_overlay_position(&mut self, hwnd: HWND, index: usize) {
        let position = overlay_position(index);
        if position == OverlayPosition::Center {
            return;
        }
        let before = self.draft.clone();
        self.draft.overlay.position = position;
        self.commit_local_change(hwnd, before);
    }
}

#[cfg(test)]
mod tests {
    use super::overlay_duration_label;

    #[test]
    fn overlay_duration_label_removes_redundant_decimals() {
        assert_eq!(overlay_duration_label(1000), "1 s");
        assert_eq!(overlay_duration_label(1300), "1.3 s");
        assert_eq!(overlay_duration_label(5000), "5 s");
    }
}
