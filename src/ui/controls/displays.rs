//! Displays for the controls.

use super::state::{interaction_state, Interaction, InteractionState};
use crate::ui::layout::{Element, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub(crate) fn draw_display_route_card(
    r: &Renderer,
    element: &Element,
    card: &crate::ui::presentation::DisplayOutputCard,
    interaction: Interaction,
) {
    let primary = card.primary.as_str();
    let detail = card.detail.as_str();
    let selected = card.selected;
    let available = card.available;
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    let fill = if selected {
        BrushRole::BackgroundSubtle
    } else if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        BrushRole::CardHover
    } else {
        BrushRole::Card
    };
    r.fill_rounded(rect.d2d(), 10.0, fill);
    r.stroke_rounded(
        rect.d2d(),
        10.0,
        if !available {
            BrushRole::Warning
        } else if selected {
            BrushRole::Accent
        } else {
            BrushRole::Border
        },
        if selected { 1.5 } else { 1.0 },
    );
    let box_rect = Rect::new(rect.x + 16.0, rect.y + 22.0, 18.0, 18.0);
    r.stroke_rounded(
        box_rect.d2d(),
        4.0,
        if available {
            BrushRole::Accent
        } else {
            BrushRole::Warning
        },
        1.5,
    );
    if selected {
        r.line(
            rect.x + 20.0,
            rect.y + 31.0,
            rect.x + 24.0,
            rect.y + 35.0,
            BrushRole::Accent,
            1.8,
        );
        r.line(
            rect.x + 24.0,
            rect.y + 35.0,
            rect.x + 31.0,
            rect.y + 26.0,
            BrushRole::Accent,
            1.8,
        );
    }
    r.text_clipped(
        primary,
        Rect::new(rect.x + 48.0, rect.y + 10.0, rect.w - 64.0, 22.0).d2d(),
        TextStyle::BodyStrong,
        BrushRole::Text,
    );
    r.text_clipped(
        detail,
        Rect::new(rect.x + 48.0, rect.y + 35.0, rect.w - 64.0, 20.0).d2d(),
        TextStyle::Caption,
        if available {
            BrushRole::TextSecondary
        } else {
            BrushRole::Warning
        },
    );
    if interaction.focused {
        r.stroke_rounded(rect.inset(-3.0).d2d(), 13.0, BrushRole::Focus, 1.5);
    }
}
