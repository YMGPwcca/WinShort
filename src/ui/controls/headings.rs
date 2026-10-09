//! Headings for the controls.

use crate::ui::layout::Rect;
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};
use crate::ui::theme::UiTokens;

pub(crate) fn section_title_text_rect(rect: Rect) -> Rect {
    Rect::new(rect.x + 12.0, rect.y, (rect.w - 12.0).max(1.0), 26.0)
}

pub(crate) fn section_accent_rect(rect: Rect) -> Rect {
    let title = section_title_text_rect(rect);
    let height = 18.0;
    Rect::new(rect.x, title.y + (title.h - height) * 0.5, 3.0, height)
}

pub(crate) fn section_divider_y(rect: Rect) -> f32 {
    rect.y - UiTokens::SECTION_GAP * 0.5
}

pub(crate) fn draw_section_header(r: &Renderer, rect: Rect, title: &str, description: &str) {
    // Keep the divider in the inter-section breathing room. The accent is
    // centered from the title's actual text rectangle, not the whole block.
    r.line(
        rect.x,
        section_divider_y(rect),
        rect.right(),
        section_divider_y(rect),
        BrushRole::Border,
        1.0,
    );
    r.fill_rounded(section_accent_rect(rect).d2d(), 1.5, BrushRole::Accent);
    let title_rect = section_title_text_rect(rect);
    r.text_clipped(title, title_rect.d2d(), TextStyle::Section, BrushRole::Text);
    if !description.is_empty() {
        let description_height = r
            .text_height(
                description,
                TextStyle::SectionDescription,
                title_rect.w,
                (rect.h - 30.0).max(20.0),
            )
            .unwrap_or_else(|| (rect.h - 30.0).max(20.0))
            .clamp(20.0, (rect.h - 30.0).max(20.0));
        r.text_clipped(
            description,
            Rect::new(
                title_rect.x,
                rect.y + 30.0,
                title_rect.w,
                description_height,
            )
            .d2d(),
            TextStyle::SectionDescription,
            BrushRole::TextSecondary,
        );
    }
}

pub(crate) fn draw_page_header(r: &Renderer, rect: Rect, title: &str, description: &str) {
    r.text_clipped(
        title,
        Rect::new(rect.x, rect.y, rect.w, 36.0).d2d(),
        TextStyle::Title,
        BrushRole::Text,
    );
    if !description.is_empty() {
        let description_height = r
            .text_height(
                description,
                TextStyle::Subtitle,
                rect.w,
                (rect.h - 40.0).max(30.0),
            )
            .unwrap_or_else(|| (rect.h - 40.0).max(30.0))
            .clamp(22.0, (rect.h - 40.0).max(30.0));
        r.text_clipped(
            description,
            Rect::new(rect.x, rect.y + 40.0, rect.w, description_height).d2d(),
            TextStyle::Subtitle,
            BrushRole::TextSecondary,
        );
    }
}
