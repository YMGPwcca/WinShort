//! Overlay dimensions, monitor selection and placement calculations.

use super::timeline::WindowRegion;
use crate::config::model::{MonitorChoice, OverlayPosition};
use windows::Win32::Foundation::{POINT, RECT, SIZE};

const BASE_WIDTH: f32 = 240.0;
pub(super) const MIN_WIDTH: f32 = 200.0;
pub(super) const MAX_WIDTH: f32 = 360.0;
pub(super) const BADGE_SIZE: f32 = 52.0;
pub(super) const TEXT_LEFT: f32 = 52.0;
pub(super) const TEXT_RIGHT: f32 = 12.0;

pub(super) const ROW_HEIGHT: f32 = 48.0;

pub(super) const PAD: f32 = 10.0;

pub(super) const CARD_CORNER_RADIUS_DIP: f32 = 12.0;

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
    presentation_geometry(scale, row_count, 0.0)
}

/// A 52-DIP square fits a 32-DIP icon badge and 10-DIP padding.
pub(super) fn presentation_geometry(scale: f32, row_count: usize, compact: f32) -> SurfaceGeometry {
    geometry_with_width(scale, row_count, BASE_WIDTH, compact)
}

pub(super) fn model_geometry(
    scale: f32,
    model: &super::model::OverlayModel,
    compact: f32,
) -> SurfaceGeometry {
    geometry_with_width(
        scale,
        model.rows.len(),
        model.width_dip.unwrap_or(BASE_WIDTH),
        compact,
    )
}

fn geometry_with_width(scale: f32, row_count: usize, width: f32, compact: f32) -> SurfaceGeometry {
    let scale = scale.clamp(0.7, 1.6);
    let compact = compact.clamp(0.0, 1.0);
    let width = width.clamp(MIN_WIDTH, MAX_WIDTH);
    let body_width = (width + (BADGE_SIZE - width) * compact) * scale;
    let expanded_height = PAD * 2.0 + ROW_HEIGHT * row_count as f32;
    let body_height = (expanded_height + (BADGE_SIZE - expanded_height) * compact) * scale;
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
    let margin = (22.0 * dpi as f32 / 96.0).round() as i64;
    let work_left = i64::from(work.left);
    let work_top = i64::from(work.top);
    let work_right = i64::from(work.right);
    let work_bottom = i64::from(work.bottom);
    let width = i64::from(size.cx);
    let height = i64::from(size.cy);
    let left = work_left + margin;
    let right = work_right - margin - width;
    let top = work_top + margin;
    let bottom = work_bottom - margin - height;
    let center_x = work_left + (work_right - work_left - width) / 2;
    let center_y = work_top + (work_bottom - work_top - height) / 2;
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
    let x = x.clamp(work_left, (work_right - width).max(work_left));
    let y = y.clamp(work_top, (work_bottom - height).max(work_top));
    POINT {
        x: x as i32,
        y: y as i32,
    }
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
    let gap = stack_gap_px(scale, dpi);
    let mut cursor: Option<i64> = None;

    inputs
        .iter()
        .map(|input| {
            let anchor = position_for(work, input.size, size_position, dpi);
            let unbounded_y = match cursor {
                None => i64::from(anchor.y),
                Some(previous_edge) if down => previous_edge + i64::from(gap),
                Some(previous_edge) => previous_edge - i64::from(gap) - i64::from(input.size.cy),
            };
            let max_y =
                (i64::from(work.bottom) - i64::from(input.size.cy)).max(i64::from(work.top));
            let y = unbounded_y.clamp(i64::from(work.top), max_y) as i32;
            let placement = CardPlacement {
                position: POINT { x: anchor.x, y },
            };
            cursor = Some(if down {
                i64::from(y) + i64::from(input.size.cy)
            } else {
                i64::from(y)
            });
            placement
        })
        .collect()
}

pub(super) fn stack_gap_px(scale: f32, dpi: u32) -> i32 {
    (STACK_GAP_DIP * scale.clamp(0.7, 1.6) * dpi as f32 / 96.0)
        .round()
        .max(1.0) as i32
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
