//! Values for the controls.

use super::geometry::{control_rect, value_text_rect_for, CONTROL_RADIUS};
use super::state::{interaction_state, Interaction, InteractionState};
use crate::ui::layout::{ElementKind, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub(super) fn draw_toggle(r: &Renderer, row: Rect, value: bool, interaction: Interaction) {
    let rect = control_rect(row, 48.0);
    let state = interaction_state(interaction);
    let track = match state {
        InteractionState::Disabled => BrushRole::Border,
        InteractionState::Pressed => {
            if value {
                BrushRole::AccentPressed
            } else {
                BrushRole::CardPressed
            }
        }
        InteractionState::Hovered => {
            if value {
                BrushRole::AccentHover
            } else {
                BrushRole::ControlHover
            }
        }
        InteractionState::Focused | InteractionState::Idle => {
            if value {
                BrushRole::Accent
            } else {
                BrushRole::Border
            }
        }
    };
    r.fill_rounded(rect.d2d(), 17.0, track);
    let outline = match state {
        InteractionState::Focused => BrushRole::Focus,
        InteractionState::Pressed => BrushRole::AccentPressed,
        InteractionState::Hovered => BrushRole::BorderStrong,
        InteractionState::Disabled | InteractionState::Idle => BrushRole::BorderStrong,
    };
    r.stroke_rounded(
        rect.d2d(),
        17.0,
        outline,
        if matches!(state, InteractionState::Focused) {
            1.5
        } else {
            1.0
        },
    );
    let t = interaction.state_t.clamp(0.0, 1.0);
    let knob_x = rect.x + 12.0 + (rect.w - 24.0) * t;
    r.ellipse(
        knob_x,
        rect.y + rect.h * 0.5,
        7.0,
        7.0,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else if value {
            BrushRole::AccentText
        } else {
            BrushRole::TextSecondary
        },
        true,
        0.0,
    );
}

pub(super) fn draw_value_box(
    r: &Renderer,
    row: Rect,
    text: &str,
    kind: ElementKind,
    control_width: f32,
    interaction: Interaction,
) {
    let rect = control_rect(row, control_width);
    let state = interaction_state(interaction);
    let bg = match state {
        InteractionState::Disabled | InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::ControlHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::BackgroundSubtle,
    };
    let border = match state {
        InteractionState::Disabled => BrushRole::Border,
        InteractionState::Pressed => BrushRole::AccentPressed,
        InteractionState::Focused => BrushRole::Focus,
        InteractionState::Hovered => BrushRole::BorderStrong,
        InteractionState::Idle => BrushRole::Border,
    };
    r.fill_rounded(rect.d2d(), CONTROL_RADIUS, bg);
    r.stroke_rounded(
        rect.d2d(),
        CONTROL_RADIUS,
        border,
        if matches!(state, InteractionState::Focused) {
            1.75
        } else {
            1.0
        },
    );
    let role = if interaction.disabled {
        BrushRole::TextDisabled
    } else if matches!(kind, ElementKind::Hotkey) && text.starts_with("Press") {
        BrushRole::Accent
    } else {
        BrushRole::Text
    };
    r.text_clipped(
        text,
        value_text_rect_for(row, kind, control_width).d2d(),
        TextStyle::Value,
        role,
    );
    if matches!(kind, ElementKind::Value) {
        let x = rect.right() - 15.0;
        let y = rect.y + rect.h * 0.5;
        let chevron = match state {
            InteractionState::Disabled => BrushRole::TextDisabled,
            InteractionState::Focused => BrushRole::Focus,
            InteractionState::Pressed | InteractionState::Hovered => BrushRole::Text,
            InteractionState::Idle => BrushRole::TextSecondary,
        };
        r.line(x - 3.0, y - 2.0, x, y + 1.0, chevron, 1.25);
        r.line(x, y + 1.0, x + 3.0, y - 2.0, chevron, 1.25);
    }
}
