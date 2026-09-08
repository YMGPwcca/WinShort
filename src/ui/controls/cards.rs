//! Cards for the controls.

use super::buttons::draw_button_style;
use super::icons::draw_icon_kind;
use super::row::{BODY_LEFT, ROW_RADIUS};
use super::state::{interaction_state, ButtonStyle, IconKind, Interaction, InteractionState};
use super::surface::draw_surface;
use crate::ui::layout::{Element, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

#[allow(clippy::too_many_arguments)] // Drawing a card keeps its content explicit at call sites.
pub(crate) fn draw_home_card(
    r: &Renderer,
    rect: Rect,
    title: &str,
    value: &str,
    detail: &str,
    action: &str,
    icon: IconKind,
    interaction: Interaction,
) {
    let state = interaction_state(interaction);
    let surface = match state {
        InteractionState::Disabled => BrushRole::CardPressed,
        InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::CardHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,
    };
    draw_surface(r, rect.inset(1.0), surface, state, 12.0);
    draw_icon_kind(
        r,
        Rect::new(rect.x + 18.0, rect.y + 18.0, 28.0, 28.0),
        icon,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Accent
        },
    );
    r.text_clipped(
        title,
        Rect::new(rect.x + 60.0, rect.y + 13.0, rect.w - 78.0, 22.0).d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        value,
        Rect::new(rect.x + 60.0, rect.y + 37.0, rect.w - 78.0, 25.0).d2d(),
        TextStyle::Section,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        detail,
        Rect::new(rect.x + 60.0, rect.y + 67.0, rect.w - 190.0, 18.0).d2d(),
        TextStyle::Caption,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
    r.text(
        action,
        Rect::new(rect.right() - 116.0, rect.y + 66.0, 92.0, 28.0).d2d(),
        TextStyle::ButtonSmall,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Accent
        },
    );
    if interaction.focused {
        r.stroke_rounded(rect.inset(-2.0).d2d(), 14.0, BrushRole::Focus, 1.5);
    }
}

#[allow(clippy::too_many_arguments)] // Card fields map directly to the profile view model.
pub(crate) fn draw_profile_card(
    r: &Renderer,
    rect: Rect,
    card: &crate::ui::presentation::DisplayProfileCard,
    interaction: Interaction,
) {
    let name = card.name.as_str();
    let display_summary = card.summary.as_str();
    let shortcut = card.shortcut.as_str();
    let confirmed = card.readiness.is_ready();
    let needs_attention = card.readiness.needs_attention();
    let selected = card.selected;
    let state = interaction_state(interaction);
    let surface = if selected {
        BrushRole::BackgroundSubtle
    } else {
        match state {
            InteractionState::Hovered => BrushRole::CardHover,
            InteractionState::Pressed => BrushRole::CardPressed,
            _ => BrushRole::Card,
        }
    };
    draw_surface(r, rect.inset(1.0), surface, state, 12.0);
    r.text_clipped(
        name,
        Rect::new(
            rect.x + 18.0,
            rect.y + 16.0,
            (rect.w - 170.0).max(120.0),
            26.0,
        )
        .d2d(),
        TextStyle::Section,
        BrushRole::Text,
    );
    r.text_clipped(
        display_summary,
        Rect::new(
            rect.x + 18.0,
            rect.y + 49.0,
            (rect.w - 170.0).max(120.0),
            20.0,
        )
        .d2d(),
        TextStyle::Body,
        BrushRole::TextSecondary,
    );
    let status = if needs_attention {
        "Needs attention"
    } else if confirmed {
        "Ready to activate"
    } else {
        "Test before activating"
    };
    r.text(
        status,
        Rect::new(rect.x + 18.0, rect.y + 74.0, rect.w - 160.0, 18.0).d2d(),
        TextStyle::Caption,
        if needs_attention {
            BrushRole::Warning
        } else if confirmed {
            BrushRole::Success
        } else {
            BrushRole::Warning
        },
    );
    r.text_clipped(
        shortcut,
        Rect::new(rect.right() - 136.0, rect.y + 18.0, 116.0, 20.0).d2d(),
        TextStyle::CaptionRight,
        BrushRole::TextSecondary,
    );
    draw_button_style(
        r,
        Rect::new(rect.right() - 136.0, rect.y + 64.0, 116.0, 30.0),
        if selected && confirmed {
            "Activate"
        } else if selected {
            "Review"
        } else {
            "Select"
        },
        if selected && confirmed {
            ButtonStyle::Primary
        } else {
            ButtonStyle::Secondary
        },
        Interaction {
            focused: false,
            ..interaction
        },
    );
    if selected {
        r.stroke_rounded(rect.d2d(), 12.0, BrushRole::Accent, 1.25);
    }
    if interaction.focused {
        r.stroke_rounded(rect.inset(-2.0).d2d(), 14.0, BrushRole::Focus, 1.5);
    }
}

pub(crate) fn draw_profile_name_row(
    r: &Renderer,
    element: &Element,
    name: &str,
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
    r.text_clipped(
        "Profile name",
        Rect::new(rect.x + BODY_LEFT, rect.y + 7.0, rect.w - 190.0, 20.0).d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        name,
        Rect::new(rect.x + BODY_LEFT, rect.y + 29.0, rect.w - 190.0, 20.0).d2d(),
        TextStyle::Value,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
    draw_button_style(
        r,
        Rect::new(rect.right() - 136.0, rect.y + 12.0, 116.0, 34.0),
        "Change",
        ButtonStyle::Secondary,
        Interaction {
            focused: interaction.focused,
            ..interaction
        },
    );
}
