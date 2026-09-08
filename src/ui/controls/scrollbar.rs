//! Scrollbar for the controls.

use crate::ui::layout::Rect;
use crate::ui::renderer::{BrushRole, Renderer};

pub(crate) fn scrollbar_thumb_rect(viewport: Rect, scroll: f32, max_scroll: f32) -> Option<Rect> {
    if max_scroll <= 0.0 || viewport.h <= 0.0 {
        return None;
    }
    let total = viewport.h + max_scroll;
    let thumb_h = (viewport.h * viewport.h / total).clamp(40.0, viewport.h - 8.0);
    let travel = (viewport.h - thumb_h - 8.0).max(0.0);
    let ratio = (scroll / max_scroll).clamp(0.0, 1.0);
    Some(Rect::new(
        viewport.right() - 10.0,
        viewport.y + 4.0 + travel * ratio,
        6.0,
        thumb_h,
    ))
}

pub(crate) fn scrollbar_hit_rect(viewport: Rect) -> Rect {
    Rect::new(viewport.right() - 16.0, viewport.y, 16.0, viewport.h)
}

pub(crate) fn scroll_from_scrollbar_pointer(
    viewport: Rect,
    pointer_y: f32,
    pointer_offset: f32,
    max_scroll: f32,
) -> f32 {
    let Some(thumb) = scrollbar_thumb_rect(viewport, 0.0, max_scroll) else {
        return 0.0;
    };
    let travel = (viewport.h - thumb.h - 8.0).max(0.0);
    if travel <= f32::EPSILON {
        return 0.0;
    }
    let top = (pointer_y - pointer_offset).clamp(viewport.y + 4.0, viewport.y + 4.0 + travel);
    ((top - viewport.y - 4.0) / travel * max_scroll).clamp(0.0, max_scroll)
}

pub(crate) fn draw_scrollbar(r: &Renderer, viewport: Rect, scroll: f32, max_scroll: f32) {
    let Some(thumb) = scrollbar_thumb_rect(viewport, scroll, max_scroll) else {
        return;
    };
    r.fill_rounded(thumb.d2d(), 3.0, BrushRole::BorderStrong);
}
