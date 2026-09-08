//! Typed picker commits for the control center.

use super::state::{ConfirmationTarget, SettingsUi};
use crate::config::validate::Violation;
use crate::ui::picker::PickerCommit;
use crate::ui::presentation::DisplayWizardStep;

impl SettingsUi {
    pub(super) fn apply_picker(&mut self, commit: PickerCommit) {
        match commit {
            PickerCommit::InputDevice(value) => {
                self.draft.audio.input_device = value;
            }
            PickerCommit::OutputDevice(value) => {
                self.draft.audio.output_device = value;
            }
            PickerCommit::InputAllowlist(value) => {
                self.draft.audio.cycle_input_allowlist = value.into_config();
            }
            PickerCommit::OutputAllowlist(value) => {
                self.draft.audio.cycle_output_allowlist = value.into_config();
            }
            PickerCommit::DisplayProfile(value) => {
                self.draft.display_profiles.active_profile = value;
                let route_count = self
                    .draft
                    .display_profiles
                    .active()
                    .map_or(0, |profile| profile.routes.len());
                self.display.select_first_route(route_count);
            }
            PickerCommit::DisplayOutputs(routes) => {
                if !self.apply_display_outputs(routes) {
                    return;
                }
            }
            PickerCommit::DisplayTopology(topology) => {
                self.apply_display_topology(topology);
            }
            PickerCommit::DisplayRoute(index) => {
                self.display.select_route(Some(index));
            }
            PickerCommit::InputRole(value) => {
                self.draft.audio.input_role = value;
            }
            PickerCommit::OutputRole(value) => {
                self.draft.audio.output_role = value;
            }
            PickerCommit::DesktopNumberModifier(value) => {
                self.draft.virtual_desktops.number_modifier = value;
            }
            PickerCommit::MoveDesktopModifier(value) => {
                self.draft.virtual_desktops.move_follow_modifier =
                    (!value.is_empty()).then_some(value);
            }
            PickerCommit::SilentMoveDesktopModifier(value) => {
                self.draft.virtual_desktops.move_silent_modifier =
                    (!value.is_empty()).then_some(value);
            }
            PickerCommit::OverlayAppearance(value) => {
                self.draft.overlay.appearance = value;
            }
            PickerCommit::OverlayPosition(value) => {
                self.draft.overlay.position = value;
            }
            PickerCommit::OverlayMonitor(value) => {
                self.draft.overlay.monitor = value;
                self.refresh_overlay_preview_aspect();
            }
        }
        if self
            .interaction
            .confirmations()
            .is_pending(ConfirmationTarget::ResetSettings)
        {
            self.interaction.confirmations_mut().clear();
        }
        self.validation.clear();
    }

    fn apply_display_outputs(&mut self, routes: Vec<crate::display::DisplayRoute>) -> bool {
        if routes.is_empty() {
            self.validation = vec![Violation {
                field: "display_profiles.outputs".into(),
                message: "Select at least one output for this profile".into(),
            }];
            return false;
        }
        let route_count = if let Some(profile) = self
            .draft
            .display_profiles
            .active_profile
            .as_deref()
            .and_then(|id| {
                self.draft
                    .display_profiles
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id.eq_ignore_ascii_case(id))
            }) {
            let mut merged = Vec::with_capacity(routes.len());
            for selected in routes {
                if let Some(existing) = profile
                    .routes
                    .iter()
                    .find(|route| crate::display::same_output(route, &selected))
                {
                    merged.push(existing.clone());
                } else {
                    merged.push(selected);
                }
            }
            profile.routes = merged;
            profile.confirmed = false;
            if profile.routes.len() <= 1 {
                profile.topology = crate::display::DisplayTopology::Custom;
            } else if !matches!(
                profile.topology,
                crate::display::DisplayTopology::Extend | crate::display::DisplayTopology::Clone
            ) {
                profile.topology = crate::display::DisplayTopology::Extend;
            }
            Some(profile.routes.len())
        } else {
            None
        };
        if let Some(route_count) = route_count {
            self.display.clamp_selected_route(route_count);
            self.mark_risky_display_edit();
        }
        true
    }

    fn apply_display_topology(&mut self, topology: crate::display::DisplayTopology) {
        let changed = if let Some(profile) = self
            .draft
            .display_profiles
            .active_profile
            .as_deref()
            .and_then(|id| {
                self.draft
                    .display_profiles
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id.eq_ignore_ascii_case(id))
            }) {
            profile.topology = topology;
            profile.confirmed = false;
            true
        } else {
            false
        };
        if changed {
            self.mark_risky_display_edit();
        }
    }

    fn mark_risky_display_edit(&mut self) {
        if self.display.is_editing() {
            self.display.mark_dirty();
        } else {
            self.display.open(DisplayWizardStep::Review, true);
        }
    }
}
