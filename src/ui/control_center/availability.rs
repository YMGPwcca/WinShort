//! Availability for the control center.

use super::state::SettingsUi;
use crate::config::model::DeviceSelection;
use crate::ui::layout::{ElementId, HotkeySlot};
use crate::ui::presentation::DisplayWizardStep;

impl SettingsUi {
    pub(super) fn endpoint_role_enabled(selection: &DeviceSelection) -> bool {
        matches!(selection, DeviceSelection::Default)
    }

    pub(super) fn is_disabled(&self, id: ElementId) -> bool {
        match id {
            ElementId::HotkeyEnabled(slot) | ElementId::HotkeyUnassign(slot) => {
                self.configured_hotkey(slot).is_none()
                    || (matches!(
                        slot,
                        HotkeySlot::PreviousDesktop
                            | HotkeySlot::AssignSpecial
                            | HotkeySlot::ToggleSpecial
                    ) && !self.draft.virtual_desktops.enabled)
                    || (slot == HotkeySlot::DisplayProfile
                        && self.draft.display_profiles.active().is_none())
            }
            ElementId::HotkeyCard(_) => false,
            ElementId::WinNumberEnabled => !self.draft.virtual_desktops.enabled,
            ElementId::DesktopNumberModifier => {
                !self.draft.virtual_desktops.enabled
                    || !self.draft.virtual_desktops.win_number_switching
            }
            ElementId::MoveDesktopModifier
            | ElementId::SilentMoveDesktopModifier
            | ElementId::PreviousDesktopHotkey
            | ElementId::AssignScratchpadHotkey
            | ElementId::ToggleScratchpadHotkey => !self.draft.virtual_desktops.enabled,
            ElementId::DisplayProfile => {
                !self.draft.display_profiles.enabled
                    || self.draft.display_profiles.active().is_none()
                    || self.display_draft_dirty
            }
            ElementId::DisplayProfileHotkey
            | ElementId::DisplayOutputs
            | ElementId::DisplayRoute
            | ElementId::EditDisplayRoute => self.display_edit_unavailable(),
            ElementId::DisplayTopology => {
                !self.draft.display_profiles.enabled
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.len() <= 1)
                    || self.display_rollback_active
            }
            ElementId::NewDisplayProfile => self.display_creation_unavailable(),
            ElementId::EditDisplayProfile => self.display_edit_unavailable(),
            ElementId::UpdateDisplayProfile
            | ElementId::DuplicateDisplayProfile
            | ElementId::DeleteDisplayProfile => {
                self.display_edit_unavailable() || self.display_editor.is_some()
            }
            ElementId::RenameDisplayProfile => self.display_edit_unavailable(),
            ElementId::TestApplyDisplayProfile => {
                self.display_edit_unavailable()
                    || self.display_editor.is_none()
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.is_empty())
            }
            ElementId::ApplyDisplayProfile => {
                !self.draft.display_profiles.enabled
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| !profile.confirmed)
                    || self.display_rollback_active
            }
            ElementId::DisplayProfileCard(index) => {
                index as usize >= self.draft.display_profiles.profiles.len()
                    || self.display_creation_unavailable()
            }
            ElementId::DisplayOutputCard(index) => {
                self.display_editor.is_none()
                    || self.display_rollback_active
                    || index as usize >= self.display_route_candidates().len()
            }
            ElementId::DisplayTopologyChoice(_) => {
                self.display_editor.is_none()
                    || self.display_rollback_active
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.len() <= 1)
            }
            ElementId::DisplayWizardNext => {
                self.display_editor.is_none_or(|editor| match editor.step {
                    DisplayWizardStep::Displays => self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.is_empty()),
                    DisplayWizardStep::Arrangement => false,
                    DisplayWizardStep::NameAndShortcut => self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.name.trim().is_empty()),
                    DisplayWizardStep::Review => true,
                })
            }
            ElementId::DisplayWizardBack | ElementId::DisplayWizardCancel => {
                self.display_editor.is_none()
            }
            ElementId::DisplayProfilesEnabled => self.display_draft_dirty,
            ElementId::KeepDisplayChange => !self.display_keep_available,
            ElementId::UndoDisplayChange => !self.display_rollback_active,
            ElementId::DiscardDisplayEdits => !self.display_draft_dirty,
            ElementId::InputRole => !Self::endpoint_role_enabled(&self.draft.audio.input_device),
            ElementId::OutputRole => !Self::endpoint_role_enabled(&self.draft.audio.output_device),
            ElementId::OverlayAppearance => false,
            ElementId::OverlayExternalChanges
            | ElementId::OverlayPosition
            | ElementId::OverlayMonitor
            | ElementId::OverlayPositionCell(_)
            | ElementId::OverlayDuration
            | ElementId::OverlayOpacity
            | ElementId::OverlayScale
            | ElementId::OverlayPreview => !self.draft.overlay.enabled,
            ElementId::Search
            | ElementId::Nav(_)
            | ElementId::SearchResult(_)
            | ElementId::HomeCurrentDesktop
            | ElementId::HomePreviousDesktop
            | ElementId::HomeSpecial
            | ElementId::HomeDisplayProfile
            | ElementId::HomeShortcutHealth
            | ElementId::HomeDiagnostics
            | ElementId::OnboardingContinue
            | ElementId::OnboardingOpen
            | ElementId::DisplayWizardSummary
            | ElementId::InputAllowlist
            | ElementId::OutputAllowlist
            | ElementId::InputCycleMode(_)
            | ElementId::OutputCycleMode(_)
            | ElementId::InputCycleDevice(_)
            | ElementId::OutputCycleDevice(_)
            | ElementId::DebugLogging
            | ElementId::DiagnosticsStatus
            | ElementId::OpenConfigFolder
            | ElementId::ResetSettings => false,
            ElementId::Save => !self.dirty(),
            ElementId::HomeSpeaker => {
                matches!(
                    &self.runtime.output,
                    crate::audio::OutputState::Unavailable { .. }
                )
            }
            ElementId::HomeMicrophone => {
                matches!(
                    &self.runtime.microphone,
                    crate::audio::AudioState::Unavailable { .. }
                )
            }
            _ => false,
        }
    }
}

impl SettingsUi {
    fn display_edit_unavailable(&self) -> bool {
        !self.draft.display_profiles.enabled
            || self.draft.display_profiles.active().is_none()
            || self.display_rollback_active
    }
    fn display_creation_unavailable(&self) -> bool {
        !self.draft.display_profiles.enabled
            || self.display_rollback_active
            || self.display_editor.is_some()
    }
}
