//! Audio for the controls.

use super::geometry::{control_rect, CONTROL_RADIUS, CONTROL_WIDTH};
use super::row::{BODY_LEFT, ROW_RADIUS};
use super::state::{interaction_state, Interaction, InteractionState};
use super::surface::draw_surface;
use crate::ui::layout::{Element, Rect};
use crate::ui::presentation::DeviceSelectionPresentation;
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub(crate) fn device_value_rect(row: Rect) -> Rect {
    control_rect(row, CONTROL_WIDTH)
}

pub(crate) fn draw_device_row(
    r: &Renderer,
    element: &Element,
    presentation: &DeviceSelectionPresentation,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    let surface = match state {
        InteractionState::Disabled => BrushRole::CardPressed,
        InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::CardHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,
    };
    draw_surface(r, rect, surface, state, ROW_RADIUS);
    let value_rect = device_value_rect(rect);
    let text_width = (value_rect.x - rect.x - BODY_LEFT - 14.0).max(110.0);
    let text_role = if interaction.disabled {
        BrushRole::TextDisabled
    } else {
        BrushRole::Text
    };
    let secondary_role = if interaction.disabled {
        BrushRole::TextDisabled
    } else {
        BrushRole::TextSecondary
    };
    let stack_top = rect.y + (rect.h - 40.0) * 0.5;
    r.text_clipped(
        &element.label,
        Rect::new(rect.x + BODY_LEFT, stack_top, text_width, 20.0).d2d(),
        TextStyle::BodyStrong,
        text_role,
    );
    r.text_clipped(
        &element.description,
        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, text_width, 18.0).d2d(),
        TextStyle::Caption,
        secondary_role,
    );
    let value_state = interaction_state(interaction);
    let value_background = match value_state {
        InteractionState::Disabled | InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::ControlHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::BackgroundSubtle,
    };
    r.fill_rounded(value_rect.d2d(), CONTROL_RADIUS, value_background);
    r.stroke_rounded(
        value_rect.d2d(),
        CONTROL_RADIUS,
        if interaction.focused {
            BrushRole::Focus
        } else {
            BrushRole::Border
        },
        if interaction.focused { 1.5 } else { 1.0 },
    );
    let inner = Rect::new(
        value_rect.x + 10.0,
        value_rect.y + 5.0,
        (value_rect.w - 32.0).max(1.0),
        (value_rect.h - 10.0).max(1.0),
    );
    r.text_clipped(
        &presentation.primary,
        inner.d2d(),
        TextStyle::Value,
        text_role,
    );
    let x = value_rect.right() - 14.0;
    let y = value_rect.y + value_rect.h * 0.5;
    let chevron = if interaction.disabled {
        BrushRole::TextDisabled
    } else if interaction.focused {
        BrushRole::Focus
    } else {
        BrushRole::TextSecondary
    };
    r.line(x - 3.0, y - 2.0, x, y + 1.0, chevron, 1.25);
    r.line(x, y + 1.0, x + 3.0, y - 2.0, chevron, 1.25);
}
