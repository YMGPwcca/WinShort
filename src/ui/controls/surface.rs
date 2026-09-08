//! Surface for the controls.

use super::state::InteractionState;
use crate::ui::layout::Rect;
use crate::ui::renderer::{BrushRole, Renderer};

pub(super) fn draw_surface(
    r: &Renderer,
    rect: Rect,
    surface: BrushRole,
    state: InteractionState,
    radius: f32,
) {
    r.fill_rounded(rect.translated_y(2.0).d2d(), radius, BrushRole::Shadow);
    r.fill_rounded(rect.d2d(), radius, surface);
    match state {
        // Focus belongs to the actionable child (switch, picker, or button),
        // not to the full setting surface. This prevents nested cyan outlines.
        InteractionState::Focused => {}
        InteractionState::Hovered | InteractionState::Pressed => {
            r.stroke_rounded(rect.d2d(), radius, BrushRole::BorderStrong, 1.0)
        }
        InteractionState::Disabled | InteractionState::Idle => {}
    }
}
