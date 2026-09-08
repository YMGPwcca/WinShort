//! Main-thread application event routing by domain.

use super::App;
use crate::event::{
    AudioEventOrigin, AudioRuntimeEvent, ConfigEvent, ControlCenterEvent, DesktopEvent,
    DiagnosticsEvent, DisplayEvent, OverlayEvent,
};
use windows::Win32::Foundation::HWND;

impl App {
    pub(super) fn handle_control_center_event(&mut self, event: ControlCenterEvent) {
        match event {
            ControlCenterEvent::Show => self.show_settings(),
            ControlCenterEvent::OpenPicker(kind) => self.open_settings_picker(kind),
            ControlCenterEvent::FocusFromPicker { reverse } => {
                self.focus_settings_from_picker(reverse)
            }
            ControlCenterEvent::CommitPicker { commit } => self.commit_settings_picker(commit),
            ControlCenterEvent::CancelPicker {
                popup_hwnd,
                restore_focus,
            } => self.cancel_settings_picker(HWND(popup_hwnd as *mut _), restore_focus),
            ControlCenterEvent::WindowClosed => {
                self.close_settings_window();
                self.remember_settings_position();
            }
        }
    }

    pub(super) fn handle_display_event(&mut self, event: DisplayEvent) {
        match event {
            DisplayEvent::OpenRenamePrompt {
                profile_id,
                current_name,
            } => {
                if let Some(settings) = &mut self.settings {
                    if let Err(error) =
                        settings.open_display_profile_rename(profile_id, current_name)
                    {
                        crate::error_!("display profile rename prompt failed: {error}");
                    }
                }
            }
            DisplayEvent::RenameSubmitted { profile_id, name } => {
                if let Some(settings) = &mut self.settings {
                    settings.rename_display_profile(&profile_id, &name);
                }
            }
            DisplayEvent::RenameCancelled => {
                if let Some(settings) = &mut self.settings {
                    settings.cancel_display_profile_rename();
                }
            }
            DisplayEvent::OpenRouteEditPrompt {
                profile_id,
                route_index,
                initial,
            } => {
                if let Some(settings) = &mut self.settings {
                    if let Err(error) =
                        settings.open_display_route_edit(profile_id, route_index, initial)
                    {
                        crate::error_!("display route editor prompt failed: {error}");
                    }
                }
            }
            DisplayEvent::RouteEditSubmitted {
                profile_id,
                route_index,
                value,
            } => {
                if let Some(settings) = &mut self.settings {
                    settings.edit_display_route(&profile_id, route_index, &value);
                }
            }
            DisplayEvent::TestApply { profile } => self.apply_display_profile(profile, true),
            DisplayEvent::Apply { profile } => self.apply_display_profile(profile, false),
            DisplayEvent::Keep => self.keep_display_profile(),
            DisplayEvent::Revert => self.revert_display_profile(),
        }
    }

    pub(super) fn handle_diagnostics_event(&mut self, event: DiagnosticsEvent) {
        match event {
            DiagnosticsEvent::Show => self.show_diagnostics(),
            DiagnosticsEvent::RunSelfTest => self.run_diagnostics_self_test(),
            DiagnosticsEvent::Copy => self.copy_diagnostics(),
            DiagnosticsEvent::OpenLogs => self.open_diagnostics_logs(),
            DiagnosticsEvent::CreateSupportBundle => self.start_support_bundle(),
            DiagnosticsEvent::SupportBundleFinished { path, error } => {
                if let Some(join) = self.support_bundle.take() {
                    if join.join().is_err() {
                        crate::error_!("support bundle worker panicked before completion cleanup");
                    }
                }
                if let Some(window) = &mut self.diagnostics {
                    window.set_bundle_running(false);
                    let status = match (path, error) {
                        (Some(path), None) => format!("Support bundle created: {}", path.display()),
                        (_, Some(error)) => format!("Support bundle failed — {error}"),
                        _ => "Support bundle finished without a result".into(),
                    };
                    window.set_action_status(status);
                }
            }
        }
    }

    pub(super) fn handle_overlay_event(&mut self, event: OverlayEvent) {
        match event {
            OverlayEvent::ShowStatus => self.show_status_overlay(),
            OverlayEvent::Preview { config } => self.show_preview_overlay(config),
        }
    }

