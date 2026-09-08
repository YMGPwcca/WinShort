//! Row for the controls.

use super::buttons::{button_style, draw_button_style};
use super::choices::draw_choice;
use super::geometry::{control_rect, CONTROL_WIDTH};
use super::shortcuts::draw_shortcut_card;
use super::sliders::draw_slider_cluster;
use super::state::{interaction_state, ControlValue, Interaction, InteractionState};
use super::surface::draw_surface;
use super::values::{draw_toggle, draw_value_box};
use crate::ui::layout::{Element, ElementId, ElementKind, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};
use crate::ui::theme::UiTokens;

pub(super) const ROW_RADIUS: f32 = UiTokens::CARD_RADIUS - 2.0;

pub(super) const BODY_LEFT: f32 = 18.0;

pub(super) const COMPACT_MONITOR_CONTROL_WIDTH: f32 = 136.0;

pub(super) fn row_control_width(element: &Element, value: &ControlValue<'_>) -> f32 {
    if element.id == ElementId::OverlayMonitor && matches!(value, ControlValue::Text(_)) {
        return COMPACT_MONITOR_CONTROL_WIDTH;
    }
    match value {
        ControlValue::Toggle(_) => 48.0,
        ControlValue::Text(_) => CONTROL_WIDTH,
        ControlValue::Slider { .. } => 70.0,
        ControlValue::Action(_) => 136.0,
    }
}

pub(crate) fn draw_row(
    r: &Renderer,
    element: &Element,
    value: ControlValue<'_>,
    interaction: Interaction,
) {
    if element.kind == ElementKind::Hotkey {
        if let ControlValue::Text(text) = value {
            draw_shortcut_card(r, element, &text, interaction);
        }
        return;
    }
    if element.kind == ElementKind::Choice {
        let selected = matches!(value, ControlValue::Toggle(true));
        draw_choice(r, element, selected, interaction, true);
        return;
    }
    if element.kind == ElementKind::Checkbox {
        let selected = matches!(value, ControlValue::Toggle(true));
        draw_choice(r, element, selected, interaction, false);
        return;
    }
    if element.kind == ElementKind::Slider {
        if let ControlValue::Slider { ratio, label } = value {
            draw_slider_cluster(r, element, ratio.clamp(0.0, 1.0), &label, interaction);
        }
        return;
    }
    if matches!(
        element.kind,
        ElementKind::ButtonPrimary | ElementKind::ButtonSecondary | ElementKind::ButtonDanger
    ) && element.rect.h <= 40.0
    {
        if let ControlValue::Action(label) = value {
            draw_button_style(
                r,
                element.rect.inset(1.0),
                &label,
                button_style(element.kind),
                interaction,
            );
        }
        return;
    }

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

    let label_role = if interaction.disabled {
        BrushRole::TextDisabled
    } else {
        BrushRole::Text
    };
    let secondary_role = if interaction.disabled {
        BrushRole::TextDisabled
    } else {
        BrushRole::TextSecondary
    };
    let control_width = row_control_width(element, &value);
    let text_width = (rect.right() - 18.0 - control_width - rect.x - BODY_LEFT - 14.0).max(1.0);
    let stack_top = rect.y + (rect.h - 40.0) * 0.5;
    r.text_clipped(
        &element.label,
        Rect::new(rect.x + BODY_LEFT, stack_top, text_width, 20.0).d2d(),
        TextStyle::BodyStrong,
        label_role,
    );
    r.text_clipped(
        &element.description,
        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, text_width, 18.0).d2d(),
        TextStyle::Caption,
        secondary_role,
    );

    match value {
        ControlValue::Toggle(value) => draw_toggle(r, rect, value, interaction),
        ControlValue::Text(text) => {
            draw_value_box(r, rect, &text, element.kind, control_width, interaction)
        }
        ControlValue::Slider { ratio, label } => {
            draw_slider_cluster(r, element, ratio.clamp(0.0, 1.0), &label, interaction)
        }
        ControlValue::Action(label) => draw_button_style(
            r,
            control_rect(rect, 136.0),
            &label,
            button_style(element.kind),
            interaction,
        ),
    }
}
