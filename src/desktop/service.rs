//! STA-owned virtual desktop controller: build-pinned Shell backend with
//! automatic best-effort keyboard fallback.

use std::sync::mpsc::{self, Sender};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GWL_STYLE,
    SW_HIDE, SW_RESTORE, SW_SHOWNA, WS_DISABLED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows_core::GUID;

use crate::desktop::backend::{
    BackendAvailability, BackendKind, BackendStatus, DesktopError, VirtualDesktopBackend,
};
use crate::desktop::detect::{detect, OsBuild};
use crate::desktop::internal_api::InternalBackend;
use crate::desktop::keyboard_fallback::KeyboardFallback;
use crate::desktop::state::DesktopHistory;
use crate::error::{Error, Result};
use crate::event::AppEvent;

pub enum DesktopCommand {
    SwitchTo(usize),
    MoveForeground {
        index: usize,
        hwnd_raw: isize,
        follow: bool,
    },
    SwitchPrevious,
    ForegroundChanged {
        hwnd_raw: isize,
    },
    ConfigureScratchpad {
        managed: bool,
    },
    AssignScratchpad {
        hwnd_raw: isize,
    },
    ToggleScratchpad,
    Shutdown,
}

pub struct DesktopService {
    sender: Sender<DesktopCommand>,
    join: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy)]
struct ScratchpadState {
    hwnd: HWND,
    owner_pid: u32,
    /// True only when WinShort itself issued the successful hide operation.
    hidden_by_winshort: bool,
}

impl DesktopService {
    pub fn start(main_hwnd: windows::Win32::Foundation::HWND) -> Result<Self> {
        let hwnd_raw = main_hwnd.0 as isize;
        let (sender, receiver) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("winshort-desktop".into())
            .spawn(move || desktop_thread(hwnd_raw, receiver, ready_tx))
            .map_err(|e| Error::internal(format!("spawn desktop thread: {e}")))?;
        let ready = match ready_rx.recv_timeout(std::time::Duration::from_secs(8)) {
            Ok(result) => result,
            Err(_) => Err(Error::desktop("desktop startup timed out")),
        };
        if let Err(error) = ready {
            let _ = sender.send(DesktopCommand::Shutdown);
            let _ = join.join();
            return Err(error);
        }
        Ok(Self {
            sender,
            join: Some(join),
        })
    }

    pub fn move_foreground_to(&self, index: usize, hwnd_raw: isize, follow: bool) {
        let _ = self.sender.send(DesktopCommand::MoveForeground {
            index,
            hwnd_raw,
            follow,
        });
    }

    pub fn switch_previous(&self) {
        let _ = self.sender.send(DesktopCommand::SwitchPrevious);
    }

    pub fn foreground_changed(&self, hwnd_raw: isize) {
        let _ = self
            .sender
            .send(DesktopCommand::ForegroundChanged { hwnd_raw });
    }

    pub fn configure_scratchpad(&self, managed: bool) {
        let _ = self
            .sender
            .send(DesktopCommand::ConfigureScratchpad { managed });
    }

    pub fn switch_to(&self, index: usize) {
        let _ = self.sender.send(DesktopCommand::SwitchTo(index));
    }

    pub fn assign_scratchpad(&self, hwnd_raw: isize) {
        let _ = self
            .sender
            .send(DesktopCommand::AssignScratchpad { hwnd_raw });
    }

    pub fn toggle_scratchpad(&self) {
        let _ = self.sender.send(DesktopCommand::ToggleScratchpad);
    }