    pub(super) fn handle_config_event(&mut self, event: ConfigEvent) {
        match event {
            ConfigEvent::Applied { seq, stamp } => {
                let config = crate::app::config();
                self.set_suspended(!config.general.start_hotkeys_enabled);
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ConfigChanged { stamp });
                }
                if let Some(desktop) = &self.desktop {
                    desktop.configure_scratchpad(
                        config.virtual_desktops.enabled
                            && (config.virtual_desktops.scratchpad_assign.is_some()
                                || config.virtual_desktops.scratchpad_toggle.is_some()),
                    );
                }
                crate::info!("config applied (seq {seq}, origin {:?})", stamp.origin);
                self.refresh_settings_runtime();
            }
        }
    }

    pub(super) fn handle_desktop_event(&mut self, event: DesktopEvent) {
        match event {
            DesktopEvent::SwitchPreviousFromUi => {
                if let Some(desktop) = &self.desktop {
                    desktop.switch_previous();
                } else {
                    self.show_overlay_model(crate::ui::overlay::OverlayModel::single(
                        crate::ui::overlay::OverlayRow {
                            icon: crate::ui::overlay::OverlayIcon::Info,
                            tone: crate::ui::overlay::OverlayTone::Unavailable,
                            title: "Previous desktop unavailable".into(),
                            detail: "Workspace service is not available right now".into(),
                        },
                    ));
                }
            }
            DesktopEvent::ToggleSpecialFromUi => {
                if let Some(desktop) = &self.desktop {
                    desktop.toggle_scratchpad();
                } else {
                    self.show_overlay_model(crate::ui::overlay::OverlayModel::single(
                        crate::ui::overlay::OverlayRow {
                            icon: crate::ui::overlay::OverlayIcon::Info,
                            tone: crate::ui::overlay::OverlayTone::Unavailable,
                            title: "Special Desktop unavailable".into(),
                            detail: "Workspace service is not available right now".into(),
                        },
                    ));
                }
            }
            DesktopEvent::ForegroundWindowChanged { hwnd_raw } => {
                if let Some(desktop) = &self.desktop {
                    desktop.foreground_changed(hwnd_raw);
                }
            }
            DesktopEvent::ActionCompleted { kind } => {
                let (title, detail) = match kind {
                    crate::event::DesktopActionKind::Switched => {
                        ("Desktop changed", "Switched to the selected desktop")
                    }
                    crate::event::DesktopActionKind::MovedAndFollowed => {
                        ("Window moved", "Moved to the selected desktop")
                    }
                    crate::event::DesktopActionKind::MovedSilently => {
                        ("Window moved", "Moved without changing desktops")
                    }
                    crate::event::DesktopActionKind::Previous => {
                        ("Previous desktop", "Returned to the last normal desktop")
                    }
                    crate::event::DesktopActionKind::SentToSpecial => {
                        ("Special Desktop", "Window moved")
                    }
                    crate::event::DesktopActionKind::EnteredSpecial => {
                        ("Special Desktop", "Desktop opened")
                    }
                    crate::event::DesktopActionKind::LeftSpecial => {
                        ("Special Desktop", "Returned to the previous desktop")
                    }
                };
                self.show_overlay_model(crate::ui::overlay::OverlayModel::single(
                    crate::ui::overlay::OverlayRow {
                        icon: crate::ui::overlay::OverlayIcon::Workspace,
                        tone: crate::ui::overlay::OverlayTone::Changed,
                        title: title.into(),
                        detail: detail.into(),
                    },
                ));
                self.refresh_settings_runtime();
            }
            DesktopEvent::ActionFailed { action, reason } => {
                crate::error_!("desktop action {action} failed: {reason}");
                self.show_overlay_model(crate::ui::overlay::OverlayModel::single(
                    crate::ui::overlay::OverlayRow {
                        icon: crate::ui::overlay::OverlayIcon::Info,
                        tone: crate::ui::overlay::OverlayTone::Unavailable,
                        title: "Couldn't change workspace".into(),
                        detail: "Try again or open Diagnostics for help".into(),
                    },
                ));
            }
            DesktopEvent::BackendChanged(status) => {
                self.desktop_status = status;
                self.refresh_settings_runtime();
            }
        }
    }

    pub(super) fn handle_audio_event(&mut self, event: AudioRuntimeEvent) {
        match event {
            AudioRuntimeEvent::DeviceCycleResolved { request_id, result } => {
                self.handle_device_cycle_result(request_id, result)
            }
            AudioRuntimeEvent::MicrophoneStateChanged { state } => {
                self.microphone_state = state;
                self.microphone_seen = true;
                self.refresh_settings_runtime();
            }
            AudioRuntimeEvent::OutputStateChanged { state } => {
                self.output_state = state;
                self.output_seen = true;
                self.refresh_settings_runtime();
            }
            AudioRuntimeEvent::DefaultOutputChanged(_device) => {}
            AudioRuntimeEvent::DevicesChanged => {
                let devices = self.audio_devices();
                if let Some(settings) = &mut self.settings {
                    settings.refresh_devices(devices);
                }
                self.refresh_settings_runtime();
            }
            AudioRuntimeEvent::ForegroundAudioChanged { state, origin } => {
                let changed = self.foreground_state != state;
                let is_status_request = matches!(origin, AudioEventOrigin::StatusRequest(_));
                let status_request_matches = match origin {
                    AudioEventOrigin::StatusRequest(request_id) => {
                        self.status_request_id == Some(request_id)
                    }
                    _ => false,
                };
                if is_status_request && !status_request_matches {
                    crate::log_debug!("dropping stale foreground status result");
                    return;
                }
                let should_show = Self::should_show_audio_overlay(
                    origin,
                    self.foreground_seen,
                    changed,
                    crate::app::config().overlay.show_external_audio_changes,
                    status_request_matches,
                );
                self.foreground_state = state;
                self.foreground_seen = true;
                self.status_request_id = None;
                if should_show {
                    if is_status_request {
                        self.show_overlay_model(self.status_overlay_model());
                    } else {
                        let row = crate::ui::overlay::application_row(&self.foreground_state);
                        self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));
                    }
                }
                self.refresh_settings_runtime();
            }
            AudioRuntimeEvent::ForegroundVolumeChanged { state, origin } => {
                if matches!(origin, AudioEventOrigin::WinShortAction(_)) {
                    self.show_overlay_model(crate::ui::overlay::OverlayModel::single(
                        crate::ui::overlay::application_volume_row(&state),
                    ));
                }
                self.refresh_settings_runtime();
            }
        }
    }
}
