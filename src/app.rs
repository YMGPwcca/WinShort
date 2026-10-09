//! Application runtime: hidden main window, tray integration, event routing,
//! startup/shutdown orchestration (spec §5–§7, §46–§47).

use std::sync::atomic::{AtomicU64, Ordering};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, KillTimer, RegisterWindowMessageW, SetTimer,
    HWND_MESSAGE, WINDOW_STYLE, WM_CLOSE, WM_DESTROY, WM_TIMER,
};

use crate::error::{Error, Result};
use crate::event::{self, AppEvent, AudioEventOrigin, HotkeyAction, WM_APP_EVENT};
use crate::platform::window as win;
use crate::tray::{menu as tray_menu, Tray, TrayEvent, TrayState};
use crate::ui::control_center::ControlCenterWindow;

mod event_router;

pub static CONFIG: std::sync::OnceLock<std::sync::Arc<crate::config::ConfigHandle>> =
    std::sync::OnceLock::new();

static CONFIG_SEQ: AtomicU64 = AtomicU64::new(1);
const DISPLAY_ROLLBACK_TIMER_ID: usize = 0x4450;
const DISPLAY_ROLLBACK_WINDOW_MS: u32 = 15_000;

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
    settings: Option<ControlCenterWindow>,
    diagnostics: Option<crate::ui::diagnostics::DiagnosticsWindow>,
    overlay: Option<crate::ui::overlay::OverlayManager>,
    foreground: Option<crate::platform::foreground::ForegroundTracker>,
    keyboard: Option<crate::keyboard::hook::KeyboardService>,
    audio: Option<crate::audio::AudioService>,
    display_rollback: Option<PendingDisplayRollback>,
    display_rollback_error: Option<String>,
    desktop: Option<crate::desktop::DesktopService>,
    desktop_status: crate::desktop::BackendStatus,
    microphone_state: crate::audio::AudioState,
    output_state: crate::audio::OutputState,
    foreground_state: crate::audio::AppAudioState,
    foreground_pid: Option<u32>,
    foreground_query_id: Option<u64>,
    foreground_request_floor: u64,
    microphone_seen: bool,
    output_seen: bool,
    foreground_seen: bool,
    next_audio_request_id: u64,
    status_request_id: Option<u64>,
    acceptance_overlay_only: bool,
    suspended: bool,
    shutting_down: bool,
    support_bundle: Option<std::thread::JoinHandle<()>>,
    /// Per-subsystem startup failures (degraded startup, #27): the app stays
    /// up with tray/Settings and surfaces why a subsystem is dark.
    degraded: Vec<(&'static str, String)>,
    #[cfg(test)]
    test_last_overlay_model: Option<crate::ui::overlay::OverlayModel>,
}

#[derive(Debug)]
pub(crate) enum DeferredShellAction {
    OpenConfigFolder { directory: std::path::PathBuf },
    OpenDiagnosticsLogs { directory: std::path::PathBuf },
}

