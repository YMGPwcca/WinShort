//! Shortcuts for the controls.

use super::geometry::{value_control_rect, CONTROL_RADIUS};
use super::row::BODY_LEFT;
use super::state::{interaction_state, Interaction, InteractionState};
use super::surface::draw_surface;
use crate::ui::layout::{Element, ElementKind, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub(crate) fn draw_hotkey_card(r: &Renderer, element: &Element, enabled: bool) {
    let rect = element.rect.inset(1.0);
    draw_surface(r, rect, BrushRole::Card, InteractionState::Idle, 10.0);
    let text_width = (rect.w - 222.0).max(120.0);
    let stack_top = rect.y + (rect.h - 40.0) * 0.5;
    r.text_clipped(
        &element.label,
        Rect::new(rect.x + BODY_LEFT, stack_top, text_width, 20.0).d2d(),
        TextStyle::BodyStrong,
        if enabled {
            BrushRole::Text
        } else {
            BrushRole::TextSecondary
        },
    );
    r.text_clipped(
        &element.description,
        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, text_width, 18.0).d2d(),
        TextStyle::Caption,
        BrushRole::TextSecondary,
    );
}

pub(crate) fn draw_hotkey_keycap(
    r: &Renderer,
    element: &Element,
    value: &str,
    interaction: Interaction,
) {
    let rect = element.rect;
    let state = interaction_state(interaction);
    r.fill_rounded(
        rect.d2d(),
        CONTROL_RADIUS,
        if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
            BrushRole::ControlHover
        } else {
            BrushRole::BackgroundSubtle
        },
    );
    r.stroke_rounded(
        rect.d2d(),
        CONTROL_RADIUS,
        if interaction.focused {
            BrushRole::Focus
        } else {
            BrushRole::BorderStrong
        },
        if interaction.focused { 1.5 } else { 1.0 },
    );
    r.text_clipped(
        value,
        rect.d2d(),
        TextStyle::Button,
        if value.starts_with("Press") {
            BrushRole::Accent
        } else {
            BrushRole::Text
        },
    );
}

pub(crate) fn draw_shortcut_card(
    r: &Renderer,
    element: &Element,
    value: &str,
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
    let keycap = value_control_rect(rect, ElementKind::Hotkey);
    draw_surface(r, rect, surface, state, 10.0);
    let text_width = (keycap.x - rect.x - BODY_LEFT - 14.0).max(120.0);
    let keycap_role = if interaction.disabled {
        BrushRole::CardPressed
    } else {
        BrushRole::BackgroundSubtle
    };
    r.fill_rounded(keycap.d2d(), CONTROL_RADIUS, keycap_role);
    r.stroke_rounded(
        keycap.d2d(),
        CONTROL_RADIUS,
        if interaction.focused {
            BrushRole::Focus
        } else {
            BrushRole::BorderStrong
        },
        if interaction.focused { 1.5 } else { 1.0 },
    );
    r.text_clipped(
        &element.label,
        Rect::new(
            rect.x + BODY_LEFT,
            rect.y + (rect.h - 40.0) * 0.5,
            text_width,
            20.0,
        )
        .d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        &element.description,
        Rect::new(
            rect.x + BODY_LEFT,
            rect.y + (rect.h - 40.0) * 0.5 + 22.0,
            text_width,
            18.0,
        )
        .d2d(),
        TextStyle::Caption,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
    r.text_clipped(
        value,
        keycap.d2d(),
        TextStyle::Button,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else if value.starts_with("Press") {
            BrushRole::Accent
        } else {
            BrushRole::Text
        },
    );
    if interaction.focused {
        r.stroke_rounded(
            keycap.inset(-3.0).d2d(),
            CONTROL_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
}
