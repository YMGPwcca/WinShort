//! Domain-routed activation for the Control Center.

use super::config_toggle::ConfigToggle;
use super::native::{invalidate, open_config_folder, post_main, start_timer};
use super::state::SettingsUi;
use crate::audio::DeviceCycleFlow;
use crate::keyboard::binding::ModifierMask;
use crate::ui::control_center_automation::node_has_invoke;
use crate::ui::layout::{
    AudioElement, DisplayElement, ElementDomain, ElementId, HomeElement, OverlayElement,
    ShellElement, ShortcutElement, SystemElement, WorkspaceElement,
};
use crate::ui::navigation::Page;
use crate::ui::picker::PickerKind;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::WM_CLOSE;

impl SettingsUi {
    pub(super) fn consume_reset_confirmation(confirm: &mut bool) -> bool {
        if *confirm {
            *confirm = false;
            true
        } else {
            *confirm = true;
            false
        }
    }

    pub(super) fn activate(&mut self, hwnd: HWND, id: ElementId) {
        if self.closing || self.is_disabled(id) {
            return;
        }

        self.queue_invoked_automation(id);
        self.clear_stale_confirmations(id);
        match id.domain() {
            ElementDomain::Shell(element) => self.activate_shell(hwnd, element),
            ElementDomain::Home(element) => self.activate_home(hwnd, element),
            ElementDomain::Audio(element) => self.activate_audio(hwnd, element),
            ElementDomain::Displays(element) => self.activate_display(hwnd, element),
            ElementDomain::Shortcuts(element) => self.activate_shortcut(hwnd, element),
            ElementDomain::Workspaces(element) => self.activate_workspace(hwnd, element),
            ElementDomain::Overlay(element) => self.activate_overlay(hwnd, element),
            ElementDomain::System(element) => self.activate_system(hwnd, element),
        }

        self.rebuild_layout(hwnd);
        invalidate(hwnd);
        self.publish_automation_snapshot(hwnd);
    }

    fn queue_invoked_automation(&self, id: ElementId) {
        let invokes = self
            .layout
            .element(id)
            .is_some_and(|element| node_has_invoke(element.kind));
        if invokes {
            if let Some(automation) = &self.automation {
                automation.queue_invoked(id);
            }
        }
    }

    fn clear_stale_confirmations(&mut self, id: ElementId) {
        if id != ElementId::ResetSettings {
            self.reset_confirm = false;
        }
        if id != ElementId::DeleteDisplayProfile {
            self.delete_profile_confirm = false;
        }
    }

    fn activate_config_toggle(&mut self, hwnd: HWND, id: ElementId) {
        let Some(toggle) = ConfigToggle::from_element(id) else {
            crate::warn_!("element {id:?} was routed as a config toggle without a toggle policy");
            return;
        };
        let before = self.draft.clone();
        toggle.toggle(&mut self.draft);
        if self.commit_local_change(hwnd, before) {
            self.animate_toggle(hwnd, id, toggle.selected(&self.draft));
        }
    }

