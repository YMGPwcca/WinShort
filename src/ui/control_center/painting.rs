//! Painting for the control center.

use super::appearance::settings_theme;
use super::home::friendly_violation;
use super::state::SettingsUi;
use crate::error::Result;
use crate::ui::controls;
use crate::ui::layout::{top_chrome_separator_rect, ElementId, Rect as UiRect, RegionKind};
use crate::ui::navigation::Page;
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};
use std::time::Instant;
use windows::Win32::Foundation::HWND;

pub(super) const APPLIED_STATUS: &str = "Changes applied";

impl SettingsUi {
    pub(super) fn paint(&mut self, hwnd: HWND) -> Result<()> {
        self.rebuild_layout(hwnd);
        if self
            .applied_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.applied_until = None;
        }

        let renderer = match self.renderer.take() {
            Some(renderer) => renderer,
            None => Renderer::new(hwnd, self.dpi, settings_theme())?,
        };
        renderer.begin();
        renderer.fill_rect(
            UiRect::new(0.0, 0.0, self.layout.nav_width, self.layout.height).d2d(),
            BrushRole::BackgroundSubtle,
        );
        renderer.line(
            self.layout.nav_width,
            0.0,
            self.layout.nav_width,
            self.layout.height,
            BrushRole::Border,
            1.0,
        );
        let brand = self.layout.brand;
        controls::draw_app_mark(&renderer, brand.icon);
        renderer.text(
            "WinShort",
            brand.text.d2d(),
            TextStyle::Section,
            BrushRole::Text,
        );
        for element in &self.layout.elements {
            if let ElementId::Nav(page) = element.id {
                controls::draw_nav_item(
                    &renderer,
                    element,
                    self.page,
                    self.interaction.hovered() == Some(element.id),
                    self.interaction.pressed() == Some(element.id),
                    self.visual_focus(element.id),
                );
                if page == Page::Advanced {
                    renderer.line(
                        28.0,
                        element.rect.y - 12.0,
                        self.layout.nav_width - 28.0,
                        element.rect.y - 12.0,
                        BrushRole::Border,
                        1.0,
                    );
                }
            }
        }

