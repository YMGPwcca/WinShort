//! Overlay for the layout.

use super::builder::{add_element, add_heading, add_region, add_row, add_section_content_gap};
use super::geometry::Rect;
use super::model::{ElementId, ElementKind, LayoutContext, RegionKind};
use super::shell::SettingsLayout;
use crate::ui::theme::UiTokens;

const PREVIEW_SIDE_INSET: f32 = 18.0;

const PREVIEW_CANVAS_MAX_WIDTH: f32 = 720.0;

const PREVIEW_CANVAS_MIN_HEIGHT: f32 = 118.0;

const PREVIEW_CANVAS_MAX_HEIGHT: f32 = 220.0;

const PREVIEW_TITLE_HEIGHT: f32 = 44.0;

const PREVIEW_BOTTOM_INSET: f32 = 18.0;

pub(super) const OVERLAY_PLACEMENT_GAP: f32 = 16.0;

const OVERLAY_PLACEMENT_HEADER_HEIGHT: f32 = 44.0;

const OVERLAY_POSITION_GRID_GAP: f32 = 8.0;

const OVERLAY_POSITION_GRID_MIN_STEP: f32 = 44.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OverlayPlacementGeometry {
    pub region: Rect,
    pub preview: Rect,
    pub controls: Rect,
}

pub(crate) fn overlay_position_controls_height() -> f32 {
    OVERLAY_PLACEMENT_HEADER_HEIGHT + OVERLAY_POSITION_GRID_MIN_STEP * 3.0
}

pub(crate) fn overlay_position_grid_rect(controls: Rect, index: usize) -> Rect {
    let column = index % 3;
    let row = index / 3;
    let cell_width = (controls.w - OVERLAY_POSITION_GRID_GAP * 2.0).max(3.0) / 3.0;
    let grid_height = (controls.h - OVERLAY_PLACEMENT_HEADER_HEIGHT).max(3.0);
    let row_step = grid_height / 3.0;
    let cell_height = (row_step - OVERLAY_POSITION_GRID_GAP).max(1.0);
    Rect::new(
        controls.x + column as f32 * (cell_width + OVERLAY_POSITION_GRID_GAP),
        controls.y + OVERLAY_PLACEMENT_HEADER_HEIGHT + row as f32 * row_step,
        cell_width,
        cell_height,
    )
}

pub(crate) fn overlay_status_row_rects(row: Rect) -> (Rect, Rect) {
    let gap = UiTokens::CARD_COLUMN_GAP;
    let half = ((row.w - gap).max(2.0)) * 0.5;
    let left = Rect::new(row.x, row.y, half, row.h);
    let right = Rect::new(left.right() + gap, row.y, half, row.h);
    (left, right)
}

pub(crate) fn overlay_placement_geometry(
    origin: Rect,
    aspect: (u32, u32),
) -> OverlayPlacementGeometry {
    let column_width = ((origin.w - OVERLAY_PLACEMENT_GAP).max(2.0)) * 0.5;
    let preview = Rect::new(origin.x, origin.y, column_width, 0.0);
    let position = Rect::new(
        preview.right() + OVERLAY_PLACEMENT_GAP,
        origin.y,
        column_width,
        overlay_position_controls_height(),
    );
    let height = overlay_preview_region_height(column_width, aspect).max(position.h);
    OverlayPlacementGeometry {
        region: Rect::new(origin.x, origin.y, origin.w, height),
        preview: Rect::new(preview.x, preview.y, preview.w, height),
        controls: Rect::new(position.x, position.y, position.w, height),
    }
}

fn add_overlay_position_elements(layout: &mut SettingsLayout, controls: Rect) {
    let mut end = controls.y;
    for index in 0..9 {
        add_element(
            layout,
            &mut end,
            ElementId::OverlayPositionCell(index),
            ElementKind::Choice,
            "Overlay position",
            "Choose this position",
            overlay_position_grid_rect(controls, index as usize),
        );
    }
}