    fn activate_shell(&mut self, hwnd: HWND, element: ShellElement) {
        match element {
            ShellElement::WindowClose => unsafe {
                if windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(hwnd),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                )
                .is_err()
                {
                    crate::warn_!("failed to post Control Center close request");
                }
            },
            ShellElement::Nav(page) => self.activate_navigation(page),
            ShellElement::Search => self.focus.set_target(Some(ElementId::Search)),
            ShellElement::SearchResult(index) => self.activate_search_result(index),
            ShellElement::OnboardingContinue => {
                self.onboarding_step = Some(2);
                self.reset_scroll();
            }
            ShellElement::OnboardingOpen => {
                if crate::ui::first_run::mark_completed(&crate::config::data_dir()).is_err() {
                    crate::warn_!("could not persist onboarding completion marker");
                }
                self.onboarding_step = None;
                self.set_page(Page::Home);
            }
            ShellElement::Cancel => {
                self.replace_draft((*self.config_access.current()).clone());
                self.validation.clear();
                self.recording = None;
                self.stop_capture();
            }
            ShellElement::Save => {}
        }
    }

    fn activate_home(&mut self, hwnd: HWND, element: HomeElement) {
        match element {
            HomeElement::Speaker => {
                self.set_page(Page::Audio);
                self.focus.set_target(Some(ElementId::OutputDevice));
                post_main(crate::event::AppEvent::OpenSettingsPicker(PickerKind::OutputDevice));
            }
            HomeElement::Microphone => {
                self.set_page(Page::Audio);
                self.focus.set_target(Some(ElementId::InputDevice));
                post_main(crate::event::AppEvent::OpenSettingsPicker(PickerKind::InputDevice));
            }
            HomeElement::CurrentDesktop => self.set_page(Page::Workspaces),
            HomeElement::PreviousDesktop => {
                post_main(crate::event::AppEvent::SwitchPreviousDesktopFromUi);
            }
            HomeElement::Special => self.activate_special_workspace(hwnd),
            HomeElement::DisplayProfile => {
                self.set_page(Page::Displays);
                self.refresh_display_outputs();
            }
            HomeElement::ShortcutHealth => self.set_page(Page::Shortcuts),
            HomeElement::Diagnostics => post_main(crate::event::AppEvent::ShowDiagnostics),
        }
    }

    fn activate_audio(&mut self, hwnd: HWND, element: AudioElement) {
        match element {
            AudioElement::InputCycleMode(index) => {
                if let Some(mode) = super::audio_view::cycle_mode_choice(index) {
                    self.set_cycle_mode(hwnd, DeviceCycleFlow::Input, mode);
                }
            }
            AudioElement::OutputCycleMode(index) => {
                if let Some(mode) = super::audio_view::cycle_mode_choice(index) {
                    self.set_cycle_mode(hwnd, DeviceCycleFlow::Output, mode);
                }
            }
            AudioElement::InputCycleDevice(index) => {
                self.toggle_cycle_device(hwnd, DeviceCycleFlow::Input, index as usize);
            }
            AudioElement::OutputCycleDevice(index) => {
                self.toggle_cycle_device(hwnd, DeviceCycleFlow::Output, index as usize);
            }
            AudioElement::InputDevice => Self::request_picker(PickerKind::InputDevice),
            AudioElement::OutputDevice => Self::request_picker(PickerKind::OutputDevice),
            AudioElement::InputAllowlist => Self::request_picker(PickerKind::InputAllowlist),
            AudioElement::OutputAllowlist => Self::request_picker(PickerKind::OutputAllowlist),
            AudioElement::InputRole => Self::request_picker(PickerKind::InputRole),
            AudioElement::OutputRole => Self::request_picker(PickerKind::OutputRole),
        }
    }

    fn activate_display(&mut self, hwnd: HWND, element: DisplayElement) {
        match element {
            DisplayElement::ProfilesEnabled => {
                self.activate_config_toggle(hwnd, ElementId::DisplayProfilesEnabled)
            }
            DisplayElement::ProfileCard(index) => self.activate_display_profile_card(hwnd, index),
            DisplayElement::OutputCard(index) => self.toggle_display_output(index),
            DisplayElement::TopologyChoice(index) => self.set_display_topology(index),
            DisplayElement::WizardBack => self.move_display_editor(hwnd, false),
            DisplayElement::WizardNext => self.move_display_editor(hwnd, true),
            DisplayElement::WizardCancel => self.cancel_display_editor(),
            DisplayElement::WizardSummary => {}
            DisplayElement::Profile => Self::request_picker(PickerKind::DisplayProfile),
            DisplayElement::Outputs => Self::request_picker(PickerKind::DisplayOutputs),
            DisplayElement::Topology => Self::request_picker(PickerKind::DisplayTopology),
            DisplayElement::Route => Self::request_picker(PickerKind::DisplayRoute),
            DisplayElement::EditRoute => self.request_display_route_edit(),
            DisplayElement::NewProfile => self.start_display_editor(hwnd, false),
            DisplayElement::UpdateProfile => self.start_display_editor(hwnd, true),
            DisplayElement::EditProfile => self.start_existing_display_editor(),
            DisplayElement::RenameProfile => self.request_display_profile_rename(),
            DisplayElement::DuplicateProfile => self.activate_display_profile_duplicate(hwnd),
            DisplayElement::TestApply => {
                if let Some(profile) = self.draft.display_profiles.active().cloned() {
                    post_main(crate::event::AppEvent::TestApplyDisplayProfile { profile });
                }
            }
            DisplayElement::Apply => {
                if let Some(profile) = self.draft.display_profiles.active().cloned() {
                    post_main(crate::event::AppEvent::ApplyDisplayProfile { profile });
                }
            }
            DisplayElement::DeleteProfile => self.activate_display_profile_delete(hwnd),
            DisplayElement::KeepChange => post_main(crate::event::AppEvent::KeepDisplayProfile),
            DisplayElement::UndoChange => post_main(crate::event::AppEvent::RevertDisplayProfile),
            DisplayElement::DiscardEdits => {
                self.cancel_display_editor();
                self.validation.clear();
            }
        }
    }

    fn activate_shortcut(&mut self, hwnd: HWND, element: ShortcutElement) {
        match element {
            ShortcutElement::Card(_) => {}
            ShortcutElement::Enabled(slot) => self.toggle_hotkey_enabled(hwnd, slot),
            ShortcutElement::Unassign(slot) => self.unassign_hotkey(hwnd, slot),
            ShortcutElement::Capture(capture) => {
                self.recording = Some(capture.id());
                self.recording_modifiers = ModifierMask::NONE;
                self.validation.clear();
                crate::keyboard::hook::begin_capture();
                self.capture_armed = true;
                start_timer(hwnd);
            }
        }
    }

    fn activate_workspace(&mut self, hwnd: HWND, element: WorkspaceElement) {
        match element {
            WorkspaceElement::Enabled => self.activate_config_toggle(hwnd, ElementId::DesktopsEnabled),
            WorkspaceElement::WinNumberEnabled => {
                self.activate_config_toggle(hwnd, ElementId::WinNumberEnabled)
            }
            WorkspaceElement::DesktopNumberModifier => {
                Self::request_picker(PickerKind::DesktopNumberModifier)
            }
            WorkspaceElement::MoveDesktopModifier => {
                Self::request_picker(PickerKind::MoveDesktopModifier)
            }
            WorkspaceElement::SilentMoveDesktopModifier => {
                Self::request_picker(PickerKind::SilentMoveDesktopModifier)
            }
        }
    }

    fn activate_overlay(&mut self, hwnd: HWND, element: OverlayElement) {
        match element {
            OverlayElement::Enabled => self.activate_config_toggle(hwnd, ElementId::OverlayEnabled),
            OverlayElement::ExternalChanges => {
                self.activate_config_toggle(hwnd, ElementId::OverlayExternalChanges)
            }
            OverlayElement::PositionCell(index) => self.set_overlay_position(hwnd, index as usize),
            OverlayElement::Appearance => Self::request_picker(PickerKind::OverlayAppearance),
            OverlayElement::Position => Self::request_picker(PickerKind::OverlayPosition),
            OverlayElement::Monitor => Self::request_picker(PickerKind::OverlayMonitor),
            OverlayElement::Preview => post_main(crate::event::AppEvent::PreviewOverlay {
                config: self.draft.overlay.clone(),
            }),
            OverlayElement::Duration | OverlayElement::Opacity | OverlayElement::Scale => {}
        }
    }

    fn activate_system(&mut self, hwnd: HWND, element: SystemElement) {
        match element {
            SystemElement::StartWithWindows => self.toggle_startup_registration(hwnd),
            SystemElement::StartHotkeysEnabled => {
                self.activate_config_toggle(hwnd, ElementId::StartHotkeysEnabled)
            }
            SystemElement::DebugLogging => self.toggle_debug_logging(hwnd, ElementId::DebugLogging),
            SystemElement::DiagnosticsStatus => post_main(crate::event::AppEvent::ShowDiagnostics),
            SystemElement::OpenConfigFolder => open_config_folder(),
            SystemElement::ResetSettings => self.activate_reset_settings(hwnd),
        }
    }

    fn request_picker(kind: PickerKind) {
        post_main(crate::event::AppEvent::OpenSettingsPicker(kind));
    }
}
