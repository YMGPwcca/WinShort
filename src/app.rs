//! Application runtime: hidden main window, tray integration, event routing,
//! startup/shutdown orchestration (spec §5–§7, §46–§47).

use std::sync::atomic::{AtomicU64, Ordering};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterWindowMessageW, HWND_MESSAGE,
    WINDOW_STYLE, WM_CLOSE, WM_DESTROY, WM_TIMER,
};

use crate::error::{Error, Result};
use crate::event::{self, AppEvent, AudioEventOrigin, HotkeyAction, WM_APP_EVENT};
use crate::platform::window as win;
use crate::tray::{menu as tray_menu, Tray, TrayEvent, TrayState};
use crate::ui::settings::SettingsWindow;

pub static CONFIG: std::sync::OnceLock<std::sync::Arc<crate::config::ConfigHandle>> =
    std::sync::OnceLock::new();

static CONFIG_SEQ: AtomicU64 = AtomicU64::new(1);

/// Access the live config snapshot from any thread.
pub fn config() -> std::sync::Arc<crate::config::Config> {
    CONFIG
        .get()
        .map(|h| h.get())
        .unwrap_or_else(|| std::sync::Arc::new(crate::config::Config::default()))
}

/// Commit one complete typed configuration on the main thread.
///
/// Persistence precedes publication: a failed or read-only save leaves the
/// live [`ConfigHandle`] unchanged. Successful commits publish exactly one
/// `ConfigApplied` event, which drives dependent worker refreshes.
pub(crate) fn commit_config(
    candidate: crate::config::Config,
    origin: crate::event::ConfigCommitOrigin,
) -> Result<()> {
    let violations = crate::config::validate(&candidate);
    if !violations.is_empty() {
        let message = violations
            .iter()
            .map(|violation| format!("{}: {}", violation.field, violation.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(Error::config(format!("configuration invalid: {message}")));
    }
    let handle = CONFIG
        .get()
        .ok_or_else(|| Error::config("configuration handle unavailable"))?;
    let stamp = handle.replace_after_save(candidate, origin, |config| {
        crate::config::save::save(&crate::config::data_dir(), config)
    })?;
    let seq = CONFIG_SEQ.fetch_add(1, Ordering::Relaxed);
    event::post_main(AppEvent::ConfigApplied { seq, stamp });
    Ok(())
}

pub const CLASS_NAME: &str = "WinShort.Main";

/// Registered message broadcast when Explorer restarts (spec §6).
pub fn taskbar_created_msg() -> u32 {
    static MSG: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *MSG.get_or_init(|| unsafe {
        RegisterWindowMessageW(PCWSTR(HSTRING::from("TaskbarCreated").as_ptr()))
    })
}

pub struct App {
    pub hwnd: HWND,
    tray: Option<Tray>,
    settings: Option<SettingsWindow>,
    diagnostics: Option<crate::ui::diagnostics::DiagnosticsWindow>,
    overlay: Option<crate::ui::overlay::OverlayWindow>,
    foreground: Option<crate::platform::foreground::ForegroundTracker>,
    keyboard: Option<crate::keyboard::hook::KeyboardService>,
    audio: Option<crate::audio::AudioService>,
    desktop: Option<crate::desktop::DesktopService>,
    desktop_status: crate::desktop::BackendStatus,
    microphone_state: crate::audio::AudioState,
    output_state: crate::audio::OutputState,
    foreground_state: crate::audio::AppAudioState,
    microphone_seen: bool,
    output_seen: bool,
    foreground_seen: bool,
    next_audio_request_id: u64,
    status_request_id: Option<u64>,
    suspended: bool,
    shutting_down: bool,
    support_bundle: Option<std::thread::JoinHandle<()>>,
    /// Per-subsystem startup failures (degraded startup, #27): the app stays
    /// up with tray/Settings and surfaces why a subsystem is dark.
    degraded: Vec<(&'static str, String)>,
}

thread_local! {
    /// Process-singleton [`App`]. Owned by the main thread; the WndProc and
    /// main-thread callbacks borrow it here. Reentrant Win32 dispatch while a
    /// borrow is live panics on the `RefCell` (loud failure) instead of
    /// aliasing mutable state.
    static APP: std::cell::RefCell<Option<App>> = const { std::cell::RefCell::new(None) };
}

static MAIN_HWND: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

/// Run `f` with mutable access to the app singleton (main thread only).
/// Returns `None` before window creation and after teardown.
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with_borrow_mut(|slot| slot.as_mut().map(f))
}

/// Store the singleton exactly once; called by [`App::create_main_window`].
pub fn set_app(app: App) {
    APP.with_borrow_mut(|slot| {
        debug_assert!(slot.is_none(), "app singleton installed twice");
        *slot = Some(app);
    });
}

/// The main window handle, valid from creation until process exit.
pub fn main_hwnd() -> Option<HWND> {
    MAIN_HWND.get().map(|h| HWND(*h as *mut _))
}

impl App {
    /// Create the hidden message window and install the App singleton.
    pub fn create_main_window() -> Result<()> {
        win::register_class(CLASS_NAME, Some(main_wndproc))?;

        // SAFETY: plain message-only window; no state passed through NCCREATE.
        let hwnd = unsafe {
            CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE::default(),
                PCWSTR(HSTRING::from(CLASS_NAME).as_ptr()),
                PCWSTR(HSTRING::from("WinShort").as_ptr()),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .map_err(|e| Error::win("CreateWindowExW(main)", &e))?;
        // Session lock/unlock resets (#13). Failure is non-fatal: hook install
        // and suspend toggle still reset engine state.
        if unsafe {
            windows::Win32::System::RemoteDesktop::WTSRegisterSessionNotification(
                hwnd,
                windows::Win32::System::RemoteDesktop::NOTIFY_FOR_THIS_SESSION,
            )
        }
        .is_err()
        {
            crate::warn_!("WTSRegisterSessionNotification failed; lock/unlock resets disabled");
        }
        let _ = MAIN_HWND.set(hwnd.0 as isize);
        set_app(App {
            hwnd,
            tray: None,
            settings: None,
            diagnostics: None,
            overlay: None,
            foreground: None,
            keyboard: None,
            audio: None,
            desktop: None,
            desktop_status: crate::desktop::BackendStatus {
                native: crate::desktop::BackendAvailability::Failed {
                    reason: "Detecting…".into(),
                },
                fallback: crate::desktop::BackendAvailability::Available,
                active: crate::desktop::BackendKind::KeyboardFallback,
                desktop_count: None,
                last_served: None,
            },
            microphone_state: crate::audio::AudioState::Unavailable {
                reason: "Starting audio…".into(),
            },
            output_state: crate::audio::OutputState::Unavailable {
                reason: "Starting audio…".into(),
            },
            foreground_state: crate::audio::AppAudioState::no_external(),
            microphone_seen: false,
            output_seen: false,
            foreground_seen: false,
            next_audio_request_id: 0,
            status_request_id: None,
            suspended: false,
            shutting_down: false,
            support_bundle: None,
            degraded: Vec::new(),
        });
        Ok(())
    }

    /// Install the tray icon (after the main window exists).
    pub fn install_tray(&mut self) -> Result<()> {
        let size = small_icon_size();
        let icons = crate::tray::icon::create_icons(size)?;
        let tray = Tray::install(self.hwnd, icons)?;
        self.tray = Some(tray);
        info!("tray icon installed ({size}px)");
        Ok(())
    }

    pub fn install_overlay(&mut self) -> Result<()> {
        self.overlay = Some(crate::ui::overlay::OverlayWindow::create()?);
        Ok(())
    }
    pub fn install_foreground_tracker(&mut self) -> Result<()> {
        self.foreground = Some(crate::platform::foreground::ForegroundTracker::install()?);
        Ok(())
    }

    pub fn install_keyboard(
        &mut self,
        config: std::sync::Arc<crate::config::ConfigHandle>,
    ) -> Result<()> {
        let service = crate::keyboard::hook::KeyboardService::start(self.hwnd, config)?;
        let suspended = !crate::app::config().general.start_hotkeys_enabled;
        service.set_suspended(suspended);
        self.suspended = suspended;
        if let Some(tray) = &mut self.tray {
            let state = if suspended {
                TrayState::HotkeysSuspended
            } else {
                TrayState::Normal
            };
            tray.set_state(state)?;
        }
        self.keyboard = Some(service);
        Ok(())
    }

    pub fn install_audio(
        &mut self,
        config: std::sync::Arc<crate::config::ConfigHandle>,
    ) -> Result<()> {
        self.audio = Some(crate::audio::AudioService::start(self.hwnd, config)?);
        Ok(())
    }

    pub fn install_desktop(&mut self) -> Result<()> {
        self.desktop = Some(crate::desktop::DesktopService::start(self.hwnd)?);
        Ok(())
    }

    pub fn audio_devices(&self) -> crate::audio::devices::DeviceLists {
        self.audio
            .as_ref()
            .map(crate::audio::AudioService::devices)
            .unwrap_or_default()
    }

    /// Record a subsystem startup failure and keep going (#27).
    pub fn degrade(&mut self, name: &'static str, error: &crate::error::Error) {
        crate::warn_!("subsystem {name} unavailable: {error}");
        self.degraded.push((name, error.to_string()));
    }

    pub fn degraded_reason(&self, name: &str) -> Option<String> {
        self.degraded
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, r)| r.clone())
    }
    fn next_audio_request_id(&mut self) -> u64 {
        self.next_audio_request_id = self.next_audio_request_id.wrapping_add(1);
        if self.next_audio_request_id == 0 {
            self.next_audio_request_id = 1;
        }
        self.next_audio_request_id
    }

    fn should_show_audio_overlay(
        origin: AudioEventOrigin,
        seen: bool,
        changed: bool,
        show_external: bool,
        status_request_matches: bool,
    ) -> bool {
        match origin {
            AudioEventOrigin::Initial | AudioEventOrigin::Config(_) => false,
            AudioEventOrigin::External => show_external && seen && changed,
            AudioEventOrigin::WinShortAction(_) => true,
            AudioEventOrigin::StatusRequest(_) => status_request_matches,
        }
    }

    /// Clear keyboard chord state (lock/unlock, sleep/resume, suspend
    /// toggle). Keys physically released while the app could not see them
    /// must not stay "held" (#13).
    pub fn reset_keyboard_state(&mut self) {
        if let Some(keyboard) = &self.keyboard {
            keyboard.reset_state();
        }
        crate::info!("keyboard engine state reset (lifecycle transition)");
    }

    fn ensure_settings(
        &mut self,
        devices: crate::audio::devices::DeviceLists,
    ) -> Result<&mut SettingsWindow> {
        if self.settings.is_none() {
            self.settings = Some(SettingsWindow::create(devices)?);
            info!("settings window created");
        }
        Ok(self.settings.as_mut().expect("just created"))
    }

    pub fn show_settings(&mut self) {
        let devices = self.audio_devices();
        match self.ensure_settings(devices.clone()) {
            Ok(settings) => {
                settings.refresh_devices(devices);
                if let Err(error) = settings.show() {
                    error_!("show settings failed: {error}");
                }
            }
            Err(error) => error_!("create settings failed: {error}"),
        }
    }

    fn ensure_diagnostics(&mut self) -> Result<&mut crate::ui::diagnostics::DiagnosticsWindow> {
        if self.diagnostics.is_none() {
            let snapshot = self.diagnostics_snapshot();
            self.diagnostics = Some(crate::ui::diagnostics::DiagnosticsWindow::create(snapshot)?);
            info!("diagnostics window created");
        }
        Ok(self.diagnostics.as_mut().expect("just created"))
    }

    pub fn show_diagnostics(&mut self) {
        let snapshot = self.diagnostics_snapshot();
        match self.ensure_diagnostics() {
            Ok(window) => {
                window.set_snapshot(snapshot, None);
                if let Err(error) = window.show() {
                    error_!("show diagnostics failed: {error}");
                }
            }
            Err(error) => error_!("create diagnostics failed: {error}"),
        }
    }
    fn open_settings_picker(&mut self, kind: crate::ui::picker::PickerKind) {
        let devices = self.audio_devices();
        let monitors = crate::platform::monitor::all();
        if let Some(settings) = &mut self.settings {
            if let Err(error) = settings.open_picker(kind, devices, monitors) {
                error_!("open settings picker failed: {error}");
            }
        }
    }

    pub(crate) fn commit_settings_picker(
        &mut self,
        kind: crate::ui::picker::PickerKind,
        value: crate::ui::picker::PickerValue,
    ) {
        if let Some(settings) = &mut self.settings {
            settings.commit_picker(kind, value);
        }
    }

    pub(crate) fn cancel_settings_picker(
        &mut self,
        popup_hwnd: windows::Win32::Foundation::HWND,
        restore_focus: bool,
    ) {
        if let Some(settings) = &mut self.settings {
            if settings.picker_hwnd() == Some(popup_hwnd) {
                if restore_focus {
                    settings.cancel_picker();
                } else {
                    settings.cancel_picker_without_focus();
                }
            }
        }
    }

    pub(crate) fn close_settings_window(&mut self) {
        if let Some(settings) = &mut self.settings {
            settings.close_for_hide();
        }
    }
    pub(crate) fn remember_settings_position(&mut self) {
        if let Some(settings) = &mut self.settings {
            settings.persist_position();
        }
    }
    pub(crate) fn focus_settings_from_picker(&mut self, reverse: bool) {
        if let Some(settings) = &mut self.settings {
            settings.focus_next_from_picker(reverse);
        }
    }

    fn copy_diagnostics(&mut self) {
        let snapshot = self.diagnostics_snapshot();
        let owner = self.diagnostics.as_ref().map(|window| window.hwnd);
        let status = match owner {
            Some(owner) => match crate::diagnostics::support::copy_diagnostics(owner, &snapshot) {
                Ok(()) => "Diagnostics copied to the Unicode clipboard".into(),
                Err(error) => format!("Copy failed — {error}"),
            },
            None => "Copy failed — Diagnostics window is not available".into(),
        };
        if let Some(window) = &mut self.diagnostics {
            window.set_action_status(status);
        }
    }

    fn open_diagnostics_logs(&mut self) {
        crate::diagnostics::logging::flush();
        let snapshot = self.diagnostics_snapshot();
        let status =
            match crate::diagnostics::support::open_logs(snapshot.logging.directory.as_deref()) {
                Ok(()) => "Opened the WinShort log directory".into(),
                Err(error) => format!("Open Logs failed — {error}"),
            };
        if let Some(window) = &mut self.diagnostics {
            window.set_action_status(status);
        }
    }

    fn run_diagnostics_self_test(&mut self) {
        let snapshot = self.diagnostics_snapshot();
        let report = crate::diagnostics::snapshot::run_self_test(&snapshot);
        let status = report.summary();
        if let Some(window) = &mut self.diagnostics {
            window.set_snapshot(snapshot, Some(report));
            window.set_action_status(status);
        }
    }

    fn start_support_bundle(&mut self) {
        if self.support_bundle.is_some() {
            if let Some(window) = &mut self.diagnostics {
                window.set_action_status("Support bundle is already being created".into());
            }
            return;
        }
        crate::diagnostics::logging::flush();
        let snapshot = self.diagnostics_snapshot();
        if let Some(window) = &mut self.diagnostics {
            window.set_bundle_running(true);
            window.set_action_status(
                "Creating a sanitized bundle (newest 3 logs, bounded to 2 MiB)…".into(),
            );
        }
        let hwnd_raw = self.hwnd.0 as isize;
        match std::thread::Builder::new()
            .name("winshort-support-bundle".into())
            .spawn(move || {
                let result = crate::diagnostics::support::create_support_bundle(&snapshot);
                let event = match result {
                    Ok(path) => crate::event::AppEvent::SupportBundleFinished {
                        path: Some(path),
                        error: None,
                    },
                    Err(error) => crate::event::AppEvent::SupportBundleFinished {
                        path: None,
                        error: Some(error.to_string()),
                    },
                };
                let hwnd = windows::Win32::Foundation::HWND(hwnd_raw as *mut _);
                unsafe {
                    let _ = crate::event::post_event(hwnd, event);
                }
            }) {
            Ok(join) => self.support_bundle = Some(join),
            Err(error) => {
                if let Some(window) = &mut self.diagnostics {
                    window.set_bundle_running(false);
                    window.set_action_status(format!("Support bundle failed to start — {error}"));
                }
            }
        }
    }

    pub fn toggle_suspended(&mut self) {
        self.suspended = !self.suspended;
        if let Some(keyboard) = &self.keyboard {
            keyboard.set_suspended(self.suspended);
            keyboard.reset_state();
        }
        let state = if self.suspended {
            TrayState::HotkeysSuspended
        } else {
            TrayState::Normal
        };
        if let Some(t) = &mut self.tray {
            if let Err(e) = t.set_state(state) {
                error_!("tray state update failed: {e}");
            }
        }
        info!(
            "hotkeys {}",
            if self.suspended {
                "suspended"
            } else {
                "resumed"
            }
        );
    }

    fn set_suspended(&mut self, suspended: bool) {
        self.suspended = suspended;
        if let Some(keyboard) = &self.keyboard {
            keyboard.set_suspended(suspended);
        }
        if let Some(tray) = &mut self.tray {
            let state = if suspended {
                TrayState::HotkeysSuspended
            } else {
                TrayState::Normal
            };
            if let Err(e) = tray.set_state(state) {
                error_!("tray state update failed: {e}");
            }
        }
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// Route a hotkey action recognized by the keyboard engine.
    pub fn dispatch_action(&mut self, action: HotkeyAction) {
        // Fail-closed during teardown (#43): workers may still emit stragglers
        // while they wind down; none may resurrect user-facing state.
        if self.shutting_down {
            return;
        }
        if self.suspended {
            return;
        }
        match action {
            HotkeyAction::ToggleMicrophone => {
                let request_id = self.next_audio_request_id();
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ToggleMicrophone(request_id));
                } else {
                    // Degraded startup (#27): tell the user why nothing happened.
                    let reason = self
                        .degraded_reason("audio")
                        .unwrap_or_else(|| "audio subsystem unavailable".into());
                    self.route_event(AppEvent::MicrophoneStateChanged {
                        state: crate::audio::AudioState::Unavailable { reason },
                        origin: AudioEventOrigin::WinShortAction(request_id),
                    });
                }
            }
            HotkeyAction::ToggleOutput => {
                let request_id = self.next_audio_request_id();
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ToggleOutput(request_id));
                } else {
                    let reason = self
                        .degraded_reason("audio")
                        .unwrap_or_else(|| "audio subsystem unavailable".into());
                    self.route_event(AppEvent::OutputStateChanged {
                        state: crate::audio::OutputState::Unavailable { reason },
                        origin: AudioEventOrigin::WinShortAction(request_id),
                    });
                }
            }
            HotkeyAction::ToggleForegroundAppAudio => {
                let request_id = self.next_audio_request_id();
                let pid = self
                    .foreground
                    .as_ref()
                    .and_then(|tracker| tracker.target_pid());
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ToggleForeground { pid, request_id });
                } else {
                    let reason = self
                        .degraded_reason("audio")
                        .unwrap_or_else(|| "audio subsystem unavailable".into());
                    self.route_event(AppEvent::ForegroundAudioChanged {
                        state: crate::audio::AppAudioState {
                            app_name: None,
                            aggregate: crate::audio::Aggregate::Error,
                            sessions: 0,
                            error: Some(reason),
                        },
                        origin: AudioEventOrigin::WinShortAction(request_id),
                    });
                }
            }
            HotkeyAction::CycleInputDevice => {
                self.dispatch_device_cycle(crate::audio::DeviceCycleFlow::Input);
            }
            HotkeyAction::CycleOutputDevice => {
                self.dispatch_device_cycle(crate::audio::DeviceCycleFlow::Output);
            }
            HotkeyAction::ForegroundVolumeUp => {
                self.dispatch_foreground_volume(crate::audio::sessions::VolumeAdjustment::Up);
            }
            HotkeyAction::ForegroundVolumeDown => {
                self.dispatch_foreground_volume(crate::audio::sessions::VolumeAdjustment::Down);
            }
            HotkeyAction::SwitchDesktop(n) => {
                if let Some(desktop) = &self.desktop {
                    desktop.switch_to(n as usize);
                }
            }
        }
    }

    fn dispatch_device_cycle(&mut self, flow: crate::audio::DeviceCycleFlow) {
        let request_id = self.next_audio_request_id();
        if let Some(audio) = &self.audio {
            audio.send(crate::audio::AudioCommand::CycleDevice { flow, request_id });
            return;
        }
        let error = self
            .degraded_reason("audio")
            .unwrap_or_else(|| "audio subsystem unavailable".into());
        self.handle_device_cycle_result(
            request_id,
            crate::audio::DeviceCycleResult::Failed {
                flow,
                previous: None,
                target: None,
                error,
            },
        );
    }

    fn handle_device_cycle_result(
        &mut self,
        request_id: u64,
        result: crate::audio::DeviceCycleResult,
    ) {
        match result {
            crate::audio::DeviceCycleResult::Changed { flow, device, .. } => {
                self.show_overlay_model(crate::ui::overlay::OverlayModel::single(
                    crate::ui::overlay::device_cycle_row(flow, &device),
                ));
                crate::log_debug!("device cycle request {request_id} applied for {flow:?}");
            }
            crate::audio::DeviceCycleResult::NoDevices { flow, .. } => {
                self.show_overlay_model(crate::ui::overlay::OverlayModel::single(
                    crate::ui::overlay::device_cycle_no_devices_row(flow),
                ));
            }
            crate::audio::DeviceCycleResult::Failed { flow, error, .. } => {
                crate::warn_!("device cycle request {request_id} failed: {error}");
                self.show_overlay_model(crate::ui::overlay::OverlayModel::single(
                    crate::ui::overlay::device_cycle_error_row(flow, &error),
                ));
            }
        }
    }

    fn dispatch_foreground_volume(&mut self, adjustment: crate::audio::sessions::VolumeAdjustment) {
        let request_id = self.next_audio_request_id();
        let pid = self
            .foreground
            .as_ref()
            .and_then(|tracker| tracker.target_pid());
        if let Some(audio) = &self.audio {
            audio.send(crate::audio::AudioCommand::AdjustForegroundVolume {
                pid,
                adjustment,
                request_id,
            });
        } else {
            let reason = self
                .degraded_reason("audio")
                .unwrap_or_else(|| "audio subsystem unavailable".into());
            self.route_event(AppEvent::ForegroundVolumeChanged {
                state: crate::audio::AppVolumeState::error(None, reason),
                origin: AudioEventOrigin::WinShortAction(request_id),
            });
        }
    }
    fn show_overlay_model_with_config(
        &mut self,
        model: crate::ui::overlay::OverlayModel,
        config: crate::config::model::OverlayCfg,
    ) {
        if !config.enabled {
            return;
        }
        if let Some(overlay) = &self.overlay {
            if let Err(error) = overlay.show(model, config) {
                error_!("overlay show failed: {error}");
            }
        }
    }

    fn show_overlay_model(&mut self, model: crate::ui::overlay::OverlayModel) {
        let config = crate::app::config();
        self.show_overlay_model_with_config(model, config.overlay.clone());
    }
    fn show_microphone_overlay(&mut self) {
        let row = crate::ui::overlay::microphone_row(&self.microphone_state);
        self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));
    }

    fn show_output_overlay(&mut self) {
        let row = crate::ui::overlay::output_row(&self.output_state);
        self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));
    }

    fn show_preview_overlay(&mut self, config: crate::config::model::OverlayCfg) {
        let state = crate::audio::AudioState::Active { volume_pct: 50 };
        let model =
            crate::ui::overlay::OverlayModel::single(crate::ui::overlay::microphone_row(&state));
        self.show_overlay_model_with_config(model, config);
    }

    fn status_overlay_model(&self) -> crate::ui::overlay::OverlayModel {
        let mut rows = vec![
            crate::ui::overlay::microphone_row(&self.microphone_state),
            crate::ui::overlay::output_row(&self.output_state),
        ];
        if self.foreground_state.aggregate != crate::audio::Aggregate::NoExternalApp {
            rows.push(crate::ui::overlay::application_row(&self.foreground_state));
        }
        crate::ui::overlay::OverlayModel { rows }
    }
    fn show_status_overlay(&mut self) {
        let request_id = self.next_audio_request_id();
        let pid = self
            .foreground
            .as_ref()
            .and_then(|tracker| tracker.target_pid());
        if self.audio.is_some() {
            self.status_request_id = Some(request_id);
            if let Some(audio) = &self.audio {
                audio.send(crate::audio::AudioCommand::QueryForeground { pid, request_id });
            }
        } else {
            self.status_request_id = None;
        }
        // Cached rows show immediately; a matching delayed query result
        // refreshes this same multi-row presentation (#18, #72).
        self.show_overlay_model(self.status_overlay_model());
    }
    /// Route an event posted from any thread.
    pub fn route_event(&mut self, ev: AppEvent) {
        // Fail-closed during teardown (#43): late ShowSettings/overlay/config
        // events must not create or resurrect user-facing state.
        if self.shutting_down {
            crate::log_debug!("dropping {:?} during shutdown", std::mem::discriminant(&ev));
            return;
        }
        match ev {
            AppEvent::ShowSettings => self.show_settings(),
            AppEvent::ShowDiagnostics => self.show_diagnostics(),
            AppEvent::OpenSettingsPicker(kind) => self.open_settings_picker(kind),
            AppEvent::ShowStatusOverlay => self.show_status_overlay(),
            AppEvent::PreviewOverlay { config } => self.show_preview_overlay(config),
            AppEvent::FocusSettingsFromPicker { reverse } => {
                self.focus_settings_from_picker(reverse);
            }
            AppEvent::CommitSettingsPicker { kind, value } => {
                self.commit_settings_picker(kind, value);
            }
            AppEvent::CancelSettingsPicker {
                popup_hwnd,
                restore_focus,
            } => {
                self.cancel_settings_picker(HWND(popup_hwnd as *mut _), restore_focus);
            }
            AppEvent::SettingsWindowClosed => {
                self.close_settings_window();
                self.remember_settings_position();
            }
            AppEvent::RunDiagnosticsSelfTest => self.run_diagnostics_self_test(),
            AppEvent::CopyDiagnostics => self.copy_diagnostics(),
            AppEvent::OpenDiagnosticsLogs => self.open_diagnostics_logs(),
            AppEvent::CreateSupportBundle => self.start_support_bundle(),
            AppEvent::SupportBundleFinished { path, error } => {
                if let Some(join) = self.support_bundle.take() {
                    let _ = join.join();
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
            AppEvent::ConfigApplied { seq, stamp } => {
                let config = crate::app::config();
                self.set_suspended(!config.general.start_hotkeys_enabled);
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ConfigChanged { stamp });
                }
                info!("config applied (seq {seq}, origin {:?})", stamp.origin);
            }
            AppEvent::DeviceCycleResolved { request_id, result } => {
                self.handle_device_cycle_result(request_id, result);
            }
            AppEvent::MicrophoneStateChanged { state, origin } => {
                let changed = self.microphone_state != state;
                let config = crate::app::config();
                let should_show = Self::should_show_audio_overlay(
                    origin,
                    self.microphone_seen,
                    changed,
                    config.overlay.show_external_audio_changes,
                    false,
                );
                self.microphone_state = state;
                self.microphone_seen = true;
                if should_show {
                    self.show_microphone_overlay();
                }
            }
            AppEvent::OutputStateChanged { state, origin } => {
                let changed = self.output_state != state;
                let config = crate::app::config();
                let should_show = Self::should_show_audio_overlay(
                    origin,
                    self.output_seen,
                    changed,
                    config.overlay.show_external_audio_changes,
                    false,
                );
                self.output_state = state;
                self.output_seen = true;
                if should_show {
                    self.show_output_overlay();
                }
            }
            AppEvent::DefaultOutputChanged(device) => {
                let row = crate::ui::overlay::output_changed_row(&device);
                self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));
            }
            AppEvent::DevicesChanged => {
                let devices = self.audio_devices();
                if let Some(settings) = &mut self.settings {
                    settings.refresh_devices(devices);
                }
            }
            AppEvent::ForegroundAudioChanged { state, origin } => {
                let changed = self.foreground_state != state;
                let config = crate::app::config();
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
                    config.overlay.show_external_audio_changes,
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
            }
            AppEvent::ForegroundVolumeChanged { state, origin } => {
                if matches!(origin, AudioEventOrigin::WinShortAction(_)) {
                    self.show_overlay_model(crate::ui::overlay::OverlayModel::single(
                        crate::ui::overlay::application_volume_row(&state),
                    ));
                }
            }
            AppEvent::DesktopBackendChanged(status) => {
                self.desktop_status = status;
            }
        }
    }
    pub(crate) fn diagnostics_snapshot(&self) -> crate::diagnostics::snapshot::DiagnosticsSnapshot {
        use crate::audio::state::Aggregate;
        use crate::desktop::{BackendAvailability, BackendKind};
        use crate::diagnostics::snapshot::{
            aggregate_label, ApplicationDiagnostics, AudioDiagnostics, ConfigDiagnostics,
            DegradedSubsystem, DesktopDiagnostics, DiagnosticsSnapshot, ForegroundAudioDiagnostics,
            Health, KeyboardDiagnostics, LoggingDiagnostics, OverlayDiagnostics,
            StartupDiagnostics, WindowsDiagnostics,
        };

        let config = crate::app::config();
        let raw_config = (*config).clone();
        let load = crate::config::load_diagnostics();
        let validation = crate::config::validate(&raw_config);
        let validation_messages: Vec<String> = validation
            .iter()
            .map(|violation| format!("{}: {}", violation.field, violation.message))
            .collect();
        let config_health = if load
            .source_schema_version
            .is_some_and(|version| version > crate::config::model::CURRENT_SCHEMA_VERSION)
            || crate::config::config_readonly()
        {
            Health::Error
        } else if !load.warnings.is_empty() || !load.repaired_fields.is_empty() {
            Health::Warning
        } else if !validation_messages.is_empty() {
            Health::Error
        } else {
            Health::Healthy
        };

        let runtime: crate::audio::AudioRuntimeSnapshot = self
            .audio
            .as_ref()
            .map(crate::audio::AudioService::runtime_snapshot)
            .unwrap_or_default();
        let input = crate::diagnostics::snapshot::endpoint_diagnostic(
            &raw_config.audio.input_device,
            raw_config.audio.input_role.label(),
            runtime.capture.as_ref(),
            runtime.capture_error.as_deref(),
        );
        let output = crate::diagnostics::snapshot::endpoint_diagnostic(
            &raw_config.audio.output_device,
            raw_config.audio.output_role.label(),
            runtime.render.as_ref(),
            runtime.render_error.as_deref(),
        );
        let foreground_health = match self.foreground_state.aggregate {
            Aggregate::Error => Health::Error,
            Aggregate::NoExternalApp | Aggregate::NoSession => Health::Warning,
            _ => Health::Healthy,
        };
        let audio = AudioDiagnostics {
            input,
            output,
            microphone_state: audio_state_label(&self.microphone_state),
            output_state: output_state_label(&self.output_state),
            foreground: ForegroundAudioDiagnostics {
                health: foreground_health,
                aggregate: aggregate_label(self.foreground_state.aggregate).into(),
                app_name: self.foreground_state.app_name.clone(),
                sessions: self.foreground_state.sessions,
                error: self.foreground_state.error.clone(),
            },
        };

        let keyboard_health = if self.keyboard.is_none() {
            Health::Unavailable
        } else if self.suspended {
            Health::Warning
        } else if !crate::keyboard::hook::hook_active() {
            Health::Error
        } else {
            Health::Healthy
        };
        let bindings = [
            (
                "Microphone".into(),
                raw_config
                    .hotkeys
                    .toggle_microphone
                    .map_or_else(|| "Not assigned".into(), |value| value.to_string()),
            ),
            (
                "Output".into(),
                raw_config
                    .hotkeys
                    .toggle_output
                    .map_or_else(|| "Not assigned".into(), |value| value.to_string()),
            ),
            (
                "Foreground app".into(),
                raw_config
                    .hotkeys
                    .toggle_foreground_audio
                    .map_or_else(|| "Not assigned".into(), |value| value.to_string()),
            ),
            (
                "Cycle input device".into(),
                raw_config
                    .hotkeys
                    .cycle_input_device
                    .map_or_else(|| "Not assigned".into(), |value| value.to_string()),
            ),
            (
                "Cycle output device".into(),
                raw_config
                    .hotkeys
                    .cycle_output_device
                    .map_or_else(|| "Not assigned".into(), |value| value.to_string()),
            ),
            (
                "Foreground volume up".into(),
                raw_config
                    .hotkeys
                    .foreground_volume_up
                    .map_or_else(|| "Not assigned".into(), |value| value.to_string()),
            ),
            (
                "Foreground volume down".into(),
                raw_config
                    .hotkeys
                    .foreground_volume_down
                    .map_or_else(|| "Not assigned".into(), |value| value.to_string()),
            ),
        ];
        let keyboard = KeyboardDiagnostics {
            health: keyboard_health,
            installed: self.keyboard.is_some(),
            hook_active: crate::keyboard::hook::hook_active(),
            suspended: self.suspended,
            capture_active: crate::keyboard::hook::capture_active(),
            bindings: bindings.into_iter().collect(),
            conflicts: validation_messages.clone(),
            reserved_win_numbers: raw_config.virtual_desktops.enabled
                && raw_config.virtual_desktops.win_number_switching,
        };

        let os = crate::desktop::detect::detect();
        let (build, update_revision, windows_error) = match os {
            Ok(value) => (Some(value.build), Some(value.update_revision), None),
            Err(error) => (None, None, Some(error.to_string())),
        };
        let desktop_native_error = match &self.desktop_status.native {
            BackendAvailability::Available => None,
            BackendAvailability::Failed { reason } => Some(reason.clone()),
            BackendAvailability::UnsupportedBuild { build } => {
                Some(format!("unsupported build {build}"))
            }
        };
        let desktop_health = if self.desktop.is_none() {
            Health::Unavailable
        } else if matches!(self.desktop_status.native, BackendAvailability::Available) {
            Health::Healthy
        } else {
            Health::Warning
        };
        let desktop = DesktopDiagnostics {
            health: desktop_health,
            native: self.desktop_status.native.label(),
            fallback: self.desktop_status.fallback.label(),
            active: self.desktop_status.active.label().into(),
            last_served: self
                .desktop_status
                .last_served
                .map(BackendKind::label)
                .unwrap_or("none yet")
                .into(),
            desktop_count: self.desktop_status.desktop_count,
            build,
            update_revision,
            error: desktop_native_error,
        };

        let overlay_status = self
            .overlay
            .as_ref()
            .map(crate::ui::overlay::OverlayWindow::status)
            .unwrap_or_default();
        let overlay_health = if !overlay_status.window_available {
            Health::Unavailable
        } else {
            Health::Healthy
        };
        let overlay = OverlayDiagnostics {
            health: overlay_health,
            enabled: raw_config.overlay.enabled,
            appearance: raw_config.overlay.appearance.as_str().into(),
            resolved_appearance: overlay_status.resolved_appearance,
            external_audio_changes: raw_config.overlay.show_external_audio_changes,
            animations_enabled: overlay_status.animations_enabled,
            high_contrast: overlay_status.high_contrast,
            disable_overlapped_content: overlay_status.disable_overlapped_content,
            position: raw_config.overlay.position.label().into(),
            monitor_selector: raw_config.overlay.monitor.as_str(),
            target_monitor: overlay_status.target_monitor,
            render_dpi: overlay_status.render_dpi,
            last_shown: overlay_status.last_shown,
            window_available: overlay_status.window_available,
        };

        let startup = crate::platform::startup::details();
        let (startup_state, startup_health) = match &startup.state {
            crate::platform::startup::StartupState::Enabled => ("Enabled".into(), Health::Healthy),
            crate::platform::startup::StartupState::Disabled => {
                ("Disabled".into(), Health::Healthy)
            }
            crate::platform::startup::StartupState::Stale { .. } => {
                ("Stale".into(), Health::Warning)
            }
        };
        let startup = StartupDiagnostics {
            health: if startup.error.is_some() {
                Health::Error
            } else {
                startup_health
            },
            state: startup_state,
            registered_command: startup.registered_command,
            current_command: startup.current_command,
            error: startup.error,
        };

        let logger = crate::diagnostics::logging::info();
        let logging = LoggingDiagnostics {
            health: if logger
                .as_ref()
                .and_then(|value| value.directory.as_ref())
                .is_some()
            {
                Health::Healthy
            } else {
                Health::Unavailable
            },
            directory: logger.as_ref().and_then(|value| value.directory.clone()),
            current_file: logger
                .as_ref()
                .and_then(|value| value.current_file.clone())
                .or_else(crate::diagnostics::logging::current_log_path),
            level: logger.as_ref().map_or_else(
                || "unavailable".into(),
                |value| {
                    if value.temporary_debug {
                        format!("{} (temporary)", value.level.as_str())
                    } else {
                        value.level.as_str().into()
                    }
                },
            ),
            default_level: logger.as_ref().map_or_else(
                || "unavailable".into(),
                |value| value.default_level.as_str().into(),
            ),
            temporary_debug: logger.as_ref().is_some_and(|value| value.temporary_debug),
            retention_days: logger
                .as_ref()
                .map_or(crate::diagnostics::logging::LOG_RETENTION_DAYS, |value| {
                    value.retention_days
                }),
            buffering: logger.as_ref().map_or_else(
                || "unavailable".into(),
                |value| {
                    if value.buffered {
                        format!(
                            "BufWriter; Warn/Error + {}s dirty flush",
                            crate::diagnostics::logging::FLUSH_TIMER_MS / 1000
                        )
                    } else {
                        "unbuffered".into()
                    }
                },
            ),
        };

        let degraded: Vec<DegradedSubsystem> = self
            .degraded
            .iter()
            .map(|(name, reason)| DegradedSubsystem {
                name: (*name).into(),
                reason: reason.clone(),
            })
            .collect();
        DiagnosticsSnapshot {
            generated_at: std::time::SystemTime::now(),
            application: ApplicationDiagnostics {
                version: env!("CARGO_PKG_VERSION").into(),
                profile: if cfg!(debug_assertions) {
                    "debug".into()
                } else {
                    "release".into()
                },
                architecture: std::env::consts::ARCH.into(),
            },
            windows: WindowsDiagnostics {
                architecture: std::env::consts::ARCH.into(),
                build,
                update_revision,
                error: windows_error,
            },
            keyboard,
            audio,
            desktop,
            config: ConfigDiagnostics {
                warnings: load.warnings,
                health: config_health,
                path: if load.path.as_os_str().is_empty() {
                    crate::config::load::config_path(&crate::config::data_dir())
                } else {
                    load.path
                },
                source_schema_version: load.source_schema_version,
                effective_schema_version: load.effective_schema_version,
                read_only: crate::config::config_readonly(),
                repaired_fields: load.repaired_fields,
                migrations: load.migrations,
                validation: validation_messages,
                hotkey_count: [
                    raw_config.hotkeys.toggle_microphone,
                    raw_config.hotkeys.toggle_output,
                    raw_config.hotkeys.toggle_foreground_audio,
                    raw_config.hotkeys.cycle_input_device,
                    raw_config.hotkeys.cycle_output_device,
                    raw_config.hotkeys.foreground_volume_up,
                    raw_config.hotkeys.foreground_volume_down,
                ]
                .into_iter()
                .flatten()
                .count(),
                raw: raw_config,
            },
            overlay,
            startup,
            logging,
            degraded,
        }
    }
    fn begin_shutdown(&mut self) {
        if self.shutting_down {
            return;
        }
        self.shutting_down = true;
        crate::diagnostics::logging::stop_flush_timer(self.hwnd);

        // Wake-and-stop the second-instance watcher before any window goes
        // away (#24 ordering).
        crate::platform::single_instance::signal_shutdown();

        // Stop dispatch before any worker or window it targets disappears.
        if let Some(mut keyboard) = self.keyboard.take() {
            keyboard.set_suspended(true);
            keyboard.shutdown();
        }
        if let Some(mut audio) = self.audio.take() {
            audio.shutdown();
        }
        if let Some(mut desktop) = self.desktop.take() {
            desktop.shutdown();
        }
        self.foreground.take();
        if let Some(join) = self.support_bundle.take() {
            let _ = join.join();
        }
        if let Some(diagnostics) = self.diagnostics.take() {
            unsafe {
                let _ = DestroyWindow(diagnostics.hwnd);
            }
        }
        if let Some(overlay) = self.overlay.take() {
            overlay.hide();
            unsafe {
                let _ = DestroyWindow(overlay.hwnd);
            }
        }

        if let Some(t) = self.tray.take() {
            t.remove();
        }
        if let Some(mut s) = self.settings.take() {
            s.persist_position();
            s.close_for_hide();
            unsafe {
                let _ = DestroyWindow(s.hwnd);
            }
        }
        crate::diagnostics::logging::flush();
        // Never DestroyWindow the borrowed main window here (reentrancy);
        // post WM_CLOSE so destruction happens outside any App borrow.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(self.hwnd),
                windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}

