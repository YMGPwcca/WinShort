//! Icons for the controls.

use super::state::IconKind;
use crate::ui::layout::Rect;
use crate::ui::navigation::Page;
use crate::ui::renderer::{BrushRole, Renderer};

pub(super) fn draw_icon_kind(r: &Renderer, rect: Rect, icon: IconKind, role: BrushRole) {
    match icon {
        IconKind::Page(page) => draw_page_icon(r, rect, page, role),
        IconKind::Speaker => draw_speaker_icon(r, rect, role),
        IconKind::Microphone => draw_microphone_icon(r, rect, role),
    }
}

pub(crate) fn draw_icon(r: &Renderer, rect: Rect, page: Page, role: BrushRole) {
    draw_icon_kind(r, rect, IconKind::Page(page), role);
}

pub(crate) fn draw_app_mark(r: &Renderer, rect: Rect) {
    r.fill_rounded(rect.d2d(), 9.0, BrushRole::Accent);
    let cx = rect.x + rect.w * 0.5;
    let cy = rect.y + rect.h * 0.5;
    r.fill_rounded(
        Rect::new(cx - 10.0, cy - 3.0, 5.0, 6.0).d2d(),
        1.5,
        BrushRole::AccentText,
    );
    r.line(
        cx - 5.0,
        cy - 3.0,
        cx + 2.0,
        cy - 9.0,
        BrushRole::AccentText,
        2.5,
    );
    r.line(
        cx - 5.0,
        cy + 3.0,
        cx + 2.0,
        cy + 9.0,
        BrushRole::AccentText,
        2.5,
    );
    r.line(
        cx + 2.0,
        cy - 9.0,
        cx + 2.0,
        cy + 9.0,
        BrushRole::AccentText,
        2.5,
    );
    r.line(
        cx + 7.0,
        cy - 5.0,
        cx + 10.0,
        cy - 2.0,
        BrushRole::AccentText,
        1.8,
    );
    r.line(
        cx + 10.0,
        cy - 2.0,
        cx + 10.0,
        cy + 2.0,
        BrushRole::AccentText,
        1.8,
    );
    r.line(
        cx + 10.0,
        cy + 2.0,
        cx + 7.0,
        cy + 5.0,
        BrushRole::AccentText,
        1.8,
    );
}

fn draw_speaker_icon(r: &Renderer, rect: Rect, role: BrushRole) {
    let cy = rect.y + rect.h * 0.5;
    let scale = rect.w.min(rect.h) / 20.0;
    let x = rect.x + 1.0 * scale;
    r.fill_rounded(
        Rect::new(x, cy - 2.5 * scale, 3.5 * scale, 5.0 * scale).d2d(),
        scale,
        role,
    );
    r.line(
        x + 3.0 * scale,
        cy - 2.5 * scale,
        x + 8.0 * scale,
        cy - 6.5 * scale,
        role,
        1.7 * scale,
    );
    r.line(
        x + 3.0 * scale,
        cy + 2.5 * scale,
        x + 8.0 * scale,
        cy + 6.5 * scale,
        role,
        1.7 * scale,
    );
    r.line(
        x + 8.0 * scale,
        cy - 6.5 * scale,
        x + 8.0 * scale,
        cy + 6.5 * scale,
        role,
        1.7 * scale,
    );
    r.line(
        x + 12.0 * scale,
        cy - 4.0 * scale,
        x + 15.0 * scale,
        cy - 1.5 * scale,
        role,
        1.4 * scale,
    );
    r.line(
        x + 15.0 * scale,
        cy - 1.5 * scale,
        x + 15.0 * scale,
        cy + 1.5 * scale,
        role,
        1.4 * scale,
    );
    r.line(
        x + 15.0 * scale,
        cy + 1.5 * scale,
        x + 12.0 * scale,
        cy + 4.0 * scale,
        role,
        1.4 * scale,
    );
}

