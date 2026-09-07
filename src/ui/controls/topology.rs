//! Topology for the controls.

use super::state::{interaction_state, Interaction, InteractionState};
use crate::ui::layout::{Element, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

#[allow(clippy::too_many_arguments)] // Diagram content mirrors the topology choice view model.
pub(crate) fn draw_topology_choice(
    r: &Renderer,
    element: &Element,
    label: &str,
    description: &str,
    outputs: &[String],
    selected: bool,
    duplicate: bool,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    r.fill_rounded(
        rect.d2d(),
        10.0,
        if selected {
            BrushRole::BackgroundSubtle
        } else if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
            BrushRole::CardHover
        } else {
            BrushRole::Card
        },
    );
    r.stroke_rounded(
        rect.d2d(),
        10.0,
        if selected {
            BrushRole::Accent
        } else {
            BrushRole::Border
        },
        if selected { 1.5 } else { 1.0 },
    );
    let center_x = rect.x + 20.0;
    let center_y = rect.y + 25.0;
    r.ellipse(center_x, center_y, 8.0, 8.0, BrushRole::Accent, false, 1.5);
    if selected {
        r.ellipse(center_x, center_y, 4.0, 4.0, BrushRole::Accent, true, 0.0);
    }
    let diagram_x = rect.x + (rect.w * 0.36).max(190.0);
    let left_width = (diagram_x - rect.x - 56.0).max(120.0);
    r.text_clipped(
        label,
        Rect::new(rect.x + 40.0, rect.y + 10.0, left_width, 22.0).d2d(),
        TextStyle::BodyStrong,
        BrushRole::Text,
    );
    let description_height = r
        .text_height(description, TextStyle::SectionDescription, left_width, 36.0)
        .unwrap_or(36.0)
        .clamp(16.0, 36.0);
    r.text_clipped(
        description,
        Rect::new(rect.x + 40.0, rect.y + 39.0, left_width, description_height).d2d(),
        TextStyle::SectionDescription,
        BrushRole::TextSecondary,
    );
    let diagram = Rect::new(
        diagram_x,
        rect.y + 14.0,
        (rect.right() - diagram_x - 18.0).max(160.0),
        (rect.h - 28.0).max(100.0),
    );
    r.fill_rounded(diagram.d2d(), 8.0, BrushRole::BackgroundSubtle);
    r.stroke_rounded(diagram.d2d(), 8.0, BrushRole::BorderStrong, 1.0);
    let tile_area = Rect::new(
        diagram.x + 10.0,
        diagram.y + 10.0,
        diagram.w - 20.0,
        diagram.h - 38.0,
    );
    if duplicate {
        r.fill_rounded(tile_area.d2d(), 7.0, BrushRole::Card);
        r.stroke_rounded(tile_area.d2d(), 7.0, BrushRole::BorderStrong, 1.0);
        let names = if outputs.is_empty() {
            "Selected screens".to_string()
        } else {
            outputs
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(" + ")
        };
        r.text_clipped(
            &names,
            Rect::new(
                tile_area.x + 12.0,
                tile_area.y + 13.0,
                tile_area.w - 24.0,
                tile_area.h - 34.0,
            )
            .d2d(),
            TextStyle::BodyStrong,
            BrushRole::Text,
        );
        r.line(
            tile_area.x + 12.0,
            tile_area.bottom() - 22.0,
            tile_area.right() - 12.0,
            tile_area.bottom() - 22.0,
            BrushRole::Border,
            1.0,
        );
        r.text_clipped(
            "Same picture",
            Rect::new(
                diagram.x + 10.0,
                diagram.bottom() - 24.0,
                diagram.w - 20.0,
                16.0,
            )
            .d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    } else {
        let gap = 10.0;
        let tile_w = ((tile_area.w - gap) * 0.5).max(64.0);
        let tile_h = tile_area.h;
        for index in 0..2 {
            let tile = Rect::new(
                tile_area.x + index as f32 * (tile_w + gap),
                tile_area.y,
                tile_w,
                tile_h,
            );
            r.fill_rounded(tile.d2d(), 7.0, BrushRole::Card);
            r.stroke_rounded(tile.d2d(), 7.0, BrushRole::BorderStrong, 1.0);
            let name = outputs
                .get(index)
                .map(String::as_str)
                .unwrap_or("Saved screen");
            r.text_clipped(
                name,
                Rect::new(tile.x + 8.0, tile.y + 12.0, tile.w - 16.0, 28.0).d2d(),
                TextStyle::BodyStrong,
                BrushRole::Text,
            );
            r.line(
                tile.x + 8.0,
                tile.bottom() - 28.0,
                tile.right() - 8.0,
                tile.bottom() - 28.0,
                BrushRole::Border,
                1.0,
            );
            r.text(
                &format!("Screen {}", index + 1),
                Rect::new(tile.x + 8.0, tile.bottom() - 23.0, tile.w - 16.0, 16.0).d2d(),
                TextStyle::Caption,
                BrushRole::TextSecondary,
            );
        }
        r.text_clipped(
            "Separate desktops",
            Rect::new(
                diagram.x + 10.0,
                diagram.bottom() - 24.0,
                diagram.w - 20.0,
                16.0,
            )
            .d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }
    if interaction.focused {
        r.stroke_rounded(rect.inset(-3.0).d2d(), 13.0, BrushRole::Focus, 1.5);
    }
}
