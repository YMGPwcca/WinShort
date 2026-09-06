//! Domain availability policy for the Control Center.

use super::state::SettingsUi;
use crate::config::model::DeviceSelection;
use crate::ui::layout::{
    AudioElement, DisplayElement, ElementDomain, ElementId, HomeElement, HotkeySlot, OverlayElement,
    ShellElement, ShortcutCaptureElement, ShortcutElement, SystemElement, WorkspaceElement,
};
use crate::ui::presentation::DisplayWizardStep;

impl SettingsUi {
    pub(super) fn endpoint_role_enabled(selection: &DeviceSelection) -> bool {
        matches!(selection, DeviceSelection::Default)
    }

    pub(super) fn is_disabled(&self, id: ElementId) -> bool {
        match id.domain() {
            ElementDomain::Shell(element) => self.shell_disabled(element),
            ElementDomain::Home(element) => self.home_disabled(element),
            ElementDomain::Audio(element) => self.audio_disabled(element),
            ElementDomain::Displays(element) => self.display_disabled(element),
            ElementDomain::Shortcuts(element) => self.shortcut_disabled(element),
            ElementDomain::Workspaces(element) => self.workspace_disabled(element),
            ElementDomain::Overlay(element) => self.overlay_disabled(element),
            ElementDomain::System(element) => self.system_disabled(element),
        }
    }

    fn shell_disabled(&self, element: ShellElement) -> bool {
        match element {
            ShellElement::Save => !self.dirty(),
            ShellElement::Search
            | ShellElement::Nav(_)
            | ShellElement::SearchResult(_)
            | ShellElement::WindowClose
            | ShellElement::OnboardingContinue
            | ShellElement::OnboardingOpen
            | ShellElement::Cancel => false,
        }
    }

    fn home_disabled(&self, element: HomeElement) -> bool {
        match element {
            HomeElement::Speaker => matches!(
                &self.runtime.output,
                crate::audio::OutputState::Unavailable { .. }
            ),
            HomeElement::Microphone => matches!(
                &self.runtime.microphone,
                crate::audio::AudioState::Unavailable { .. }
            ),
            HomeElement::CurrentDesktop
            | HomeElement::PreviousDesktop
            | HomeElement::Special
            | HomeElement::DisplayProfile
            | HomeElement::ShortcutHealth
            | HomeElement::Diagnostics => false,
        }
    }

    fn audio_disabled(&self, element: AudioElement) -> bool {
        match element {
            AudioElement::InputRole => !Self::endpoint_role_enabled(&self.draft.audio.input_device),
            AudioElement::OutputRole => {
                !Self::endpoint_role_enabled(&self.draft.audio.output_device)
            }
            AudioElement::InputCycleMode(_)
            | AudioElement::OutputCycleMode(_)
            | AudioElement::InputCycleDevice(_)
            | AudioElement::OutputCycleDevice(_)
            | AudioElement::InputDevice
            | AudioElement::OutputDevice
            | AudioElement::InputAllowlist
            | AudioElement::OutputAllowlist => false,
        }
    }

