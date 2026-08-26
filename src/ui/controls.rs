//! Direct2D-drawn Fluent controls. No child HWNDs: one owner-drawn surface
//! handles hover, press, focus, disabled states, and keyboard navigation.

use std::borrow::Cow;

use crate::ui::layout::{Element, ElementKind, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub enum ControlValue<'a> {
    Toggle(bool),
    Text(Cow<'a, str>),
    Slider { ratio: f32, label: Cow<'a, str> },
    Action(Cow<'a, str>),
}

#[derive(Debug, Clone, Copy)]
pub struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
    /// 0→1 hover transition sampled from Motion.
    pub hover_t: f32,
    /// 0→1 toggle state transition sampled from Motion.
    pub state_t: f32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InteractionState {
    Disabled,
    Pressed,
    Focused,
    Hovered,
    Idle,
}

pub(crate) fn interaction_state(interaction: Interaction) -> InteractionState {
    if interaction.disabled {
        InteractionState::Disabled
    } else if interaction.pressed {
        InteractionState::Pressed
    } else if interaction.focused {
        InteractionState::Focused
    } else if interaction.hovered || interaction.hover_t > 0.02 {
        InteractionState::Hovered
    } else {
        InteractionState::Idle
    }
}

pub fn draw_row(
    r: &Renderer,
    element: &Element,
    value: ControlValue<'_>,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(interaction);
    let bg = match state {
        InteractionState::Disabled | InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::ControlHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,
    };

    // Offset depth without a ghost border under shadow.
    if rect.y > 0.0 {
        r.fill_rounded(rect.translated_y(2.0).d2d(), 8.0, BrushRole::Shadow);
    }
    r.fill_rounded(rect.d2d(), 8.0, bg);

    match state {
        InteractionState::Focused => {
            r.stroke_rounded(rect.inset(2.0).d2d(), 6.0, BrushRole::Focus, 1.5);
        }
        InteractionState::Hovered | InteractionState::Pressed => {
            r.stroke_rounded(rect.d2d(), 8.0, BrushRole::BorderStrong, 1.0);
        }
        InteractionState::Disabled | InteractionState::Idle => {}
    }

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
    r.text(
        element.label,
        Rect::new(rect.x + 18.0, rect.y + 8.0, rect.w - 225.0, 26.0).d2d(),
        TextStyle::BodyStrong,
        label_role,
    );
    r.text(
        element.description,
        Rect::new(rect.x + 18.0, rect.y + 31.0, rect.w - 225.0, 20.0).d2d(),
        TextStyle::Caption,
        secondary_role,
    );

    match value {
        ControlValue::Toggle(value) => draw_toggle(r, rect, value, interaction),
        ControlValue::Text(text) => draw_value_box(r, rect, &text, element.kind, interaction),
        ControlValue::Slider { ratio, label } => {
            draw_slider(r, rect, ratio.clamp(0.0, 1.0), &label, interaction)
        }
        ControlValue::Action(label) => draw_action(r, rect, &label, interaction),
    }
}

pub fn draw_button(r: &Renderer, rect: Rect, label: &str, primary: bool, interaction: Interaction) {
    let state = interaction_state(interaction);
    let bg = match (primary, state) {
        (_, InteractionState::Disabled) => BrushRole::CardPressed,
        (true, InteractionState::Pressed) => BrushRole::AccentPressed,
        (true, InteractionState::Hovered) => BrushRole::AccentHover,
        (true, _) => BrushRole::Accent,
        (false, InteractionState::Pressed) => BrushRole::CardPressed,
        (false, InteractionState::Hovered) => BrushRole::ControlHover,
        (false, _) => BrushRole::Card,
    };
    let text = if interaction.disabled {
        BrushRole::TextDisabled
    } else if primary {
        BrushRole::AccentText
    } else {
        BrushRole::Text
    };

    r.fill_rounded(rect.d2d(), 6.0, bg);
    if !interaction.disabled
        && (!primary || matches!(state, InteractionState::Pressed | InteractionState::Hovered))
    {
        let border = match state {
            InteractionState::Pressed => BrushRole::AccentPressed,
            InteractionState::Hovered => BrushRole::BorderStrong,
            _ => BrushRole::Border,
        };
        r.stroke_rounded(
            rect.d2d(),
            6.0,
            border,
            if matches!(state, InteractionState::Pressed | InteractionState::Hovered) {
                1.25
            } else {
                1.0
            },
        );
    }
    if interaction.focused {
        r.stroke_rounded(rect.inset(-2.0).d2d(), 8.0, BrushRole::Focus, 1.5);
    }
    r.text(label, rect.d2d(), TextStyle::Button, text);
}

pub fn draw_app_mark(r: &Renderer, rect: Rect) {
    r.fill_rounded(rect.d2d(), 7.0, BrushRole::AccentPressed);
    let cx = rect.x + rect.w * 0.5;
    let cy = rect.y + rect.h * 0.5;
    // Compact equalizer mark; authored vector, no Unicode icon.
    r.line(
        cx - 7.0,
        cy + 5.0,
        cx - 7.0,
        cy - 3.0,
        BrushRole::AccentText,
        2.0,
    );
    r.line(cx, cy + 5.0, cx, cy - 7.0, BrushRole::AccentText, 2.0);
    r.line(
        cx + 7.0,
        cy + 5.0,
        cx + 7.0,
        cy + 1.0,
        BrushRole::AccentText,
        2.0,
    );
}

pub fn draw_scrollbar(r: &Renderer, viewport: Rect, scroll: f32, max_scroll: f32) {
    if max_scroll <= 0.0 {
        return;
    }
    let total = viewport.h + max_scroll;
    let thumb_h = (viewport.h * viewport.h / total).max(36.0);
    let travel = viewport.h - thumb_h - 8.0;
    let y = viewport.y + 4.0 + travel * (scroll / max_scroll);
    r.fill_rounded(
        Rect::new(viewport.right() - 7.0, y, 3.0, thumb_h).d2d(),
        1.5,
        BrushRole::BorderStrong,
    );
}

fn control_rect(row: Rect, width: f32) -> Rect {
    Rect::new(
        row.right() - 18.0 - width,
        row.y + (row.h - 34.0) * 0.5,
        width,
        34.0,
    )
}
pub(crate) fn value_control_rect(row: Rect, kind: ElementKind) -> Rect {
    let width = match kind {
        ElementKind::Hotkey => 178.0,
        _ => 190.0,
    };
    control_rect(row, width)
}

fn draw_toggle(r: &Renderer, row: Rect, value: bool, interaction: Interaction) {
    let rect = control_rect(row, 46.0);
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
    if !value || !matches!(state, InteractionState::Idle) {
        let width = match state {
            InteractionState::Pressed => 1.5,
            InteractionState::Focused => 1.5,
            InteractionState::Hovered => 1.25,
            InteractionState::Disabled | InteractionState::Idle => 1.0,
        };
        r.stroke_rounded(rect.d2d(), 17.0, outline, width);
    }

    // The logical value controls this channel; pointer state only changes the
    // surrounding track and outline.
    let t = interaction.state_t.clamp(0.0, 1.0);
    let left = rect.x + 12.0;
    let right = rect.right() - 12.0;
    let knob_x = left + (right - left) * t;
    let knob = if value {
        BrushRole::AccentText
    } else {
        BrushRole::TextSecondary
    };
    r.ellipse(knob_x, rect.y + rect.h * 0.5, 7.0, 7.0, knob, true, 0.0);
}

fn draw_value_box(
    r: &Renderer,
    row: Rect,
    text: &str,
    kind: ElementKind,
    interaction: Interaction,
) {
    let rect = value_control_rect(row, kind);
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
    r.fill_rounded(rect.d2d(), 6.0, bg);
    r.stroke_rounded(
        rect.d2d(),
        6.0,
        border,
        if matches!(state, InteractionState::Focused) {
            1.75
        } else if matches!(state, InteractionState::Pressed | InteractionState::Hovered) {
            1.25
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
    r.text(text, rect.inset(8.0).d2d(), TextStyle::Value, role);

    // Dropdown chevron for value fields.
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

fn draw_slider(r: &Renderer, row: Rect, ratio: f32, label: &str, interaction: Interaction) {
    let rect = control_rect(row, 190.0);
    let state = interaction_state(interaction);
    let label_w = 52.0;
    let track = Rect::new(rect.x, rect.y + 15.0, rect.w - label_w - 12.0, 4.0);
    let track_role = match state {
        InteractionState::Disabled => BrushRole::Border,
        InteractionState::Pressed | InteractionState::Hovered => BrushRole::BorderStrong,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Border,
    };
    r.fill_rounded(track.d2d(), 2.0, track_role);
    let filled = Rect::new(track.x, track.y, track.w * ratio, track.h);
    if filled.w > 0.0 {
        let filled_role = match state {
            InteractionState::Pressed => BrushRole::AccentPressed,
            InteractionState::Hovered => BrushRole::AccentHover,
            InteractionState::Disabled => BrushRole::BorderStrong,
            InteractionState::Focused | InteractionState::Idle => BrushRole::Accent,
        };
        r.fill_rounded(filled.d2d(), 2.0, filled_role);
    }
    if matches!(
        state,
        InteractionState::Pressed | InteractionState::Focused | InteractionState::Hovered
    ) {
        let outline = match state {
            InteractionState::Focused => BrushRole::Focus,
            InteractionState::Pressed => BrushRole::AccentPressed,
            InteractionState::Hovered => BrushRole::BorderStrong,
            InteractionState::Disabled | InteractionState::Idle => BrushRole::Border,
        };
        r.stroke_rounded(track.inset(-1.5).d2d(), 3.0, outline, 1.0);
    }
    let knob_x = track.x + track.w * ratio;
    let knob = match state {
        InteractionState::Disabled => BrushRole::TextDisabled,
        InteractionState::Pressed => BrushRole::AccentPressed,
        InteractionState::Hovered => BrushRole::AccentHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Accent,
    };
    let knob_radius = if matches!(state, InteractionState::Pressed) {
        7.0
    } else if matches!(state, InteractionState::Hovered | InteractionState::Focused) {
        6.5
    } else {
        6.0
    };
    r.ellipse(
        knob_x,
        track.y + 2.0,
        knob_radius,
        knob_radius,
        knob,
        true,
        0.0,
    );
    r.text(
        label,
        Rect::new(rect.right() - label_w, rect.y, label_w, rect.h).d2d(),
        TextStyle::CaptionRight,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
}

fn draw_action(r: &Renderer, row: Rect, label: &str, interaction: Interaction) {
    let rect = control_rect(row, 132.0);
    let state = interaction_state(interaction);
    let bg = match state {
        InteractionState::Disabled | InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::ControlHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,
    };
    let border = match state {
        InteractionState::Disabled | InteractionState::Idle => BrushRole::Border,
        InteractionState::Pressed => BrushRole::AccentPressed,
        InteractionState::Focused => BrushRole::Focus,
        InteractionState::Hovered => BrushRole::BorderStrong,
    };
    r.fill_rounded(rect.d2d(), 6.0, bg);
    r.stroke_rounded(
        rect.d2d(),
        6.0,
        border,
        if matches!(state, InteractionState::Focused) {
            1.75
        } else if matches!(state, InteractionState::Pressed | InteractionState::Hovered) {
            1.25
        } else {
            1.0
        },
    );
    r.text(
        label,
        rect.d2d(),
        TextStyle::ButtonSmall,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else if matches!(state, InteractionState::Pressed) {
            BrushRole::AccentPressed
        } else if matches!(state, InteractionState::Hovered) {
            BrushRole::AccentHover
        } else {
            BrushRole::Accent
        },
    );
}
#[cfg(test)]
mod tests {
    use super::{interaction_state, value_control_rect, Interaction, InteractionState};
    use crate::ui::layout::{ElementKind, Rect};

    fn interaction() -> Interaction {
        Interaction {
            hovered: false,
            pressed: false,
            focused: false,
            disabled: false,
            hover_t: 0.0,
            state_t: 0.0,
        }
    }

    #[test]
    fn interaction_state_has_explicit_precedence() {
        let mut value = interaction();
        assert_eq!(interaction_state(value), InteractionState::Idle);

        value.hovered = true;
        assert_eq!(interaction_state(value), InteractionState::Hovered);

        value.focused = true;
        assert_eq!(interaction_state(value), InteractionState::Focused);

        value.pressed = true;
        assert_eq!(interaction_state(value), InteractionState::Pressed);

        value.disabled = true;
        assert_eq!(interaction_state(value), InteractionState::Disabled);
    }

    #[test]
    fn active_hover_transition_is_hover_state_without_focus() {
        let mut value = interaction();
        value.hover_t = 0.5;
        assert_eq!(interaction_state(value), InteractionState::Hovered);
    }

    #[test]
    fn value_control_geometry_is_right_aligned_and_kind_aware() {
        let row = Rect::new(24.0, 100.0, 560.0, 58.0);
        let value = value_control_rect(row, ElementKind::Value);
        let hotkey = value_control_rect(row, ElementKind::Hotkey);
        assert_eq!(value.w, 190.0);
        assert_eq!(hotkey.w, 178.0);
        assert!(value.x > row.x);
        assert_eq!(value.right(), row.right() - 18.0);
    }
}
