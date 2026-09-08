//! Sliders for the controls.

use super::row::{BODY_LEFT, ROW_RADIUS};
use super::state::{interaction_state, Interaction, InteractionState};
use super::surface::draw_surface;
use crate::ui::layout::{Element, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub(crate) fn slider_track_rect(row: Rect) -> Rect {
    let rect = row.inset(1.0);
    let label_end = (rect.x + rect.w * 0.34).clamp(rect.x + 150.0, rect.x + 230.0);
    let value_width = 74.0;
    let right = rect.right() - 18.0 - value_width;
    Rect::new(
        label_end,
        rect.y + rect.h * 0.5 - 3.0,
        (right - label_end - 14.0).max(100.0),
        6.0,
    )
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
    let stack_top = rect.y + (rect.h - 40.0) * 0.5;
    r.text_clipped(
        &element.label,
        Rect::new(rect.x + BODY_LEFT, stack_top, 150.0, 20.0).d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        &element.description,
        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, 150.0, 18.0).d2d(),
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
    let value_rect = Rect::new(rect.right() - 88.0, rect.y, 70.0, rect.h);
    r.text_clipped(
        label,
        value_rect.d2d(),
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