    fn display_disabled(&self, element: DisplayElement) -> bool {
        match element {
            DisplayElement::Profile => {
                !self.draft.display_profiles.enabled
                    || self.draft.display_profiles.active().is_none()
                    || self.display_draft_dirty
            }
            DisplayElement::Outputs | DisplayElement::Route | DisplayElement::EditRoute => {
                self.display_edit_unavailable()
            }
            DisplayElement::Topology => {
                !self.draft.display_profiles.enabled
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.len() <= 1)
                    || self.display_rollback_active
            }
            DisplayElement::NewProfile => self.display_creation_unavailable(),
            DisplayElement::EditProfile => self.display_edit_unavailable(),
            DisplayElement::UpdateProfile
            | DisplayElement::DuplicateProfile
            | DisplayElement::DeleteProfile => {
                self.display_edit_unavailable() || self.display_editor.is_some()
            }
            DisplayElement::RenameProfile => self.display_edit_unavailable(),
            DisplayElement::TestApply => {
                self.display_edit_unavailable()
                    || self.display_editor.is_none()
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.is_empty())
            }
            DisplayElement::Apply => {
                !self.draft.display_profiles.enabled
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| !profile.confirmed)
                    || self.display_rollback_active
            }
            DisplayElement::ProfileCard(index) => {
                index as usize >= self.draft.display_profiles.profiles.len()
                    || self.display_creation_unavailable()
            }
            DisplayElement::OutputCard(index) => {
                self.display_editor.is_none()
                    || self.display_rollback_active
                    || index as usize >= self.display_route_candidates().len()
            }
            DisplayElement::TopologyChoice(_) => {
                self.display_editor.is_none()
                    || self.display_rollback_active
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.len() <= 1)
            }
            DisplayElement::WizardNext => self.display_editor.is_none_or(|editor| match editor.step {
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
            }),
            DisplayElement::WizardBack | DisplayElement::WizardCancel => self.display_editor.is_none(),
            DisplayElement::ProfilesEnabled => self.display_draft_dirty,
            DisplayElement::KeepChange => !self.display_keep_available,
            DisplayElement::UndoChange => !self.display_rollback_active,
            DisplayElement::DiscardEdits => !self.display_draft_dirty,
            DisplayElement::WizardSummary => false,
        }
    }

    fn shortcut_disabled(&self, element: ShortcutElement) -> bool {
        match element {
            ShortcutElement::Card(_) => false,
            ShortcutElement::Enabled(slot) | ShortcutElement::Unassign(slot) => {
                self.configured_hotkey(slot).is_none() || self.hotkey_domain_disabled(slot)
            }
            ShortcutElement::Capture(capture) => self.capture_domain_disabled(capture),
        }
    }

    fn capture_domain_disabled(&self, capture: ShortcutCaptureElement) -> bool {
        match capture {
            ShortcutCaptureElement::PreviousDesktop
            | ShortcutCaptureElement::AssignSpecial
            | ShortcutCaptureElement::ToggleSpecial => !self.draft.virtual_desktops.enabled,
            ShortcutCaptureElement::DisplayProfile => {
                self.draft.display_profiles.active().is_none()
                    || !self.draft.display_profiles.enabled
            }
            ShortcutCaptureElement::Microphone
            | ShortcutCaptureElement::Output
            | ShortcutCaptureElement::Foreground
            | ShortcutCaptureElement::CycleInput
            | ShortcutCaptureElement::CycleOutput
            | ShortcutCaptureElement::ForegroundVolumeUp
            | ShortcutCaptureElement::ForegroundVolumeDown => false,
        }
    }

    fn hotkey_domain_disabled(&self, slot: HotkeySlot) -> bool {
        match slot {
            HotkeySlot::PreviousDesktop | HotkeySlot::AssignSpecial | HotkeySlot::ToggleSpecial => {
                !self.draft.virtual_desktops.enabled
            }
            HotkeySlot::DisplayProfile => {
                self.draft.display_profiles.active().is_none()
                    || !self.draft.display_profiles.enabled
            }
            HotkeySlot::Microphone
            | HotkeySlot::Output
            | HotkeySlot::Foreground
            | HotkeySlot::CycleInput
            | HotkeySlot::CycleOutput
            | HotkeySlot::ForegroundVolumeUp
            | HotkeySlot::ForegroundVolumeDown => false,
        }
    }

    fn workspace_disabled(&self, element: WorkspaceElement) -> bool {
        match element {
            WorkspaceElement::Enabled => false,
            WorkspaceElement::WinNumberEnabled => !self.draft.virtual_desktops.enabled,
            WorkspaceElement::DesktopNumberModifier => {
                !self.draft.virtual_desktops.enabled
                    || !self.draft.virtual_desktops.win_number_switching
            }
            WorkspaceElement::MoveDesktopModifier
            | WorkspaceElement::SilentMoveDesktopModifier => !self.draft.virtual_desktops.enabled,
        }
    }

    fn overlay_disabled(&self, element: OverlayElement) -> bool {
        match element {
            OverlayElement::Enabled | OverlayElement::Appearance => false,
            OverlayElement::ExternalChanges
            | OverlayElement::Position
            | OverlayElement::Monitor
            | OverlayElement::PositionCell(_)
            | OverlayElement::Duration
            | OverlayElement::Opacity
            | OverlayElement::Scale
            | OverlayElement::Preview => !self.draft.overlay.enabled,
        }
    }

    fn system_disabled(&self, element: SystemElement) -> bool {
        match element {
            SystemElement::StartWithWindows
            | SystemElement::StartHotkeysEnabled
            | SystemElement::DebugLogging
            | SystemElement::DiagnosticsStatus
            | SystemElement::OpenConfigFolder
            | SystemElement::ResetSettings => false,
        }
    }

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
