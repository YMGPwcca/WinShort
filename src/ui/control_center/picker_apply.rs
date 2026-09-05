//! Picker apply for the control center.

use super::state::SettingsUi;

use crate::config::validate::Violation;
use crate::ui::picker::{PickerKind, PickerValue};

impl SettingsUi {
    pub(super) fn apply_picker(&mut self, kind: PickerKind, value: PickerValue) {
        match (kind, value) {
            (PickerKind::InputDevice, PickerValue::Device(value)) => {
                self.draft.audio.input_device = value;
            }
            (PickerKind::OutputDevice, PickerValue::Device(value)) => {
                self.draft.audio.output_device = value;
            }
            (PickerKind::InputAllowlist, PickerValue::Allowlist(value)) => {
                self.draft.audio.cycle_input_allowlist = value;
            }
            (PickerKind::OutputAllowlist, PickerValue::Allowlist(value)) => {
                self.draft.audio.cycle_output_allowlist = value;
            }
            (PickerKind::DisplayProfile, PickerValue::DisplayProfile(value)) => {
                self.draft.display_profiles.active_profile = value;
                self.selected_display_route = 0;
            }
            (PickerKind::DisplayOutputs, PickerValue::DisplayOutputs(routes)) => {
                if routes.is_empty() {
                    self.validation = vec![Violation {
                        field: "display_profiles.outputs".into(),
                        message: "Select at least one output for this profile".into(),
                    }];
                    return;
                }
                if let Some(profile) = self
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
                    })
                {
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
                        crate::display::DisplayTopology::Extend
                            | crate::display::DisplayTopology::Clone
                    ) {
                        profile.topology = crate::display::DisplayTopology::Extend;
                    }
                    self.selected_display_route = self
                        .selected_display_route
                        .min(profile.routes.len().saturating_sub(1));
                    self.display_draft_dirty = true;
                }
            }
            (PickerKind::DisplayTopology, PickerValue::DisplayTopology(topology)) => {
                if let Some(profile) = self
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
                    })
                {
                    profile.topology = topology;
                    profile.confirmed = false;
                    self.display_draft_dirty = true;
                }
            }
            (PickerKind::DisplayRoute, PickerValue::DisplayRoute(index)) => {
                self.selected_display_route = index;
            }
            (PickerKind::InputRole, PickerValue::Role(value)) => {
                self.draft.audio.input_role = value;
            }
            (PickerKind::OutputRole, PickerValue::Role(value)) => {
                self.draft.audio.output_role = value;
            }
            (PickerKind::DesktopNumberModifier, PickerValue::Modifier(value)) => {
                self.draft.virtual_desktops.number_modifier = value;
            }
            (PickerKind::MoveDesktopModifier, PickerValue::Modifier(value)) => {
                self.draft.virtual_desktops.move_follow_modifier =
                    (!value.is_empty()).then_some(value);
            }
            (PickerKind::SilentMoveDesktopModifier, PickerValue::Modifier(value)) => {
                self.draft.virtual_desktops.move_silent_modifier =
                    (!value.is_empty()).then_some(value);
            }
            (PickerKind::OverlayAppearance, PickerValue::Appearance(value)) => {
                self.draft.overlay.appearance = value;
            }
            (PickerKind::OverlayPosition, PickerValue::Position(value)) => {
                self.draft.overlay.position = value;
            }
            (PickerKind::OverlayMonitor, PickerValue::Monitor(value)) => {
                self.draft.overlay.monitor = value;
                self.refresh_overlay_preview_aspect();
            }
            _ => {}
        }
        self.reset_confirm = false;
        self.validation.clear();
    }
}
