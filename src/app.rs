//! Application runtime: hidden main window, tray integration, event routing,
//! startup/shutdown orchestration (spec §5–§7, §46–§47).

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, RegisterWindowMessageW,
    WINDOW_STYLE, WM_CLOSE, WM_DESTROY,
};
use windows::core::{HSTRING, PCWSTR};

use crate::error::{Error, Result};
use crate::event::{self, AppEvent, HotkeyAction, WM_APP_EVENT};
use crate::platform::window as win;
use crate::tray::{menu as tray_menu, Tray, TrayEvent, TrayState};
use crate::ui::settings::SettingsWindow;


pub static CONFIG: std::sync::OnceLock<std::sync::Arc<crate::config::ConfigHandle>> =
    std::sync::OnceLock::new();

/// Access the live config snapshot from any thread.
pub fn config() -> std::sync::Arc<crate::config::Config> {
    CONFIG
        .get()
        .map(|h| h.get())
        .unwrap_or_else(|| std::sync::Arc::new(crate::config::Config::default()))
}

pub const CLASS_NAME: &str = "WinShort.Main";

/// Registered message broadcast when Explorer restarts (spec §6).
pub fn taskbar_created_msg() -> u32 {
    static MSG: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *MSG.get_or_init(|| unsafe {
        RegisterWindowMessageW(PCWSTR(HSTRING::from("TaskbarCreated").as_ptr()))
    })
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingOverlay {
    Microphone,
    Output,
    Foreground,
}
pub struct App {
    pub hwnd: HWND,
    tray: Option<Tray>,
    settings: Option<SettingsWindow>,
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
    pending_overlay: Option<PendingOverlay>,
    suspended: bool,
    shutting_down: bool,
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
        win::register_class::<App>(CLASS_NAME, Some(main_wndproc))?;

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
        let _ = MAIN_HWND.set(hwnd.0 as isize);
        set_app(App {
            hwnd,
            tray: None,
            settings: None,
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
            pending_overlay: None,
            suspended: false,
            shutting_down: false,
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

    pub fn desktop_status(&self) -> crate::desktop::BackendStatus {
        self.desktop_status.clone()
    }

    pub fn audio_devices(&self) -> crate::audio::devices::DeviceLists {
        self.audio
            .as_ref()
            .map(crate::audio::AudioService::devices)
            .unwrap_or_default()
    }

    fn ensure_settings(&mut self) -> Result<&mut SettingsWindow> {
        if self.settings.is_none() {
            self.settings = Some(SettingsWindow::create()?);
            info!("settings window created");
        }
        Ok(self.settings.as_mut().expect("just created"))
    }

    pub fn show_settings(&mut self) {
        match self.ensure_settings() {
            Ok(s) => {
                if let Err(e) = s.show() {
                    error_!("show settings failed: {e}");
                }
            }
            Err(e) => error_!("create settings failed: {e}"),
        }
    }

    pub fn toggle_suspended(&mut self) {
        self.suspended = !self.suspended;
        if let Some(keyboard) = &self.keyboard {
            keyboard.set_suspended(self.suspended);
        }
        let state = if self.suspended { TrayState::HotkeysSuspended } else { TrayState::Normal };
        if let Some(t) = &mut self.tray {
            if let Err(e) = t.set_state(state) {
                error_!("tray state update failed: {e}");
            }
        }
        info!("hotkeys {}", if self.suspended { "suspended" } else { "resumed" });
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
        if self.suspended {
            return;
        }
        match action {
            HotkeyAction::ToggleMicrophone => {
                self.pending_overlay = Some(PendingOverlay::Microphone);
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ToggleMicrophone);
                }
            }
            HotkeyAction::ToggleOutput => {
                self.pending_overlay = Some(PendingOverlay::Output);
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ToggleOutput);
                }
            }
            HotkeyAction::ToggleForegroundAppAudio => {
                self.pending_overlay = Some(PendingOverlay::Foreground);
                let pid = self.foreground.as_ref().and_then(|tracker| tracker.target_pid());
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ToggleForeground(pid));
                }
            }
            HotkeyAction::SwitchDesktop(n) => {
                if let Some(desktop) = &self.desktop {
                    desktop.switch_to(n as usize);
                }
            }
        }
    }

    fn show_overlay_model(&mut self, model: crate::ui::overlay::OverlayModel) {
        let config = crate::app::config();
        if !config.overlay.enabled {
            return;
        }
        if let Some(overlay) = &self.overlay {
            if let Err(e) = overlay.show(model, config.overlay) {
                error_!("overlay show failed: {e}");
            }
        }
    }

    fn show_microphone_overlay(&mut self) {
        let row = crate::ui::overlay::microphone_row(&self.microphone_state);
        self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));
    }

    fn show_output_overlay(&mut self) {
        let row = crate::ui::overlay::output_row(&self.output_state);
        self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));
    }

    fn show_status_overlay(&mut self) {
        let mut rows = vec![
            crate::ui::overlay::microphone_row(&self.microphone_state),
            crate::ui::overlay::output_row(&self.output_state),
        ];
        if self.foreground_state.aggregate != crate::audio::Aggregate::NoExternalApp {
            rows.push(crate::ui::overlay::application_row(&self.foreground_state));
        }
        self.show_overlay_model(crate::ui::overlay::OverlayModel { rows });
    }
    /// Route an event posted from any thread.
    pub fn route_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::ShowSettings => self.show_settings(),
            AppEvent::ShowStatusOverlay => self.show_status_overlay(),
            AppEvent::Exit => self.begin_shutdown(),
            AppEvent::ConfigApplied(seq) => {
                let config = crate::app::config();
                self.set_suspended(!config.general.start_hotkeys_enabled);
                if let Some(audio) = &self.audio {
                    audio.send(crate::audio::AudioCommand::ConfigChanged);
                }
                info!("config applied (seq {seq})");
            }
            AppEvent::MicrophoneStateChanged(state) => {
                let changed = self.microphone_state != state;
                let should_show = self.pending_overlay == Some(PendingOverlay::Microphone)
                    || (self.microphone_seen && changed);
                self.microphone_state = state;
                self.microphone_seen = true;
                if should_show {
                    self.pending_overlay = None;
                    self.show_microphone_overlay();
                }
            }
            AppEvent::OutputStateChanged(state) => {
                let changed = self.output_state != state;
                let should_show = self.pending_overlay == Some(PendingOverlay::Output)
                    || (self.output_seen && changed);
                self.output_state = state;
                self.output_seen = true;
                if should_show {
                    self.pending_overlay = None;
                    self.show_output_overlay();
                }
            }
            AppEvent::DevicesChanged => {
                if let Some(settings) = &self.settings {
                    settings.refresh();
                }
            }
            AppEvent::ForegroundAudioChanged(state) => {
                self.foreground_state = state;
                self.pending_overlay = None;
                let row = crate::ui::overlay::application_row(&self.foreground_state);
                self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));
            }
            AppEvent::DesktopBackendChanged(status) => {
                self.desktop_status = status;
            }
            _ => { /* later phases */ }
        }
    }
    fn begin_shutdown(&mut self) {
        if self.shutting_down {
            return;
        }
        self.shutting_down = true;

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
        if let Some(overlay) = self.overlay.take() {
            overlay.hide();
            unsafe {
                let _ = DestroyWindow(overlay.hwnd);
            }
        }

        if let Some(t) = self.tray.take() {
            t.remove();
        }
        if let Some(s) = self.settings.take() {
            unsafe {
                let _ = DestroyWindow(s.hwnd);
            }
        }
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
            handle_tray(wparam, lparam);
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

        WM_CLOSE => {
            // Idempotent ordered teardown, then destroy. Posted by
            // begin_shutdown; external close requests land here too.
            with_app(App::begin_shutdown);
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            crate::platform::message_loop::quit(0);
            LRESULT(DefWindowProcW(hwnd, msg, wparam, lparam).0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
unsafe fn handle_tray(wparam: WPARAM, lparam: LPARAM) {
    match crate::tray::decode_callback(wparam, lparam) {
        TrayEvent::DoubleClick { .. } => {
            with_app(|app| app.show_settings());
        }
        TrayEvent::ContextMenu { x, y } => {
            // TrackPopupMenu runs a modal dispatch loop; never hold the App
            // borrow across it: snapshot state, run the menu, re-borrow to apply.
            let Some((hwnd, state)) = with_app(|app| (app.hwnd, tray_menu_state(app))) else {
                return;
            };
            let cmd = tray_menu::track_tray_menu(hwnd, POINT { x, y }, &state);
            with_app(|app| apply_menu_command(app, cmd));
        }
        TrayEvent::Select { .. } => {}
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