    pub fn shutdown(&mut self) {
        let _ = self.sender.send(DesktopCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for DesktopService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct DesktopController {
    hwnd_raw: isize,
    build: OsBuild,
    native: Option<InternalBackend>,
    native_availability: BackendAvailability,
    fallback: KeyboardFallback,
    known_count: Option<usize>,
    /// Backend that actually completed the last switch (#21).
    last_served: Option<BackendKind>,
    /// Process-lifetime focus and previous-desktop identity state.
    history: DesktopHistory,
    /// Runtime-only scratchpad ownership; never persisted.
    scratchpad: Option<ScratchpadState>,
}

impl DesktopController {
    fn create(hwnd_raw: isize) -> Result<Self> {
        let build = detect()?;
        let (native, native_availability) = if build.native_shell_supported() {
            match InternalBackend::create(build) {
                Ok(backend) => (Some(backend), BackendAvailability::Available),
                Err(e) => (
                    None,
                    BackendAvailability::Failed {
                        reason: e.to_string(),
                    },
                ),
            }
        } else {
            (
                None,
                BackendAvailability::UnsupportedBuild { build: build.build },
            )
        };
        let known_count = native
            .as_ref()
            .and_then(|native| native.desktop_count().ok());
        let mut history = DesktopHistory::default();
        if let Some(native) = &native {
            if let Ok(current) = native.current_desktop_id() {
                history.observe_desktop(current);
            }
        }
        let controller = Self {
            hwnd_raw,
            build,
            native,
            native_availability,
            fallback: KeyboardFallback::new(),
            known_count,
            last_served: None,
            history,
            scratchpad: None,
        };
        controller.publish_status();
        Ok(controller)
    }

    fn switch_to(&mut self, index: usize) {
        self.clear_stale_scratchpad();
        // A hotkey can arrive before the asynchronous foreground event. Take
        // one event-driven sample here so the source desktop is not forgotten
        // during a rapid switch.
        self.remember_current_foreground();
        let previous = self
            .native
            .as_ref()
            .and_then(|native| native.current_desktop_id().ok());
        match self.native_ensure_switch(index) {
            Ok(target) => {
                self.history.note_numbered_switch(previous, target);
                self.last_served = Some(BackendKind::NativeShell);
                if let Err(error) = self.restore_focus(target) {
                    self.publish_failure("restore desktop focus", error);
                }
                self.publish_status();
            }
            Err(error)
                if error.permits_fallback()
                    && self.known_count.is_some_and(|count| index < count) =>
            {
                match self.fallback.switch_to(index) {
                    Ok(()) => {
                        crate::info!(
                            "switched to virtual desktop {} via keyboard fallback (target existed)",
                            index + 1
                        );
                        // Keyboard fallback cannot identify the destination;
                        // discard identity-based navigation rather than
                        // pointing Previous Desktop at an unknown desktop.
                        self.history.clear_identity();
                        self.last_served = Some(BackendKind::KeyboardFallback);
                        self.publish_status();
                    }
                    Err(fallback_error) => self.publish_failure("switch desktop", fallback_error),
                }
            }
            Err(error) => self.publish_failure("switch desktop", error),
        }
    }
    fn move_foreground(&mut self, index: usize, hwnd_raw: isize, follow: bool) {
        self.clear_stale_scratchpad();
        let hwnd = raw_hwnd(hwnd_raw);
        if !eligible_window(hwnd) {
            self.publish_failure(
                if follow {
                    "move and follow foreground window"
                } else {
                    "move foreground window silently"
                },
                DesktopError::WindowUnavailable("foreground HWND is not eligible".into()),
            );
            return;
        }
        self.remember_current_foreground();
        let previous = self
            .native
            .as_ref()
            .and_then(|native| native.current_desktop_id().ok());
        let result = (|| {
            // Creation is deliberately native-only. A move action must never
            // synthesize Ctrl+Win+N because that changes the requested
            // semantics and cannot report which desktop was created.
            self.native_ensure_count(index + 1)?;
            let native = self.native.as_ref().ok_or_else(|| {
                DesktopError::MoveUnavailable("native backend is unavailable".into())
            })?;
            let ids = native.desktop_ids()?;
            let target = ids
                .get(index)
                .copied()
                .ok_or(DesktopError::TargetOutOfRange {
                    requested: index,
                    count: ids.len(),
                })?;
            let current = native.window_desktop_id(hwnd)?;
            if current != target {
                native.move_window_to_desktop(hwnd, index)?;
            }
            // The moved window is the preferred target window even for a
            // silent move; it will be restored when that desktop is visited.
            self.history.remember(target, hwnd.0 as isize);

            if follow {
                if let Err(error) = native.switch_to(index) {
                    return Err(DesktopError::Partial {
                        completed: format!("window moved to Desktop {}", index + 1),
                        failure: format!("desktop switch failed: {error}"),
                    });
                }
                // The switch completed even if foreground activation is
                // rejected; retain truthful backend status for that partial
                // outcome.
                self.last_served = Some(BackendKind::NativeShell);
                self.history.note_numbered_switch(previous, target);
                if !activate_window(hwnd) {
                    return Err(DesktopError::Partial {
                        completed: format!(
                            "window moved to Desktop {} and the desktop switch completed",
                            index + 1
                        ),
                        failure: "SetForegroundWindow rejected the moved window".into(),
                    });
                }
            } else if previous.is_some() && previous != Some(target) {
                // Moving the foreground window can leave the source desktop
                // with shell focus. Restore its remembered application when
                // Windows permits it, without navigating to the destination.
                if let Err(error) = self.restore_focus(previous.expect("checked above")) {
                    return Err(DesktopError::Partial {
                        completed: format!("window moved silently to Desktop {}", index + 1),
                        failure: format!("source desktop focus restoration failed: {error}"),
                    });
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.last_served = Some(BackendKind::NativeShell);
                self.publish_status();
            }
            Err(error) => {
                if follow && matches!(&error, DesktopError::Partial { .. }) {
                    self.publish_status();
                }
                self.publish_failure(
                    if follow {
                        "move and follow foreground window"
                    } else {
                        "move foreground window silently"
                    },
                    error,
                );
            }
        }
    }

    fn switch_previous(&mut self) {
        self.clear_stale_scratchpad();
        self.remember_current_foreground();
        let result = (|| {
            let native = self.native.as_ref().ok_or_else(|| {
                DesktopError::NavigationUnavailable("native desktop identity is unavailable".into())
            })?;
            let current = native.current_desktop_id()?;
            // Reconcile an external desktop visit before consuming the
            // previous target; foreground events can be queued behind this
            // hotkey message.
            self.history.observe_desktop(current);
            let ids = native.desktop_ids()?;
            if self.history.previous().is_none() {
                return Err(DesktopError::NavigationUnavailable(
                    "no previous desktop is remembered".into(),
                ));
            }
            let target = self.history.previous_if_present(&ids).ok_or_else(|| {
                DesktopError::NavigationUnavailable("remembered desktop was deleted".into())
            })?;
            if target == current {
                return Err(DesktopError::NavigationUnavailable(
                    "remembered desktop is already active".into(),
                ));
            }
            let index = ids
                .iter()
                .position(|id| *id == target)
                .expect("target was checked in the Shell desktop list");
            native.switch_to(index)?;
            self.history.note_previous_switch(current, target);
            Ok(target)
        })();
        match result {
            Ok(target) => {
                self.last_served = Some(BackendKind::NativeShell);
                if let Err(error) = self.restore_focus(target) {
                    self.publish_failure("restore previous desktop focus", error);
                }
                self.publish_status();
            }
            Err(error) => {
                if matches!(error, DesktopError::NavigationUnavailable(_)) {
                    self.history.clear_previous();
                }
                self.publish_failure("switch previous desktop", error);
            }
        }
    }

    fn remember_foreground(&mut self, hwnd_raw: isize) {
        self.clear_stale_scratchpad();
        let hwnd = raw_hwnd(hwnd_raw);
        if !eligible_window(hwnd) {
            return;
        }
        let Some(native) = &self.native else {
            return;
        };
        if let Ok(desktop) = native.window_desktop_id(hwnd) {
            self.history.remember(desktop, hwnd.0 as isize);
        }
    }

    fn remember_current_foreground(&mut self) {
        let hwnd = unsafe { GetForegroundWindow() };
        self.remember_foreground(hwnd.0 as isize);
    }

    fn observe_current_desktop(&mut self) {
        if let Some(native) = &self.native {
            if let Ok(current) = native.current_desktop_id() {
                self.history.observe_desktop(current);
            }
        }
    }
    fn foreground_changed(&mut self, hwnd_raw: isize) {
        self.remember_foreground(hwnd_raw);
        // Foreground events also let Previous Desktop observe switches made
        // outside WinShort without introducing a polling loop.
        self.observe_current_desktop();
    }

    fn configure_scratchpad(&mut self, managed: bool) {
        if managed {
            return;
        }
        if let Err(error) = self.release_scratchpad() {
            self.publish_failure("release scratchpad", error);
        }
    }

    fn release_scratchpad(&mut self) -> std::result::Result<(), DesktopError> {
        let Some(state) = self.scratchpad else {
            return Ok(());
        };
        if !scratchpad_window(state.hwnd, state.owner_pid) {
            self.scratchpad = None;
            return Ok(());
        }
        if state.hidden_by_winshort {
            // Recheck the process identity immediately before mutating the
            // HWND. PID tracking blocks cross-process recycling; a same-process
            // HWND reuse remains an unavoidable Win32 handle-generation limit.
            if !scratchpad_window(state.hwnd, state.owner_pid) {
                self.scratchpad = None;
                return Ok(());
            }
            unsafe {
                let _ = ShowWindow(state.hwnd, SW_SHOWNA);
            }
            if !scratchpad_window(state.hwnd, state.owner_pid) {
                self.scratchpad = None;
                return Ok(());
            }
            let visible = unsafe { IsWindowVisible(state.hwnd) }.as_bool();
            if !visible {
                return Err(DesktopError::WindowUnavailable(
                    "WinShort could not reveal the hidden scratchpad window".into(),
                ));
            }
        }
        self.scratchpad = None;
        Ok(())
    }

    fn assign_scratchpad(&mut self, hwnd_raw: isize) {
        let hwnd = raw_hwnd(hwnd_raw);
        let Some(owner_pid) = valid_external_window(hwnd, true, true) else {
            self.publish_failure(
                "assign scratchpad",
                DesktopError::WindowUnavailable("foreground HWND is not eligible".into()),
            );
            return;
        };
        if self.scratchpad.is_some() {
            // Release the old assignment before moving the new window so a
            // failed reveal cannot leave both a moved new window and a hidden old one.
            if let Err(error) = self.release_scratchpad() {
                self.publish_failure("assign scratchpad", error);
                return;
            }
        }
        // The old reveal can yield to HWND destruction/recycling. Do not
        // capture a replacement process as the new assignment.
        if valid_external_window(hwnd, true, true) != Some(owner_pid) {
            self.publish_failure(
                "assign scratchpad",
                DesktopError::WindowUnavailable(
                    "foreground HWND changed while assigning scratchpad".into(),
                ),
            );
            return;
        }
        if let Err(error) = self.ensure_window_on_current_desktop(hwnd, owner_pid) {
            self.publish_failure("assign scratchpad", error);
            return;
        }
        self.scratchpad = Some(ScratchpadState {
            hwnd,
            owner_pid,
            hidden_by_winshort: false,
        });
        crate::info!("scratchpad assigned to HWND {:?}", hwnd);
    }

    fn toggle_scratchpad(&mut self) {
        let Some(mut state) = self.scratchpad else {
            self.publish_failure(
                "toggle scratchpad",
                DesktopError::WindowUnavailable("no scratchpad window is assigned".into()),
            );
            return;
        };
        if !scratchpad_window(state.hwnd, state.owner_pid) {
            self.scratchpad = None;
            self.publish_failure(
                "toggle scratchpad",
                DesktopError::WindowUnavailable("assigned scratchpad window is stale".into()),
            );
            return;
        }

        let visible = unsafe { IsWindowVisible(state.hwnd) }.as_bool();
        if visible {
            // If another component made the window visible, WinShort no longer
            // owns a hidden state. A visible Scratchpad on another VD should be
            // brought here, not hidden on the remote desktop.
            state.hidden_by_winshort = false;
            match self.window_on_current_desktop(state.hwnd, state.owner_pid) {
                Ok(true) => {
                    if !scratchpad_window(state.hwnd, state.owner_pid) {
                        self.scratchpad = None;
                        self.publish_failure(
                            "toggle scratchpad",
                            DesktopError::WindowUnavailable(
                                "assigned scratchpad window became stale".into(),
                            ),
                        );
                        return;
                    }
                    unsafe {
                        let _ = ShowWindow(state.hwnd, SW_HIDE);
                    }
                    if !scratchpad_window(state.hwnd, state.owner_pid) {
                        self.scratchpad = None;
                        self.publish_failure(
                            "toggle scratchpad",
                            DesktopError::WindowUnavailable(
                                "assigned scratchpad window became stale while hiding".into(),
                            ),
                        );
                        return;
                    }
                    if unsafe { IsWindowVisible(state.hwnd) }.as_bool() {
                        self.scratchpad = Some(state);
                        self.publish_failure(
                            "toggle scratchpad",
                            DesktopError::WindowUnavailable(
                                "WinShort could not hide the scratchpad window".into(),
                            ),
                        );
                        return;
                    }
                    state.hidden_by_winshort = true;
                    self.scratchpad = Some(state);
                    crate::info!("scratchpad hidden");
                    return;
                }
                Ok(false) => {}
                Err(error) => {
                    self.scratchpad = Some(state);
                    self.publish_failure("toggle scratchpad", error);
                    return;
                }
            }
        } else if !state.hidden_by_winshort {
            // Do not take ownership of a window hidden by its application or
            // some other component. Releasing the assignment avoids surprising
            // resurrection later.
            self.scratchpad = None;
            self.publish_failure(
                "toggle scratchpad",
                DesktopError::WindowUnavailable(
                    "assigned window was hidden outside WinShort; scratchpad assignment released"
                        .into(),
                ),
            );
            return;
        }

        if let Err(error) = self.ensure_window_on_current_desktop(state.hwnd, state.owner_pid) {
            self.scratchpad = Some(state);
            if state.hidden_by_winshort {
                // A failed desktop query/move must not leave an application
                // invisible under WinShort's ownership. Reveal and release
                // even when the native backend is no longer usable.
                if let Err(release_error) = self.release_scratchpad() {
                    self.publish_failure("release scratchpad", release_error);
                }
            }
            self.publish_failure("toggle scratchpad", error);
            return;
        }
        if !scratchpad_window(state.hwnd, state.owner_pid) {
            self.scratchpad = None;
            self.publish_failure(
                "toggle scratchpad",
                DesktopError::WindowUnavailable("assigned scratchpad window became stale".into()),
            );
            return;
        }
        if !unsafe { IsWindowVisible(state.hwnd) }.as_bool() {
            unsafe {
                // SW_SHOWNA separates visibility restoration from the explicit
                // foreground request and does not force a maximized window back
                // to its normal placement as SW_RESTORE would.
                let _ = ShowWindow(state.hwnd, SW_SHOWNA);
            }
        }
        if !scratchpad_window(state.hwnd, state.owner_pid) {
            self.scratchpad = None;
            self.publish_failure(
                "toggle scratchpad",
                DesktopError::WindowUnavailable("assigned scratchpad window became stale".into()),
            );
            return;
        }
        if !unsafe { IsWindowVisible(state.hwnd) }.as_bool() {
            self.scratchpad = Some(state);
            self.publish_failure(
                "toggle scratchpad",
                DesktopError::WindowUnavailable("scratchpad could not be shown".into()),
            );
            return;
        }

        state.hidden_by_winshort = false;
        self.scratchpad = Some(state);
        if !scratchpad_window(state.hwnd, state.owner_pid) {
            self.scratchpad = None;
            self.publish_failure(
                "toggle scratchpad",
                DesktopError::WindowUnavailable("assigned scratchpad window became stale".into()),
            );
            return;
        }
        if !unsafe { SetForegroundWindow(state.hwnd) }.as_bool() {
            self.publish_failure(
                "toggle scratchpad",
                DesktopError::Partial {
                    completed: "scratchpad shown on the current desktop".into(),
                    failure: "SetForegroundWindow rejected scratchpad activation".into(),
                },
            );
            return;
        }
        crate::info!("scratchpad shown and focused");
    }

    fn window_on_current_desktop(
        &self,
        hwnd: HWND,
        owner_pid: u32,
    ) -> std::result::Result<bool, DesktopError> {
        if !scratchpad_window(hwnd, owner_pid) {
            return Err(DesktopError::WindowUnavailable(
                "assigned scratchpad window became stale".into(),
            ));
        }
        let native = self.native.as_ref().ok_or_else(|| {
            DesktopError::MoveUnavailable("native desktop window manager is unavailable".into())
        })?;
        Ok(native.window_desktop_id(hwnd)? == native.current_desktop_id()?)
    }

    fn ensure_window_on_current_desktop(
        &self,
        hwnd: HWND,
        owner_pid: u32,
    ) -> std::result::Result<(), DesktopError> {
        if !scratchpad_window(hwnd, owner_pid) {
            return Err(DesktopError::WindowUnavailable(
                "assigned scratchpad window became stale".into(),
            ));
        }
        let native = self.native.as_ref().ok_or_else(|| {
            DesktopError::MoveUnavailable("native desktop window manager is unavailable".into())
        })?;
        let current = native.current_desktop_id()?;
        let ids = native.desktop_ids()?;
        let current_index = ids.iter().position(|id| *id == current).ok_or_else(|| {
            DesktopError::NavigationUnavailable(
                "current desktop was not found in Shell ordering".into(),
            )
        })?;
        if !scratchpad_window(hwnd, owner_pid) {
            return Err(DesktopError::WindowUnavailable(
                "assigned scratchpad window became stale".into(),
            ));
        }
        if native.window_desktop_id(hwnd)? != current {
            if !scratchpad_window(hwnd, owner_pid) {
                return Err(DesktopError::WindowUnavailable(
                    "assigned scratchpad window became stale while moving".into(),
                ));
            }
            native.move_window_to_desktop(hwnd, current_index)?;
        }
        Ok(())
    }

    fn clear_stale_scratchpad(&mut self) {
        if self
            .scratchpad
            .is_some_and(|state| !scratchpad_window(state.hwnd, state.owner_pid))
        {
            self.scratchpad = None;
            crate::info!("cleared stale scratchpad window");
        }
    }

    fn restore_focus(
        &mut self,
        target: windows_core::GUID,
    ) -> std::result::Result<(), DesktopError> {
        let native = self.native.as_ref().ok_or_else(|| {
            DesktopError::FocusFailed("native desktop identity is unavailable".into())
        })?;
        let mut candidate = self.history.remembered(target).map(raw_hwnd);
        if candidate.is_some_and(|hwnd| {
            !eligible_window(hwnd) || native.window_desktop_id(hwnd).ok() != Some(target)
        }) {
            if let Some(hwnd) = candidate {
                self.history.forget_window(hwnd.0 as isize);
            }
            candidate = None;
        }
        if candidate.is_none() {
            candidate = enumerate_windows().into_iter().find(|hwnd| {
                eligible_window(*hwnd) && native.window_desktop_id(*hwnd).ok() == Some(target)
            });
        }
        let Some(hwnd) = candidate else {
            // A desktop with no usable application window is a successful
            // switch; there is simply nothing meaningful to activate.
            return Ok(());
        };
        if activate_window(hwnd) {
            self.history.remember(target, hwnd.0 as isize);
            Ok(())
        } else {
            Err(DesktopError::FocusFailed(
                "SetForegroundWindow rejected the destination window".into(),
            ))
        }
    }

    fn native_ensure_count(
        &mut self,
        target_count: usize,
    ) -> std::result::Result<(), DesktopError> {
        let first = match self.native.as_ref() {
            Some(native) => native.ensure_desktop_count(target_count),
            None => Err(self.native_unavailable_error("desktop creation")),
        };
        match first {
            Ok(()) => {
                self.known_count = Some(self.known_count.unwrap_or(0).max(target_count));
                Ok(())
            }
            Err(error) if error.permits_fallback() => {
                crate::warn_!("native desktop operation failed: {error}; rebuilding Shell proxy");
                self.recreate_native()?;
                self.native
                    .as_ref()
                    .expect("native backend recreated")
                    .ensure_desktop_count(target_count)?;
                self.known_count = Some(self.known_count.unwrap_or(0).max(target_count));
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn native_ensure_switch(&mut self, index: usize) -> std::result::Result<GUID, DesktopError> {
        self.native_ensure_count(index + 1)?;
        let native = self
            .native
            .as_ref()
            .ok_or_else(|| self.native_unavailable_error("desktop switching"))?;
        let ids = native.desktop_ids()?;
        let target = ids
            .get(index)
            .copied()
            .ok_or(DesktopError::TargetOutOfRange {
                requested: index,
                count: ids.len(),
            })?;
        native.switch_to(index)?;
        Ok(target)
    }

    fn recreate_native(&mut self) -> std::result::Result<(), DesktopError> {
        match InternalBackend::create(self.build) {
            Ok(native) => {
                self.native = Some(native);
                self.native_availability = BackendAvailability::Available;
                Ok(())
            }
            Err(error) => {
                self.native = None;
                self.native_availability = BackendAvailability::Failed {
                    reason: error.to_string(),
                };
                if let Err(release_error) = self.release_scratchpad() {
                    crate::error_!("native desktop backend unavailable and scratchpad release failed: {release_error}");
                }
                Err(DesktopError::BackendUnavailable(error.to_string()))
            }
        }
    }

    fn native_unavailable_error(&self, operation: &str) -> DesktopError {
        match &self.native_availability {
            BackendAvailability::UnsupportedBuild { build } => {
                DesktopError::UnsupportedBuild(*build)
            }
            BackendAvailability::Failed { reason } => {
                DesktopError::BackendUnavailable(format!("{operation}: {reason}"))
            }
            BackendAvailability::Available => {
                DesktopError::BackendUnavailable(format!("{operation}: native backend unavailable"))
            }
        }
    }

    fn status(&self) -> BackendStatus {
        let count = self
            .native
            .as_ref()
            .and_then(|native| native.desktop_count().ok());
        BackendStatus {
            native: self.native_availability.clone(),
            fallback: BackendAvailability::Available,
            active: if self.native.is_some() {
                BackendKind::NativeShell
            } else {
                BackendKind::KeyboardFallback
            },
            desktop_count: count,
            last_served: self.last_served,
        }
    }

    fn publish_failure(&self, action: &str, error: DesktopError) {
        crate::error_!("desktop action {action} failed: {error}");
        let hwnd = HWND(self.hwnd_raw as *mut _);
        unsafe {
            let _ = crate::event::post_event(
                hwnd,
                AppEvent::DesktopActionFailed {
                    action: action.into(),
                    reason: error.to_string(),
                },
            );
        }
    }

    fn publish_status(&self) {
        let hwnd = windows::Win32::Foundation::HWND(self.hwnd_raw as *mut _);
        unsafe {
            let _ = crate::event::post_event(hwnd, AppEvent::DesktopBackendChanged(self.status()));
        }
    }
}

fn raw_hwnd(raw: isize) -> HWND {
    crate::platform::foreground::normalize_foreground_hwnd(HWND(raw as *mut _))
}

fn activate_window(hwnd: HWND) -> bool {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
    }
}

fn is_shell_surface_class(class: &str) -> bool {
    matches!(
        class,
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "DV2ControlHost"
            | "Windows.UI.Core.CoreWindow"
    )
}
fn eligible_window(hwnd: HWND) -> bool {
    valid_external_window(hwnd, true, true).is_some()
}

fn scratchpad_window(hwnd: HWND, owner_pid: u32) -> bool {
    // Retained Scratchpad ownership is intentionally broader than assignment
    // eligibility. Once WinShort hides a window, temporary disabled,
    // no-activate, tool-window, hidden, or off-desktop/cloaked state must not
    // make us forget the obligation to reveal it later. The process identity
    // prevents a recycled HWND from targeting an unrelated process.
    if hwnd.0.is_null() {
        return false;
    }
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return false;
        }
        if window_process_id(hwnd) != Some(owner_pid) || owner_pid == std::process::id() {
            return false;
        }
        let mut class = [0u16; 128];
        let length = GetClassNameW(hwnd, &mut class);
        if length <= 0 {
            return false;
        }
        let class = String::from_utf16_lossy(&class[..length as usize]);
        !is_shell_surface_class(&class)
    }
}

fn window_process_id(hwnd: HWND) -> Option<u32> {
    let mut pid = 0u32;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (thread_id != 0 && pid != 0).then_some(pid)
}
fn valid_external_window(hwnd: HWND, require_visible: bool, reject_cloaked: bool) -> Option<u32> {
    if hwnd.0.is_null() {
        return None;
    }
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return None;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if (require_visible && !IsWindowVisible(hwnd).as_bool())
            || style & WS_DISABLED.0 != 0
            || ex_style & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0
        {
            return None;
        }
        let pid = window_process_id(hwnd)?;
        if pid == std::process::id() {
            return None;
        }
        if reject_cloaked {
            let mut cloaked = 0u32;
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                (&mut cloaked as *mut u32).cast(),
                std::mem::size_of::<u32>() as u32,
            )
            .ok()?;
            if cloaked != 0 {
                return None;
            }
        }
        let mut class = [0u16; 128];
        let length = GetClassNameW(hwnd, &mut class);
        if length <= 0 {
            return None;
        }
        let class = String::from_utf16_lossy(&class[..length as usize]);
        (!is_shell_surface_class(&class)).then_some(pid)
    }
}

fn enumerate_windows() -> Vec<HWND> {
    let mut windows = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_proc),
            windows::Win32::Foundation::LPARAM(&mut windows as *mut _ as isize),
        );
    }
    windows
}