fn draw_microphone_icon(r: &Renderer, rect: Rect, role: BrushRole) {
    let size = rect.w.min(rect.h);
    let scale = size / 28.0;
    let cx = rect.x + rect.w * 0.5;
    let cy = rect.y + rect.h * 0.5;
    r.stroke_rounded(
        Rect::new(
            cx - 4.0 * scale,
            cy - 10.0 * scale,
            8.0 * scale,
            14.0 * scale,
        )
        .d2d(),
        4.0 * scale,
        role,
        1.7 * scale,
    );
    let stroke = 1.7 * scale;
    for (x1, y1, x2, y2) in [
        (-8.0, 1.0, -8.0, 3.0),
        (-8.0, 3.0, -6.0, 6.0),
        (-6.0, 6.0, 0.0, 8.0),
        (0.0, 8.0, 6.0, 6.0),
        (6.0, 6.0, 8.0, 3.0),
        (8.0, 3.0, 8.0, 1.0),
        (0.0, 8.0, 0.0, 11.0),
        (-4.0, 11.0, 4.0, 11.0),
    ] {
        r.line(
            cx + x1 * scale,
            cy + y1 * scale,
            cx + x2 * scale,
            cy + y2 * scale,
            role,
            stroke,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ShortcutIconGeometry {
    pub outer: Rect,
    pub upper_keys: [Rect; 4],
    pub spacebar: Rect,
}

pub(crate) fn shortcut_icon_geometry(rect: Rect) -> ShortcutIconGeometry {
    let outer = Rect::new(rect.x + 1.5, rect.y + 3.5, rect.w - 3.0, rect.h - 7.0);
    let key_width = (rect.w * 0.11).max(1.5);
    let key_left = rect.x + rect.w * 0.25;
    let key_right = rect.right() - rect.w * 0.25;
    let key_gap = ((key_right - key_left) - key_width * 4.0) / 3.0;
    let upper_y = rect.y + rect.h * 0.43;
    let upper_keys = std::array::from_fn(|index| {
        Rect::new(
            key_left + index as f32 * (key_width + key_gap),
            upper_y,
            key_width,
            1.5,
        )
    });
    let spacebar = Rect::new(
        rect.x + rect.w * 0.30,
        rect.y + rect.h * 0.70,
        rect.w * 0.40,
        1.5,
    );
    ShortcutIconGeometry {
        outer,
        upper_keys,
        spacebar,
    }
}

fn draw_shortcut_icon(r: &Renderer, rect: Rect, role: BrushRole) {
    let geometry = shortcut_icon_geometry(rect);
    r.stroke_rounded(geometry.outer.d2d(), 3.0, role, 1.5);
    for key in geometry.upper_keys {
        r.fill_rounded(key.d2d(), 0.75, role);
    }
    r.fill_rounded(geometry.spacebar.d2d(), 0.75, role);
}

fn draw_audio_icon(r: &Renderer, rect: Rect, role: BrushRole) {
    draw_microphone_icon(r, rect, role);
}

pub(super) fn draw_page_icon(r: &Renderer, rect: Rect, page: Page, role: BrushRole) {
    let cx = rect.x + rect.w * 0.5;
    let cy = rect.y + rect.h * 0.5;
    match page {
        Page::Home => {
            r.line(rect.x + 4.0, cy, cx, rect.y + 4.0, role, 1.7);
            r.line(cx, rect.y + 4.0, rect.right() - 4.0, cy, role, 1.7);
            r.line(
                rect.x + 7.0,
                cy - 1.0,
                rect.x + 7.0,
                rect.bottom() - 4.0,
                role,
                1.7,
            );
            r.line(
                rect.right() - 7.0,
                cy - 1.0,
                rect.right() - 7.0,
                rect.bottom() - 4.0,
                role,
                1.7,
            );
        }
        Page::Shortcuts => draw_shortcut_icon(r, rect, role),
        Page::Audio => draw_audio_icon(r, rect, role),
        Page::Workspaces => {
            r.stroke_rounded(
                Rect::new(rect.x + 3.0, rect.y + 3.0, rect.w - 6.0, 7.0).d2d(),
                2.0,
                role,
                1.5,
            );
            r.stroke_rounded(
                Rect::new(rect.x + 3.0, rect.y + 10.0, rect.w - 6.0, 7.0).d2d(),
                2.0,
                role,
                1.5,
            );
        }
        Page::Displays => {
            r.stroke_rounded(
                Rect::new(rect.x + 3.0, rect.y + 3.0, rect.w - 6.0, rect.h - 9.0).d2d(),
                2.0,
                role,
                1.5,
            );
            r.line(cx, rect.bottom() - 6.0, cx, rect.bottom() - 2.0, role, 1.5);
            r.line(
                rect.x + 6.0,
                rect.bottom() - 2.0,
                rect.right() - 6.0,
                rect.bottom() - 2.0,
                role,
                1.5,
            );
        }
        Page::Overlay => {
            r.stroke_rounded(
                Rect::new(rect.x + 3.0, rect.y + 4.0, rect.w - 6.0, rect.h - 8.0).d2d(),
                3.0,
                role,
                1.5,
            );
            r.line(
                rect.x + 7.0,
                rect.bottom() - 6.0,
                rect.right() - 7.0,
                rect.bottom() - 6.0,
                role,
                1.5,
            );
        }
        Page::System | Page::Advanced => {
            r.ellipse(cx, cy, 7.0, 7.0, role, false, 1.5);
            r.ellipse(cx, cy, 2.0, 2.0, role, true, 0.0);
            for (dx, dy) in [(0.0, -9.0), (0.0, 9.0), (-9.0, 0.0), (9.0, 0.0)] {
                r.line(cx + dx * 0.8, cy + dy * 0.8, cx + dx, cy + dy, role, 1.4);
            }
        }
    }
}
