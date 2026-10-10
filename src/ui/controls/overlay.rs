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
    if element.id == crate::ui::layout::ElementId::OverlayPositionCell(4) {
        r.fill_rounded(rect.d2d(), 7.0, BrushRole::BackgroundSubtle);
        r.stroke_rounded(rect.d2d(), 7.0, BrushRole::Border, 1.0);
        let (cx, cy) = (rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
        let radius = (rect.h * 0.24).clamp(6.0, 10.0);
        r.ellipse(cx, cy, radius, radius, BrushRole::TextDisabled, false, 1.6);
        let diagonal = radius * 0.7;
        r.line(
            cx - diagonal,
            cy - diagonal,
            cx + diagonal,
            cy + diagonal,
            BrushRole::TextDisabled,
            1.6,
        );
        return;
    }
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
