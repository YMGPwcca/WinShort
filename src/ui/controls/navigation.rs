//! Navigation for the controls.

use super::icons::draw_page_icon;
use super::state::{interaction_state, Interaction, InteractionState};
use crate::ui::layout::{Element, Rect};
use crate::ui::navigation::Page;
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};
use crate::ui::theme::UiTokens;

const NAV_RADIUS: f32 = UiTokens::NAV_RADIUS;

pub(crate) fn draw_nav_item(
    r: &Renderer,
    element: &Element,
    page: Page,
    hovered: bool,
    pressed: bool,
    focused: bool,
) {
    let selected =
        matches!(element.id, crate::ui::layout::ElementId::Nav(candidate) if candidate == page);
    let state = interaction_state(Interaction {
        hovered,
        pressed,
        focused,
        disabled: false,
        hover_t: 0.0,
        state_t: if selected { 1.0 } else { 0.0 },
    });
    if selected {
        // Selection is persistent context, not keyboard focus. Keep the rail
        // quiet and add one accent indicator instead of a saturated block.
        r.fill_rounded(element.rect.d2d(), NAV_RADIUS, BrushRole::BackgroundSubtle);
        r.fill_rounded(
            Rect::new(
                element.rect.x,
                element.rect.y + 8.0,
                3.0,
                element.rect.h - 16.0,
            )
            .d2d(),
            1.5,
            BrushRole::Accent,
        );
    } else if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        r.fill_rounded(element.rect.d2d(), NAV_RADIUS, BrushRole::CardHover);
    }
    if focused {
        r.stroke_rounded(
            element.rect.inset(-2.0).d2d(),
            NAV_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
    draw_page_icon(
        r,
        Rect::new(element.rect.x + 12.0, element.rect.y + 10.0, 20.0, 20.0),
        page_for_nav(element.id),
        if selected {
            BrushRole::Accent
        } else {
            BrushRole::TextSecondary
        },
    );
    r.text_clipped(
        &element.label,
        Rect::new(
            element.rect.x + 44.0,
            element.rect.y,
            element.rect.w - 52.0,
            element.rect.h,
        )
        .d2d(),
        TextStyle::BodyStrong,
        BrushRole::Text,
    );
}

fn page_for_nav(id: crate::ui::layout::ElementId) -> Page {
    match id {
        crate::ui::layout::ElementId::Nav(page) => page,
        _ => Page::Home,
    }
}