fn audio_state_label(state: &crate::audio::AudioState) -> String {
    match state {
        crate::audio::AudioState::Unavailable { reason } => format!("Unavailable — {reason}"),
        crate::audio::AudioState::Muted { volume_pct } => format!("Muted ({volume_pct}%)"),
        crate::audio::AudioState::Active { volume_pct } => format!("Active ({volume_pct}%)"),
    }
}

fn output_state_label(state: &crate::audio::OutputState) -> String {
    match state {
        crate::audio::OutputState::Unavailable { reason } => format!("Unavailable — {reason}"),
        crate::audio::OutputState::Current {
            device,
            muted,
            volume_pct,
        } => format!(
            "{} ({volume_pct}%) — {}",
            device.name,
            if *muted { "muted" } else { "active" }
        ),
    }
}

/// State snapshot for tray menu checkmarks.
fn tray_menu_state(app: &App) -> tray_menu::MenuState {
    tray_menu::MenuState {
        suspended: app.is_suspended(),
        start_with_windows: crate::platform::startup::is_enabled(),
    }
}

unsafe extern "system" fn main_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let taskbar_created = taskbar_created_msg();

    if msg == taskbar_created && taskbar_created != 0 {
        with_app(|app| {
            if let Some(t) = &mut app.tray {
                if let Err(e) = t.recreate() {
                    error_!("tray recreate failed: {e}");
                } else {
                    info!("tray recreated after Explorer restart");
                }
            }
        });
        return LRESULT(0);
    }

    match msg {
        event::WM_APP_TRAY => {
            unsafe {
                handle_tray(wparam, lparam);
            }
            LRESULT(0)
        }

        event::WM_APP_ACTION => {
            if lparam.0 != 0 {
                if let Err(e) = crate::keyboard::dispatcher::dirty_win_chord() {
                    error_!("failed to consume Win chord: {e}");
                }
            }
            if let Some(action) = HotkeyAction::unpack(wparam.0) {
                with_app(|app| app.dispatch_action(action));
            }
            LRESULT(0)
        }

        WM_APP_EVENT => {
            for ev in event::EVENTS.get_or_init(event::EventQueue::new).drain() {
                with_app(|app| app.route_event(ev));
            }
            LRESULT(0)
        }

        WM_TIMER if wparam.0 == crate::diagnostics::logging::FLUSH_TIMER_ID => {
            crate::diagnostics::logging::flush_if_dirty();
            LRESULT(0)
        }
        WM_CLOSE => {
            // Idempotent ordered teardown, then destroy. Posted by
            // begin_shutdown; external close requests land here too.
            with_app(App::begin_shutdown);
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }

        // NOTE: message constants in match patterns MUST be paths or
        // pre-imported names — a bare unknown identifier becomes an
        // irrefutable binding that swallows every message.
        windows::Win32::UI::WindowsAndMessaging::WM_WTSSESSION_CHANGE => {
            // wparam values from wtsapi32.h (not exported by the crate).
            const WTS_SESSION_LOCK: u32 = 0x7;
            const WTS_SESSION_UNLOCK: u32 = 0x8;
            match wparam.0 as u32 {
                WTS_SESSION_LOCK | WTS_SESSION_UNLOCK => {
                    with_app(App::reset_keyboard_state);
                }
                _ => {}
            }
            LRESULT(0)
        }

        windows::Win32::UI::WindowsAndMessaging::WM_POWERBROADCAST => match wparam.0 as u32 {
            windows::Win32::UI::WindowsAndMessaging::PBT_APMSUSPEND
            | windows::Win32::UI::WindowsAndMessaging::PBT_APMRESUMEAUTOMATIC
            | windows::Win32::UI::WindowsAndMessaging::PBT_APMRESUMESUSPEND => {
                with_app(App::reset_keyboard_state);
                LRESULT(1)
            }
            _ => LRESULT(1),
        },

        WM_DESTROY => {
            unsafe {
                let _ =
                    windows::Win32::System::RemoteDesktop::WTSUnRegisterSessionNotification(hwnd);
            }
            crate::platform::message_loop::quit(0);
            unsafe { LRESULT(DefWindowProcW(hwnd, msg, wparam, lparam).0) }
        }

        _ => win::def_proc(hwnd, msg, wparam, lparam),
    }
}