unsafe extern "system" fn enum_windows_proc(
    hwnd: HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::core::BOOL {
    // SAFETY: EnumWindows invokes this callback synchronously with the caller
    // owned collection passed through LPARAM.
    let windows = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    windows.push(hwnd);
    true.into()
}

fn desktop_thread(
    hwnd_raw: isize,
    receiver: mpsc::Receiver<DesktopCommand>,
    ready: mpsc::SyncSender<std::result::Result<(), Error>>,
) {
    let com = crate::platform::com::ComApartment::init_sta();
    if !com.ok() {
        let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
        let _ = ready.send(Err(Error::os("CoInitializeEx(desktop)", code)));
        return;
    }
    let mut controller = match DesktopController::create(hwnd_raw) {
        Ok(controller) => controller,
        Err(e) => {
            let _ = ready.send(Err(e));
            return; // ComApartment guard uninitializes (#24)
        }
    };
    if let Some(native) = controller.native.as_ref() {
        match (native.desktop_count(), native.current_desktop()) {
            (Ok(count), Ok(current)) => {
                crate::info!("virtual desktops: count={count}, current={}", current + 1)
            }
            (count, current) => crate::warn_!(
                "virtual desktop diagnostics unavailable: count={count:?}, current={current:?}"
            ),
        }
    }
    let _ = ready.send(Ok(()));
    crate::info!(
        "virtual desktop backend: {}",
        controller.status().active.label()
    );

    while let Ok(command) = receiver.recv() {
        match command {
            DesktopCommand::SwitchTo(index) => controller.switch_to(index),
            DesktopCommand::MoveForeground {
                index,
                hwnd_raw,
                follow,
            } => controller.move_foreground(index, hwnd_raw, follow),
            DesktopCommand::SwitchPrevious => controller.switch_previous(),
            DesktopCommand::ForegroundChanged { hwnd_raw } => {
                controller.foreground_changed(hwnd_raw)
            }
            DesktopCommand::ConfigureScratchpad { managed } => {
                controller.configure_scratchpad(managed)
            }
            DesktopCommand::AssignScratchpad { hwnd_raw } => controller.assign_scratchpad(hwnd_raw),
            DesktopCommand::ToggleScratchpad => controller.toggle_scratchpad(),
            DesktopCommand::Shutdown => break,
        }
    }
    // The worker owns Scratchpad hide state. Always make a best-effort reveal
    // before releasing that ownership, including receiver-disconnect shutdown.
    if let Err(error) = controller.release_scratchpad() {
        crate::error_!("failed to restore hidden scratchpad during shutdown: {error}");
    }
    drop(controller);
    drop(com);
    crate::info!("virtual desktop controller stopped");
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    use crate::desktop::backend::DesktopError;
    use std::cell::RefCell;

    struct ScriptedBackend {
        errors: RefCell<Vec<DesktopError>>,
        fallback_used: RefCell<bool>,
    }
    impl VirtualDesktopBackend for ScriptedBackend {
        fn desktop_count(&self) -> std::result::Result<usize, DesktopError> {
            Err(DesktopError::BackendUnavailable("scripted".into()))
        }
        fn current_desktop(&self) -> std::result::Result<usize, DesktopError> {
            Ok(0)
        }
        fn switch_to(&self, _index: usize) -> std::result::Result<(), DesktopError> {
            match self.errors.borrow_mut().pop() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        }
    }

    #[test]
    fn target_out_of_range_never_permits_fallback() {
        // #20 acceptance: desktop 9 requested with count 3 must NOT inject
        // keyboard chords.
        let err = DesktopError::TargetOutOfRange {
            requested: 8,
            count: 3,
        };
        assert!(!err.permits_fallback());
        assert!(matches!(err, DesktopError::TargetOutOfRange { .. }));
    }

    #[test]
    fn rpc_class_permits_fallback_but_semantic_does_not() {
        assert!(DesktopError::RpcDisconnected.permits_fallback());
        assert!(DesktopError::BackendUnavailable("x".into()).permits_fallback());
        assert!(!DesktopError::SwitchFailed(-2_147_024_846).permits_fallback());
        assert!(!DesktopError::UnsupportedBuild(19041).permits_fallback());
        assert!(!DesktopError::AbiMismatch("slot".into()).permits_fallback());
        assert!(!DesktopError::CreationUnavailable("unsupported".into()).permits_fallback());
        assert!(!DesktopError::MoveUnavailable("unsupported".into()).permits_fallback());
        assert!(!DesktopError::WindowUnavailable("stale".into()).permits_fallback());
        assert!(!DesktopError::FocusFailed("blocked".into()).permits_fallback());
        assert!(!DesktopError::NavigationUnavailable("deleted".into()).permits_fallback());
        assert!(!DesktopError::Partial {
            completed: "move".into(),
            failure: "switch".into(),
        }
        .permits_fallback());
    }
    #[test]
    fn scripted_backend_surfaces_typed_error() {
        let backend = ScriptedBackend {
            errors: RefCell::new(vec![DesktopError::TargetOutOfRange {
                requested: 8,
                count: 3,
            }]),
            fallback_used: RefCell::new(false),
        };
        let out = VirtualDesktopBackend::switch_to(&backend, 8);
        assert_eq!(
            out,
            Err(DesktopError::TargetOutOfRange {
                requested: 8,
                count: 3
            })
        );
        assert!(!*backend.fallback_used.borrow());
    }
    #[test]
    fn shell_surface_classes_are_never_meaningful_focus_candidates() {
        for class in [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
            "DV2ControlHost",
            "Windows.UI.Core.CoreWindow",
        ] {
            assert!(is_shell_surface_class(class), "{class}");
        }
        assert!(!is_shell_surface_class("Chrome_WidgetWin_1"));
    }
    #[test]
    fn stale_scratchpad_handle_is_cleared_without_shell_calls() {
        let mut controller = DesktopController {
            hwnd_raw: 0,
            build: OsBuild {
                build: 0,
                update_revision: 0,
            },
            native: None,
            native_availability: BackendAvailability::UnsupportedBuild { build: 0 },
            fallback: KeyboardFallback::new(),
            known_count: None,
            last_served: None,
            history: DesktopHistory::default(),
            scratchpad: Some(ScratchpadState {
                hwnd: HWND::default(),
                owner_pid: u32::MAX,
                hidden_by_winshort: true,
            }),
        };

        controller.clear_stale_scratchpad();

        assert!(controller.scratchpad.is_none());
    }
}
