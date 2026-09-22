//! Painting for the diagnostics.

use super::layout::{CONTENT_PADDING_X, SECTION_HEADER_HEIGHT};
use super::model::{Action, Layout, Line, ReportSection};
use super::report::compact;
use super::state::DiagnosticsUi;
use super::window::ROW_HEIGHT;
use crate::diagnostics::snapshot::Health;
use crate::error::Result;
use crate::ui::controls;
use crate::ui::controls::Interaction;
use crate::ui::layout::{top_chrome_separator_rect, Rect};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};
use crate::ui::theme::UiTokens;
use windows::Win32::Foundation::HWND;

impl DiagnosticsUi {
    pub(super) fn paint(&mut self, hwnd: HWND) -> Result<()> {
        let layout = self.layout(hwnd)?;
        self.scroll = layout.scroll;
        let renderer = match self.renderer.take() {
            Some(renderer) => renderer,
            None => Renderer::new(hwnd, self.dpi, crate::ui::theme::Theme::current())?,
        };
        renderer.begin();
        self.draw_header(&renderer, &layout);
        draw_report(&renderer, &layout);
        self.draw_footer(&renderer, &layout);
        self.draw_actions(&renderer, &layout);
        let result = renderer.end();
        if result.is_ok() {
            self.renderer = Some(renderer);
        }
        result
    }