unsafe fn handle_tray(wparam: WPARAM, lparam: LPARAM) {
    match crate::tray::decode_callback(wparam, lparam) {
        TrayEvent::DoubleClick { .. } => {
            with_app(|app| app.show_settings());
        }
        TrayEvent::ContextMenu { x, y, .. } => {
            // TrackPopupMenu runs a modal dispatch loop; never hold the App
            // borrow across it: snapshot state, run the menu, re-borrow to apply.
            let Some((hwnd, state)) = with_app(|app| (app.hwnd, tray_menu_state(app))) else {
                return;
            };
            let cmd = tray_menu::track_tray_menu(hwnd, POINT { x, y }, &state);
            with_app(|app| apply_menu_command(app, cmd));
        }
        TrayEvent::Select { .. } => {
            with_app(|app| app.show_settings());
        }
        TrayEvent::Other => {}
    }
}

fn apply_menu_command(app: &mut App, cmd: Option<u32>) {
    use tray_menu::cmd;
    match cmd {
        Some(cmd::OPEN_SETTINGS) => app.show_settings(),
        Some(cmd::SHOW_STATUS) => app.route_event(AppEvent::ShowStatusOverlay),
        Some(cmd::SUSPEND_HOTKEYS) => app.toggle_suspended(),
        Some(cmd::START_WITH_WINDOWS) => {
            let enable = !crate::platform::startup::is_enabled();
            if let Err(e) = crate::platform::startup::set_enabled(enable) {
                error_!("startup registry update failed: {e}");
            } else {
                info!("start with windows = {enable}");
            }
        }
        Some(cmd::EXIT) => app.begin_shutdown(),
        _ => {}
    }
}

