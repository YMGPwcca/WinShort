//! Values for the control center.

use super::audio::allowlist_label;
use super::config_toggle::ConfigToggle;
use super::overlay_preview::{overlay_duration_label, overlay_position, overlay_scale_label};
use super::state::{ConfirmationTarget, SettingsUi};
use crate::config::model::{OVERLAY_DURATION_MAX_MS, OVERLAY_DURATION_MIN_MS};
use crate::ui::controls::ControlValue;
use crate::ui::layout::{
    AudioElement, DisplayElement, ElementDomain, ElementId, HomeElement, HotkeySlot,
    OverlayElement, ShellElement, ShortcutCaptureElement, ShortcutElement, SystemElement,
    WorkspaceElement,
};
use crate::ui::navigation::search;
use crate::ui::presentation::{
    format_desktop_modifier, format_modifier as format_modifier_display,
};
use std::borrow::Cow;

impl SettingsUi {
    pub(super) fn value_for(&self, id: ElementId) -> ControlValue<'_> {
        match id.domain() {
            ElementDomain::Shell(element) => self.shell_value(element),
            ElementDomain::Home(element) => self.home_value(element),
            ElementDomain::Audio(element) => self.audio_value(element),
            ElementDomain::Displays(element) => self.display_value(element),
            ElementDomain::Shortcuts(element) => self.shortcut_value(element),
            ElementDomain::Workspaces(element) => self.workspace_value(element),
            ElementDomain::Overlay(element) => self.overlay_value(element),
            ElementDomain::System(element) => self.system_value(element),
        }
    }

    fn shell_value(&self, element: ShellElement) -> ControlValue<'_> {
        match element {
            ShellElement::Search => ControlValue::Text(Cow::Borrowed(&self.search_query)),
            ShellElement::Nav(page) => ControlValue::Action(Cow::Borrowed(page.label())),
            ShellElement::SearchResult(index) => ControlValue::Action(Cow::Borrowed(
                if search(&self.search_query).get(index as usize).is_some() {
                    "Open"
                } else {
                    ""
                },
            )),
            ShellElement::WindowClose => ControlValue::Action(Cow::Borrowed("Close")),
            ShellElement::ResumeDisplayDraft => {
                ControlValue::Action(Cow::Borrowed("Continue display edits"))
            }
            ShellElement::OnboardingContinue => ControlValue::Action(Cow::Borrowed("Continue")),
            ShellElement::OnboardingOpen => ControlValue::Action(Cow::Borrowed("Open WinShort")),
        }
    }

    fn home_value(&self, element: HomeElement) -> ControlValue<'_> {
        match element {
            HomeElement::Speaker => {
                ControlValue::Text(Cow::Owned(self.audio_view().current_output_name()))
            }
            HomeElement::CurrentDesktop => ControlValue::Text(Cow::Owned(
                self.runtime.desktop.current_desktop.map_or_else(
                    || "Desktop status unavailable".into(),
                    |index| format!("Desktop {}", index + 1),
                ),
            )),
            HomeElement::Microphone => {
                ControlValue::Text(Cow::Owned(self.audio_view().current_input_name()))
            }
            HomeElement::PreviousDesktop => ControlValue::Action(Cow::Borrowed("Switch")),
            HomeElement::Special => {
                ControlValue::Action(Cow::Owned(self.special_workspace_summary().2))
            }
            HomeElement::DisplayProfile => ControlValue::Text(Cow::Owned(self.display_summary().0)),
            HomeElement::ShortcutHealth => {
                ControlValue::Text(Cow::Owned(self.shortcut_health_copy().0))
            }
            HomeElement::Diagnostics => ControlValue::Action(Cow::Borrowed("Open")),
        }
    }

    fn audio_value(&self, element: AudioElement) -> ControlValue<'_> {
        match element {
            AudioElement::InputCycleMode(index) => {
                ControlValue::Toggle(self.choice_selected(ElementId::InputCycleMode(index)))
            }
            AudioElement::OutputCycleMode(index) => {
                ControlValue::Toggle(self.choice_selected(ElementId::OutputCycleMode(index)))
            }
            AudioElement::InputCycleDevice(index) => ControlValue::Toggle(
                self.audio_view()
                    .cycle_device_selected(crate::audio::DeviceCycleFlow::Input, index as usize),
            ),
            AudioElement::OutputCycleDevice(index) => ControlValue::Toggle(
                self.audio_view()
                    .cycle_device_selected(crate::audio::DeviceCycleFlow::Output, index as usize),
            ),
            AudioElement::InputDevice => ControlValue::Text(Cow::Owned(
                self.audio_view()
                    .selection(crate::audio::DeviceCycleFlow::Input)
                    .primary,
            )),
            AudioElement::OutputDevice => ControlValue::Text(Cow::Owned(
                self.audio_view()
                    .selection(crate::audio::DeviceCycleFlow::Output)
                    .primary,
            )),
            AudioElement::InputAllowlist => {
                let selection = self
                    .audio_view()
                    .cycle_selection(crate::audio::DeviceCycleFlow::Input);
                ControlValue::Text(Cow::Owned(allowlist_label(&selection)))
            }
            AudioElement::OutputAllowlist => {
                let selection = self
                    .audio_view()
                    .cycle_selection(crate::audio::DeviceCycleFlow::Output);
                ControlValue::Text(Cow::Owned(allowlist_label(&selection)))
            }
            AudioElement::InputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.input_role.label()))
            }
            AudioElement::OutputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.output_role.label()))
            }
        }
    }

    fn display_value(&self, element: DisplayElement) -> ControlValue<'_> {
        match element {
            DisplayElement::ProfileAction(index) => ControlValue::Action(Cow::Borrowed(
                if self
                    .profile_card_data(index as usize)
                    .is_some_and(|card| card.readiness.is_ready())
                {
                    "Activate"
                } else {
                    "Review"
                },
            )),
            DisplayElement::ProfileCard(index) => ControlValue::Text(Cow::Owned(
                self.draft
                    .display_profiles
                    .profiles
                    .get(index as usize)
                    .map(|profile| profile.name.clone())
                    .unwrap_or_else(|| "Unavailable".into()),
            )),
            DisplayElement::OutputCard(index) => ControlValue::Toggle(
                self.display_output_card_data(index as usize)
                    .is_some_and(|card| card.selected),
            ),
            DisplayElement::TopologyChoice(index) => {
                let topology = if index == 0 {
                    crate::display::DisplayTopology::Extend
                } else {
                    crate::display::DisplayTopology::Clone
                };
                ControlValue::Toggle(
                    self.draft
                        .display_profiles
                        .active()
                        .is_some_and(|profile| profile.topology == topology),
                )
            }
            DisplayElement::WizardBack => ControlValue::Action(Cow::Borrowed("Back")),
            DisplayElement::WizardNext => ControlValue::Action(Cow::Borrowed("Next")),
            DisplayElement::WizardCancel => ControlValue::Action(Cow::Borrowed("Cancel")),
            DisplayElement::WizardSummary => ControlValue::Text(Cow::Borrowed("")),
            DisplayElement::ProfilesEnabled => {
                ControlValue::Toggle(ConfigToggle::DisplayProfiles.selected(&self.draft))
            }
            DisplayElement::EditProfile => ControlValue::Action(Cow::Borrowed("Edit")),
            DisplayElement::Profile => ControlValue::Text(Cow::Owned(
                self.draft
                    .display_profiles
                    .active()
                    .map(|profile| profile.name.clone())
                    .unwrap_or_else(|| "No profile selected".into()),
            )),
            DisplayElement::Outputs => ControlValue::Text(Cow::Owned(self.display_outputs_label())),
            DisplayElement::Topology => ControlValue::Text(Cow::Owned(
                self.draft
                    .display_profiles
                    .active()
                    .map(|profile| {
                        if profile.routes.len() <= 1 {
                            "Single display".to_string()
                        } else {
                            profile.topology.label().to_string()
                        }
                    })
                    .unwrap_or_else(|| "No profile selected".into()),
            )),
            DisplayElement::Route => {
                ControlValue::Text(Cow::Owned(self.selected_display_route_label()))
            }
            DisplayElement::EditRoute => ControlValue::Action(Cow::Borrowed("Edit")),
            DisplayElement::NewProfile => ControlValue::Action(Cow::Borrowed("New from current")),
            DisplayElement::UpdateProfile => {
                ControlValue::Action(Cow::Borrowed("Replace from current"))
            }
            DisplayElement::RenameProfile => ControlValue::Action(Cow::Borrowed("Edit name")),
            DisplayElement::DuplicateProfile => ControlValue::Action(Cow::Borrowed("Duplicate")),
            DisplayElement::TestApply => ControlValue::Action(Cow::Borrowed("Test")),
            DisplayElement::DeleteProfile => ControlValue::Action(Cow::Borrowed(
                if self
                    .interaction
                    .confirmations()
                    .is_pending(ConfirmationTarget::DeleteDisplayProfile)
                {
                    "Confirm delete"
                } else {
                    "Delete"
                },
            )),
            DisplayElement::KeepChange => ControlValue::Action(Cow::Borrowed("Keep")),
            DisplayElement::UndoChange => ControlValue::Action(Cow::Borrowed("Revert")),
            DisplayElement::DiscardEdits => ControlValue::Action(Cow::Borrowed("Discard")),
        }
    }

    fn shortcut_value(&self, element: ShortcutElement) -> ControlValue<'_> {
        match element {
            ShortcutElement::Card(_) => ControlValue::Action(Cow::Borrowed("")),
            ShortcutElement::Enabled(slot) => {
                ControlValue::Action(Cow::Borrowed(if self.hotkey_enabled(slot) {
                    "Disable"
                } else {
                    "Enable"
                }))
            }
            ShortcutElement::Unassign(_) => ControlValue::Action(Cow::Borrowed("Unassign")),
            ShortcutElement::Capture(capture) => {
                let slot = Self::capture_slot(capture);
                self.hotkey_value(capture.id(), self.configured_hotkey(slot))
            }
        }
    }

    fn capture_slot(capture: ShortcutCaptureElement) -> HotkeySlot {
        match capture {
            ShortcutCaptureElement::Microphone => HotkeySlot::Microphone,
            ShortcutCaptureElement::Output => HotkeySlot::Output,
            ShortcutCaptureElement::Foreground => HotkeySlot::Foreground,
            ShortcutCaptureElement::CycleInput => HotkeySlot::CycleInput,
            ShortcutCaptureElement::CycleOutput => HotkeySlot::CycleOutput,
            ShortcutCaptureElement::ForegroundVolumeUp => HotkeySlot::ForegroundVolumeUp,
            ShortcutCaptureElement::ForegroundVolumeDown => HotkeySlot::ForegroundVolumeDown,
            ShortcutCaptureElement::PreviousDesktop => HotkeySlot::PreviousDesktop,
            ShortcutCaptureElement::AssignSpecial => HotkeySlot::AssignSpecial,
            ShortcutCaptureElement::ToggleSpecial => HotkeySlot::ToggleSpecial,
            ShortcutCaptureElement::DisplayProfile => HotkeySlot::DisplayProfile,
        }
    }

    fn workspace_value(&self, element: WorkspaceElement) -> ControlValue<'_> {
        match element {
            WorkspaceElement::Enabled => {
                ControlValue::Toggle(ConfigToggle::Workspaces.selected(&self.draft))
            }
            WorkspaceElement::WinNumberEnabled => {
                ControlValue::Toggle(ConfigToggle::WorkspaceNumbers.selected(&self.draft))
            }
            WorkspaceElement::DesktopNumberModifier => ControlValue::Text(Cow::Owned(
                format_desktop_modifier(self.draft.virtual_desktops.number_modifier),
            )),
            WorkspaceElement::MoveDesktopModifier => ControlValue::Text(Cow::Owned(
                self.draft
                    .virtual_desktops
                    .move_follow_modifier
                    .map_or_else(|| "Not assigned".into(), format_modifier_display),
            )),
            WorkspaceElement::SilentMoveDesktopModifier => ControlValue::Text(Cow::Owned(
                self.draft
                    .virtual_desktops
                    .move_silent_modifier
                    .map_or_else(|| "Not assigned".into(), format_modifier_display),
            )),
        }
    }

    fn overlay_value(&self, element: OverlayElement) -> ControlValue<'_> {
        match element {
            OverlayElement::Enabled => {
                ControlValue::Toggle(ConfigToggle::Overlay.selected(&self.draft))
            }
            OverlayElement::Microphone => {
                ControlValue::Toggle(ConfigToggle::OverlayMicrophone.selected(&self.draft))
            }
            OverlayElement::Speaker => {
                ControlValue::Toggle(ConfigToggle::OverlaySpeaker.selected(&self.draft))
            }
            OverlayElement::CurrentAppAudio => {
                ControlValue::Toggle(ConfigToggle::OverlayCurrentAppAudio.selected(&self.draft))
            }
            OverlayElement::Workspace => {
                ControlValue::Toggle(ConfigToggle::OverlayWorkspace.selected(&self.draft))
            }
            OverlayElement::DisplayProfile => {
                ControlValue::Toggle(ConfigToggle::OverlayDisplayProfile.selected(&self.draft))
            }
            OverlayElement::PositionCell(index) => ControlValue::Toggle(
                self.draft.overlay.position == overlay_position(index as usize),
            ),
            OverlayElement::Appearance => {
                ControlValue::Text(Cow::Borrowed(self.draft.overlay.appearance.label()))
            }
            OverlayElement::Position => {
                ControlValue::Text(Cow::Borrowed(self.draft.overlay.position.label()))
            }
            OverlayElement::Monitor => ControlValue::Text(Cow::Owned(
                crate::ui::presentation::monitor_choice_label(&self.draft.overlay.monitor),
            )),
            OverlayElement::Duration => ControlValue::Slider {
                ratio: (self
                    .draft
                    .overlay
                    .duration_ms
                    .saturating_sub(OVERLAY_DURATION_MIN_MS) as f32
                    / (OVERLAY_DURATION_MAX_MS - OVERLAY_DURATION_MIN_MS) as f32)
                    .clamp(0.0, 1.0),
                label: Cow::Owned(overlay_duration_label(self.draft.overlay.duration_ms)),
            },
            OverlayElement::Blur => ControlValue::Slider {
                ratio: self.draft.overlay.blur.index() as f32 / 4.0,
                label: Cow::Borrowed(self.draft.overlay.blur.label()),
            },
            OverlayElement::Scale => ControlValue::Slider {
                ratio: ((self.draft.overlay.scale - crate::config::model::OVERLAY_SCALE_MIN)
                    / (crate::config::model::OVERLAY_SCALE_MAX
                        - crate::config::model::OVERLAY_SCALE_MIN))
                    .clamp(0.0, 1.0),
                label: Cow::Owned(overlay_scale_label(self.draft.overlay.scale)),
            },
            OverlayElement::Preview => ControlValue::Action(Cow::Borrowed("Show on screen")),
            OverlayElement::HoverOpacity => ControlValue::Slider {
                ratio: (self.draft.overlay.hover_opacity - 0.1) / 0.9,
                label: Cow::Owned(if self.draft.overlay.hover_opacity >= 1.0 {
                    "100% (off)".into()
                } else {
                    format!("{:.0}%", self.draft.overlay.hover_opacity * 100.0)
                }),
            },
        }
    }

    fn system_value(&self, element: SystemElement) -> ControlValue<'_> {
        match element {
            SystemElement::StartWithWindows => ControlValue::Toggle(self.startup_enabled),
            SystemElement::CopyVersionInfo => ControlValue::Action(Cow::Borrowed("Copy")),
            SystemElement::StartHotkeysEnabled => {
                ControlValue::Toggle(ConfigToggle::PauseShortcuts.selected(&self.draft))
            }
            SystemElement::DebugLogging => ControlValue::Toggle(self.debug_logging_enabled),
            SystemElement::DiagnosticsStatus => ControlValue::Action(Cow::Borrowed("Open")),
            SystemElement::OpenConfigFolder => ControlValue::Action(Cow::Borrowed("Open folder")),
            SystemElement::ResetSettings => ControlValue::Action(Cow::Borrowed(
                if self
                    .interaction
                    .confirmations()
                    .is_pending(ConfirmationTarget::ResetSettings)
                {
                    "Confirm reset"
                } else {
                    "Reset"
                },
            )),
        }
    }
}
