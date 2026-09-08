//! Sliders for the controls.

use super::row::{BODY_LEFT, ROW_RADIUS};
use super::state::{interaction_state, Interaction, InteractionState};
use super::surface::draw_surface;
use crate::ui::layout::{Element, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

const SLIDER_DESCRIPTION_WIDTH: f32 = 300.0;
const SLIDER_DESCRIPTION_TRACK_GAP: f32 = 24.0;
const SLIDER_TRACK_VALUE_GAP: f32 = 18.0;
const SLIDER_TRACK_REDUCTION: f32 = 2.0 / 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SliderClusterGeometry {
    pub(crate) label: Rect,
    pub(crate) description: Rect,
    pub(crate) track: Rect,
    pub(crate) value: Rect,
}

pub(crate) fn slider_cluster_geometry(row: Rect) -> SliderClusterGeometry {
    let rect = row.inset(1.0);
    let stack_top = rect.y + (rect.h - 40.0) * 0.5;
    let text_x = rect.x + BODY_LEFT;
    let value = Rect::new(rect.right() - 88.0, rect.y, 70.0, rect.h);
    let description_width =
        SLIDER_DESCRIPTION_WIDTH.min((value.x - SLIDER_DESCRIPTION_TRACK_GAP - text_x).max(1.0));
    let label = Rect::new(text_x, stack_top, description_width, 20.0);
    let description = Rect::new(text_x, stack_top + 22.0, description_width, 18.0);

    // Keep the existing slider column as the reference span, then center the
    // shortened track inside the space left by the wider help text and value.
    let reference_left = (rect.x + rect.w * 0.34).clamp(rect.x + 150.0, rect.x + 230.0);
    let reference_right = value.x - 14.0;
    let reference_width = (reference_right - reference_left).max(1.0);
    let track_width = (reference_width * SLIDER_TRACK_REDUCTION).min(
        (value.x - SLIDER_TRACK_VALUE_GAP - description.right() - SLIDER_DESCRIPTION_TRACK_GAP)
            .max(1.0),
    );
    let available_left = description.right() + SLIDER_DESCRIPTION_TRACK_GAP;
    let available_right = value.x - SLIDER_TRACK_VALUE_GAP;
    let centered_left = reference_left + (reference_width - track_width) * 0.5;
    let track_x = if available_right >= available_left + track_width {
        centered_left.clamp(available_left, available_right - track_width)
    } else {
        available_left.min(available_right)
    };
    let track = Rect::new(track_x, rect.y + rect.h * 0.5 - 3.0, track_width, 6.0);

    SliderClusterGeometry {
        label,
        description,
        track,
        value,
    }
}

pub(crate) fn slider_track_rect(row: Rect) -> Rect {
    slider_cluster_geometry(row).track
}

pub(super) fn draw_slider_cluster(
    r: &Renderer,
    element: &Element,
    ratio: f32,
    label: &str,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    draw_surface(
        r,
        rect,
        if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
            BrushRole::CardHover
        } else {
            BrushRole::Card
        },
        state,
        ROW_RADIUS,
    );
    let geometry = slider_cluster_geometry(element.rect);
    r.text_clipped(
        &element.label,
        geometry.label.d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        &element.description,
        geometry.description.d2d(),
        TextStyle::Caption,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
    let track = slider_track_rect(element.rect);
    r.fill_rounded(
        track.d2d(),
        3.0,
        if interaction.disabled {
            BrushRole::Border
        } else {
            BrushRole::BorderStrong
        },
    );
    let filled = Rect::new(track.x, track.y, track.w * ratio.clamp(0.0, 1.0), track.h);
    if filled.w > 0.0 {
        r.fill_rounded(
            filled.d2d(),
            3.0,
            if interaction.disabled {
                BrushRole::BorderStrong
            } else if matches!(state, InteractionState::Pressed) {
                BrushRole::AccentPressed
            } else {
                BrushRole::Accent
            },
        );
    }
    let knob_x = track.x + track.w * ratio.clamp(0.0, 1.0);
    let radius = if matches!(state, InteractionState::Pressed) {
        8.0
    } else {
        7.0
    };
    r.ellipse(
        knob_x,
        track.y + track.h * 0.5,
        radius,
        radius,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Accent
        },
        true,
        0.0,
    );
    r.text_clipped(
        label,
        geometry.value.d2d(),
        TextStyle::CaptionRight,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    if interaction.focused {
        r.stroke_rounded(
            rect.inset(-2.0).d2d(),
            ROW_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
}