fn small_icon_size() -> u32 {
    // SAFETY: trivial metric queries.
    unsafe {
        windows::Win32::UI::HiDpi::GetSystemMetricsForDpi(
            windows::Win32::UI::WindowsAndMessaging::SM_CXSMICON,
            windows::Win32::UI::HiDpi::GetDpiForSystem(),
        ) as u32
    }
}

#[cfg(test)]
mod shutdown_gate_tests {
    use super::*;
    use windows::Win32::Foundation::HWND;

    fn test_app() -> App {
        App {
            hwnd: HWND(std::ptr::null_mut()),
            tray: None,
            settings: None,
            diagnostics: None,
            overlay: None,
            foreground: None,
            keyboard: None,
            audio: None,
            desktop: None,
            desktop_status: crate::desktop::BackendStatus {
                native: crate::desktop::BackendAvailability::Failed {
                    reason: "test".into(),
                },
                fallback: crate::desktop::BackendAvailability::Available,
                active: crate::desktop::BackendKind::KeyboardFallback,
                desktop_count: None,
                last_served: None,
            },
            microphone_state: crate::audio::AudioState::Unavailable {
                reason: "test".into(),
            },
            output_state: crate::audio::OutputState::Unavailable {
                reason: "test".into(),
            },
            foreground_state: crate::audio::AppAudioState::no_external(),
            microphone_seen: false,
            output_seen: false,
            foreground_seen: false,
            next_audio_request_id: 0,
            status_request_id: None,
            suspended: false,
            shutting_down: false,
            support_bundle: None,
            degraded: Vec::new(),
        }
    }