    fn draw_footer(&self, renderer: &Renderer, layout: &Layout) {
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
        renderer.text_clipped(
            status,
            Rect::new(
                CONTENT_PADDING_X,
                layout.footer.y + 10.0,
                layout.width - CONTENT_PADDING_X * 2.0,
                22.0,
            )
            .d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }

    fn draw_actions(&self, renderer: &Renderer, layout: &Layout) {
        for (action, button) in &layout.buttons {
            let disabled = *action == Action::Bundle && self.bundle_running;
            controls::draw_button(
                renderer,
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
    }
}

impl DiagnosticsUi {
    fn draw_header(&self, renderer: &Renderer, layout: &Layout) {
        renderer.fill_rect(layout.chrome.row.d2d(), BrushRole::Background);
        let separator = top_chrome_separator_rect(layout.chrome.row);
        renderer.line(
            separator.x,
            separator.y,
            separator.right(),
            separator.y,
            BrushRole::Border,
            1.0,
        );
        controls::draw_close_button_rect(
            renderer,
            layout.chrome.close,
            self.hovered == Some(Action::Close),
            self.pressed == Some(Action::Close),
            self.focused == Some(Action::Close),
        );
        let title_x = CONTENT_PADDING_X + 46.0;
        controls::draw_app_mark(renderer, Rect::new(CONTENT_PADDING_X, 56.0, 34.0, 34.0));
        renderer.text_clipped(
            "Diagnostics & Support",
            Rect::new(
                title_x,
                48.0,
                (layout.width - title_x - CONTENT_PADDING_X).max(1.0),
                36.0,
            )
            .d2d(),
            TextStyle::Title,
            BrushRole::Text,
        );
        renderer.text_clipped(
            "Review runtime health and support details",
            Rect::new(
                title_x,
                82.0,
                (layout.width - title_x - CONTENT_PADDING_X).max(1.0),
                24.0,
            )
            .d2d(),
            TextStyle::Subtitle,
            BrushRole::TextSecondary,
        );
    }
}

fn draw_report(renderer: &Renderer, layout: &Layout) {
    renderer.push_clip(layout.content.d2d());
    for section in &layout.sections {
        let rect = section.rect.translated_y(-layout.scroll);
        if layout.content.intersects(rect) {
            draw_section_card(renderer, section, rect);
        }
    }
    renderer.pop_clip();
}

fn draw_section_card(renderer: &Renderer, section: &ReportSection, rect: Rect) {
    renderer.fill_rounded(
        rect.translated_y(2.0).d2d(),
        UiTokens::CARD_RADIUS,
        BrushRole::Shadow,
    );
    renderer.fill_rounded(rect.d2d(), UiTokens::CARD_RADIUS, BrushRole::Card);
    renderer.stroke_rounded(rect.d2d(), UiTokens::CARD_RADIUS, BrushRole::Border, 1.0);
    renderer.fill_rounded(
        Rect::new(rect.x + 16.0, rect.y + 15.0, 3.0, 18.0).d2d(),
        1.5,
        BrushRole::Accent,
    );
    renderer.text_clipped(
        &section.title,
        Rect::new(rect.x + 30.0, rect.y + 8.0, rect.w - 46.0, 32.0).d2d(),
        TextStyle::Section,
        BrushRole::Text,
    );
    renderer.line(
        rect.x + 16.0,
        rect.y + SECTION_HEADER_HEIGHT,
        rect.right() - 16.0,
        rect.y + SECTION_HEADER_HEIGHT,
        BrushRole::Border,
        1.0,
    );

    let label_width = (rect.w * 0.28).clamp(190.0, 240.0);
    let value_x = rect.x + 16.0 + label_width + 20.0;
    for (index, line) in section.rows.iter().enumerate() {
        let y = rect.y + SECTION_HEADER_HEIGHT + index as f32 * ROW_HEIGHT;
        if index > 0 {
            renderer.line(
                rect.x + 16.0,
                y,
                rect.right() - 16.0,
                y,
                BrushRole::Border,
                1.0,
            );
        }
        draw_data_line(renderer, rect, line, y, value_x);
    }
}

fn draw_data_line(renderer: &Renderer, rect: Rect, line: &Line, y: f32, value_x: f32) {
    renderer.text_clipped(
        &line.key,
        Rect::new(rect.x + 16.0, y, value_x - rect.x - 24.0, ROW_HEIGHT).d2d(),
        TextStyle::BodyStrong,
        BrushRole::Text,
    );
    let value_right = rect.right() - if line.health.is_some() { 42.0 } else { 16.0 };
    renderer.text_clipped(
        &compact(&line.value),
        Rect::new(value_x, y, (value_right - value_x).max(1.0), ROW_HEIGHT).d2d(),
        TextStyle::Body,
        diagnostic_value_role(line.health),
    );
    if let Some(health) = line.health {
        renderer.ellipse(
            rect.right() - 24.0,
            y + ROW_HEIGHT * 0.5,
            4.0,
            4.0,
            health_role(health),
            true,
            0.0,
        );
    }
}

fn diagnostic_value_role(health: Option<Health>) -> BrushRole {
    match health {
        Some(Health::Error) => BrushRole::Danger,
        Some(Health::Warning) => BrushRole::Warning,
        Some(Health::Unavailable) => BrushRole::TextDisabled,
        Some(Health::Healthy) | None => BrushRole::TextSecondary,
    }
}

fn health_role(health: Health) -> BrushRole {
    match health {
        Health::Healthy => BrushRole::Success,
        Health::Warning => BrushRole::Warning,
        Health::Unavailable => BrushRole::TextDisabled,
        Health::Error => BrushRole::Danger,
    }
}

#[cfg(test)]
mod tests {
    use super::{diagnostic_value_role, health_role};
    use crate::diagnostics::snapshot::Health;
    use crate::ui::renderer::BrushRole;

    #[test]
    fn diagnostics_health_states_use_shared_theme_roles() {
        assert_eq!(
            diagnostic_value_role(Some(Health::Healthy)),
            BrushRole::TextSecondary
        );
        assert_eq!(
            diagnostic_value_role(Some(Health::Warning)),
            BrushRole::Warning
        );
        assert_eq!(
            diagnostic_value_role(Some(Health::Unavailable)),
            BrushRole::TextDisabled
        );
        assert_eq!(
            diagnostic_value_role(Some(Health::Error)),
            BrushRole::Danger
        );
        assert_eq!(health_role(Health::Healthy), BrushRole::Success);
    }
}
