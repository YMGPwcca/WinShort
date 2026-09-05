//! Choices for the controls.

use super::geometry::CONTROL_RADIUS;
use super::state::{interaction_state, Interaction, InteractionState};
use crate::ui::layout::{Element, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub(crate) fn draw_choice(
    r: &Renderer,
    element: &Element,
    selected: bool,
    interaction: Interaction,
    radio: bool,
) {
    draw_labeled_choice(r, element, &element.label, selected, interaction, radio);
}

pub(crate) fn draw_labeled_choice(
    r: &Renderer,
    element: &Element,
    label: &str,
    selected: bool,
    interaction: Interaction,
    radio: bool,
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
        InteractionState::Focused | InteractionState::Idle => BrushRole::BackgroundSubtle,
    };
    r.fill_rounded(rect.d2d(), CONTROL_RADIUS, surface);
    r.stroke_rounded(rect.d2d(), CONTROL_RADIUS, BrushRole::Border, 1.0);
    let center_x = rect.x + 20.0;
    let center_y = rect.y + rect.h * 0.5;
    let stroke = if interaction.disabled {
        BrushRole::TextDisabled
    } else if selected {
        BrushRole::Accent
    } else {
        BrushRole::BorderStrong
    };
    if radio {
        r.ellipse(center_x, center_y, 8.0, 8.0, stroke, false, 1.5);
        if selected {
            r.ellipse(center_x, center_y, 4.0, 4.0, stroke, true, 0.0);
        }
    } else {
        let box_rect = Rect::new(center_x - 8.0, center_y - 8.0, 16.0, 16.0);
        r.stroke_rounded(box_rect.d2d(), 3.0, stroke, 1.5);
        if selected {
            r.line(
                center_x - 4.0,
                center_y,
                center_x - 1.0,
                center_y + 4.0,
                stroke,
                1.8,
            );
            r.line(
                center_x - 1.0,
                center_y + 4.0,
                center_x + 5.0,
                center_y - 4.0,
                stroke,
                1.8,
            );
        }
    }
    r.text_clipped(
        label,
        Rect::new(rect.x + 40.0, rect.y + 5.0, rect.w - 54.0, rect.h - 10.0).d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    if interaction.focused {
        r.stroke_rounded(
            rect.inset(-3.0).d2d(),
            CONTROL_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
}