        renderer.push_clip(self.layout.content_clip.d2d());
        for section in &self.layout.sections {
            let section_rect = UiRect::new(
                self.layout.content_column.x,
                section.y,
                self.layout.content_column.w,
                section.height,
            );
            if self.layout.content_clip.intersects(section_rect) {
                if section.page_header {
                    controls::draw_page_header(
                        &renderer,
                        section_rect,
                        &section.title,
                        &section.description,
                    );
                } else {
                    controls::draw_section_header(
                        &renderer,
                        section_rect,
                        &section.title,
                        &section.description,
                    );
                }
            }
        }
        if self.onboarding_step.is_some() {
            self.draw_onboarding(&renderer);
        } else if !self.search_query.trim().is_empty() {
            self.draw_search_results(&renderer);
        } else {
            self.draw_page(&renderer);
        }
        controls::draw_scrollbar(
            &renderer,
            self.layout.content_clip,
            self.scroll,
            self.layout.max_scroll,
        );
        renderer.pop_clip();
        // Paint the persistent top bar after the scrollable viewport. The
        // viewport clip is still authoritative; this final layer also makes
        // the shell visually non-scrollable if a backend render call overdraws.
        self.draw_top_bar(&renderer);
        renderer.fill_rect(self.layout.footer.d2d(), BrushRole::BackgroundSubtle);
        renderer.line(
            0.0,
            self.layout.footer.y,
            self.layout.width,
            self.layout.footer.y,
            BrushRole::Border,
            1.0,
        );
        self.draw_footer(&renderer);
        let result = renderer.end();
        if result.is_ok() {
            self.renderer = Some(renderer);
            self.publish_automation_snapshot(hwnd);
        }
        result
    }

    pub(super) fn draw_top_bar(&self, renderer: &Renderer) {
        renderer.fill_rect(self.layout.top_bar.d2d(), BrushRole::Background);
        let separator = top_chrome_separator_rect(self.layout.top_bar);
        renderer.line(
            separator.x,
            separator.y,
            separator.right(),
            separator.y,
            BrushRole::Border,
            1.0,
        );
        controls::draw_search_box(
            renderer,
            self.layout.search_rect,
            &self.search_query,
            self.search_has_focus(),
            self.interaction.hovered() == Some(ElementId::Search),
            self.caret.visible(),
        );
        if let Some(element) = self
            .layout
            .elements
            .iter()
            .find(|element| element.id == ElementId::WindowClose)
        {
            controls::draw_close_button(
                renderer,
                element,
                self.interaction.hovered() == Some(ElementId::WindowClose),
                self.interaction.pressed() == Some(ElementId::WindowClose),
                self.visual_focus(ElementId::WindowClose),
            );
        }
    }

    pub(super) fn draw_onboarding(&self, renderer: &Renderer) {
        for element in &self.layout.elements {
            if !element.scrolls || !self.layout.content_clip.intersects(element.rect) {
                continue;
            }
            controls::draw_row(
                renderer,
                element,
                self.value_for(element.id),
                self.interaction(element.id, self.is_disabled(element.id)),
            );
        }
    }

    pub(super) fn draw_visual_regions(&self, renderer: &Renderer) {
        for region in &self.layout.regions {
            if !self.layout.content_clip.intersects(region.rect) {
                continue;
            }
            match region.kind {
                RegionKind::WorkspaceNotice => self.draw_workspace_notice(renderer, region.rect),
                RegionKind::PauseNotice => self.draw_pause_notice(renderer, region.rect),
                RegionKind::AudioCurrentApp => self.draw_current_app_audio(renderer, region.rect),
                RegionKind::OverlayPreview => self.draw_overlay_preview(renderer, region.rect),
                RegionKind::DisplaySafety => self.draw_display_safety(renderer, region.rect),
                RegionKind::DisplayWizardSteps => self.draw_wizard_steps(renderer, region.rect),
                RegionKind::DisplayWizardSummary => {
                    self.draw_display_wizard_summary(renderer, region.rect)
                }
                RegionKind::SelectedDisplayProfile => {
                    let name = self
                        .draft
                        .display_profiles
                        .active()
                        .map_or("No profile selected", |profile| profile.name.as_str());
                    renderer.text_clipped(
                        &format!("Manage: {name}"),
                        region.rect.d2d(),
                        TextStyle::Section,
                        BrushRole::Text,
                    );
                }
            }
        }
    }

    pub(super) fn draw_workspace_notice(&self, renderer: &Renderer, rect: UiRect) {
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 10.0, BrushRole::Warning, 1.0);
        renderer.text(
            "Workspace shortcuts are turned off",
            UiRect::new(rect.x + 16.0, rect.y + 12.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            BrushRole::Warning,
        );
        renderer.text_clipped(
            "Enable Workspace shortcuts above to use desktop and Special actions.",
            UiRect::new(rect.x + 16.0, rect.y + 40.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::SectionDescription,
            BrushRole::TextSecondary,
        );
    }

    pub(super) fn draw_pause_notice(&self, renderer: &Renderer, rect: UiRect) {
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 10.0, BrushRole::Warning, 1.0);
        renderer.text(
            "Shortcuts are paused",
            UiRect::new(rect.x + 16.0, rect.y + 12.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            BrushRole::Warning,
        );
        renderer.text(
            "Your configured bindings stay saved; they will not fire until resumed.",
            UiRect::new(rect.x + 16.0, rect.y + 40.0, rect.w - 32.0, 20.0).d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }

    pub(super) fn draw_current_app_audio(&self, renderer: &Renderer, rect: UiRect) {
        let aggregate = self.runtime.foreground.aggregate;
        let (state, role) = match aggregate {
            crate::audio::Aggregate::AllMuted => ("Muted", BrushRole::Warning),
            crate::audio::Aggregate::AllActive => ("Active", BrushRole::Success),
            crate::audio::Aggregate::Mixed => ("Mixed sessions", BrushRole::Accent),
            crate::audio::Aggregate::NoSession => ("No audio session", BrushRole::TextSecondary),
            crate::audio::Aggregate::NoExternalApp => {
                ("No controllable app", BrushRole::TextSecondary)
            }
            crate::audio::Aggregate::Error => ("Audio unavailable", BrushRole::Danger),
        };
        let (primary, detail) = match aggregate {
            crate::audio::Aggregate::NoExternalApp => (
                "No controllable app".to_string(),
                "Switch to another app to control its audio".to_string(),
            ),
            crate::audio::Aggregate::NoSession => (
                self.runtime
                    .foreground
                    .app_name
                    .as_deref()
                    .map_or_else(|| state.to_string(), |app| format!("{app} · {state}")),
                "The selected app has no active audio session".to_string(),
            ),
            _ => {
                let app = self
                    .runtime
                    .foreground
                    .app_name
                    .as_deref()
                    .unwrap_or("Current app");
                (
                    format!("{app} · {state}"),
                    "Audio sessions from another app".to_string(),
                )
            }
        };
        renderer.fill_rounded(rect.d2d(), 8.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 8.0, role, 1.0);
        controls::draw_icon(
            renderer,
            UiRect::new(rect.x + 16.0, rect.y + 18.0, 28.0, 28.0),
            Page::Audio,
            role,
        );
        renderer.text_clipped(
            &primary,
            UiRect::new(rect.x + 58.0, rect.y + 10.0, rect.w - 76.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            BrushRole::Text,
        );
        renderer.text_clipped(
            &detail,
            UiRect::new(rect.x + 58.0, rect.y + 34.0, rect.w - 76.0, 20.0).d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }

    pub(super) fn draw_footer(&self, renderer: &Renderer) {
        let rollback = self.display_rollback_status();
        let text = if let Some(first) = self.validation.first() {
            format!("Couldn't apply change — {}", friendly_violation(first))
        } else if rollback.active() {
            if rollback.keep_available() {
                "Display test is active — keep it or revert before the timer ends".into()
            } else {
                "Display recovery is active — use Revert to retry".into()
            }
        } else if self.display.is_editing() {
            "Display draft retained — return to Displays to test or discard it".into()
        } else if self.applied_until.is_some() {
            self.applied_message.into()
        } else if self.onboarding_step.is_some() {
            "You can change these choices later".into()
        } else {
            "Changes save automatically".into()
        };
        let role = if self.validation.is_empty() {
            if self.applied_until.is_some() {
                BrushRole::Success
            } else {
                BrushRole::TextSecondary
            }
        } else {
            BrushRole::Danger
        };
        renderer.text_clipped(
            &text,
            UiRect::new(
                24.0,
                self.layout.footer.y + 8.0,
                self.layout.width
                    - 48.0
                    - if self.layout.element(ElementId::ResumeDisplayDraft).is_some() {
                        200.0
                    } else {
                        0.0
                    },
                22.0,
            )
            .d2d(),
            TextStyle::Caption,
            role,
        );
        if let Some(element) = self.layout.element(ElementId::ResumeDisplayDraft) {
            controls::draw_button(
                renderer,
                element.rect,
                "Continue display edits",
                false,
                self.interaction(element.id, false),
            );
        }
    }
}