struct PendingDisplayRollback {
    profile: crate::display::DisplayProfile,
    rollback: crate::display::DisplayRollback,
    state: crate::display::ConfirmationState,
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
            display_rollback: None,
            display_rollback_error: None,
            desktop_status: crate::desktop::BackendStatus {
                native: crate::desktop::BackendAvailability::Failed {
                    reason: "Detecting…".into(),
                },
                fallback: crate::desktop::BackendAvailability::Available,
                active: crate::desktop::BackendKind::KeyboardFallback,
                desktop_count: None,
                current_desktop: None,
                last_served: None,
            },
            microphone_state: crate::audio::AudioState::Unavailable {
                reason: "Starting audio…".into(),
            },
            output_state: crate::audio::OutputState::Unavailable {
                reason: "Starting audio…".into(),
            },
            foreground_state: crate::audio::AppAudioState::no_external(),
            foreground_pid: None,
            foreground_query_id: None,
            foreground_request_floor: 0,
            microphone_seen: false,
            output_seen: false,
            foreground_seen: false,
            next_audio_request_id: 0,
            status_request_id: None,
            acceptance_overlay_only: false,
            suspended: false,
            shutting_down: false,
            support_bundle: None,
            degraded: Vec::new(),
            #[cfg(test)]
            test_last_overlay_model: None,
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
        self.overlay = Some(crate::ui::overlay::OverlayManager::create()?);
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
        let pid = self
            .foreground
            .as_ref()
            .and_then(|tracker| tracker.target_pid());
        self.select_foreground_audio(pid, true);
        Ok(())
    }

    pub fn install_desktop(&mut self) -> Result<()> {
        let service = crate::desktop::DesktopService::start(self.hwnd)?;
        let config = crate::app::config();
        service.configure_scratchpad(
            config.virtual_desktops.enabled
                && (config.virtual_desktops.scratchpad_assign.is_some()
                    || config.virtual_desktops.scratchpad_toggle.is_some()),
        );
        self.desktop = Some(service);
        Ok(())
    }
    fn apply_display_profile(&mut self, profile: crate::display::DisplayProfile, test: bool) {
        let mut profile = profile;
        if !crate::app::config().display_profiles.enabled {
            self.show_display_feedback(
                crate::ui::overlay::OverlayTone::Unavailable,
                "Display profile",
                "Display profiles are disabled in Displays",
            );
            return;
        }
        if self.display_rollback.is_some() {
            self.show_display_feedback(
                crate::ui::overlay::OverlayTone::Unavailable,
                "Display apply blocked",
                "Revert or Keep the pending display test before applying another profile",
            );
            return;
        }
        if !test && !profile.confirmed {
            self.show_display_feedback(
                crate::ui::overlay::OverlayTone::Unavailable,
                "Display profile needs testing",
                "Test this profile, then Keep it before using it normally",
            );
            return;
        }
        self.display_rollback_error = None;
        match crate::display::apply_profile(&mut profile, !test) {
            Ok(rollback) if test => {
                self.display_rollback = Some(PendingDisplayRollback {
                    profile: profile.clone(),
                    rollback,
                    state: crate::display::ConfirmationState::Pending,
                });
                self.sync_display_settings_state();
                let timer_started = unsafe {
                    SetTimer(
                        Some(self.hwnd),
                        DISPLAY_ROLLBACK_TIMER_ID,
                        DISPLAY_ROLLBACK_WINDOW_MS,
                        None,
                    ) != 0
                };
                if !timer_started {
                    self.rollback_pending_display(
                        crate::display::ConfirmationIntent::Timeout,
                        "Display configuration reverted because confirmation timer could not start",
                    );
                } else {
                    self.invalidate_settings();
                    self.show_display_feedback(
                        crate::ui::overlay::OverlayTone::Changed,
                        "Display configuration tested",
                        "Keep display configuration or Revert within 15 seconds",
                    );
                }
            }
            Ok(_rollback) => {
                self.display_rollback_error = None;
                self.refresh_settings_runtime();
                self.show_display_feedback(
                    crate::ui::overlay::OverlayTone::Changed,
                    format!("Display profile: {}", profile.name),
                    "Applied confirmed display configuration",
                );
            }
            Err(failure) => {
                let reason = failure.error.to_string();
                crate::error_!("display profile apply failed: {reason}");
                if let Some(rollback) = failure.rollback {
                    self.display_rollback = Some(PendingDisplayRollback {
                        profile: profile.clone(),
                        rollback: *rollback,
                        state: crate::display::ConfirmationState::RollbackFailed,
                    });
                    self.display_rollback_error = Some(reason.clone());
                    self.sync_display_settings_state();
                    self.show_display_feedback(
                        crate::ui::overlay::OverlayTone::Unavailable,
                        "Display recovery failed",
                        "The display setup could not be recovered. Retry Revert in Displays",
                    );
                } else {
                    self.show_display_feedback(
                        crate::ui::overlay::OverlayTone::Unavailable,
                        "Display profile not applied",
                        "The display setup is not available. Open Displays and try again",
                    );
                }
            }
        }
    }

    fn keep_display_profile(&mut self) {
        let Some(mut pending) = self.display_rollback.take() else {
            return;
        };
        if pending.state != crate::display::ConfirmationState::Pending {
            self.display_rollback = Some(pending);
            self.sync_display_settings_state();
            return;
        }
        let mut config = (*crate::app::config()).clone();
        let mut confirmed_profile = pending.profile.clone();
        confirmed_profile.confirmed = true;
        if !config.display_profiles.upsert(confirmed_profile.clone()) {
            let reason = format!(
                "at most {} display profiles are supported",
                crate::display::MAX_PROFILES
            );
            self.display_rollback_error = Some(reason.clone());
            self.display_rollback = Some(pending);
            self.sync_display_settings_state();
            self.show_display_feedback(
                crate::ui::overlay::OverlayTone::Unavailable,
                "Display configuration not kept",
                "The display setup could not be kept",
            );
            return;
        }
        if let Err(error) = crate::display::persist_current() {
            let reason = error.to_string();
            self.display_rollback_error = Some(reason.clone());
            self.display_rollback = Some(pending);
            self.sync_display_settings_state();
            self.show_display_feedback(
                crate::ui::overlay::OverlayTone::Unavailable,
                "Display configuration not kept",
                "The display setup could not be saved",
            );
            return;
        }
        if let Err(error) =
            crate::app::commit_config(config, crate::event::ConfigCommitOrigin::Settings)
        {
            let reason = error.to_string();
            let mut recovery = pending;
            recovery.state = crate::display::transition_confirmation(
                recovery.state,
                crate::display::ConfirmationIntent::Revert,
            );
            match crate::display::rollback(&recovery.rollback) {
                Ok(()) => {
                    self.display_rollback_error = Some(reason.clone());
                    unsafe {
                        let _ = KillTimer(Some(self.hwnd), DISPLAY_ROLLBACK_TIMER_ID);
                    }
                    self.sync_display_settings_state();
                    if let Some(settings) = &mut self.settings {
                        settings.discard_uncommitted_draft();
                    }
                    self.invalidate_settings();
                    self.show_display_feedback(
                        crate::ui::overlay::OverlayTone::Unavailable,
                        "Display configuration not kept",
                        "The setting was not saved, so the previous display setup was restored",
                    );
                }
                Err(rollback_error) => {
                    let recovery_reason = format!(
                        "saving confirmation failed: {reason}; rollback failed: {rollback_error}"
                    );
                    recovery.state = crate::display::transition_confirmation(
                        recovery.state,
                        crate::display::ConfirmationIntent::RollbackFailed,
                    );
                    self.display_rollback_error = Some(recovery_reason.clone());
                    self.display_rollback = Some(recovery);
                    self.sync_display_settings_state();
                    unsafe {
                        let _ = KillTimer(Some(self.hwnd), DISPLAY_ROLLBACK_TIMER_ID);
                    }
                    self.invalidate_settings();
                    self.show_display_feedback(
                        crate::ui::overlay::OverlayTone::Unavailable,
                        "Display recovery failed",
                        "The display setup could not be recovered. Retry Revert in Displays",
                    );
                }
            }
            return;
        }
        pending.profile = confirmed_profile;
        pending.state = crate::display::transition_confirmation(
            pending.state,
            crate::display::ConfirmationIntent::Keep,
        );
        self.display_rollback_error = None;
        self.sync_display_settings_state();
        if let Some(settings) = &mut self.settings {
            settings.update_display_profile_after_keep(&pending.profile);
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), DISPLAY_ROLLBACK_TIMER_ID);
        }
        self.invalidate_settings();
        self.show_display_feedback(
            crate::ui::overlay::OverlayTone::Changed,
            "Display configuration kept",
            "The tested topology is now confirmed",
        );
    }

    fn revert_display_profile(&mut self) {
        self.rollback_pending_display(
            crate::display::ConfirmationIntent::Revert,
            "Display configuration reverted",
        );
    }

    fn rollback_pending_display(
        &mut self,
        intent: crate::display::ConfirmationIntent,
        success_title: &str,
    ) {
        let Some(mut pending) = self.display_rollback.take() else {
            return;
        };
        pending.state = crate::display::transition_confirmation(pending.state, intent);
        match crate::display::rollback(&pending.rollback) {
            Ok(()) => {
                pending.state = crate::display::transition_confirmation(
                    pending.state,
                    crate::display::ConfirmationIntent::RollbackSucceeded,
                );
                self.display_rollback_error = None;
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), DISPLAY_ROLLBACK_TIMER_ID);
                }
                self.sync_display_settings_state();
                if let Some(settings) = &mut self.settings {
                    settings.discard_uncommitted_draft();
                }
                self.invalidate_settings();
                self.show_display_feedback(
                    crate::ui::overlay::OverlayTone::Changed,
                    success_title,
                    "The previous display topology was restored",
                );
            }
            Err(error) => {
                let reason = error.to_string();
                pending.state = crate::display::transition_confirmation(
                    pending.state,
                    crate::display::ConfirmationIntent::RollbackFailed,
                );
                crate::error_!("display rollback failed: {reason}");
                self.display_rollback_error = Some(reason.clone());
                self.display_rollback = Some(pending);
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), DISPLAY_ROLLBACK_TIMER_ID);
                }
                self.sync_display_settings_state();
                self.invalidate_settings();
                self.show_display_feedback(
                    crate::ui::overlay::OverlayTone::Unavailable,
                    "Display recovery failed",
                    "The previous display setup could not be restored. Retry Revert in Displays",
                );
            }
        }
    }

    fn expire_display_rollback(&mut self) {
        self.rollback_pending_display(
            crate::display::ConfirmationIntent::Timeout,
            "Display configuration reverted after timeout",
        );
    }

    fn show_display_feedback(
        &mut self,
        tone: crate::ui::overlay::OverlayTone,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.show_overlay(crate::ui::overlay::OverlayRequest::toast(
            crate::ui::overlay::OverlayKey::DisplayProfile,
            crate::ui::overlay::OverlayModel::single(crate::ui::overlay::OverlayRow {
                category: Some(crate::config::model::OverlayNotificationCategory::DisplayProfile),
                icon: crate::ui::overlay::OverlayIcon::Info,
                tone,
                title: title.into(),
                detail: detail.into(),
            }),
        ));
    }

    fn invalidate_settings(&self) {
        if let Some(settings) = &self.settings {
            unsafe {
                let _ =
                    windows::Win32::Graphics::Gdi::InvalidateRect(Some(settings.hwnd), None, false);
            }
        }
    }
    fn sync_display_settings_state(&mut self) {
        let active = self.display_rollback.is_some();
        let keep_available = self
            .display_rollback
            .as_ref()
            .is_some_and(|pending| pending.state == crate::display::ConfirmationState::Pending);
        let runtime = self.control_center_runtime();
        if let Some(settings) = &mut self.settings {
            settings.set_display_rollback_state(active, keep_available);
            settings.set_runtime_snapshot(runtime);
        }
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
    pub(crate) fn control_center_runtime(
        &self,
    ) -> crate::ui::control_center::ControlCenterRuntimeSnapshot {
        crate::ui::control_center::ControlCenterRuntimeSnapshot {
            microphone: self.microphone_state.clone(),
            microphone_name: self
                .audio
                .as_ref()
                .and_then(|audio| audio.runtime_snapshot().capture.map(|device| device.name)),
            output: self.output_state.clone(),
            foreground: if std::env::var_os("WINSHORT_UI_ACCEPTANCE_NO_EXTERNAL").is_some() {
                crate::audio::AppAudioState::no_external()
            } else {
                self.foreground_state.clone()
            },
            desktop: self.desktop_status.clone(),
            degraded: self
                .degraded
                .iter()
                .map(|(name, reason)| ((*name).into(), reason.clone()))
                .collect(),
            display_rollback_active: self.display_rollback.is_some(),
            display_keep_available: self
                .display_rollback
                .as_ref()
                .is_some_and(|pending| pending.state == crate::display::ConfirmationState::Pending),
            display_rollback_error: self.display_rollback_error.clone(),
        }
    }

    fn refresh_settings_runtime(&mut self) {
        let snapshot = self.control_center_runtime();
        if let Some(settings) = &mut self.settings {
            settings.set_runtime_snapshot(snapshot);
        }
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
        show_external_current_app_audio: bool,
        status_request_matches: bool,
    ) -> bool {
        match origin {
            AudioEventOrigin::Initial
            | AudioEventOrigin::Config(_)
            | AudioEventOrigin::ForegroundSelection(_) => false,
            AudioEventOrigin::External => show_external_current_app_audio && seen && changed,
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
    ) -> Result<&mut ControlCenterWindow> {
        if self.settings.is_none() {
            self.settings = Some(ControlCenterWindow::create(
                devices,
                crate::ui::control_center::ConfigAccess::new(config, |candidate| {
                    commit_config(candidate, crate::event::ConfigCommitOrigin::Settings)
                }),
                crate::ui::control_center::ControlCenterAccess::system(),
            )?);
            info!("settings window created");
        }
        let active = self.display_rollback.is_some();
        let keep_available = self
            .display_rollback
            .as_ref()
            .is_some_and(|pending| pending.state == crate::display::ConfirmationState::Pending);
        let runtime = self.control_center_runtime();
        if let Some(settings) = &mut self.settings {
            settings.set_display_rollback_state(active, keep_available);
            settings.set_runtime_snapshot(runtime);
        }
        self.settings.as_mut().ok_or_else(|| {
            crate::error::Error::internal(
                "Control Center state missing after successful window construction",
            )
        })
    }

    pub fn show_settings(&mut self) {
        let devices = self.audio_devices();
        let runtime = self.control_center_runtime();
        match self.ensure_settings(devices.clone()) {
            Ok(settings) => {
                settings.set_runtime_snapshot(runtime);
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
        self.diagnostics.as_mut().ok_or_else(|| {
            crate::error::Error::internal(
                "Diagnostics state missing after successful window construction",
            )
        })
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
        if let Some(settings) = &mut self.settings {
            if let Err(error) = settings.open_picker(kind, devices) {
                error_!("open settings picker failed: {error}");
            }
        }
    }

    pub(crate) fn commit_settings_picker(&mut self, commit: crate::ui::picker::PickerCommit) {
        let system_target = match &commit {
            crate::ui::picker::PickerCommit::InputDevice(
                crate::config::model::DeviceSelection::Endpoint(endpoint),
            ) => Some((
                crate::audio::DeviceCycleFlow::Input,
                endpoint.clone(),
                crate::ui::picker::PickerCommit::InputDevice(
                    crate::config::model::DeviceSelection::Default,
                ),
            )),
            crate::ui::picker::PickerCommit::OutputDevice(
                crate::config::model::DeviceSelection::Endpoint(endpoint),
            ) => Some((
                crate::audio::DeviceCycleFlow::Output,
                endpoint.clone(),
                crate::ui::picker::PickerCommit::OutputDevice(
                    crate::config::model::DeviceSelection::Default,
                ),
            )),
            crate::ui::picker::PickerCommit::InputDevice(
                crate::config::model::DeviceSelection::Default,
            )
            | crate::ui::picker::PickerCommit::OutputDevice(
                crate::config::model::DeviceSelection::Default,
            )
            | crate::ui::picker::PickerCommit::InputAllowlist(_)
            | crate::ui::picker::PickerCommit::OutputAllowlist(_)
            | crate::ui::picker::PickerCommit::DisplayProfile(_)
            | crate::ui::picker::PickerCommit::DisplayOutputs(_)
            | crate::ui::picker::PickerCommit::DisplayTopology(_)
            | crate::ui::picker::PickerCommit::DisplayRoute(_)
            | crate::ui::picker::PickerCommit::InputRole(_)
            | crate::ui::picker::PickerCommit::OutputRole(_)
            | crate::ui::picker::PickerCommit::DesktopNumberModifier(_)
            | crate::ui::picker::PickerCommit::MoveDesktopModifier(_)
            | crate::ui::picker::PickerCommit::SilentMoveDesktopModifier(_)
            | crate::ui::picker::PickerCommit::OverlayPosition(_)
            | crate::ui::picker::PickerCommit::OverlayAppearance(_)
            | crate::ui::picker::PickerCommit::OverlayMonitor(_) => None,
        };

        if let Some((flow, endpoint, persisted_commit)) = system_target {
            if let Some(settings) = &mut self.settings {
                settings.commit_picker(persisted_commit);
            }
            let live = crate::app::config();
            let follows_system_default = match flow {
                crate::audio::DeviceCycleFlow::Input => matches!(
                    &live.audio.input_device,
                    crate::config::model::DeviceSelection::Default
                ),
                crate::audio::DeviceCycleFlow::Output => matches!(
                    &live.audio.output_device,
                    crate::config::model::DeviceSelection::Default
                ),
            };
            drop(live);
            if follows_system_default {
                self.dispatch_default_device_selection(flow, endpoint);
            }
            return;
        }

        if let Some(settings) = &mut self.settings {
            settings.commit_picker(commit);
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

    fn prepare_diagnostics_logs_open(&mut self) -> DeferredShellAction {
        crate::diagnostics::logging::flush();
        let snapshot = self.diagnostics_snapshot();
        DeferredShellAction::OpenDiagnosticsLogs {
            directory: crate::diagnostics::support::log_directory(
                snapshot.logging.directory.as_deref(),
            ),
        }
    }

    fn prepare_config_folder_open(&self) -> DeferredShellAction {
        DeferredShellAction::OpenConfigFolder {
            directory: crate::config::data_dir(),
        }
    }

    fn finish_diagnostics_logs_open(&mut self, error: Option<String>) {
        let status = match error {
            Some(error) => {
                crate::error_!("open diagnostics logs failed: {error}");
                format!("Open Logs failed — {error}")
            }
            None => "Opened the WinShort log directory".into(),
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
                if !unsafe { crate::event::post_event(hwnd, event) } {
                    crate::warn_!("support bundle completion queued but main-window wake failed");
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
                self.select_foreground_audio(pid, false);
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ToggleForeground { pid, request_id });
                } else {
                    let reason = self
                        .degraded_reason("audio")
                        .unwrap_or_else(|| "audio subsystem unavailable".into());
                    self.route_event(AppEvent::ForegroundAudioChanged {
                        pid,
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
                } else {
                    self.report_desktop_failure("switch desktop", "desktop subsystem unavailable");
                }
            }
            HotkeyAction::MoveForegroundToDesktop(n) => {
                self.dispatch_move_foreground(n as usize, true);
            }
            HotkeyAction::MoveForegroundToDesktopSilent(n) => {
                self.dispatch_move_foreground(n as usize, false);
            }
            HotkeyAction::SwitchPreviousDesktop => self.dispatch_previous_desktop(),
            HotkeyAction::AssignScratchpad => self.dispatch_assign_scratchpad(),
            HotkeyAction::ToggleScratchpad => self.dispatch_toggle_scratchpad(),
            HotkeyAction::ApplyDisplayProfile(profile_key) => {
                self.dispatch_profile_hotkey(profile_key)
            }
        }
    }

    fn dispatch_move_foreground(&mut self, index: usize, follow: bool) {
        let Some(desktop) = &self.desktop else {
            self.report_desktop_failure(
                if follow {
                    "move and follow foreground window"
                } else {
                    "move foreground window silently"
                },
                "desktop subsystem unavailable",
            );
            return;
        };
        let Some(hwnd) = crate::platform::foreground::current_external_hwnd() else {
            self.report_desktop_failure(
                if follow {
                    "move and follow foreground window"
                } else {
                    "move foreground window silently"
                },
                "no eligible foreground application window",
            );
            return;
        };
        desktop.move_foreground_to(index, hwnd.0 as isize, follow);
    }

    fn dispatch_previous_desktop(&mut self) {
        if let Some(desktop) = &self.desktop {
            desktop.switch_previous();
        } else {
            self.report_desktop_failure("switch previous desktop", "desktop subsystem unavailable");
        }
    }
    fn dispatch_assign_scratchpad(&mut self) {
        let Some(desktop) = &self.desktop else {
            self.report_desktop_failure("send to Special Desktop", "desktop subsystem unavailable");
            return;
        };
        let Some(hwnd) = crate::platform::foreground::current_external_hwnd() else {
            self.report_desktop_failure(
                "send to Special Desktop",
                "no eligible foreground application window",
            );
            return;
        };
        desktop.assign_scratchpad(hwnd.0 as isize);
    }

    fn dispatch_toggle_scratchpad(&mut self) {
        if let Some(desktop) = &self.desktop {
            desktop.toggle_scratchpad();
        } else {
            self.report_desktop_failure("toggle Special Desktop", "desktop subsystem unavailable");
        }
    }

    fn dispatch_profile_hotkey(&mut self, profile_key: u16) {
        let config = crate::app::config();
        let mut binding = None;
        for candidate in &config.hotkeys.display_profiles {
            if crate::display::profile_id_key(&candidate.profile_id) == profile_key {
                if binding.is_some() {
                    self.show_display_feedback(
                        crate::ui::overlay::OverlayTone::Unavailable,
                        "Display profile hotkey ambiguous",
                        "A display profile shortcut is ambiguous. Open Displays to fix it",
                    );
                    return;
                }
                binding = Some(candidate);
            }
        }
        let Some(binding) = binding else {
            self.show_display_feedback(
                crate::ui::overlay::OverlayTone::Unavailable,
                "Display profile hotkey unavailable",
                "This display profile shortcut is no longer available",
            );
            return;
        };
        let Some(profile) = config
            .display_profiles
            .profiles
            .iter()
            .find(|profile| profile.id.eq_ignore_ascii_case(&binding.profile_id))
        else {
            self.show_display_feedback(
                crate::ui::overlay::OverlayTone::Unavailable,
                "Display profile unavailable",
                "This display profile is no longer available",
            );
            return;
        };
        if !profile.confirmed {
            self.show_display_feedback(
                crate::ui::overlay::OverlayTone::Unavailable,
                format!("Display profile: {}", profile.name),
                "Profile must pass Test Apply and Keep before hotkey activation",
            );
            return;
        }
        self.apply_display_profile(profile.clone(), false);
    }

    fn report_desktop_failure(&mut self, action: &str, reason: &str) {
        self.route_event(AppEvent::DesktopActionFailed {
            action: action.into(),
            reason: reason.into(),
        });
    }

    fn dispatch_default_device_selection(
        &mut self,
        flow: crate::audio::DeviceCycleFlow,
        endpoint: String,
    ) {
        let request_id = self.next_audio_request_id();
        if let Some(audio) = &self.audio {
            audio.send(crate::audio::AudioCommand::SetDefaultDevice {
                flow,
                endpoint,
                request_id,
            });
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
            crate::audio::DeviceCycleResult::AlreadySelected { flow, device } => {
                crate::log_debug!(
                    "device selection request {request_id} was already selected for {flow:?}: {}",
                    device.name
                );
            }
            crate::audio::DeviceCycleResult::Changed { flow, device, .. } => {
                let key = Self::device_cycle_overlay_key(flow);
                self.show_overlay(crate::ui::overlay::OverlayRequest::toast(
                    key,
                    crate::ui::overlay::OverlayModel::single(crate::ui::overlay::device_cycle_row(
                        flow, &device,
                    )),
                ));
                crate::log_debug!("device cycle request {request_id} applied for {flow:?}");
            }
            crate::audio::DeviceCycleResult::NoDevices { flow, .. } => {
                let key = Self::device_cycle_overlay_key(flow);
                self.show_overlay(crate::ui::overlay::OverlayRequest::toast(
                    key,
                    crate::ui::overlay::OverlayModel::single(
                        crate::ui::overlay::device_cycle_no_devices_row(flow),
                    ),
                ));
            }
            crate::audio::DeviceCycleResult::Failed { flow, error, .. } => {
                crate::warn_!("device cycle request {request_id} failed: {error}");
                let key = Self::device_cycle_overlay_key(flow);
                self.show_overlay(crate::ui::overlay::OverlayRequest::toast(
                    key,
                    crate::ui::overlay::OverlayModel::single(
                        crate::ui::overlay::device_cycle_error_row(flow, &error),
                    ),
                ));
            }
        }
    }

    fn device_cycle_overlay_key(
        flow: crate::audio::DeviceCycleFlow,
    ) -> crate::ui::overlay::OverlayKey {
        match flow {
            crate::audio::DeviceCycleFlow::Input => crate::ui::overlay::OverlayKey::InputDevice,
            crate::audio::DeviceCycleFlow::Output => crate::ui::overlay::OverlayKey::OutputDevice,
        }
    }

    fn dispatch_foreground_volume(&mut self, adjustment: crate::audio::sessions::VolumeAdjustment) {
        let request_id = self.next_audio_request_id();
        let pid = self
            .foreground
            .as_ref()
            .and_then(|tracker| tracker.target_pid());
        self.select_foreground_audio(pid, false);
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
                pid,
                state: crate::audio::AppVolumeState::error(None, reason),
                origin: AudioEventOrigin::WinShortAction(request_id),
            });
        }
    }
    fn show_overlay_with_config(
        &mut self,
        request: crate::ui::overlay::OverlayRequest,
        config: crate::config::model::OverlayCfg,
    ) {
        if !config.enabled {
            if let Some(overlay) = &mut self.overlay {
                overlay.clear();
            }
            return;
        }
        if self.acceptance_overlay_only && request.key != crate::ui::overlay::OverlayKey::Status {
            return;
        }
        let model = request.model.filter_enabled(config.notifications);
        if model.rows.is_empty() {
            if let Some(overlay) = &mut self.overlay {
                if let Err(error) = overlay.remove_key(request.key, &config) {
                    error_!("overlay empty-entry removal failed: {error}");
                }
            }
            return;
        }
        #[cfg(test)]
        {
            self.test_last_overlay_model = (!model.rows.is_empty()).then(|| model.clone());
        }
        if let Some(overlay) = &mut self.overlay {
            if let Err(error) = overlay.present(
                crate::ui::overlay::OverlayRequest { model, ..request },
                config,
            ) {
                error_!("overlay show failed: {error}");
            }
        }
    }

    fn show_overlay(&mut self, request: crate::ui::overlay::OverlayRequest) {
        let config = crate::app::config();
        self.show_overlay_with_config(request, config.overlay.clone());
    }

    fn remove_overlay_key(
        &mut self,
        key: crate::ui::overlay::OverlayKey,
        config: &crate::config::model::OverlayCfg,
    ) {
        if let Some(overlay) = &mut self.overlay {
            if let Err(error) = overlay.remove_key(key, config) {
                error_!("overlay entry removal failed: {error}");
            }
        }
    }

    fn select_foreground_audio(&mut self, pid: Option<u32>, refresh: bool) {
        let changed = self.foreground_pid != pid;
        if !changed && !refresh {
            return;
        }
        if changed {
            self.foreground_request_floor = self.next_audio_request_id;
            self.foreground_pid = pid;
            self.foreground_state = crate::audio::AppAudioState::no_external();
            self.foreground_seen = false;
            self.status_request_id = None;
            let config = crate::app::config();
            for key in [
                crate::ui::overlay::OverlayKey::CurrentAppAudioPermanent,
                crate::ui::overlay::OverlayKey::CurrentAppAudio,
                crate::ui::overlay::OverlayKey::CurrentAppVolume,
            ] {
                self.remove_overlay_key(key, &config.overlay);
            }
            self.refresh_settings_runtime();
        }
        self.foreground_query_id = None;
        if self.audio.is_some() {
            let request_id = self.next_audio_request_id();
            self.foreground_query_id = Some(request_id);
            if let Some(audio) = &self.audio {
                audio.send(crate::audio::AudioCommand::ObserveForeground { pid, request_id });
            }
        }
    }

    fn accepts_foreground_audio_result(
        &mut self,
        pid: Option<u32>,
        origin: AudioEventOrigin,
    ) -> bool {
        if let Some(tracker) = &self.foreground {
            self.select_foreground_audio(tracker.target_pid(), false);
        }
        if pid != self.foreground_pid {
            return false;
        }
        // Returning to the same PID must not revive an action dispatched before
        // a more recent selection. Queries also carry their own request identity.
        !matches!(origin, AudioEventOrigin::WinShortAction(id) if id < self.foreground_request_floor)
    }

    fn reconcile_app_audio_overlay(&mut self, show_feedback: bool) {
        use crate::audio::Aggregate;
        use crate::ui::overlay::{OverlayKey, OverlayModel, OverlayRequest};
        let config = crate::app::config();
        if self.acceptance_overlay_only
            || !config.overlay.enabled
            || !config.overlay.notifications.current_app_audio
        {
            self.remove_overlay_key(OverlayKey::CurrentAppAudioPermanent, &config.overlay);
            self.remove_overlay_key(OverlayKey::CurrentAppAudio, &config.overlay);
            self.remove_overlay_key(OverlayKey::CurrentAppVolume, &config.overlay);
            return;
        }
        let model =
            OverlayModel::single(crate::ui::overlay::application_row(&self.foreground_state));
        if self.foreground_state.aggregate == Aggregate::AllMuted {
            self.show_overlay_with_config(
                OverlayRequest::permanent(OverlayKey::CurrentAppAudioPermanent, model)
                    .replacing(OverlayKey::CurrentAppAudio),
                config.overlay.clone(),
            );
        } else if show_feedback {
            self.show_overlay_with_config(
                OverlayRequest::toast(OverlayKey::CurrentAppAudio, model)
                    .replacing(OverlayKey::CurrentAppAudioPermanent),
                config.overlay.clone(),
            );
        } else {
            self.remove_overlay_key(OverlayKey::CurrentAppAudioPermanent, &config.overlay);
        }
    }

    fn reconcile_microphone_overlay(&mut self, show_unmute_feedback: bool) {
        let config = crate::app::config();
        if self.acceptance_overlay_only {
            self.remove_overlay_key(
                crate::ui::overlay::OverlayKey::MicrophonePermanent,
                &config.overlay,
            );
            self.remove_overlay_key(
                crate::ui::overlay::OverlayKey::MicrophoneToast,
                &config.overlay,
            );
            return;
        }
        if !config.overlay.enabled || !config.overlay.notifications.microphone {
            self.remove_overlay_key(
                crate::ui::overlay::OverlayKey::MicrophonePermanent,
                &config.overlay,
            );
            self.remove_overlay_key(
                crate::ui::overlay::OverlayKey::MicrophoneToast,
                &config.overlay,
            );
            return;
        }

        match self.microphone_state {
            crate::audio::AudioState::Muted { .. } => {
                self.show_overlay_with_config(
                    crate::ui::overlay::OverlayRequest::permanent(
                        crate::ui::overlay::OverlayKey::MicrophonePermanent,
                        crate::ui::overlay::OverlayModel::single(
                            crate::ui::overlay::microphone_row(&self.microphone_state),
                        ),
                    )
                    .replacing(crate::ui::overlay::OverlayKey::MicrophoneToast),
                    config.overlay.clone(),
                );
            }
            crate::audio::AudioState::Active { .. } => {
                if show_unmute_feedback {
                    self.show_overlay_with_config(
                        crate::ui::overlay::OverlayRequest::toast(
                            crate::ui::overlay::OverlayKey::MicrophoneToast,
                            crate::ui::overlay::OverlayModel::single(
                                crate::ui::overlay::microphone_row(&self.microphone_state),
                            ),
                        )
                        .replacing(crate::ui::overlay::OverlayKey::MicrophonePermanent),
                        config.overlay.clone(),
                    );
                } else {
                    self.remove_overlay_key(
                        crate::ui::overlay::OverlayKey::MicrophonePermanent,
                        &config.overlay,
                    );
                }
            }
            crate::audio::AudioState::Unavailable { .. } => {
                self.remove_overlay_key(
                    crate::ui::overlay::OverlayKey::MicrophonePermanent,
                    &config.overlay,
                );
                if show_unmute_feedback {
                    self.show_overlay_with_config(
                        crate::ui::overlay::OverlayRequest::toast(
                            crate::ui::overlay::OverlayKey::MicrophoneToast,
                            crate::ui::overlay::OverlayModel::single(
                                crate::ui::overlay::microphone_row(&self.microphone_state),
                            ),
                        ),
                        config.overlay.clone(),
                    );
                }
            }
        }
    }

    fn show_preview_overlay(&mut self, config: crate::config::model::OverlayCfg) {
        let model =
            crate::ui::overlay::OverlayModel::preview(crate::ui::overlay::OverlayRow::preview(
                "WinShort overlay preview",
                "Previewing the current overlay settings",
            ));
        self.show_overlay_with_config(
            crate::ui::overlay::OverlayRequest::toast(
                crate::ui::overlay::OverlayKey::Preview,
                model,
            ),
            config,
        );
    }

    fn status_overlay_model(&self) -> crate::ui::overlay::OverlayModel {
        let mut rows = vec![
            crate::ui::overlay::microphone_row(&self.microphone_state),
            crate::ui::overlay::output_row(&self.output_state),
        ];
        if self.foreground_state.aggregate != crate::audio::Aggregate::NoExternalApp {
            rows.push(crate::ui::overlay::application_row(&self.foreground_state));
        }
        crate::ui::overlay::OverlayModel::from_rows(rows)
    }

    pub(super) fn hide_all_overlays_for_acceptance(&mut self) {
        self.status_request_id = None;
        if let Some(overlay) = &mut self.overlay {
            overlay.clear();
        }
        #[cfg(test)]
        {
            self.test_last_overlay_model = None;
        }
    }

    pub(super) fn show_deterministic_acceptance_overlay(&mut self) {
        self.acceptance_overlay_only = true;
        self.hide_all_overlays_for_acceptance();
        let config = crate::app::config();
        self.show_overlay_with_config(
            crate::ui::overlay::OverlayRequest::toast(
                crate::ui::overlay::OverlayKey::Status,
                self.status_overlay_model(),
            ),
            config.overlay.clone(),
        );
    }

    pub(super) fn show_deterministic_acceptance_multicard(&mut self) {
        self.acceptance_overlay_only = true;
        self.hide_all_overlays_for_acceptance();
        let config = crate::app::config().overlay.clone();
        let requests = [
            crate::ui::overlay::OverlayRequest::permanent(
                crate::ui::overlay::OverlayKey::MicrophonePermanent,
                crate::ui::overlay::OverlayModel::single(crate::ui::overlay::OverlayRow {
                    category: Some(crate::config::model::OverlayNotificationCategory::Microphone),
                    icon: crate::ui::overlay::OverlayIcon::Microphone,
                    tone: crate::ui::overlay::OverlayTone::Muted,
                    title: "Microphone muted".into(),
                    detail: "Acceptance permanent card".into(),
                }),
            ),
            crate::ui::overlay::OverlayRequest::toast(
                crate::ui::overlay::OverlayKey::Speaker,
                crate::ui::overlay::OverlayModel::single(crate::ui::overlay::OverlayRow {
                    category: Some(crate::config::model::OverlayNotificationCategory::Speaker),
                    icon: crate::ui::overlay::OverlayIcon::Output,
                    tone: crate::ui::overlay::OverlayTone::Changed,
                    title: "Speaker changed".into(),
                    detail: "Acceptance speaker toast".into(),
                }),
            ),
            crate::ui::overlay::OverlayRequest::toast(
                crate::ui::overlay::OverlayKey::Workspace,
                crate::ui::overlay::OverlayModel::single(crate::ui::overlay::OverlayRow {
                    category: Some(crate::config::model::OverlayNotificationCategory::Workspace),
                    icon: crate::ui::overlay::OverlayIcon::Workspace,
                    tone: crate::ui::overlay::OverlayTone::Changed,
                    title: "Workspace changed".into(),
                    detail: "Acceptance workspace toast".into(),
                }),
            ),
        ];
        let Some(overlay) = &mut self.overlay else {
            return;
        };
        for request in requests {
            if let Err(error) = overlay.present(request, config.clone()) {
                crate::error_!("multi-card acceptance overlay show failed: {error}");
                break;
            }
        }
    }

    pub(super) fn replace_deterministic_acceptance_speaker(&mut self) {
        self.acceptance_overlay_only = true;
        let config = crate::app::config().overlay.clone();
        let Some(overlay) = &mut self.overlay else {
            return;
        };
        if let Err(error) = overlay.present(
            crate::ui::overlay::OverlayRequest::toast(
                crate::ui::overlay::OverlayKey::Speaker,
                crate::ui::overlay::OverlayModel::single(crate::ui::overlay::OverlayRow {
                    category: Some(crate::config::model::OverlayNotificationCategory::Speaker),
                    icon: crate::ui::overlay::OverlayIcon::Output,
                    tone: crate::ui::overlay::OverlayTone::Changed,
                    title: "Speaker replaced".into(),
                    detail: "Acceptance replacement toast".into(),
                }),
            ),
            config,
        ) {
            crate::error_!("multi-card acceptance speaker replacement failed: {error}");
        }
    }

    fn show_status_overlay(&mut self) {
        if std::env::var_os("WINSHORT_UI_ACCEPTANCE").is_some() {
            self.show_deterministic_acceptance_overlay();
            return;
        }
        let request_id = self.next_audio_request_id();
        let pid = self
            .foreground
            .as_ref()
            .and_then(|tracker| tracker.target_pid());
        if self.audio.is_some() {
            self.select_foreground_audio(pid, false);
            self.status_request_id = Some(request_id);
            if let Some(audio) = &self.audio {
                audio.send(crate::audio::AudioCommand::QueryForeground { pid, request_id });
            }
        } else {
            self.status_request_id = None;
        }
        // Cached rows show immediately; a matching delayed query result
        // refreshes this same multi-row presentation (#18, #72).
        self.show_overlay(crate::ui::overlay::OverlayRequest::toast(
            crate::ui::overlay::OverlayKey::Status,
            self.status_overlay_model(),
        ));
    }
    /// Route a cross-thread transport event into one main-thread domain handler.
    pub(crate) fn route_event(&mut self, event: AppEvent) -> Option<DeferredShellAction> {
        if self.shutting_down {
            crate::log_debug!(
                "dropping {:?} during shutdown",
                std::mem::discriminant(&event)
            );
            return None;
        }
        match crate::event::RoutedAppEvent::from(event) {
            crate::event::RoutedAppEvent::ControlCenter(event) => {
                self.handle_control_center_event(event)
            }
            crate::event::RoutedAppEvent::Display(event) => {
                self.handle_display_event(event);
                None
            }
            crate::event::RoutedAppEvent::Diagnostics(event) => {
                self.handle_diagnostics_event(event)
            }
            crate::event::RoutedAppEvent::Overlay(event) => {
                self.handle_overlay_event(event);
                None
            }
            crate::event::RoutedAppEvent::Config(event) => {
                self.handle_config_event(event);
                None
            }
            crate::event::RoutedAppEvent::Desktop(event) => {
                self.handle_desktop_event(event);
                None
            }
            crate::event::RoutedAppEvent::Audio(event) => {
                self.handle_audio_event(event);
                None
            }
        }
    }

    pub(crate) fn diagnostics_snapshot(&self) -> crate::diagnostics::snapshot::DiagnosticsSnapshot {
        let audio_runtime = self
            .audio
            .as_ref()
            .map(crate::audio::AudioService::runtime_snapshot)
            .unwrap_or_default();
        let overlay_status = self
            .overlay
            .as_ref()
            .map(crate::ui::overlay::OverlayManager::status)
            .unwrap_or_default();
        crate::diagnostics::app_snapshot::build(crate::diagnostics::app_snapshot::SnapshotInputs {
            config: (*crate::app::config()).clone(),
            audio_runtime,
            microphone_state: self.microphone_state.clone(),
            output_state: self.output_state.clone(),
            foreground_state: self.foreground_state.clone(),
            keyboard_installed: self.keyboard.is_some(),
            keyboard_hook_active: crate::keyboard::hook::hook_active(),
            keyboard_capture_active: crate::keyboard::hook::capture_active(),
            suspended: self.suspended,
            desktop_installed: self.desktop.is_some(),
            desktop_status: self.desktop_status.clone(),
            overlay_status,
            degraded: self
                .degraded
                .iter()
                .map(|(name, reason)| ((*name).into(), reason.clone()))
                .collect(),
        })
    }
    fn begin_shutdown(&mut self) -> bool {
        if self.shutting_down {
            return true;
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), DISPLAY_ROLLBACK_TIMER_ID);
        }
        if let Some(mut pending) = self.display_rollback.take() {
            pending.state = crate::display::transition_confirmation(
                pending.state,
                crate::display::ConfirmationIntent::Revert,
            );
            match crate::display::rollback(&pending.rollback) {
                Ok(()) => {
                    self.display_rollback_error = None;
                }
                Err(error) => {
                    let reason = error.to_string();
                    pending.state = crate::display::transition_confirmation(
                        pending.state,
                        crate::display::ConfirmationIntent::RollbackFailed,
                    );
                    self.display_rollback_error = Some(reason.clone());
                    self.display_rollback = Some(pending);
                    self.sync_display_settings_state();
                    crate::error_!("display rollback during shutdown failed: {reason}");
                    self.show_display_feedback(
                        crate::ui::overlay::OverlayTone::Unavailable,
                        "Shutdown cancelled",
                        "Display recovery failed. Retry Revert in Displays",
                    );
                    return false;
                }
            }
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
            if join.join().is_err() {
                crate::error_!("support bundle worker panicked during shutdown");
            }
        }
        if let Some(diagnostics) = self.diagnostics.take() {
            unsafe {
                let _ = DestroyWindow(diagnostics.hwnd);
            }
        }
        if let Some(overlay) = self.overlay.take() {
            let mut overlay = overlay;
            overlay.shutdown();
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
            if windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(self.hwnd),
                windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            )
            .is_err()
            {
                crate::warn_!("failed to post main-window close after shutdown cleanup");
            }
        }
        true
    }
}

