//! Overlay for the controls.

use super::state::Interaction;
use crate::ui::layout::Element;
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub(crate) fn draw_position_cell(
    r: &Renderer,
    element: &Element,
    label: &str,
    selected: bool,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let fill = if selected {
        BrushRole::Accent
    } else if interaction.hovered || interaction.pressed {
        BrushRole::CardHover
    } else {
        BrushRole::BackgroundSubtle
    };
    r.fill_rounded(rect.d2d(), 7.0, fill);
    r.stroke_rounded(
        rect.d2d(),
        7.0,
        if selected {
            BrushRole::Accent
        } else {
            BrushRole::Border
        },
        1.0,
    );
    r.text(
        label,
        rect.d2d(),
        TextStyle::ButtonSmall,
        if selected {
            BrushRole::AccentText
        } else {
            BrushRole::TextSecondary
        },
    );
    if interaction.focused {
        r.stroke_rounded(rect.inset(-3.0).d2d(), 10.0, BrushRole::Focus, 1.5);
    }
}
