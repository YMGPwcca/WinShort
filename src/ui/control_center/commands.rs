//! Activation routing for the Control Center.
//!
//! The entry point owns cross-cutting command bookkeeping. Domain helpers own
//! the side effects for their element families so one match no longer mixes
//! navigation, audio, display, shortcut and platform operations.

use super::config_toggle::ConfigToggle;
use super::native::{invalidate, open_config_folder, post_main, start_timer};
use super::state::SettingsUi;

use crate::audio::DeviceCycleFlow;
use crate::keyboard::binding::ModifierMask;
use crate::ui::control_center_automation::node_has_invoke;
use crate::ui::layout::ElementId;
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
        self.dispatch_activation(hwnd, id);

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

    fn dispatch_activation(&mut self, hwnd: HWND, id: ElementId) {
        match id {
            ElementId::DesktopsEnabled
            | ElementId::DisplayProfilesEnabled
            | ElementId::OverlayEnabled
            | ElementId::OverlayExternalChanges
            | ElementId::StartHotkeysEnabled
            | ElementId::WinNumberEnabled => self.activate_config_toggle(hwnd, id),

            ElementId::Search
            | ElementId::Nav(_)
            | ElementId::SearchResult(_)
            | ElementId::WindowClose
            | ElementId::OnboardingContinue
            | ElementId::OnboardingOpen
            | ElementId::Cancel
            | ElementId::Save => self.activate_shell(hwnd, id),

            ElementId::HomeSpeaker
            | ElementId::HomeCurrentDesktop
            | ElementId::HomeMicrophone
            | ElementId::HomePreviousDesktop
            | ElementId::HomeSpecial
            | ElementId::HomeDisplayProfile
            | ElementId::HomeShortcutHealth
            | ElementId::HomeDiagnostics => self.activate_home(hwnd, id),

            ElementId::InputCycleMode(_)
            | ElementId::OutputCycleMode(_)
            | ElementId::InputCycleDevice(_)
            | ElementId::OutputCycleDevice(_)
            | ElementId::InputDevice
            | ElementId::OutputDevice
            | ElementId::InputAllowlist
            | ElementId::OutputAllowlist
            | ElementId::InputRole
            | ElementId::OutputRole => self.activate_audio(hwnd, id),

            ElementId::DisplayProfileCard(_)
            | ElementId::DisplayOutputCard(_)
            | ElementId::DisplayTopologyChoice(_)
            | ElementId::DisplayWizardBack
            | ElementId::DisplayWizardNext
            | ElementId::DisplayWizardCancel
            | ElementId::DisplayWizardSummary
            | ElementId::EditDisplayProfile
            | ElementId::DisplayProfile
            | ElementId::DisplayOutputs
            | ElementId::DisplayTopology
            | ElementId::DisplayRoute
            | ElementId::EditDisplayRoute
            | ElementId::NewDisplayProfile
            | ElementId::UpdateDisplayProfile
            | ElementId::RenameDisplayProfile
            | ElementId::DuplicateDisplayProfile
            | ElementId::TestApplyDisplayProfile
            | ElementId::ApplyDisplayProfile
            | ElementId::DeleteDisplayProfile
            | ElementId::KeepDisplayChange
            | ElementId::UndoDisplayChange
            | ElementId::DiscardDisplayEdits => self.activate_display(hwnd, id),

            ElementId::HotkeyCard(_)
            | ElementId::HotkeyEnabled(_)
            | ElementId::HotkeyUnassign(_)
            | ElementId::MicHotkey
            | ElementId::OutputHotkey
            | ElementId::ForegroundHotkey
            | ElementId::CycleInputHotkey
            | ElementId::CycleOutputHotkey
            | ElementId::ForegroundVolumeUpHotkey
            | ElementId::ForegroundVolumeDownHotkey
            | ElementId::PreviousDesktopHotkey
            | ElementId::AssignScratchpadHotkey
            | ElementId::ToggleScratchpadHotkey
            | ElementId::DisplayProfileHotkey => self.activate_shortcut(hwnd, id),

            ElementId::DesktopNumberModifier
            | ElementId::MoveDesktopModifier
            | ElementId::SilentMoveDesktopModifier => self.activate_workspace(id),

            ElementId::OverlayPositionCell(_)
            | ElementId::OverlayAppearance
            | ElementId::OverlayPosition
            | ElementId::OverlayMonitor
            | ElementId::OverlayDuration
            | ElementId::OverlayOpacity
            | ElementId::OverlayScale
            | ElementId::OverlayPreview => self.activate_overlay(hwnd, id),

            ElementId::StartWithWindows
            | ElementId::DebugLogging
            | ElementId::DiagnosticsStatus
            | ElementId::OpenConfigFolder
            | ElementId::ResetSettings => self.activate_system(hwnd, id),
        }
    }

    fn activate_config_toggle(&mut self, hwnd: HWND, id: ElementId) {
        let Some(toggle) = ConfigToggle::from_element(id) else {
            return;
        };
        let before = self.draft.clone();
        toggle.toggle(&mut self.draft);
        if self.commit_local_change(hwnd, before) {
            self.animate_toggle(hwnd, id, toggle.selected(&self.draft));
        }
    }

    fn activate_shell(&mut self, hwnd: HWND, id: ElementId) {
        match id {
            ElementId::WindowClose => unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(hwnd),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            },
            ElementId::Nav(page) => self.activate_navigation(page),
            ElementId::Search => self.focus.set_target(Some(ElementId::Search)),
            ElementId::SearchResult(index) => self.activate_search_result(index),
            ElementId::OnboardingContinue => {
                self.onboarding_step = Some(2);
                self.reset_scroll();
            }
            ElementId::OnboardingOpen => {
                if crate::ui::first_run::mark_completed(&crate::config::data_dir()).is_err() {
                    crate::warn_!("could not persist onboarding completion marker");
                }
                self.onboarding_step = None;
                self.set_page(Page::Home);
            }
            ElementId::Cancel => {
                self.replace_draft((*self.config_access.current()).clone());
                self.validation.clear();
                self.recording = None;
                self.stop_capture();
            }
            ElementId::Save => {}
            _ => {}
        }
    }

    fn activate_home(&mut self, hwnd: HWND, id: ElementId) {
        match id {
            ElementId::HomeSpeaker => {
                self.set_page(Page::Audio);
                self.focus.set_target(Some(ElementId::OutputDevice));
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::OutputDevice,
                ));
            }
            ElementId::HomeMicrophone => {
                self.set_page(Page::Audio);
                self.focus.set_target(Some(ElementId::InputDevice));
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::InputDevice,
                ));
            }
            ElementId::HomeCurrentDesktop => self.set_page(Page::Workspaces),
            ElementId::HomePreviousDesktop => {
                post_main(crate::event::AppEvent::SwitchPreviousDesktopFromUi);
            }
            ElementId::HomeSpecial => self.activate_special_workspace(hwnd),
            ElementId::HomeDisplayProfile => {
                self.set_page(Page::Displays);
                self.refresh_display_outputs();
            }
            ElementId::HomeShortcutHealth => self.set_page(Page::Shortcuts),
            ElementId::HomeDiagnostics => post_main(crate::event::AppEvent::ShowDiagnostics),
            _ => {}
        }
    }

    fn activate_audio(&mut self, hwnd: HWND, id: ElementId) {
        match id {
            ElementId::InputCycleMode(index) => {
                if let Some(mode) = super::audio_view::cycle_mode_choice(index) {
                    self.set_cycle_mode(hwnd, DeviceCycleFlow::Input, mode);
                }
            }
            ElementId::OutputCycleMode(index) => {
                if let Some(mode) = super::audio_view::cycle_mode_choice(index) {
                    self.set_cycle_mode(hwnd, DeviceCycleFlow::Output, mode);
                }
            }
            ElementId::InputCycleDevice(index) => {
                self.toggle_cycle_device(hwnd, DeviceCycleFlow::Input, index as usize);
            }
            ElementId::OutputCycleDevice(index) => {
                self.toggle_cycle_device(hwnd, DeviceCycleFlow::Output, index as usize);
            }
            ElementId::InputDevice => Self::request_picker(PickerKind::InputDevice),
            ElementId::OutputDevice => Self::request_picker(PickerKind::OutputDevice),
            ElementId::InputAllowlist => Self::request_picker(PickerKind::InputAllowlist),
            ElementId::OutputAllowlist => Self::request_picker(PickerKind::OutputAllowlist),
            ElementId::InputRole => Self::request_picker(PickerKind::InputRole),
            ElementId::OutputRole => Self::request_picker(PickerKind::OutputRole),
            _ => {}
        }
    }

    fn activate_display(&mut self, hwnd: HWND, id: ElementId) {
        match id {
            ElementId::DisplayProfileCard(index) => self.activate_display_profile_card(hwnd, index),
            ElementId::DisplayOutputCard(index) => self.toggle_display_output(index),
            ElementId::DisplayTopologyChoice(index) => self.set_display_topology(index),
            ElementId::DisplayWizardBack => self.move_display_editor(hwnd, false),
            ElementId::DisplayWizardNext => self.move_display_editor(hwnd, true),
            ElementId::DisplayWizardCancel => self.cancel_display_editor(),
            ElementId::DisplayWizardSummary => {}
            ElementId::DisplayProfile => Self::request_picker(PickerKind::DisplayProfile),
            ElementId::DisplayOutputs => Self::request_picker(PickerKind::DisplayOutputs),
            ElementId::DisplayTopology => Self::request_picker(PickerKind::DisplayTopology),
            ElementId::DisplayRoute => Self::request_picker(PickerKind::DisplayRoute),
            ElementId::EditDisplayRoute => self.request_display_route_edit(),
            ElementId::NewDisplayProfile | ElementId::UpdateDisplayProfile => {
                self.start_display_editor(hwnd, id == ElementId::UpdateDisplayProfile);
            }
            ElementId::EditDisplayProfile => self.start_existing_display_editor(),
            ElementId::RenameDisplayProfile => self.request_display_profile_rename(),
            ElementId::DuplicateDisplayProfile => self.activate_display_profile_duplicate(hwnd),
            ElementId::TestApplyDisplayProfile => {
                if let Some(profile) = self.draft.display_profiles.active().cloned() {
                    post_main(crate::event::AppEvent::TestApplyDisplayProfile { profile });
                }
            }
            ElementId::ApplyDisplayProfile => {
                if let Some(profile) = self.draft.display_profiles.active().cloned() {
                    post_main(crate::event::AppEvent::ApplyDisplayProfile { profile });
                }
            }
            ElementId::DeleteDisplayProfile => self.activate_display_profile_delete(hwnd),
            ElementId::KeepDisplayChange => post_main(crate::event::AppEvent::KeepDisplayProfile),
            ElementId::UndoDisplayChange => post_main(crate::event::AppEvent::RevertDisplayProfile),
            ElementId::DiscardDisplayEdits => {
                self.cancel_display_editor();
                self.validation.clear();
            }
            _ => {}
        }
    }

    fn activate_shortcut(&mut self, hwnd: HWND, id: ElementId) {
        match id {
            ElementId::HotkeyCard(_) => {}
            ElementId::HotkeyEnabled(slot) => self.toggle_hotkey_enabled(hwnd, slot),
            ElementId::HotkeyUnassign(slot) => self.unassign_hotkey(hwnd, slot),
            ElementId::MicHotkey
            | ElementId::OutputHotkey
            | ElementId::ForegroundHotkey
            | ElementId::CycleInputHotkey
            | ElementId::CycleOutputHotkey
            | ElementId::ForegroundVolumeUpHotkey
            | ElementId::ForegroundVolumeDownHotkey
            | ElementId::PreviousDesktopHotkey
            | ElementId::AssignScratchpadHotkey
            | ElementId::ToggleScratchpadHotkey
            | ElementId::DisplayProfileHotkey => {
                self.recording = Some(id);
                self.recording_modifiers = ModifierMask::NONE;
                self.validation.clear();
                crate::keyboard::hook::begin_capture();
                self.capture_armed = true;
                start_timer(hwnd);
            }
            _ => {}
        }
    }

    fn activate_workspace(&mut self, id: ElementId) {
        let picker = match id {
            ElementId::DesktopNumberModifier => PickerKind::DesktopNumberModifier,
            ElementId::MoveDesktopModifier => PickerKind::MoveDesktopModifier,
            ElementId::SilentMoveDesktopModifier => PickerKind::SilentMoveDesktopModifier,
            _ => return,
        };
        Self::request_picker(picker);
    }

    fn activate_overlay(&mut self, hwnd: HWND, id: ElementId) {
        match id {
            ElementId::OverlayPositionCell(index) => {
                self.set_overlay_position(hwnd, index as usize);
            }
            ElementId::OverlayAppearance => Self::request_picker(PickerKind::OverlayAppearance),
            ElementId::OverlayPosition => Self::request_picker(PickerKind::OverlayPosition),
            ElementId::OverlayMonitor => Self::request_picker(PickerKind::OverlayMonitor),
            ElementId::OverlayPreview => post_main(crate::event::AppEvent::PreviewOverlay {
                config: self.draft.overlay.clone(),
            }),
            ElementId::OverlayDuration | ElementId::OverlayOpacity | ElementId::OverlayScale => {}
            _ => {}
        }
    }

    fn activate_system(&mut self, hwnd: HWND, id: ElementId) {
        match id {
            ElementId::StartWithWindows => self.toggle_startup_registration(hwnd),
            ElementId::DebugLogging => self.toggle_debug_logging(hwnd, id),
            ElementId::DiagnosticsStatus => post_main(crate::event::AppEvent::ShowDiagnostics),
            ElementId::OpenConfigFolder => open_config_folder(),
            ElementId::ResetSettings => self.activate_reset_settings(hwnd),
            _ => {}
        }
    }

    fn request_picker(kind: PickerKind) {
        post_main(crate::event::AppEvent::OpenSettingsPicker(kind));
    }
}