pub(crate) fn overlay_preview_canvas_size(available_width: f32, aspect: (u32, u32)) -> (f32, f32) {
    let ratio = if aspect.0 == 0 || aspect.1 == 0 {
        16.0 / 9.0
    } else {
        aspect.0 as f32 / aspect.1 as f32
    };
    let available_width = available_width.max(1.0);
    let mut width = available_width.min(PREVIEW_CANVAS_MAX_WIDTH);
    let mut height = width / ratio;
    if height > PREVIEW_CANVAS_MAX_HEIGHT {
        height = PREVIEW_CANVAS_MAX_HEIGHT;
        width = height * ratio;
    }
    if height < PREVIEW_CANVAS_MIN_HEIGHT {
        height = PREVIEW_CANVAS_MIN_HEIGHT;
        width = height * ratio;
        if width > available_width {
            width = available_width;
            height = width / ratio;
        }
    }
    (width, height)
}

pub(crate) fn overlay_preview_region_height(content_width: f32, aspect: (u32, u32)) -> f32 {
    let (_, height) =
        overlay_preview_canvas_size((content_width - PREVIEW_SIDE_INSET * 2.0).max(1.0), aspect);
    PREVIEW_TITLE_HEIGHT + height + PREVIEW_BOTTOM_INSET
}

pub(crate) fn overlay_preview_canvas_rect(preview: Rect, aspect: (u32, u32)) -> Rect {
    let available_width = (preview.w - PREVIEW_SIDE_INSET * 2.0).max(1.0);
    let (width, height) = overlay_preview_canvas_size(available_width, aspect);
    Rect::new(
        preview.x + PREVIEW_SIDE_INSET + (available_width - width) * 0.5,
        preview.y + PREVIEW_TITLE_HEIGHT,
        width,
        height,
    )
}

pub(super) fn add_overlay(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Overlay",
        "A compact visual cue that never interrupts your work.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    let placement = overlay_placement_geometry(
        Rect::new(layout.content_column.x, y, layout.content_column.w, 0.0),
        context.overlay_preview_aspect,
    );
    add_region(
        layout,
        RegionKind::OverlayPreview,
        &mut y,
        placement.region.h,
    );
    add_overlay_position_elements(layout, placement.controls);
    let status_row = Rect::new(
        layout.content_column.x,
        y,
        layout.content_column.w,
        UiTokens::ROW_HEIGHT,
    );
    let (enabled_rect, monitor_rect) = overlay_status_row_rects(status_row);
    add_element(
        layout,
        &mut y,
        ElementId::OverlayEnabled,
        ElementKind::Toggle,
        "Show status overlay",
        "Show feedback without stealing focus",
        enabled_rect,
    );
    add_element(
        layout,
        &mut y,
        ElementId::OverlayMonitor,
        ElementKind::Value,
        "Monitor",
        "Overlay location",
        monitor_rect,
    );
    add_row(
        layout,
        &mut y,
        ElementId::OverlayAppearance,
        ElementKind::Value,
        "Overlay style",
        "Choose the status card appearance",
    );
    add_heading(
        layout,
        "Size and timing",
        "Adjust the compact status card with familiar ranges.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    for (id, label, description) in [
        (ElementId::OverlayScale, "Size", "Small, normal, or large"),
        (ElementId::OverlayOpacity, "Opacity", "Low, normal, or high"),
        (
            ElementId::OverlayDuration,
            "Duration",
            "Short, normal, or long",
        ),
    ] {
        let slider_y = y;
        add_element(
            layout,
            &mut y,
            id,
            ElementKind::Slider,
            label,
            description,
            Rect::new(
                layout.content_column.x,
                slider_y,
                layout.content_column.w,
                56.0,
            ),
        );
    }
    let preview_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::OverlayPreview,
        ElementKind::ButtonPrimary,
        "Show on screen",
        "Show the edited overlay without saving it",
        Rect::new(layout.content_column.x, preview_y, 150.0, 36.0),
    );
}
