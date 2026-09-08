//! Overlay dimensions, monitor selection and placement calculations.

use super::timeline::WindowRegion;
use crate::config::model::{MonitorChoice, OverlayPosition};
use windows::Win32::Foundation::{POINT, RECT, SIZE};

const BASE_WIDTH: f32 = 372.0;

pub(super) const ROW_HEIGHT: f32 = 62.0;

pub(super) const PAD: f32 = 16.0;

pub(super) const CARD_CORNER_RADIUS_DIP: f32 = 14.0;

/// The existing card padding is also the separation between independent
/// cards. This keeps the stack rhythm aligned with the renderer's content
/// geometry instead of introducing a second spacing scale.
pub(super) const STACK_GAP_DIP: f32 = PAD;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SurfaceGeometry {
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) body_left: f32,
    pub(super) body_top: f32,
    pub(super) body_right: f32,
    pub(super) body_bottom: f32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LayoutInput {
    pub(super) size: SIZE,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CardPlacement {
    pub(super) position: POINT,
}

pub(super) fn surface_geometry(scale: f32, row_count: usize) -> SurfaceGeometry {
    let scale = scale.clamp(0.7, 1.6);
    let body_width = BASE_WIDTH * scale;
    let body_height = (PAD * 2.0 + ROW_HEIGHT * row_count as f32) * scale;
    SurfaceGeometry {
        width: body_width,
        height: body_height,
        body_left: 0.0,
        body_top: 0.0,
        body_right: body_width,
        body_bottom: body_height,
    }
}

pub(super) fn window_region_for(size: SIZE, dpi: u32) -> WindowRegion {
    let scale = dpi.max(96) as f32 / 96.0;
    WindowRegion {
        width: size.cx,
        height: size.cy,
        inset: 0,
        corner_diameter: (CARD_CORNER_RADIUS_DIP * 2.0 * scale).round().max(2.0) as i32,
    }
}

pub(super) fn select_monitor(
    choice: MonitorChoice,
) -> Option<crate::platform::monitor::MonitorGeometry> {
    match choice {
        MonitorChoice::Primary => crate::platform::monitor::primary(),
        MonitorChoice::Cursor => crate::platform::monitor::cursor(),
        MonitorChoice::Device(name) => {
            let found = crate::platform::monitor::all()
                .into_iter()
                .find(|m| m.device_name == name);
            if found.is_none() {
                crate::warn_!("overlay monitor {name} not present; using primary");
            }
            found.or_else(crate::platform::monitor::primary)
        }
    }
}

pub(super) fn position_for(
    work: windows::Win32::Foundation::RECT,
    size: SIZE,
    position: OverlayPosition,
    dpi: u32,
) -> POINT {
    let margin = (22.0 * dpi as f32 / 96.0).round() as i32;
    let left = work.left + margin;
    let right = work.right - margin - size.cx;
    let top = work.top + margin;
    let bottom = work.bottom - margin - size.cy;
    let center_x = work.left + ((work.right - work.left) - size.cx) / 2;
    let center_y = work.top + ((work.bottom - work.top) - size.cy) / 2;
    let (x, y) = match position {
        OverlayPosition::TopLeft => (left, top),
        OverlayPosition::TopCenter => (center_x, top),
        OverlayPosition::TopRight => (right, top),
        OverlayPosition::CenterLeft => (left, center_y),
        OverlayPosition::Center => (center_x, center_y),
        OverlayPosition::CenterRight => (right, center_y),
        OverlayPosition::BottomLeft => (left, bottom),
        OverlayPosition::BottomCenter => (center_x, bottom),
        OverlayPosition::BottomRight => (right, bottom),
    };
    let x = x.clamp(work.left, (work.right - size.cx).max(work.left));
    let y = y.clamp(work.top, (work.bottom - size.cy).max(work.top));
    POINT { x, y }
}

/// Lay out a stable permanent lane followed by a newest-first toast lane.
///
/// Top and center anchors stack down; bottom anchors stack up. For all three
/// center positions, downward stacking is the deterministic direction away
/// from the center anchor. The caller supplies permanent entries first and
/// toasts in newest-first order, so inserting or removing a toast never
/// changes a permanent card's slot.
pub(super) fn layout_cards(
    work: RECT,
    size_position: OverlayPosition,
    dpi: u32,
    scale: f32,
    inputs: &[LayoutInput],
) -> Vec<CardPlacement> {
    let down = !matches!(
        size_position,
        OverlayPosition::BottomLeft | OverlayPosition::BottomCenter | OverlayPosition::BottomRight
    );
    let gap = (STACK_GAP_DIP * scale.clamp(0.7, 1.6) * dpi as f32 / 96.0)
        .round()
        .max(1.0) as i32;
    let mut cursor = None;

    inputs
        .iter()
        .map(|input| {
            let anchor = position_for(work, input.size, size_position, dpi);
            let y = match cursor {
                None => anchor.y,
                Some(previous_edge) if down => previous_edge + gap,
                Some(previous_edge) => previous_edge - gap - input.size.cy,
            };
            let placement = CardPlacement {
                position: POINT { x: anchor.x, y },
            };
            cursor = Some(if down { y + input.size.cy } else { y });
            placement
        })
        .collect()
}

impl SurfaceGeometry {
    pub(super) fn pixel_size(self, dpi: u32) -> SIZE {
        let scale = dpi as f32 / 96.0;
        SIZE {
            cx: (self.width * scale).ceil() as i32,
            cy: (self.height * scale).ceil() as i32,
        }
    }
}
