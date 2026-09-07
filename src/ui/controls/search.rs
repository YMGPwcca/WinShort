//! Search for the controls.

use super::state::{interaction_state, Interaction, InteractionState};
use crate::ui::layout::Rect;
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

pub(crate) fn search_text_rect(rect: Rect) -> Rect {
    Rect::new(rect.x + 36.0, rect.y + 2.0, rect.w - 48.0, rect.h - 4.0)
}

pub(crate) fn search_caret_rect(rect: Rect, text_width: f32) -> Rect {
    let text = search_text_rect(rect);
    let x = (text.x + text_width).clamp(text.x, (text.right() - 1.0).max(text.x));
    Rect::new(x, rect.y + 7.0, 1.25, (rect.h - 14.0).max(1.0))
}

pub(crate) fn draw_search_box(
    r: &Renderer,
    rect: Rect,
    text: &str,
    focused: bool,
    hovered: bool,
    caret_visible: bool,
) {
    let state = interaction_state(Interaction {
        hovered,
        pressed: false,
        focused,
        disabled: false,
        hover_t: 0.0,
        state_t: 0.0,
    });
    let bg = if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        BrushRole::CardHover
    } else {
        BrushRole::Card
    };
    r.fill_rounded(rect.d2d(), 9.0, bg);
    r.stroke_rounded(
        rect.d2d(),
        9.0,
        if focused {
            BrushRole::Focus
        } else {
            BrushRole::Border
        },
        if focused { 1.5 } else { 1.0 },
    );
    let icon_role = if focused {
        BrushRole::Accent
    } else {
        BrushRole::TextSecondary
    };
    r.ellipse(
        rect.x + 18.0,
        rect.y + 17.0,
        5.5,
        5.5,
        icon_role,
        false,
        1.5,
    );
    r.line(
        rect.x + 22.0,
        rect.y + 21.0,
        rect.x + 26.0,
        rect.y + 25.0,
        icon_role,
        1.5,
    );
    let shown = if text.is_empty() {
        "Find a setting"
    } else {
        text
    };
    r.text_clipped(
        shown,
        search_text_rect(rect).d2d(),
        TextStyle::Body,
        if text.is_empty() {
            BrushRole::TextSecondary
        } else {
            BrushRole::Text
        },
    );
    if focused && caret_visible {
        if let Some(text_width) = r.text_width(text, TextStyle::Body, rect.w - 48.0) {
            let caret = search_caret_rect(rect, text_width);
            r.line(
                caret.x,
                caret.y,
                caret.x,
                caret.bottom(),
                BrushRole::Accent,
                caret.w,
            );
        }
    }
}
