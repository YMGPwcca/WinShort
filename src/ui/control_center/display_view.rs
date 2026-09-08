//! Display view for the control center.

use super::state::SettingsUi;
use crate::ui::layout::Rect as UiRect;
use crate::ui::presentation::{
    display_output_label, format_hotkey, format_optional_hotkey, DisplayWizardStep,
};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

impl SettingsUi {
    pub(super) fn draw_display_safety(&self, renderer: &Renderer, rect: UiRect) {
        let rollback = self.display_rollback_status();
        let disabled = !self.draft.display_profiles.enabled && !rollback.active();
        let recovery = rollback.error().is_some();
        let role = if recovery {
            BrushRole::Danger
        } else if disabled {
            BrushRole::TextSecondary
        } else {
            BrushRole::Warning
        };
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 10.0, role, 1.25);
        let title = if disabled {
            "Display profiles are turned off"
        } else if recovery {
            "Display recovery needs attention"
        } else {
            "Keep this display setup?"
        };
        let detail = if disabled {
            "Turn on Display profiles above to save and switch arrangements."
        } else if recovery {
            "The previous setup was not restored. Use Revert to retry recovery."
        } else {
            "Reverting automatically when the 15-second timer ends."
        };
        renderer.text_clipped(
            title,
            UiRect::new(rect.x + 16.0, rect.y + 14.0, rect.w - 32.0, 24.0).d2d(),
            TextStyle::BodyStrong,
            role,
        );
        renderer.text_clipped(
            detail,
            UiRect::new(rect.x + 16.0, rect.y + 44.0, rect.w - 32.0, 24.0).d2d(),
            TextStyle::SectionDescription,
            BrushRole::TextSecondary,
        );
    }

    pub(super) fn draw_wizard_steps(&self, renderer: &Renderer, rect: UiRect) {
        let current = self.display.step().unwrap_or(DisplayWizardStep::Displays);
        let width = rect.w / DisplayWizardStep::ALL.len() as f32;
        for (index, step) in DisplayWizardStep::ALL.into_iter().enumerate() {
            let x = rect.x + index as f32 * width;
            let selected = step == current;
            renderer.fill_rounded(
                UiRect::new(x, rect.y + 8.0, width - 8.0, 38.0).d2d(),
                7.0,
                if selected {
                    BrushRole::Accent
                } else {
                    BrushRole::Card
                },
            );
            renderer.text(
                &format!("{}  {}", step.number(), step.title()),
                UiRect::new(x + 8.0, rect.y + 8.0, width - 24.0, 38.0).d2d(),
                TextStyle::Caption,
                if selected {
                    BrushRole::AccentText
                } else {
                    BrushRole::TextSecondary
                },
            );
        }
    }

    pub(super) fn draw_display_wizard_summary(&self, renderer: &Renderer, rect: UiRect) {
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 10.0, BrushRole::Border, 1.0);
        if self.display.step() == Some(DisplayWizardStep::Review) {
            self.draw_display_review_summary(renderer, rect);
            return;
        }
        let profile = self.draft.display_profiles.active();
        let (name, summary, detail) = if let Some(profile) = profile {
            if self.inventory.error().is_some() || !self.inventory.was_queried() {
                (
                    profile.name.as_str(),
                    "Readiness unknown".to_string(),
                    if self.inventory.error().is_some() {
                        "Windows display information is unavailable".to_string()
                    } else {
                        "Windows display information has not been checked".to_string()
                    },
                )
            } else {
                let unavailable = profile
                    .routes
                    .iter()
                    .filter(|route| {
                        !self
                            .inventory
                            .outputs()
                            .iter()
                            .any(|output| crate::display::same_output(&output.route, route))
                    })
                    .count();
                let summary = if unavailable > 0 {
                    "Needs attention".to_string()
                } else if profile.routes.len() <= 1 {
                    "Single display".to_string()
                } else {
                    profile.topology.label().to_string()
                };
                let detail = if unavailable > 0 {
                    format!(
                        "{} selected · {} screen(s) unavailable",
                        profile.routes.len(),
                        unavailable
                    )
                } else {
                    format!("{} screen(s) selected", profile.routes.len())
                };
                (profile.name.as_str(), summary, detail)
            }
        } else {
            (
                "New display profile",
                "No screens selected".into(),
                "Select at least one screen to continue".into(),
            )
        };
        let summary_role = if matches!(summary.as_str(), "Needs attention" | "Readiness unknown") {
            BrushRole::Warning
        } else {
            BrushRole::Accent
        };
        renderer.text_clipped(
            name,
            UiRect::new(rect.x + 16.0, rect.y + 14.0, rect.w - 32.0, 24.0).d2d(),
            TextStyle::Section,
            BrushRole::Text,
        );
        renderer.text(
            &summary,
            UiRect::new(rect.x + 16.0, rect.y + 48.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            summary_role,
        );
        renderer.text_clipped(
            &detail,
            UiRect::new(rect.x + 16.0, rect.y + 78.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }

    pub(super) fn draw_display_review_summary(&self, renderer: &Renderer, rect: UiRect) {
        let Some(profile) = self.draft.display_profiles.active().cloned() else {
            return;
        };
        let lines = [
            ("Profile", profile.name.clone()),
            ("Screens", self.display_review_screen_names(&profile)),
            (
                "Arrangement",
                if profile.routes.len() <= 1 {
                    "Single display".into()
                } else {
                    profile.topology.label().into()
                },
            ),
            (
                "Shortcut",
                format_optional_hotkey(self.active_profile_hotkey()),
            ),
            ("Readiness", self.display_review_readiness(&profile)),
        ];
        for (index, (label, value)) in lines.into_iter().enumerate() {
            let y = rect.y + 10.0 + index as f32 * 31.0;
            renderer.text_clipped(
                label,
                UiRect::new(rect.x + 16.0, y, 92.0, 20.0).d2d(),
                TextStyle::Caption,
                BrushRole::TextSecondary,
            );
            renderer.text_clipped(
                &value,
                UiRect::new(rect.x + 116.0, y, rect.w - 132.0, 22.0).d2d(),
                TextStyle::Body,
                if label == "Readiness" && value.starts_with("Ready") {
                    BrushRole::Success
                } else if label == "Readiness" {
                    BrushRole::Warning
                } else {
                    BrushRole::Text
                },
            );
        }
    }

    pub(super) fn display_review_screen_names(
        &self,
        profile: &crate::display::DisplayProfile,
    ) -> String {
        if profile.routes.is_empty() {
            return "No screens selected".into();
        }
        profile
            .routes
            .iter()
            .enumerate()
            .map(|(index, route)| self.display_review_screen_name(index, route))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub(super) fn display_review_readiness(
        &self,
        profile: &crate::display::DisplayProfile,
    ) -> String {
        if self.inventory.error().is_some() {
            return "Unknown · Windows display information unavailable".into();
        }
        if !self.inventory.was_queried() {
            return "Unknown · Windows display information not checked".into();
        }
        let missing = profile
            .routes
            .iter()
            .filter(|route| {
                !self
                    .inventory
                    .outputs()
                    .iter()
                    .any(|output| crate::display::same_output(&output.route, route))
            })
            .count();
        if missing > 0 {
            format!("Needs attention · {missing} screen(s) unavailable")
        } else if profile.routes.is_empty() || !profile.confirmed {
            "Needs test before activation".into()
        } else {
            "Ready to activate".into()
        }
    }

    pub(super) fn profile_route_label(
        &self,
        index: usize,
        route: &crate::display::DisplayRoute,
    ) -> String {
        if self.inventory.error().is_some() {
            format!("Saved screen {} · Status unknown", index + 1)
        } else if !self.inventory.was_queried() {
            format!("Saved screen {} · Status not checked", index + 1)
        } else if self
            .inventory
            .outputs()
            .iter()
            .any(|output| crate::display::same_output(&output.route, route))
        {
            self.output_label(route)
        } else {
            format!("Saved screen {} · Unavailable", index + 1)
        }
    }

    pub(super) fn display_screen_name(
        &self,
        index: usize,
        route: &crate::display::DisplayRoute,
    ) -> String {
        if self.inventory.error().is_some() || !self.inventory.was_queried() {
            return format!("Saved screen {}", index + 1);
        }
        self.inventory
            .outputs()
            .iter()
            .find(|output| crate::display::same_output(&output.route, route))
            .map(|output| {
                display_output_label(
                    &output.monitor_name,
                    &output.adapter_name,
                    &output.connector_name,
                    output.active,
                )
                .primary
            })
            .unwrap_or_else(|| format!("Saved screen {}", index + 1))
    }

    pub(super) fn display_review_screen_name(
        &self,
        index: usize,
        route: &crate::display::DisplayRoute,
    ) -> String {
        let name = self.display_screen_name(index, route);
        if self.inventory.error().is_none()
            && self.inventory.was_queried()
            && !self
                .inventory
                .outputs()
                .iter()
                .any(|output| crate::display::same_output(&output.route, route))
        {
            format!("{name} · Unavailable")
        } else {
            name
        }
    }

    pub(super) fn selected_display_names(&self) -> Vec<String> {
        self.draft
            .display_profiles
            .active()
            .map(|profile| {
                profile
                    .routes
                    .iter()
                    .enumerate()
                    .take(2)
                    .map(|(index, route)| self.display_screen_name(index, route))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn profile_card_data(
        &self,
        index: usize,
    ) -> Option<crate::ui::presentation::DisplayProfileCard> {
        let profile = self.draft.display_profiles.profiles.get(index)?;
        let summary = match profile.routes.as_slice() {
            [] => "No displays selected".into(),
            [route] => self.profile_route_label(0, route),
            routes => {
                let names = routes
                    .iter()
                    .enumerate()
                    .take(2)
                    .map(|(index, route)| self.profile_route_label(index, route))
                    .collect::<Vec<_>>();
                if routes.len() > 2 {
                    format!("{} + {} more", names.join(" · "), routes.len() - 2)
                } else {
                    names.join(" · ")
                }
            }
        };
        let shortcut = self
            .draft
            .hotkeys
            .display_profiles
            .iter()
            .find(|binding| binding.profile_id.eq_ignore_ascii_case(&profile.id))
            .map_or_else(
                || "No shortcut".into(),
                |binding| format_hotkey(binding.hotkey),
            );
        let selected = self
            .draft
            .display_profiles
            .active_profile
            .as_deref()
            .is_some_and(|id| id.eq_ignore_ascii_case(&profile.id));
        Some(crate::ui::presentation::DisplayProfileCard {
            name: profile.name.clone(),
            summary,
            shortcut,
            readiness: self.inventory.profile_readiness(profile),
            selected,
        })
    }

    pub(super) fn display_summary(&self) -> (String, String) {
        if !self.draft.display_profiles.enabled {
            return (
                "Display profiles off".into(),
                "Enable display profiles to save arrangements".into(),
            );
        }
        let Some(profile) = self.draft.display_profiles.active() else {
            return (
                "No profile selected".into(),
                "Set up a display profile".into(),
            );
        };
        let missing = if self.inventory.error().is_some() || !self.inventory.was_queried() {
            None
        } else {
            Some(
                profile
                    .routes
                    .iter()
                    .filter(|route| {
                        !self
                            .inventory
                            .outputs()
                            .iter()
                            .any(|output| crate::display::same_output(&output.route, route))
                    })
                    .count(),
            )
        };
        let detail = if self.inventory.error().is_some() {
            "Readiness unknown · Windows display information unavailable".into()
        } else if !self.inventory.was_queried() {
            "Readiness pending · Open Displays to check connected screens".into()
        } else if missing == Some(0) && !profile.confirmed {
            "Needs a test before activation".into()
        } else if missing.is_some_and(|count| count > 0) {
            format!(
                "Needs attention · {} saved screen(s) unavailable",
                missing.unwrap_or_default()
            )
        } else if profile.routes.len() > 1 {
            format!("Ready · {} screens configured", profile.routes.len())
        } else if let Some(route) = profile.routes.first() {
            format!("Ready · {}", self.output_label(route))
        } else {
            "No displays selected".into()
        };
        (profile.name.clone(), detail)
    }
}