/// State snapshot for tray menu checkmarks.
fn tray_menu_state(app: &App) -> tray_menu::MenuState {
    tray_menu::MenuState {
        suspended: app.is_suspended(),
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

        event::WM_APP_UI_ACCEPTANCE_SHOW_DETERMINISTIC_OVERLAY => {
            if std::env::var_os("WINSHORT_UI_ACCEPTANCE").is_some() {
                with_app(App::show_deterministic_acceptance_overlay);
            }
            LRESULT(0)
        }

        event::WM_APP_UI_ACCEPTANCE_HIDE_ALL_OVERLAYS => {
            if std::env::var_os("WINSHORT_UI_ACCEPTANCE").is_some() {
                with_app(App::hide_all_overlays_for_acceptance);
            }
            LRESULT(0)
        }

        event::WM_APP_UI_ACCEPTANCE_SHOW_MULTI_OVERLAY => {
            if std::env::var_os("WINSHORT_UI_ACCEPTANCE").is_some() {
                with_app(App::show_deterministic_acceptance_multicard);
            }
            LRESULT(0)
        }

        event::WM_APP_UI_ACCEPTANCE_REPLACE_SPEAKER_OVERLAY => {
            if std::env::var_os("WINSHORT_UI_ACCEPTANCE").is_some() {
                with_app(App::replace_deterministic_acceptance_speaker);
            }
            LRESULT(0)
        }

        WM_APP_EVENT => {
            for ev in event::EVENTS.get_or_init(event::EventQueue::new).drain() {
                dispatch_main_event(ev);
            }
            LRESULT(0)
        }

        WM_TIMER if wparam.0 == crate::diagnostics::logging::FLUSH_TIMER_ID => {
            crate::diagnostics::logging::flush_if_dirty();
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == DISPLAY_ROLLBACK_TIMER_ID => {
            with_app(App::expire_display_rollback);
            LRESULT(0)
        }
        WM_CLOSE => {
            // Idempotent ordered teardown, then destroy. Posted by
            // begin_shutdown; external close requests land here too.
            let should_destroy = with_app(App::begin_shutdown).unwrap_or(true);
            if should_destroy {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
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

/// Route one event under the App borrow, then execute any native shell action
/// only after that borrow has ended. A completion event is routed through a
/// fresh App borrow after the reentrant native call returns.
fn dispatch_main_event(event: AppEvent) {
    let action = with_app(|app| app.route_event(event)).flatten();
    let Some(action) = action else {
        return;
    };
    let Some(completion) = execute_deferred_shell_action(action) else {
        return;
    };
    with_app(|app| {
        let _ = app.route_event(completion);
    });
}

fn execute_deferred_shell_action(action: DeferredShellAction) -> Option<AppEvent> {
    match action {
        DeferredShellAction::OpenConfigFolder { directory } => {
            if let Err(error) = crate::platform::shell::open_folder(&directory) {
                crate::error_!("open config folder failed: {error}");
            }
            None
        }
        DeferredShellAction::OpenDiagnosticsLogs { directory } => {
            let error = crate::platform::shell::open_folder(&directory)
                .map_err(|error| {
                    crate::error::Error::config(format!("open log directory: {error}"))
                })
                .err()
                .map(|error| error.to_string());
            Some(AppEvent::DiagnosticsLogsOpenFinished { error })
        }
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

fn apply_menu_command(app: &mut App, command: Option<tray_menu::Command>) {
    use tray_menu::Command;
    match command {
        Some(Command::OpenSettings) => app.show_settings(),
        Some(Command::ShowStatus) => {
            let _ = app.route_event(AppEvent::ShowStatusOverlay);
        }
        Some(Command::PauseShortcuts) => app.toggle_suspended(),
        Some(Command::Diagnostics) => app.show_diagnostics(),
        Some(Command::Exit) => {
            let _ = app.begin_shutdown();
        }
        None => {}
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
            display_rollback: None,
            display_rollback_error: None,
            desktop: None,
            desktop_status: crate::desktop::BackendStatus {
                native: crate::desktop::BackendAvailability::Failed {
                    reason: "test".into(),
                },
                fallback: crate::desktop::BackendAvailability::Available,
                active: crate::desktop::BackendKind::KeyboardFallback,
                desktop_count: None,
                current_desktop: None,
                last_served: None,
            },
            microphone_state: crate::audio::AudioState::Unavailable {
                reason: "test".into(),
            },
            output_state: crate::audio::OutputState::Unavailable {
                reason: "test".into(),
            },
            foreground_state: crate::audio::AppAudioState::no_external(),
            foreground_pid: None,
            foreground_query_id: None,
            foreground_request_floor: 0,
            microphone_seen: false,
            output_seen: false,
            foreground_seen: false,
            next_audio_request_id: 0,
            status_request_id: None,
            acceptance_overlay_only: false,
            suspended: false,
            shutting_down: false,
            support_bundle: None,
            degraded: Vec::new(),
            test_last_overlay_model: None,
        }
    }

    #[test]
    fn control_center_close_event_hides_without_application_shutdown() {
        let mut app = test_app();
        app.route_event(AppEvent::ControlCenterWindowClosed);
        assert!(!app.shutting_down);
        assert!(app.settings.is_none());
    }

    #[test]
    fn config_folder_event_prepares_a_deferred_shell_action() {
        let mut app = test_app();
        assert!(matches!(
            app.route_event(AppEvent::OpenConfigFolder),
            Some(DeferredShellAction::OpenConfigFolder { .. })
        ));
    }

    #[test]
    fn diagnostics_open_logs_uses_the_same_deferred_shell_action_boundary() {
        let mut app = test_app();
        assert!(matches!(
            app.route_event(AppEvent::OpenDiagnosticsLogs),
            Some(DeferredShellAction::OpenDiagnosticsLogs { .. })
        ));
    }

    #[test]
    fn shell_execution_stays_outside_ui_and_app_domain_handlers() {
        let app_source = include_str!("app.rs");
        let app_domain_source = include_str!("app/event_router.rs");
        let settings_activation_source = include_str!("ui/control_center/commands.rs");
        let diagnostics_activation_source = include_str!("ui/diagnostics/interaction.rs");
        let executor = app_source
            .split("fn execute_deferred_shell_action(")
            .nth(1)
            .and_then(|source| source.split("unsafe fn handle_tray").next())
            .expect("deferred shell executor must remain a distinct function");

        assert!(
            app_source.contains("let action = with_app(|app| app.route_event(event)).flatten();")
        );
        assert!(executor.contains("platform::shell::open_folder"));
        assert!(!app_domain_source.contains("execute_deferred_shell_action"));
        assert!(!app_domain_source.contains("platform::shell::open_folder"));
        assert!(!settings_activation_source.contains("execute_deferred_shell_action"));
        assert!(!settings_activation_source.contains("platform::shell::open_folder"));
        assert!(!diagnostics_activation_source.contains("execute_deferred_shell_action"));
        assert!(!diagnostics_activation_source.contains("platform::shell::open_folder"));
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
    fn system_endpoint_events_refresh_state_and_keep_permanent_mic_indicator() {
        let mut app = test_app();
        let microphone = crate::audio::AudioState::Muted { volume_pct: 20 };
        let output = crate::audio::OutputState::Current {
            device: crate::audio::DeviceId {
                endpoint: "output".into(),
                name: "Speakers".into(),
            },
            muted: false,
            volume_pct: 80,
        };

        app.route_event(AppEvent::MicrophoneStateChanged {
            state: microphone.clone(),
            origin: AudioEventOrigin::External,
        });
        app.route_event(AppEvent::OutputStateChanged {
            state: output.clone(),
            origin: AudioEventOrigin::External,
        });
        app.route_event(AppEvent::DefaultOutputChanged(crate::audio::DeviceId {
            endpoint: "new-output".into(),
            name: "New speakers".into(),
        }));

        assert_eq!(app.microphone_state, microphone);
        assert_eq!(app.output_state, output);
        assert!(app.microphone_seen);
        assert!(app.output_seen);
        assert!(app.overlay.is_none());
        let model = app
            .test_last_overlay_model
            .take()
            .expect("muted runtime state should restore the permanent microphone card");
        assert_eq!(model.rows[0].title, "Microphone muted");
    }

    #[test]
    fn winshort_microphone_action_shows_one_microphone_overlay() {
        let mut app = test_app();
        let state = crate::audio::AudioState::Muted { volume_pct: 20 };

        app.route_event(AppEvent::MicrophoneStateChanged {
            state: state.clone(),
            origin: AudioEventOrigin::WinShortAction(41),
        });

        assert_eq!(app.microphone_state, state);
        assert!(app.microphone_seen);
        let model = app
            .test_last_overlay_model
            .take()
            .expect("microphone action should show an overlay");
        assert_eq!(model.rows.len(), 1);
        assert_eq!(
            model.rows[0].icon,
            crate::ui::overlay::OverlayIcon::Microphone
        );
        assert_eq!(
            model.rows[0].category,
            Some(crate::config::model::OverlayNotificationCategory::Microphone)
        );
        assert_eq!(model.rows[0].title, "Microphone muted");
    }

    #[test]
    fn winshort_microphone_unmute_action_is_transient_and_explicit() {
        let mut app = test_app();
        app.route_event(AppEvent::MicrophoneStateChanged {
            state: crate::audio::AudioState::Muted { volume_pct: 20 },
            origin: AudioEventOrigin::WinShortAction(44),
        });
        app.test_last_overlay_model = None;

        app.route_event(AppEvent::MicrophoneStateChanged {
            state: crate::audio::AudioState::Active { volume_pct: 20 },
            origin: AudioEventOrigin::WinShortAction(45),
        });

        let model = app
            .test_last_overlay_model
            .take()
            .expect("microphone unmute action should show an overlay");
        assert_eq!(model.rows[0].title, "Microphone unmuted");
    }

    #[test]
    fn winshort_output_action_shows_one_speaker_overlay() {
        let mut app = test_app();
        let state = crate::audio::OutputState::Current {
            device: crate::audio::DeviceId {
                endpoint: "output".into(),
                name: "Speakers".into(),
            },
            muted: true,
            volume_pct: 80,
        };

        app.route_event(AppEvent::OutputStateChanged {
            state: state.clone(),
            origin: AudioEventOrigin::WinShortAction(42),
        });

        assert_eq!(app.output_state, state);
        assert!(app.output_seen);
        let model = app
            .test_last_overlay_model
            .take()
            .expect("output action should show an overlay");
        assert_eq!(model.rows.len(), 1);
        assert_eq!(model.rows[0].icon, crate::ui::overlay::OverlayIcon::Output);
        assert_eq!(
            model.rows[0].category,
            Some(crate::config::model::OverlayNotificationCategory::Speaker)
        );
    }

    #[test]
    fn already_selected_device_does_not_emit_a_changed_overlay() {
        let mut app = test_app();
        app.handle_device_cycle_result(
            99,
            crate::audio::DeviceCycleResult::AlreadySelected {
                flow: crate::audio::DeviceCycleFlow::Output,
                device: crate::audio::DeviceId {
                    endpoint: "output".into(),
                    name: "Speakers".into(),
                },
            },
        );
        assert!(app.test_last_overlay_model.is_none());
    }

    #[test]
    fn failed_device_change_emits_unavailable_feedback() {
        let mut app = test_app();
        app.handle_device_cycle_result(
            100,
            crate::audio::DeviceCycleResult::Failed {
                flow: crate::audio::DeviceCycleFlow::Input,
                previous: None,
                target: None,
                error: "setter failed".into(),
            },
        );
        let model = app
            .test_last_overlay_model
            .take()
            .expect("failed device changes should show feedback");
        assert_eq!(model.rows[0].title, "Next microphone unavailable");
    }

    #[test]
    fn external_audio_state_refreshes_without_an_overlay() {
        let mut app = test_app();

        app.route_event(AppEvent::MicrophoneStateChanged {
            state: crate::audio::AudioState::Active { volume_pct: 55 },
            origin: AudioEventOrigin::External,
        });
        assert!(app.microphone_seen);
        assert!(app.test_last_overlay_model.is_none());

        app.route_event(AppEvent::OutputStateChanged {
            state: crate::audio::OutputState::Unavailable {
                reason: "endpoint refresh".into(),
            },
            origin: AudioEventOrigin::External,
        });
        assert!(app.output_seen);
        assert!(app.test_last_overlay_model.is_none());
    }

    #[test]
    fn external_refresh_after_winshort_action_does_not_duplicate_overlay() {
        let mut app = test_app();
        let action_state = crate::audio::AudioState::Muted { volume_pct: 30 };
        app.route_event(AppEvent::MicrophoneStateChanged {
            state: action_state.clone(),
            origin: AudioEventOrigin::WinShortAction(43),
        });
        assert!(app.test_last_overlay_model.is_some());

        app.test_last_overlay_model = None;
        app.route_event(AppEvent::MicrophoneStateChanged {
            state: crate::audio::AudioState::Active { volume_pct: 30 },
            origin: AudioEventOrigin::External,
        });
        assert_eq!(
            app.microphone_state,
            crate::audio::AudioState::Active { volume_pct: 30 }
        );
        assert!(app.test_last_overlay_model.is_none());
    }

    #[test]
    fn degraded_microphone_toggle_keeps_action_origin_for_unavailable_overlay() {
        let mut app = test_app();
        app.dispatch_action(HotkeyAction::ToggleMicrophone);

        assert!(matches!(
            app.microphone_state,
            crate::audio::AudioState::Unavailable { .. }
        ));
        let model = app
            .test_last_overlay_model
            .take()
            .expect("degraded microphone toggle should show an overlay");
        assert_eq!(
            model.rows[0].icon,
            crate::ui::overlay::OverlayIcon::Microphone
        );
        assert_eq!(
            model.rows[0].tone,
            crate::ui::overlay::OverlayTone::Unavailable
        );
    }

    #[test]
    fn degraded_output_toggle_keeps_action_origin_for_unavailable_overlay() {
        let mut app = test_app();
        app.dispatch_action(HotkeyAction::ToggleOutput);

        assert!(matches!(
            app.output_state,
            crate::audio::OutputState::Unavailable { .. }
        ));
        let model = app
            .test_last_overlay_model
            .take()
            .expect("degraded output toggle should show an overlay");
        assert_eq!(model.rows[0].icon, crate::ui::overlay::OverlayIcon::Output);
        assert_eq!(
            model.rows[0].tone,
            crate::ui::overlay::OverlayTone::Unavailable
        );
    }

    #[test]
    fn global_overlay_switch_still_blocks_audio_action_overlay() {
        let mut app = test_app();
        let mut config = crate::config::Config::default().overlay;
        config.enabled = false;
        for state in [
            crate::audio::AudioState::Muted { volume_pct: 10 },
            crate::audio::AudioState::Active { volume_pct: 10 },
        ] {
            app.show_overlay_with_config(
                crate::ui::overlay::OverlayRequest::permanent(
                    crate::ui::overlay::OverlayKey::MicrophonePermanent,
                    crate::ui::overlay::OverlayModel::single(crate::ui::overlay::microphone_row(
                        &state,
                    )),
                ),
                config.clone(),
            );
            assert!(app.test_last_overlay_model.is_none());
        }
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
    fn legacy_external_policy_keeps_action_and_status_current_app_feedback() {
        let mut app = test_app();
        let mut overlay = crate::config::Config::default().overlay;
        overlay.notifications.current_app_audio = true;
        overlay.notifications.external_current_app_audio = false;

        assert!(!App::should_show_audio_overlay(
            AudioEventOrigin::External,
            true,
            true,
            overlay.notifications.external_current_app_audio,
            false,
        ));
        assert!(App::should_show_audio_overlay(
            AudioEventOrigin::WinShortAction(7),
            true,
            true,
            overlay.notifications.external_current_app_audio,
            false,
        ));

        app.show_overlay_with_config(
            crate::ui::overlay::OverlayRequest::toast(
                crate::ui::overlay::OverlayKey::CurrentAppAudio,
                crate::ui::overlay::OverlayModel::single(crate::ui::overlay::OverlayRow {
                    category: Some(
                        crate::config::model::OverlayNotificationCategory::CurrentAppAudio,
                    ),
                    icon: crate::ui::overlay::OverlayIcon::Application,
                    tone: crate::ui::overlay::OverlayTone::Changed,
                    title: "Current app changed".into(),
                    detail: "WinShort action feedback".into(),
                }),
            ),
            overlay.clone(),
        );
        let action_model = app
            .test_last_overlay_model
            .take()
            .expect("legacy external policy must not suppress WinShort action feedback");
        assert_eq!(action_model.rows.len(), 1);

        app.foreground_state = crate::audio::AppAudioState {
            app_name: Some("Test app".into()),
            aggregate: crate::audio::Aggregate::AllActive,
            sessions: 1,
            error: None,
        };
        let status_model = app
            .status_overlay_model()
            .filter_enabled(overlay.notifications);
        assert!(status_model.rows.iter().any(|row| {
            row.category == Some(crate::config::model::OverlayNotificationCategory::CurrentAppAudio)
        }));
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
        draft.overlay.blur = crate::config::model::OverlayBlur::BlurLight;
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
        assert_eq!(
            saved.overlay.blur,
            crate::config::model::OverlayBlur::BlurMedium
        );
    }

    #[test]
    fn matching_status_request_refreshes_multi_row_presentation() {
        let mut app = test_app();
        app.status_request_id = Some(7);
        app.route_event(AppEvent::ForegroundAudioChanged {
            pid: None,
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

    fn app_audio_state(aggregate: crate::audio::Aggregate) -> crate::audio::AppAudioState {
        crate::audio::AppAudioState {
            app_name: Some("Player".into()),
            aggregate,
            sessions: 1,
            error: None,
        }
    }

    #[test]
    fn selected_app_mute_and_unmute_feedback_are_explicit() {
        let mut app = test_app();
        app.select_foreground_audio(Some(100), false);
        for (aggregate, title) in [
            (crate::audio::Aggregate::AllMuted, "App audio muted"),
            (crate::audio::Aggregate::AllActive, "App audio unmuted"),
        ] {
            app.route_event(AppEvent::ForegroundAudioChanged {
                pid: Some(100),
                state: app_audio_state(aggregate),
                origin: AudioEventOrigin::WinShortAction(1),
            });
            let model = app.test_last_overlay_model.take().unwrap();
            assert_eq!(model.rows[0].title, title);
            assert!(model.rows[0].detail.contains("Player"));
        }
    }

    #[test]
    fn changing_selected_app_drops_previous_state_and_delayed_results() {
        let mut app = test_app();
        app.select_foreground_audio(Some(100), false);
        app.foreground_state = app_audio_state(crate::audio::Aggregate::AllMuted);
        app.foreground_seen = true;
        app.status_request_id = Some(8);
        app.select_foreground_audio(Some(200), false);
        assert_eq!(
            app.foreground_state,
            crate::audio::AppAudioState::no_external()
        );
        assert!(!app.foreground_seen);
        assert_eq!(app.status_request_id, None);
        for origin in [
            AudioEventOrigin::WinShortAction(1),
            AudioEventOrigin::StatusRequest(8),
        ] {
            app.route_event(AppEvent::ForegroundAudioChanged {
                pid: Some(100),
                state: app_audio_state(crate::audio::Aggregate::AllMuted),
                origin,
            });
        }
        app.route_event(AppEvent::ForegroundVolumeChanged {
            pid: Some(100),
            state: crate::audio::AppVolumeState::no_external(),
            origin: AudioEventOrigin::WinShortAction(2),
        });
        assert!(!app.foreground_seen);
        assert!(app.test_last_overlay_model.is_none());
    }

    #[test]
    fn passive_selection_restores_muted_badge_without_an_unmute_toast() {
        let mut app = test_app();
        app.select_foreground_audio(Some(100), false);
        app.foreground_query_id = Some(7);
        app.route_event(AppEvent::ForegroundAudioChanged {
            pid: Some(100),
            state: app_audio_state(crate::audio::Aggregate::AllMuted),
            origin: AudioEventOrigin::ForegroundSelection(6),
        });
        assert!(!app.foreground_seen);
        assert!(app.test_last_overlay_model.is_none());
        app.route_event(AppEvent::ForegroundAudioChanged {
            pid: Some(100),
            state: app_audio_state(crate::audio::Aggregate::AllMuted),
            origin: AudioEventOrigin::ForegroundSelection(7),
        });
        assert_eq!(
            app.test_last_overlay_model.take().unwrap().rows[0].title,
            "App audio muted"
        );
        app.foreground_query_id = Some(8);
        app.status_request_id = Some(9);
        app.route_event(AppEvent::ForegroundAudioChanged {
            pid: Some(100),
            state: app_audio_state(crate::audio::Aggregate::AllActive),
            origin: AudioEventOrigin::ForegroundSelection(8),
        });
        assert!(app.test_last_overlay_model.is_none());
        assert_eq!(app.status_request_id, Some(9));
    }

    #[test]
    fn action_invalidates_older_selection_result() {
        let mut app = test_app();
        app.select_foreground_audio(Some(100), false);
        app.foreground_query_id = Some(5);
        app.route_event(AppEvent::ForegroundAudioChanged {
            pid: Some(100),
            state: app_audio_state(crate::audio::Aggregate::AllActive),
            origin: AudioEventOrigin::WinShortAction(6),
        });
        app.test_last_overlay_model = None;
        app.route_event(AppEvent::ForegroundAudioChanged {
            pid: Some(100),
            state: app_audio_state(crate::audio::Aggregate::AllMuted),
            origin: AudioEventOrigin::ForegroundSelection(5),
        });
        assert_eq!(
            app.foreground_state.aggregate,
            crate::audio::Aggregate::AllActive
        );
        assert!(app.test_last_overlay_model.is_none());
    }

    #[test]
    fn returning_to_same_app_does_not_revive_a_delayed_action() {
        let mut app = test_app();
        app.select_foreground_audio(Some(100), false);
        app.next_audio_request_id = 10;
        app.select_foreground_audio(Some(200), false);
        app.next_audio_request_id = 11;
        app.select_foreground_audio(Some(100), false);
        app.route_event(AppEvent::ForegroundAudioChanged {
            pid: Some(100),
            state: app_audio_state(crate::audio::Aggregate::AllMuted),
            origin: AudioEventOrigin::WinShortAction(10),
        });
        assert!(!app.foreground_seen);
        assert!(app.test_last_overlay_model.is_none());
    }

    #[test]
    fn status_overlay_snapshot_includes_microphone_while_muted() {
        let mut app = test_app();
        app.microphone_state = crate::audio::AudioState::Muted { volume_pct: 20 };

        let model = app.status_overlay_model();

        assert_eq!(model.rows.len(), 2);
    }

    #[test]
    fn stale_status_request_has_no_observable_effect() {
        let mut app = test_app();
        app.status_request_id = Some(8);
        let before = app.foreground_state.clone();
        let before_seen = app.foreground_seen;
        app.route_event(AppEvent::ForegroundAudioChanged {
            pid: None,
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
            pid: None,
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
