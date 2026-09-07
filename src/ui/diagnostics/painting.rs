//! Painting for the diagnostics.

use super::model::Action;
use super::report::compact;
use super::state::DiagnosticsUi;
use super::window::ROW_HEIGHT;
use crate::diagnostics::snapshot::Health;
use crate::error::Result;
use crate::ui::controls;
use crate::ui::controls::Interaction;
use crate::ui::layout::Rect;
use crate::ui::renderer::{rect, BrushRole, Renderer, TextStyle};
use crate::ui::theme::Theme;
use windows::Win32::Foundation::HWND;

impl DiagnosticsUi {
    pub(super) fn paint(&mut self, hwnd: HWND) -> Result<()> {
        let layout = self.layout(hwnd)?;
        self.scroll = layout.scroll;
        let renderer = match self.renderer.take() {
            Some(renderer) => renderer,
            None => Renderer::new(hwnd, self.dpi, Theme::current())?,
        };
        renderer.begin();
        controls::draw_app_mark(&renderer, Rect::new(24.0, 22.0, 34.0, 34.0));
        renderer.text(
            "WinShort Diagnostics & Support",
            rect(70.0, 16.0, layout.width - 24.0, 44.0),
            TextStyle::Title,
            BrushRole::Text,
        );
        renderer.text(
            "Everything WinShort knows about its current runtime",
            rect(70.0, 44.0, layout.width - 24.0, 68.0),
            TextStyle::Subtitle,
            BrushRole::TextSecondary,
        );

        renderer.push_clip(layout.content.d2d());
        let mut y = layout.content.y + 8.0 - layout.scroll;
        for line in &layout.lines {
            if line.section {
                if y + 30.0 >= layout.content.y && y <= layout.content.bottom() {
                    renderer.text(
                        &line.key,
                        rect(24.0, y, layout.width - 24.0, y + 24.0),
                        TextStyle::Section,
                        BrushRole::Text,
                    );
                    renderer.line(
                        24.0,
                        y + 27.0,
                        layout.width - 24.0,
                        y + 27.0,
                        BrushRole::Border,
                        1.0,
                    );
                }
                y += 34.0;
            } else {
                if y + ROW_HEIGHT >= layout.content.y && y <= layout.content.bottom() {
                    renderer.text(
                        &line.key,
                        rect(32.0, y, 286.0, y + ROW_HEIGHT),
                        TextStyle::BodyStrong,
                        BrushRole::Text,
                    );
                    renderer.text(
                        &compact(&line.value),
                        rect(310.0, y, layout.width - 76.0, y + ROW_HEIGHT),
                        TextStyle::Body,
                        if line.health == Some(Health::Error) {
                            BrushRole::Danger
                        } else {
                            BrushRole::TextSecondary
                        },
                    );
                    if let Some(health) = line.health {
                        let role = match health {
                            Health::Healthy => BrushRole::Success,
                            Health::Warning => BrushRole::Warning,
                            Health::Unavailable => BrushRole::TextDisabled,
                            Health::Error => BrushRole::Danger,
                        };
                        renderer.ellipse(layout.width - 38.0, y + 14.0, 4.0, 4.0, role, true, 0.0);
                    }
                    renderer.line(
                        32.0,
                        y + ROW_HEIGHT - 1.0,
                        layout.width - 32.0,
                        y + ROW_HEIGHT - 1.0,
                        BrushRole::Border,
                        1.0,
                    );
                }
                y += ROW_HEIGHT;
            }
        }
        renderer.pop_clip();

        renderer.fill_rect(layout.footer.d2d(), BrushRole::BackgroundSubtle);
        renderer.line(
            0.0,
            layout.footer.y,
            layout.width,
            layout.footer.y,
            BrushRole::Border,
            1.0,
        );
        let status = self
            .action_status
            .as_deref()
            .unwrap_or("Support output is sanitized before it leaves this window");
        renderer.text(
            status,
            rect(
                24.0,
                layout.footer.y + 12.0,
                layout.width - 24.0,
                layout.footer.y + 34.0,
            ),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
        for (action, button) in &layout.buttons {
            let disabled = *action == Action::Bundle && self.bundle_running;
            controls::draw_button(
                &renderer,
                *button,
                action.label(self.bundle_running),
                matches!(action, Action::Copy | Action::Bundle),
                Interaction {
                    hovered: self.hovered == Some(*action),
                    pressed: self.pressed == Some(*action),
                    focused: self.focused == Some(*action),
                    disabled,
                    hover_t: if self.hovered == Some(*action) {
                        1.0
                    } else {
                        0.0
                    },
                    state_t: 0.0,
                },
            );
        }
        let result = renderer.end();
        if result.is_ok() {
            self.renderer = Some(renderer);
        }
        result
    }
}