    #[test]
    fn gated_route_event_creates_nothing_after_shutdown_begins() {
        // #43: once shutdown begins, user-facing events must not create state.
        let mut app = test_app();
        app.shutting_down = true;
        app.route_event(AppEvent::ShowSettings);
        assert!(app.settings.is_none(), "settings must not be created");
        app.route_event(AppEvent::ShowStatusOverlay);
        assert!(app.overlay.is_none(), "overlay must not be created");
    }

    #[test]
    fn gated_dispatch_is_a_no_op_after_shutdown_begins() {
        let mut app = test_app();
        app.shutting_down = true;
        // Must not panic or enqueue anything (no audio subsystem present).
        app.dispatch_action(HotkeyAction::ToggleMicrophone);
        app.dispatch_action(HotkeyAction::SwitchDesktop(0));
        assert_eq!(app.next_audio_request_id, 0);
    }

    #[test]
    fn external_audio_overlay_requires_opt_in_and_a_real_change() {
        assert!(!App::should_show_audio_overlay(
            AudioEventOrigin::External,
            false,
            true,
            true,
            false,
        ));
        assert!(!App::should_show_audio_overlay(
            AudioEventOrigin::External,
            true,
            false,
            true,
            false,
        ));
        assert!(!App::should_show_audio_overlay(
            AudioEventOrigin::External,
            true,
            true,
            false,
            false,
        ));
        assert!(App::should_show_audio_overlay(
            AudioEventOrigin::External,
            true,
            true,
            true,
            false,
        ));
        assert!(App::should_show_audio_overlay(
            AudioEventOrigin::WinShortAction(7),
            false,
            false,
            false,
            false,
        ));
        assert!(App::should_show_audio_overlay(
            AudioEventOrigin::StatusRequest(8),
            false,
            false,
            false,
            true,
        ));
    }

