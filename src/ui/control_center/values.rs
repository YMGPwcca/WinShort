//! Values for the control center.

use super::audio::allowlist_label;
use super::config_toggle::ConfigToggle;
use super::overlay_preview::{
    overlay_duration_label, overlay_opacity_label, overlay_position, overlay_scale_label,
};
use super::state::{ConfirmationTarget, SettingsUi};
use crate::ui::controls::ControlValue;
use crate::ui::layout::{ElementId, HotkeySlot};
use crate::ui::navigation::search;
use crate::ui::presentation::{
    format_desktop_modifier, format_modifier as format_modifier_display,
};
use std::borrow::Cow;

impl SettingsUi {
    pub(super) fn value_for(&self, id: ElementId) -> ControlValue<'_> {
        match id {
            ElementId::DesktopsEnabled
            | ElementId::DisplayProfilesEnabled
            | ElementId::OverlayEnabled
            | ElementId::OverlayExternalChanges
            | ElementId::StartHotkeysEnabled
            | ElementId::WinNumberEnabled => ControlValue::Toggle(
                ConfigToggle::from_element(id).is_some_and(|toggle| toggle.selected(&self.draft)),
            ),
            ElementId::Search => ControlValue::Text(Cow::Borrowed(&self.search_query)),
            ElementId::Nav(page) => ControlValue::Action(Cow::Borrowed(page.label())),
            ElementId::SearchResult(index) => {
                if search(&self.search_query).get(index as usize).is_some() {
                    ControlValue::Action(Cow::Borrowed("Open"))
                } else {
                    ControlValue::Action(Cow::Borrowed(""))
                }
            }
            ElementId::WindowClose => ControlValue::Action(Cow::Borrowed("Close")),
            ElementId::HomeSpeaker => {
                ControlValue::Text(Cow::Owned(self.audio_view().current_output_name()))
            }
            ElementId::HomeCurrentDesktop => ControlValue::Text(Cow::Owned(
                self.runtime.desktop.current_desktop.map_or_else(
                    || "Desktop status unavailable".into(),
                    |index| format!("Desktop {}", index + 1),
                ),
            )),
            ElementId::HomeMicrophone => {
                ControlValue::Text(Cow::Owned(self.audio_view().current_input_name()))
            }
            ElementId::HomePreviousDesktop => ControlValue::Action(Cow::Borrowed("Switch")),
            ElementId::HomeSpecial => {
                ControlValue::Action(Cow::Owned(self.special_workspace_summary().2))
            }
            ElementId::HomeDisplayProfile => {
                ControlValue::Text(Cow::Owned(self.display_summary().0))
            }
            ElementId::HomeShortcutHealth => {
                ControlValue::Text(Cow::Owned(self.shortcut_health_copy().0))
            }
            ElementId::HomeDiagnostics => ControlValue::Action(Cow::Borrowed("Open")),
            ElementId::DisplayProfileCard(index) => ControlValue::Text(Cow::Owned(
                self.draft
                    .display_profiles
                    .profiles
                    .get(index as usize)
                    .map(|profile| profile.name.clone())
                    .unwrap_or_else(|| "Unavailable".into()),
            )),
            ElementId::DisplayOutputCard(index) => ControlValue::Toggle(
                self.display_output_card_data(index as usize)
                    .is_some_and(|card| card.selected),
            ),
            ElementId::DisplayTopologyChoice(index) => {
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
            ElementId::InputCycleMode(_) | ElementId::OutputCycleMode(_) => {
                ControlValue::Toggle(self.choice_selected(id))
            }
            ElementId::InputCycleDevice(index) => ControlValue::Toggle(
                self.audio_view()
                    .cycle_device_selected(crate::audio::DeviceCycleFlow::Input, index as usize),
            ),
            ElementId::OutputCycleDevice(index) => ControlValue::Toggle(
                self.audio_view()
                    .cycle_device_selected(crate::audio::DeviceCycleFlow::Output, index as usize),
            ),
            ElementId::OverlayPositionCell(index) => ControlValue::Toggle(
                self.draft.overlay.position == overlay_position(index as usize),
            ),
            ElementId::DisplayWizardBack => ControlValue::Action(Cow::Borrowed("Back")),
            ElementId::DisplayWizardNext => ControlValue::Action(Cow::Borrowed("Next")),
            ElementId::DisplayWizardCancel => ControlValue::Action(Cow::Borrowed("Cancel")),
            ElementId::EditDisplayProfile => ControlValue::Action(Cow::Borrowed("Edit")),
            ElementId::DisplayWizardSummary => ControlValue::Text(Cow::Borrowed("")),
            ElementId::OnboardingContinue => ControlValue::Action(Cow::Borrowed("Continue")),
            ElementId::OnboardingOpen => ControlValue::Action(Cow::Borrowed("Open WinShort")),
            ElementId::StartWithWindows => ControlValue::Toggle(self.startup_enabled),
            ElementId::HotkeyCard(_) => ControlValue::Action(Cow::Borrowed("")),
            ElementId::HotkeyEnabled(slot) => {
                ControlValue::Action(Cow::Borrowed(if self.hotkey_enabled(slot) {
                    "Disable"
                } else {
                    "Enable"
                }))
            }
            ElementId::HotkeyUnassign(_) => ControlValue::Action(Cow::Borrowed("Unassign")),
            ElementId::MicHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::Microphone))
            }
            ElementId::OutputHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::Output))
            }
            ElementId::ForegroundHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::Foreground))
            }
            ElementId::CycleInputHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::CycleInput))
            }
            ElementId::CycleOutputHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::CycleOutput))
            }
            ElementId::ForegroundVolumeUpHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::ForegroundVolumeUp))
            }
            ElementId::ForegroundVolumeDownHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::ForegroundVolumeDown))
            }
            ElementId::PreviousDesktopHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::PreviousDesktop))
            }
            ElementId::AssignScratchpadHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::AssignSpecial))
            }
            ElementId::ToggleScratchpadHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::ToggleSpecial))
            }
            ElementId::InputDevice => ControlValue::Text(Cow::Owned(
                self.audio_view()
                    .selection(crate::audio::DeviceCycleFlow::Input)
                    .primary,
            )),
            ElementId::OutputDevice => ControlValue::Text(Cow::Owned(
                self.audio_view()
                    .selection(crate::audio::DeviceCycleFlow::Output)
                    .primary,
            )),
            ElementId::InputAllowlist => {
                let selection = self
                    .audio_view()
                    .cycle_selection(crate::audio::DeviceCycleFlow::Input);
                ControlValue::Text(Cow::Owned(allowlist_label(&selection)))
            }
            ElementId::OutputAllowlist => {
                let selection = self
                    .audio_view()
                    .cycle_selection(crate::audio::DeviceCycleFlow::Output);
                ControlValue::Text(Cow::Owned(allowlist_label(&selection)))
            }
            ElementId::InputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.input_role.label()))
            }
            ElementId::OutputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.output_role.label()))
            }
            ElementId::DisplayProfile => ControlValue::Text(Cow::Owned(
                self.draft
                    .display_profiles
                    .active()
                    .map(|profile| profile.name.clone())
                    .unwrap_or_else(|| "No profile selected".into()),
            )),
            ElementId::DisplayProfileHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::DisplayProfile))
            }
            ElementId::DisplayOutputs => {
                ControlValue::Text(Cow::Owned(self.display_outputs_label()))
            }
            ElementId::DisplayTopology => ControlValue::Text(Cow::Owned(
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
            ElementId::DisplayRoute => {
                ControlValue::Text(Cow::Owned(self.selected_display_route_label()))
            }
            ElementId::EditDisplayRoute => ControlValue::Action(Cow::Borrowed("Edit")),
            ElementId::NewDisplayProfile => ControlValue::Action(Cow::Borrowed("New from current")),
            ElementId::UpdateDisplayProfile => {
                ControlValue::Action(Cow::Borrowed("Replace from current"))
            }
            ElementId::RenameDisplayProfile => ControlValue::Action(Cow::Borrowed("Edit name")),
            ElementId::DuplicateDisplayProfile => ControlValue::Action(Cow::Borrowed("Duplicate")),
            ElementId::TestApplyDisplayProfile => ControlValue::Action(Cow::Borrowed("Test")),
            ElementId::ApplyDisplayProfile => ControlValue::Action(Cow::Borrowed("Activate")),
            ElementId::DeleteDisplayProfile => ControlValue::Action(Cow::Borrowed(
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
            ElementId::KeepDisplayChange => ControlValue::Action(Cow::Borrowed("Keep")),
            ElementId::UndoDisplayChange => ControlValue::Action(Cow::Borrowed("Revert")),
            ElementId::DiscardDisplayEdits => ControlValue::Action(Cow::Borrowed("Discard")),
            ElementId::DesktopNumberModifier => ControlValue::Text(Cow::Owned(
                format_desktop_modifier(self.draft.virtual_desktops.number_modifier),
            )),
            ElementId::MoveDesktopModifier => ControlValue::Text(Cow::Owned(
                self.draft
                    .virtual_desktops
                    .move_follow_modifier
                    .map_or_else(|| "Not assigned".into(), format_modifier_display),
            )),
            ElementId::SilentMoveDesktopModifier => ControlValue::Text(Cow::Owned(
                self.draft
                    .virtual_desktops
                    .move_silent_modifier
                    .map_or_else(|| "Not assigned".into(), format_modifier_display),
            )),
            ElementId::OverlayAppearance => {
                ControlValue::Text(Cow::Borrowed(self.draft.overlay.appearance.label()))
            }
            ElementId::OverlayPosition => {
                ControlValue::Text(Cow::Borrowed(self.draft.overlay.position.label()))
            }
            ElementId::OverlayMonitor => ControlValue::Text(Cow::Owned(
                crate::ui::presentation::monitor_choice_label(&self.draft.overlay.monitor),
            )),
            ElementId::OverlayDuration => ControlValue::Slider {
                ratio: (self.draft.overlay.duration_ms.saturating_sub(500) as f32 / 9500.0)
                    .clamp(0.0, 1.0),
                label: Cow::Borrowed(overlay_duration_label(self.draft.overlay.duration_ms)),
            },
            ElementId::OverlayOpacity => ControlValue::Slider {
                ratio: ((self.draft.overlay.opacity - 0.3) / 0.7).clamp(0.0, 1.0),
                label: Cow::Borrowed(overlay_opacity_label(self.draft.overlay.opacity)),
            },
            ElementId::OverlayScale => ControlValue::Slider {
                ratio: ((self.draft.overlay.scale - 0.7) / 0.9).clamp(0.0, 1.0),
                label: Cow::Borrowed(overlay_scale_label(self.draft.overlay.scale)),
            },
            ElementId::OverlayPreview => ControlValue::Action(Cow::Borrowed("Show on screen")),
            ElementId::DebugLogging => {
                ControlValue::Toggle(crate::diagnostics::logging::debug_logging_enabled())
            }
            ElementId::DiagnosticsStatus => ControlValue::Action(Cow::Borrowed("Open")),
            ElementId::OpenConfigFolder => ControlValue::Action(Cow::Borrowed("Open folder")),
            ElementId::ResetSettings => ControlValue::Action(Cow::Borrowed(
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
            ElementId::Cancel | ElementId::Save => ControlValue::Action(Cow::Borrowed("")),
        }
    }
}
