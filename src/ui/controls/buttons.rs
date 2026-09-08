//! Buttons for the controls.

use super::geometry::CONTROL_RADIUS;
use super::state::{interaction_state, ButtonStyle, Interaction, InteractionState};
use crate::ui::layout::{Element, ElementKind, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub(super) fn button_style(kind: ElementKind) -> ButtonStyle {
    match kind {
        ElementKind::ButtonPrimary => ButtonStyle::Primary,
        ElementKind::ButtonDanger => ButtonStyle::Danger,
        ElementKind::ButtonSecondary => ButtonStyle::Secondary,
        _ => ButtonStyle::Subtle,
    }
}

pub(crate) fn draw_button(
    r: &Renderer,
    rect: Rect,
    label: &str,
    primary: bool,
    interaction: Interaction,
) {
    draw_button_style(
        r,
        rect,
        label,
        if primary {
            ButtonStyle::Primary
        } else {
            ButtonStyle::Secondary
        },
        interaction,
    );
}

pub(crate) fn draw_button_style(
    r: &Renderer,
    rect: Rect,
    label: &str,
    style: ButtonStyle,
    interaction: Interaction,
) {
    let state = interaction_state(interaction);
    let (bg, border, text) = match style {
        ButtonStyle::Primary => (
            match state {
                InteractionState::Disabled => BrushRole::CardPressed,
                InteractionState::Pressed => BrushRole::AccentPressed,
                InteractionState::Hovered => BrushRole::AccentHover,
                _ => BrushRole::Accent,
            },
            BrushRole::Accent,
            if interaction.disabled {
                BrushRole::TextDisabled
            } else {
                BrushRole::AccentText
            },
        ),
        ButtonStyle::Danger => (
            match state {
                InteractionState::Disabled => BrushRole::CardPressed,
                InteractionState::Hovered => BrushRole::CardHover,
                _ => BrushRole::Card,
            },
            if interaction.disabled {
                BrushRole::Border
            } else {
                BrushRole::Danger
            },
            if interaction.disabled {
                BrushRole::TextDisabled
            } else {
                BrushRole::Danger
            },
        ),
        ButtonStyle::Secondary => (
            match state {
                InteractionState::Disabled => BrushRole::CardPressed,
                InteractionState::Pressed => BrushRole::CardPressed,
                InteractionState::Hovered => BrushRole::ControlHover,
                _ => BrushRole::Card,
            },
            if interaction.disabled {
                BrushRole::Border
            } else {
                BrushRole::BorderStrong
            },
            if interaction.disabled {
                BrushRole::TextDisabled
            } else {
                BrushRole::Text
            },
        ),
        ButtonStyle::Subtle => (
            match state {
                InteractionState::Disabled => BrushRole::CardPressed,
                InteractionState::Pressed => BrushRole::CardPressed,
                InteractionState::Hovered => BrushRole::ControlHover,
                _ => BrushRole::BackgroundSubtle,
            },
            BrushRole::Border,
            if interaction.disabled {
                BrushRole::TextDisabled
            } else {
                BrushRole::TextSecondary
            },
        ),
    };
    r.fill_rounded(rect.d2d(), CONTROL_RADIUS, bg);
    r.stroke_rounded(rect.d2d(), CONTROL_RADIUS, border, 1.0);
    if interaction.focused {
        r.stroke_rounded(
            rect.inset(-3.0).d2d(),
            CONTROL_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
    r.text_clipped(label, rect.d2d(), TextStyle::Button, text);
}

pub(crate) fn titlebar_glyph_bounds(rect: Rect) -> Rect {
    Rect::new(
        rect.x + (rect.w - 12.0) * 0.5,
        rect.y + (rect.h - 12.0) * 0.5,
        12.0,
        12.0,
    )
}

pub(crate) fn draw_close_button(
    r: &Renderer,
    element: &Element,
    hovered: bool,
    pressed: bool,
    focused: bool,
) {
    draw_close_button_rect(r, element.rect, hovered, pressed, focused);
}

pub(crate) fn draw_close_button_rect(
    r: &Renderer,
    rect: Rect,
    hovered: bool,
    pressed: bool,
    focused: bool,
) {
    let state = interaction_state(Interaction {
        hovered,
        pressed,
        focused,
        disabled: false,
        hover_t: 0.0,
        state_t: 0.0,
    });
    if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        r.fill_rounded(rect.d2d(), 4.0, BrushRole::Danger);
    }
    if focused {
        r.stroke_rounded(rect.inset(-2.0).d2d(), 6.0, BrushRole::Focus, 1.5);
    }
    let role = if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        BrushRole::AccentText
    } else {
        BrushRole::TextSecondary
    };
    let glyph = titlebar_glyph_bounds(rect);
    r.line(
        glyph.x + 1.5,
        glyph.y + 1.5,
        glyph.right() - 1.5,
        glyph.bottom() - 1.5,
        role,
        1.25,
    );
    r.line(
        glyph.right() - 1.5,
        glyph.y + 1.5,
        glyph.x + 1.5,
        glyph.bottom() - 1.5,
        role,
        1.25,
    );
}