    #[test]
    fn config_rebuild_origin_does_not_emit_external_overlay() {
        assert!(!App::should_show_audio_overlay(
            AudioEventOrigin::Config(crate::event::ConfigCommitOrigin::DeviceCycle),
            true,
            true,
            true,
            false,
        ));
    }
    #[test]
    fn preview_event_carries_draft_presentation_without_mutating_live_config() {
        let saved = crate::config::Config::default();
        let mut draft = saved.clone();
        draft.overlay.appearance = crate::config::model::OverlayAppearance::Light;
        draft.overlay.scale = 1.6;
        draft.overlay.opacity = 0.5;
        draft.overlay.position = crate::config::model::OverlayPosition::TopLeft;
        let event = AppEvent::PreviewOverlay {
            config: draft.overlay.clone(),
        };
        let AppEvent::PreviewOverlay { config } = event else {
            panic!("expected preview event");
        };
        assert_eq!(config, draft.overlay);
        assert_eq!(
            saved.overlay.appearance,
            crate::config::model::OverlayAppearance::System
        );
        assert_eq!(saved.overlay.scale, 1.0);
        assert_eq!(saved.overlay.opacity, 1.0);
    }

    #[test]
    fn matching_status_request_refreshes_multi_row_presentation() {
        let mut app = test_app();
        app.status_request_id = Some(7);
        app.route_event(AppEvent::ForegroundAudioChanged {
            state: crate::audio::AppAudioState {
                app_name: Some("Test app".into()),
                aggregate: crate::audio::Aggregate::AllActive,
                sessions: 1,
                error: None,
            },
            origin: AudioEventOrigin::StatusRequest(7),
        });
        assert_eq!(app.status_request_id, None);
        assert_eq!(app.status_overlay_model().rows.len(), 3);
    }

