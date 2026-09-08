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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OverlayPlacementGeometry {
    pub region: Rect,
    pub preview: Rect,
    pub controls: Rect,
    pub preview_canvas: Rect,
    pub position_grid: Rect,
}

pub(crate) fn overlay_position_controls_height() -> f32 {
    PREVIEW_TITLE_HEIGHT + PREVIEW_CANVAS_MIN_HEIGHT + PREVIEW_BOTTOM_INSET
}

fn overlay_position_grid_bounds(controls: Rect) -> Rect {
    Rect::new(
        controls.x,
        controls.y + OVERLAY_PLACEMENT_HEADER_HEIGHT,
        controls.w,
        (controls.h - OVERLAY_PLACEMENT_HEADER_HEIGHT - PREVIEW_BOTTOM_INSET).max(1.0),
    )
}

pub(crate) fn overlay_position_grid_rect(controls: Rect, index: usize) -> Rect {
    let column = index % 3;
    let row = index / 3;
    let grid = overlay_position_grid_bounds(controls);
    let cell_width = (grid.w - OVERLAY_POSITION_GRID_GAP * 2.0).max(3.0) / 3.0;
    let cell_height = (grid.h - OVERLAY_POSITION_GRID_GAP * 2.0).max(1.0) / 3.0;
    let row_step = cell_height + OVERLAY_POSITION_GRID_GAP;
    Rect::new(
        grid.x + column as f32 * (cell_width + OVERLAY_POSITION_GRID_GAP),
        grid.y + row as f32 * row_step,
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
    let preview = Rect::new(preview.x, preview.y, preview.w, height);
    let controls = Rect::new(position.x, position.y, position.w, height);
    OverlayPlacementGeometry {
        region: Rect::new(origin.x, origin.y, origin.w, height),
        preview_canvas: overlay_preview_canvas_rect(preview, aspect),
        position_grid: overlay_position_grid_bounds(controls),
        preview,
        controls,
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
        "Notifications",
        "Choose which changes appear in the status overlay.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    for (id, label, description) in [
        (
            ElementId::OverlayMicrophone,
            "Microphone",
            "Show microphone changes",
        ),
        (ElementId::OverlaySpeaker, "Speaker", "Show speaker changes"),
        (
            ElementId::OverlayCurrentAppAudio,
            "Current app audio",
            "Show current app audio changes",
        ),
        (
            ElementId::OverlayWorkspace,
            "Workspace",
            "Show workspace changes",
        ),
        (
            ElementId::OverlayDisplayProfile,
            "Display profile",
            "Show display profile changes",
        ),
    ] {
        add_row(layout, &mut y, id, ElementKind::Toggle, label, description);
    }
    add_heading(
        layout,
        "Size and timing",
        "Tune the status card size, treatment, and time on screen.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    for (id, kind, label, description) in [
        (
            ElementId::OverlayScale,
            ElementKind::Slider,
            "Size",
            "0.7× to 1.6×",
        ),
        (
            ElementId::OverlayBlur,
            ElementKind::Slider,
            "Blur",
            "Five-stop background treatment",
        ),
        (
            ElementId::OverlayDuration,
            ElementKind::Slider,
            "Duration",
            "Show for the configured seconds",
        ),
    ] {
        let slider_y = y;
        add_element(
            layout,
            &mut y,
            id,
            kind,
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
