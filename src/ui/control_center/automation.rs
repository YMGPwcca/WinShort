//! Adapter from Control Center presentation to immutable accessibility snapshots.

use super::audio_view::cycle_mode_choice;
use super::native::invalidate;
use super::overlay_preview::overlay_position_label;
use super::state::SettingsUi;
use crate::ui::control_center_automation::{
    snapshot_from_settings, SettingsAutomation, SettingsAutomationAction, SettingsAutomationNode,
};
use crate::ui::controls::ControlValue;
use crate::ui::layout::{
    AudioElement, DisplayElement, ElementDomain, ElementId, ElementKind, HomeElement,
    OverlayElement, ShellElement, ShortcutElement, SystemElement, WorkspaceElement,
};
use crate::ui::presentation::{format_optional_hotkey, friendly_device, AudioDeviceKind};
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn install_automation(&mut self, hwnd: HWND) {
        if self.automation.is_none() {
            self.automation = Some(SettingsAutomation::new(hwnd));
            self.publish_automation_snapshot(hwnd);
        }
    }

    pub(super) fn publish_automation_snapshot(&mut self, hwnd: HWND) {
        self.repair_focus();
        let values = self.automation_values();
        let Some(automation) = &self.automation else {
            return;
        };

        let mut snapshot =
            snapshot_from_settings(hwnd, &self.layout, &values, self.focus.target(), self.dpi);
        for node in &mut snapshot.nodes {
            self.enrich_automation_node(node);
        }
        snapshot.set_focus_state(self.focus.owner(), self.focus.picker_owner());
        automation.publish(snapshot);
    }

    fn automation_values(&self) -> Vec<(ElementId, String, bool, f32)> {
        self.layout
            .elements
            .iter()
            .filter(|element| element.kind != ElementKind::Card)
            .map(|element| {
                let id = element.id;
                let (value, ratio) = match self.value_for(id) {
                    ControlValue::Toggle(value) => {
                        (if value { "On".into() } else { "Off".into() }, 0.0)
                    }
                    ControlValue::Text(value) | ControlValue::Action(value) => {
                        (value.into_owned(), 0.0)
                    }
                    ControlValue::Slider { ratio, label } => (label.into_owned(), ratio),
                };
                (id, value, !self.is_disabled(id), ratio)
            })
            .collect()
    }

    fn enrich_automation_node(&self, node: &mut SettingsAutomationNode) {
        match node.id.domain() {
            ElementDomain::Shell(element) => self.enrich_shell_automation_node(node, element),
            ElementDomain::Home(element) => self.enrich_home_automation_node(node, element),
            ElementDomain::Audio(element) => self.enrich_audio_automation_node(node, element),
            ElementDomain::Displays(element) => self.enrich_display_automation_node(node, element),
            ElementDomain::Shortcuts(element) => {
                self.enrich_shortcut_automation_node(node, element)
            }
            ElementDomain::Workspaces(element) => {
                self.enrich_workspace_automation_node(node, element)
            }
            ElementDomain::Overlay(element) => self.enrich_overlay_automation_node(node, element),
            ElementDomain::System(element) => self.enrich_system_automation_node(node, element),
        }
    }

    fn enrich_shell_automation_node(
        &self,
        node: &mut SettingsAutomationNode,
        element: ShellElement,
    ) {
        match element {
            ShellElement::Nav(page) => {
                node.name = if page == self.page {
                    format!("{} (selected)", page.label())
                } else {
                    page.label().into()
                };
                node.help_text = page.description().into();
            }
            ShellElement::Search
            | ShellElement::SearchResult(_)
            | ShellElement::WindowClose
            | ShellElement::OnboardingContinue
            | ShellElement::OnboardingOpen
            | ShellElement::Cancel
            | ShellElement::Save => {}
        }
    }

    fn enrich_home_automation_node(&self, node: &mut SettingsAutomationNode, element: HomeElement) {
        match element {
            HomeElement::Special => {
                let (status, detail, action) = self.special_workspace_summary();
                node.name = format!("Special Desktop: {status}");
                node.value = action;
                node.help_text = detail;
            }
            HomeElement::Diagnostics => {
                let (title, value, detail) = self.home_diagnostics_copy();
                node.name = title;
                node.value = value;
                node.help_text = detail;
            }
            HomeElement::Speaker
            | HomeElement::CurrentDesktop
            | HomeElement::Microphone
            | HomeElement::PreviousDesktop
            | HomeElement::DisplayProfile
            | HomeElement::ShortcutHealth => {}
        }
    }

    fn enrich_audio_automation_node(
        &self,
        node: &mut SettingsAutomationNode,
        element: AudioElement,
    ) {
        match element {
            AudioElement::InputCycleMode(index) => self.enrich_cycle_mode_node(
                node,
                index,
                crate::audio::DeviceCycleFlow::Input,
                AudioDeviceKind::Microphone,
            ),
            AudioElement::OutputCycleMode(index) => self.enrich_cycle_mode_node(
                node,
                index,
                crate::audio::DeviceCycleFlow::Output,
                AudioDeviceKind::Speaker,
            ),
            AudioElement::InputDevice => {
                self.enrich_selected_device_node(node, crate::audio::DeviceCycleFlow::Input);
            }
            AudioElement::OutputDevice => {
                self.enrich_selected_device_node(node, crate::audio::DeviceCycleFlow::Output);
            }
            AudioElement::InputCycleDevice(index) => {
                if let Some(device) = self.devices.inputs.get(index as usize) {
                    let label = friendly_device(device, AudioDeviceKind::Microphone);
                    node.name = label.primary;
                    node.help_text = label
                        .detail
                        .unwrap_or_else(|| "Use this microphone when cycling".into());
                }
            }
            AudioElement::OutputCycleDevice(index) => {
                if let Some(device) = self.devices.outputs.get(index as usize) {
                    let label = friendly_device(device, AudioDeviceKind::Speaker);
                    node.name = label.primary;
                    node.help_text = label
                        .detail
                        .unwrap_or_else(|| "Use this speaker when cycling".into());
                }
            }
            AudioElement::InputAllowlist
            | AudioElement::OutputAllowlist
            | AudioElement::InputRole
            | AudioElement::OutputRole => {}
        }
    }

    fn enrich_cycle_mode_node(
        &self,
        node: &mut SettingsAutomationNode,
        index: u8,
        flow: crate::audio::DeviceCycleFlow,
        kind: AudioDeviceKind,
    ) {
        let Some(choice) = cycle_mode_choice(index) else {
            node.enabled = false;
            return;
        };
        let mode = self.audio_view().mode(flow);
        node.name = crate::ui::presentation::allowlist_mode_label(choice, kind).into();
        node.help_text = if mode == choice {
            "Selected cycling mode".into()
        } else {
            "Choose this cycling mode".into()
        };
    }

    fn enrich_selected_device_node(
        &self,
        node: &mut SettingsAutomationNode,
        flow: crate::audio::DeviceCycleFlow,
    ) {
        let presentation = self.audio_view().selection(flow);
        node.name = format!("{}: {}", node.name, presentation.primary);
        node.value = presentation.accessible_value();
        node.help_text = node.value.clone();
    }

    fn enrich_display_automation_node(
        &self,
        node: &mut SettingsAutomationNode,
        element: DisplayElement,
    ) {
        match element {
            DisplayElement::ProfileCard(index) => {
                let Some(profile) = self.draft.display_profiles.profiles.get(index as usize) else {
                    return;
                };
                node.name = profile.name.clone();
                let ready = self
                    .profile_card_data(index as usize)
                    .is_some_and(|card| card.readiness.is_ready());
                node.help_text = if ready {
                    "Select this ready display profile to activate it".into()
                } else if self.inventory.error().is_some() {
                    "Windows display information is unavailable; readiness is unknown".into()
                } else if profile.confirmed {
                    "Review this profile; a saved screen needs attention".into()
                } else {
                    "Select this profile and test it before activation".into()
                };
            }
            DisplayElement::WizardSummary => {
                let Some(profile) = self.draft.display_profiles.active() else {
                    return;
                };
                let arrangement = if profile.routes.len() <= 1 {
                    "Single display".into()
                } else {
                    profile.topology.label().to_string()
                };
                let screens = self.display_review_screen_names(profile);
                let shortcut = format_optional_hotkey(self.active_profile_hotkey());
                let readiness = self.display_review_readiness(profile);
                node.name = format!("Display profile review: {}", profile.name);
                node.help_text = format!(
                    "Profile: {}. Screens: {screens}. Arrangement: {arrangement}. Shortcut: {shortcut}. Readiness: {readiness}.",
                    profile.name
                );
            }
            DisplayElement::RenameProfile => {
                if let Some(profile) = self.draft.display_profiles.active() {
                    node.name = format!("Profile name: {}", profile.name);
                    node.help_text = "Change the current profile name".into();
                }
            }
            DisplayElement::OutputCard(index) => {
                if let Some(card) = self.display_output_card_data(index as usize) {
                    node.name = card.primary;
                    node.help_text = card.detail;
                }
            }
            DisplayElement::TopologyChoice(index) => {
                node.name = if index == 0 { "Extend" } else { "Duplicate" }.into();
            }
            DisplayElement::WizardBack
            | DisplayElement::WizardNext
            | DisplayElement::WizardCancel
            | DisplayElement::ProfilesEnabled
            | DisplayElement::EditProfile
            | DisplayElement::Profile
            | DisplayElement::Outputs
            | DisplayElement::Topology
            | DisplayElement::Route
            | DisplayElement::EditRoute
            | DisplayElement::NewProfile
            | DisplayElement::UpdateProfile
            | DisplayElement::DuplicateProfile
            | DisplayElement::TestApply
            | DisplayElement::Apply
            | DisplayElement::DeleteProfile
            | DisplayElement::KeepChange
            | DisplayElement::UndoChange
            | DisplayElement::DiscardEdits => {}
        }
    }

    fn enrich_shortcut_automation_node(
        &self,
        node: &mut SettingsAutomationNode,
        element: ShortcutElement,
    ) {
        match element {
            ShortcutElement::Enabled(slot) => {
                let state = if self.hotkey_enabled(slot) {
                    "Disable"
                } else {
                    "Enable"
                };
                node.name = format!("{} {}", state, Self::hotkey_subject(slot));
                node.help_text = if self.configured_hotkey(slot).is_some() {
                    format!(
                        "{} this shortcut without changing its assigned chord",
                        state
                    )
                } else {
                    "Assign a shortcut before enabling it".into()
                };
            }
            ShortcutElement::Unassign(slot) => {
                node.name = format!("Unassign {}", Self::hotkey_subject(slot));
                node.help_text = if self.configured_hotkey(slot).is_some() {
                    "Remove this shortcut completely".into()
                } else {
                    "No shortcut is assigned".into()
                };
            }
            ShortcutElement::Card(_) | ShortcutElement::Capture(_) => {}
        }
    }

    fn enrich_workspace_automation_node(
        &self,
        _node: &mut SettingsAutomationNode,
        element: WorkspaceElement,
    ) {
        match element {
            WorkspaceElement::Enabled
            | WorkspaceElement::WinNumberEnabled
            | WorkspaceElement::DesktopNumberModifier
            | WorkspaceElement::MoveDesktopModifier
            | WorkspaceElement::SilentMoveDesktopModifier => {}
        }
    }

    fn enrich_overlay_automation_node(
        &self,
        node: &mut SettingsAutomationNode,
        element: OverlayElement,
    ) {
        match element {
            OverlayElement::PositionCell(index) => {
                node.name = overlay_position_label(index as usize).into();
                node.help_text = "Choose this overlay position".into();
            }
            OverlayElement::Enabled
            | OverlayElement::ExternalChanges
            | OverlayElement::Appearance
            | OverlayElement::Position
            | OverlayElement::Monitor
            | OverlayElement::Duration
            | OverlayElement::Opacity
            | OverlayElement::Scale
            | OverlayElement::Preview => {}
        }
    }

    fn enrich_system_automation_node(
        &self,
        _node: &mut SettingsAutomationNode,
        element: SystemElement,
    ) {
        match element {
            SystemElement::StartWithWindows
            | SystemElement::StartHotkeysEnabled
            | SystemElement::DebugLogging
            | SystemElement::DiagnosticsStatus
            | SystemElement::OpenConfigFolder
            | SystemElement::ResetSettings => {}
        }
    }

    pub(super) fn drain_automation_actions(&mut self, hwnd: HWND) -> bool {
        let actions = self
            .automation
            .as_ref()
            .map(SettingsAutomation::drain_actions)
            .unwrap_or_default();
        if self.interaction.closing() {
            return false;
        }

        let mut focus_requested = false;
        for action in actions {
            match action {
                SettingsAutomationAction::Invoke(id) | SettingsAutomationAction::Toggle(id) => {
                    self.focus.set_indicator_visible(true);
                    self.focus.set_target(Some(id));
                    self.activate(hwnd, id);
                }
                SettingsAutomationAction::SetSlider { id, value } => {
                    self.focus.set_indicator_visible(true);
                    let before = self.draft.clone();
                    if !self.is_disabled(id)
                        && self.set_slider_from_value(id, value)
                        && self.commit_local_change(hwnd, before)
                    {
                        self.focus.set_target(Some(id));
                        invalidate(hwnd);
                    }
                }
                SettingsAutomationAction::SetSearch(value) => {
                    self.focus.set_indicator_visible(true);
                    self.search_query = value;
                    self.reset_scroll();
                    self.focus.set_target(Some(ElementId::Search));
                    self.rebuild_layout(hwnd);
                    invalidate(hwnd);
                    focus_requested = true;
                }
                SettingsAutomationAction::SetWindowFocus => focus_requested = true,
                SettingsAutomationAction::SetFocus(id) => {
                    if self.layout.element(id).is_some() && !self.is_disabled(id) {
                        self.focus.set_indicator_visible(true);
                        self.focus.set_target(Some(id));
                        self.scroll_focus_into_view(id);
                        self.rebuild_layout(hwnd);
                        invalidate(hwnd);
                        focus_requested = true;
                    }
                }
            }
        }
        self.publish_automation_snapshot(hwnd);
        focus_requested
    }
}