    #[test]
    fn stale_status_request_has_no_observable_effect() {
        let mut app = test_app();
        app.status_request_id = Some(8);
        let before = app.foreground_state.clone();
        let before_seen = app.foreground_seen;
        app.route_event(AppEvent::ForegroundAudioChanged {
            state: crate::audio::AppAudioState {
                app_name: Some("Stale app".into()),
                aggregate: crate::audio::Aggregate::AllActive,
                sessions: 1,
                error: None,
            },
            origin: AudioEventOrigin::StatusRequest(7),
        });
        assert_eq!(app.foreground_state, before);
        assert_eq!(app.foreground_seen, before_seen);
        assert_eq!(app.status_request_id, Some(8));
        app.route_event(AppEvent::ShowStatusOverlay);
        assert_eq!(app.status_request_id, None);
        assert_eq!(app.status_overlay_model().rows.len(), 2);
    }

    #[test]
    fn non_status_foreground_result_invalidates_pending_status_request() {
        let mut app = test_app();
        app.status_request_id = Some(8);
        app.route_event(AppEvent::ForegroundAudioChanged {
            state: crate::audio::AppAudioState {
                app_name: Some("Action app".into()),
                aggregate: crate::audio::Aggregate::AllActive,
                sessions: 1,
                error: None,
            },
            origin: AudioEventOrigin::WinShortAction(9),
        });
        assert_eq!(app.status_request_id, None);
        assert_eq!(app.foreground_state.app_name.as_deref(), Some("Action app"));
    }

    #[test]
    fn status_without_audio_does_not_leave_phantom_request() {
        let mut app = test_app();
        app.show_status_overlay();
        assert_eq!(app.status_request_id, None);
    }
}
