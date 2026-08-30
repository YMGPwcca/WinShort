//! STA-owned virtual desktop controller: build-pinned Shell backend with
//! automatic best-effort keyboard fallback.

use std::sync::mpsc::{self, Sender};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GWL_STYLE,
    SW_RESTORE, WS_DISABLED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows_core::GUID;

use crate::desktop::backend::{
    BackendAvailability, BackendKind, BackendStatus, DesktopError, VirtualDesktopBackend,
};
use crate::desktop::detect::{detect, OsBuild};
use crate::desktop::internal_api::InternalBackend;
use crate::desktop::keyboard_fallback::KeyboardFallback;
use crate::desktop::state::DesktopHistory;
use crate::desktop::workspace_state;
use crate::error::{Error, Result};
use crate::event::AppEvent;

const SPECIAL_WORKSPACE_NAME: &str = "WinShort Special Workspace";

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
    /// Durable identity of WinShort's dedicated special workspace when Shell
    /// still exposes that GUID. Persisting it prevents duplicate workspaces
    /// after a hard process kill or Windows reboot.
    special_workspace: Option<GUID>,
    /// Normal desktop to return to when leaving the special workspace.
    special_return: Option<GUID>,
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
        let special_workspace = reclaim_persisted_special_workspace(native.as_ref());
        let known_count = native.as_ref().and_then(|native| {
            native
                .desktop_ids()
                .ok()
                .map(|ids| numbered_desktop_ids(&ids, special_workspace).len())
        });
        let mut history = DesktopHistory::default();
        if let Some(native) = &native {
            if let Ok(current) = native.current_desktop_id() {
                if Some(current) != special_workspace {
                    history.observe_desktop(current);
                }
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
            special_workspace,
            special_return: None,
        };
        controller.publish_status();
        Ok(controller)
    }

    fn switch_to(&mut self, index: usize) {
        self.remember_current_foreground();
        let previous = self
            .native
            .as_ref()
            .and_then(|native| native.current_desktop_id().ok());
        match self.native_ensure_switch(index) {
            Ok(target) => {
                if previous == self.special_workspace {
                    self.special_return = None;
                    self.history.observe_desktop(target);
                } else {
                    self.history.note_numbered_switch(previous, target);
                }
                self.last_served = Some(BackendKind::NativeShell);
                if let Err(error) = self.restore_focus(target) {
                    self.publish_failure("restore desktop focus", error);
                }
                self.publish_status();
            }
            Err(error)
                if self.special_workspace.is_none()
                    && error.permits_fallback()
                    && self.known_count.is_some_and(|count| index < count) =>
            {
                match self.fallback.switch_to(index) {
                    Ok(()) => {
                        crate::info!(
                            "switched to virtual desktop {} via keyboard fallback (target existed)",
                            index + 1
                        );
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
            self.native_ensure_count(index + 1)?;
            let ids = self.normal_desktop_ids()?;
            let target = ids
                .get(index)
                .copied()
                .ok_or(DesktopError::TargetOutOfRange {
                    requested: index,
                    count: ids.len(),
                })?;
            let native = self.native.as_ref().ok_or_else(|| {
                DesktopError::MoveUnavailable("native backend is unavailable".into())
            })?;
            let current = native.window_desktop_id(hwnd)?;
            if current != target {
                native.move_window_to_desktop_id(hwnd, target)?;
            }
            self.history.remember(target, hwnd.0 as isize);

            if follow {
                native.switch_to_id(target)?;
                self.last_served = Some(BackendKind::NativeShell);
                if previous == self.special_workspace {
                    self.special_return = None;
                    self.history.observe_desktop(target);
                } else {
                    self.history.note_numbered_switch(previous, target);
                }
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
        self.remember_current_foreground();
        let result = (|| {
            let current = self
                .native
                .as_ref()
                .ok_or_else(|| {
                    DesktopError::NavigationUnavailable(
                        "native desktop identity is unavailable".into(),
                    )
                })?
                .current_desktop_id()?;
            let ids = self.normal_desktop_ids()?;

            if Some(current) == self.special_workspace {
                let target =
                    choose_special_return_target(self.special_return, self.history.current(), &ids)
                        .ok_or_else(|| {
                            DesktopError::NavigationUnavailable(
                                "no normal desktop is available to leave the special workspace"
                                    .into(),
                            )
                        })?;
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(target)?;
                self.special_return = None;
                self.history.observe_desktop(target);
                return Ok((target, true));
            }

            self.history.observe_desktop(current);
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
            self.native
                .as_ref()
                .expect("native backend was checked above")
                .switch_to_id(target)?;
            self.history.note_previous_switch(current, target);
            Ok((target, false))
        })();
        match result {
            Ok((target, left_special_workspace)) => {
                self.last_served = Some(BackendKind::NativeShell);
                if !left_special_workspace {
                    if let Err(error) = self.restore_focus(target) {
                        self.publish_failure("restore previous desktop focus", error);
                    }
                }
                self.publish_status();
            }
            Err(error) => {
                if matches!(error, DesktopError::NavigationUnavailable(_))
                    && self.special_workspace.is_none()
                {
                    self.history.clear_previous();
                }
                self.publish_failure("switch previous desktop", error);
            }
        }
    }
    fn remember_foreground(&mut self, hwnd_raw: isize) {
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
                if Some(current) != self.special_workspace {
                    self.history.observe_desktop(current);
                }
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
            if self.special_workspace.is_none() {
                self.special_workspace = reclaim_persisted_special_workspace(self.native.as_ref());
            }
            if let Some(workspace) = self.special_workspace {
                self.normalize_special_workspace(workspace);
            }
            return;
        }
        if let Err(error) = self.release_special_workspace() {
            self.publish_failure("release special workspace", error);
        }
    }

    fn reconcile_special_workspace_ids(&mut self, ids: &[GUID]) {
        if self
            .special_workspace
            .is_some_and(|workspace| !ids.contains(&workspace))
        {
            let removed = self.special_workspace.take().expect("checked above");
            crate::info!(
                "special workspace was removed outside WinShort; clearing persisted identity"
            );
            self.special_return = None;
            self.history.forget_desktop(removed);
            if let Err(error) = workspace_state::clear() {
                crate::warn_!("failed to clear stale special workspace identity: {error}");
            }
        }
        if self.special_return.is_some_and(|return_to| {
            !ids.contains(&return_to) || Some(return_to) == self.special_workspace
        }) {
            self.special_return = None;
        }
    }

    fn native_desktop_ids(&mut self) -> std::result::Result<Vec<GUID>, DesktopError> {
        let first = match self.native.as_ref() {
            Some(native) => native.desktop_ids(),
            None => Err(self.native_unavailable_error("desktop identity")),
        };
        let ids = match first {
            Ok(ids) => ids,
            Err(error) if error.permits_fallback() => {
                crate::warn_!("native desktop operation failed: {error}; rebuilding Shell proxy");
                self.recreate_native()?;
                self.native
                    .as_ref()
                    .expect("native backend recreated")
                    .desktop_ids()?
            }
            Err(error) => return Err(error),
        };
        self.reconcile_special_workspace_ids(&ids);
        Ok(ids)
    }

    fn normal_desktop_ids(&mut self) -> std::result::Result<Vec<GUID>, DesktopError> {
        let ids = self.native_desktop_ids()?;
        Ok(numbered_desktop_ids(&ids, self.special_workspace))
    }

    fn normalize_special_workspace(&mut self, workspace: GUID) {
        let ids = match self.native_desktop_ids() {
            Ok(ids) if ids.contains(&workspace) => ids,
            Ok(_) => return,
            Err(error) => {
                crate::warn_!("cannot normalize special workspace ordering/name: {error}");
                return;
            }
        };
        let Some(native) = self.native.as_ref() else {
            return;
        };

        match native.desktop_name(workspace) {
            Ok(name) if name == SPECIAL_WORKSPACE_NAME => {}
            Ok(_) => match native.set_desktop_name(workspace, SPECIAL_WORKSPACE_NAME) {
                Ok(()) => crate::info!("named special workspace '{SPECIAL_WORKSPACE_NAME}'"),
                Err(error) => crate::warn_!("failed to name special workspace: {error}"),
            },
            Err(error) => crate::warn_!("failed to read special workspace name: {error}"),
        }

        if ids.last().copied() != Some(workspace) {
            match native.move_desktop_id(workspace, ids.len() - 1) {
                Ok(()) => crate::info!("moved special workspace to the end of Shell ordering"),
                Err(error) => crate::warn_!("failed to move special workspace to the end: {error}"),
            }
        }
    }

    fn ensure_special_workspace(&mut self) -> std::result::Result<GUID, DesktopError> {
        let ids = self.native_desktop_ids()?;
        if let Some(workspace) = self.special_workspace {
            self.normalize_special_workspace(workspace);
            return Ok(workspace);
        }
        if ids.len() >= 256 {
            return Err(DesktopError::CreationUnavailable(
                "cannot create special workspace because Windows already has 256 desktops".into(),
            ));
        }
        let workspace = self
            .native
            .as_ref()
            .ok_or_else(|| self.native_unavailable_error("special workspace creation"))?
            .create_desktop()?;
        if let Err(error) = workspace_state::store(workspace) {
            if let Some(fallback) = ids.first().copied() {
                if let Some(native) = self.native.as_ref() {
                    if let Err(cleanup_error) = native.remove_desktop_id(workspace, fallback) {
                        crate::warn_!(
                            "failed to remove unpersisted special workspace after state error: {cleanup_error}"
                        );
                    }
                }
            }
            return Err(DesktopError::CreationUnavailable(format!(
                "created special workspace but could not persist its GUID: {error}"
            )));
        }
        self.special_workspace = Some(workspace);
        self.special_return = None;
        self.known_count = Some(ids.len());
        crate::info!("created dedicated special workspace {workspace:?}");
        self.normalize_special_workspace(workspace);
        Ok(workspace)
    }

    fn release_special_workspace(&mut self) -> std::result::Result<(), DesktopError> {
        let Some(workspace) = self.special_workspace else {
            self.special_return = None;
            return Ok(());
        };
        let ids = self.native_desktop_ids()?;
        if !ids.contains(&workspace) {
            self.special_workspace = None;
            self.special_return = None;
            self.history.forget_desktop(workspace);
            if let Err(error) = workspace_state::clear() {
                crate::warn_!("failed to clear removed special workspace identity: {error}");
            }
            return Ok(());
        }
        let normal = numbered_desktop_ids(&ids, Some(workspace));
        let fallback =
            choose_special_return_target(self.special_return, self.history.current(), &normal)
                .ok_or_else(|| {
                    DesktopError::NavigationUnavailable(
                        "no normal desktop is available for special workspace cleanup".into(),
                    )
                })?;
        let current = self
            .native
            .as_ref()
            .expect("native backend was checked above")
            .current_desktop_id()?;
        self.native
            .as_ref()
            .expect("native backend was checked above")
            .remove_desktop_id(workspace, fallback)?;
        self.history.forget_desktop(workspace);
        if current == workspace {
            self.history.observe_desktop(fallback);
        }
        self.special_workspace = None;
        self.special_return = None;
        self.known_count = Some(normal.len());
        if let Err(error) = workspace_state::clear() {
            crate::warn_!("failed to clear released special workspace identity: {error}");
        }
        crate::info!("removed dedicated special workspace");
        Ok(())
    }

    fn assign_scratchpad(&mut self, hwnd_raw: isize) {
        let hwnd = raw_hwnd(hwnd_raw);
        if !eligible_window(hwnd) {
            self.publish_failure(
                "send to special workspace",
                DesktopError::WindowUnavailable("foreground HWND is not eligible".into()),
            );
            return;
        }
        self.remember_current_foreground();
        let result = (|| {
            let workspace = self.ensure_special_workspace()?;
            let native = self.native.as_ref().ok_or_else(|| {
                DesktopError::MoveUnavailable(
                    "special workspace requires the native desktop backend".into(),
                )
            })?;
            if native.window_desktop_id(hwnd)? != workspace {
                native.move_window_to_desktop_id(hwnd, workspace)?;
            }
            self.history.remember(workspace, hwnd.0 as isize);
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.last_served = Some(BackendKind::NativeShell);
                self.publish_status();
                crate::info!("moved foreground window into the special workspace");
            }
            Err(error) => self.publish_failure("send to special workspace", error),
        }
    }

    fn toggle_scratchpad(&mut self) {
        self.remember_current_foreground();
        let result = (|| {
            let workspace = self.ensure_special_workspace()?;
            let current = self
                .native
                .as_ref()
                .ok_or_else(|| {
                    DesktopError::NavigationUnavailable(
                        "special workspace requires the native desktop backend".into(),
                    )
                })?
                .current_desktop_id()?;
            let ids = self.normal_desktop_ids()?;
            if current == workspace {
                let target =
                    choose_special_return_target(self.special_return, self.history.current(), &ids)
                        .ok_or_else(|| {
                            DesktopError::NavigationUnavailable(
                                "no normal desktop is available to leave the special workspace"
                                    .into(),
                            )
                        })?;
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(target)?;
                self.special_return = None;
                self.history.observe_desktop(target);
                Ok(false)
            } else {
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(workspace)?;
                self.special_return = Some(current);
                Ok(true)
            }
        })();
        match result {
            Ok(entering) => {
                self.last_served = Some(BackendKind::NativeShell);
                // This is a real Virtual Desktop transition. Deliberately let
                // Shell own foreground/focus selection instead of replaying
                // Phase-1 SetForegroundWindow restoration here.
                self.publish_status();
                crate::info!(
                    "{} special workspace",
                    if entering { "entered" } else { "left" }
                );
            }
            Err(error) => self.publish_failure("toggle special workspace", error),
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
        let current = self.normal_desktop_ids()?;
        let reserved = usize::from(self.special_workspace.is_some());
        if target_count.saturating_add(reserved) > 256 {
            return Err(DesktopError::CreationUnavailable(format!(
                "requested {target_count} normal desktops plus the special workspace exceeds the 256-desktop safety limit"
            )));
        }
        let missing = missing_normal_desktops(current.len(), target_count);
        for _ in 0..missing {
            self.native
                .as_ref()
                .ok_or_else(|| self.native_unavailable_error("desktop creation"))?
                .create_desktop()?;
        }
        if let Some(workspace) = self.special_workspace {
            // CreateDesktop appends after the current tail. If the Special
            // Workspace used to be last, new normal desktops therefore land
            // behind it. Re-pin Special to the tail before resolving ordinals
            // so Windows' visible Desktop 1..N labels match WinShort's 1..N.
            self.normalize_special_workspace(workspace);
        }
        let final_ids = self.normal_desktop_ids()?;
        if final_ids.len() < target_count {
            return Err(DesktopError::CreationUnavailable(format!(
                "CreateDesktop stopped at {} normal desktops; target was {target_count}",
                final_ids.len()
            )));
        }
        self.known_count = Some(final_ids.len());
        Ok(())
    }

    fn native_ensure_switch(&mut self, index: usize) -> std::result::Result<GUID, DesktopError> {
        self.native_ensure_count(index + 1)?;
        let ids = self.normal_desktop_ids()?;
        let target = ids
            .get(index)
            .copied()
            .ok_or(DesktopError::TargetOutOfRange {
                requested: index,
                count: ids.len(),
            })?;
        self.native
            .as_ref()
            .ok_or_else(|| self.native_unavailable_error("desktop switching"))?
            .switch_to_id(target)?;
        Ok(target)
    }

    fn recreate_native(&mut self) -> std::result::Result<(), DesktopError> {
        match InternalBackend::create(self.build) {
            Ok(native) => {
                self.native = Some(native);
                self.native_availability = BackendAvailability::Available;
                if self.special_workspace.is_none() {
                    self.special_workspace =
                        reclaim_persisted_special_workspace(self.native.as_ref());
                }
                if let Ok(ids) = self
                    .native
                    .as_ref()
                    .expect("native backend was just recreated")
                    .desktop_ids()
                {
                    self.reconcile_special_workspace_ids(&ids);
                    self.known_count =
                        Some(numbered_desktop_ids(&ids, self.special_workspace).len());
                }
                Ok(())
            }
            Err(error) => {
                self.native = None;
                self.native_availability = BackendAvailability::Failed {
                    reason: error.to_string(),
                };
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
        let count = self.native.as_ref().and_then(|native| {
            native
                .desktop_ids()
                .ok()
                .map(|ids| numbered_desktop_ids(&ids, self.special_workspace).len())
        });
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

fn reclaim_persisted_special_workspace(native: Option<&InternalBackend>) -> Option<GUID> {
    let persisted = match workspace_state::load() {
        Ok(persisted) => persisted,
        Err(error) => {
            crate::warn_!("discarding unreadable special workspace identity: {error}");
            if let Err(clear_error) = workspace_state::clear() {
                crate::warn_!("failed to clear unreadable special workspace identity: {clear_error}");
            }
            None
        }
    }?;
    let Some(native) = native else {
        // Keep the durable identity intact. A later Shell-proxy rebuild can
        // still reclaim it; absence of a backend is not evidence of deletion.
        return None;
    };
    match native.desktop_ids() {
        Ok(ids) if ids.contains(&persisted) => {
            crate::info!("reclaimed persisted special workspace {persisted:?}");
            Some(persisted)
        }
        Ok(_) => {
            crate::info!(
                "persisted special workspace no longer exists in Shell ordering; forgetting it"
            );
            if let Err(error) = workspace_state::clear() {
                crate::warn_!("failed to clear missing special workspace identity: {error}");
            }
            None
        }
        Err(error) => {
            crate::warn_!("could not verify persisted special workspace identity: {error}");
            None
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

fn missing_normal_desktops(current_count: usize, target_count: usize) -> usize {
    target_count.saturating_sub(current_count)
}

fn numbered_desktop_ids(ids: &[GUID], special_workspace: Option<GUID>) -> Vec<GUID> {
    ids.iter()
        .copied()
        .filter(|id| Some(*id) != special_workspace)
        .collect()
}

fn choose_special_return_target(
    explicit_return: Option<GUID>,
    last_normal: Option<GUID>,
    normal_desktops: &[GUID],
) -> Option<GUID> {
    explicit_return
        .filter(|id| normal_desktops.contains(id))
        .or_else(|| last_normal.filter(|id| normal_desktops.contains(id)))
        .or_else(|| normal_desktops.first().copied())
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
    // Graceful shutdown still removes the dedicated desktop. The persisted GUID
    // is crash/reboot recovery: if teardown never runs, the next process can
    // reclaim the surviving desktop instead of creating a duplicate.
    if let Err(error) = controller.release_special_workspace() {
        crate::error_!("failed to remove special workspace during shutdown: {error}");
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
    fn normal_desktop_creation_counts_only_the_missing_target() {
        assert_eq!(missing_normal_desktops(3, 9), 6);
        assert_eq!(missing_normal_desktops(9, 9), 0);
        assert_eq!(missing_normal_desktops(10, 9), 0);
    }

    #[test]
    fn special_workspace_is_excluded_from_numbered_desktop_ordinals() {
        let first = GUID::from_u128(1);
        let special = GUID::from_u128(2);
        let second = GUID::from_u128(3);
        assert_eq!(
            numbered_desktop_ids(&[first, special, second], Some(special)),
            vec![first, second]
        );
    }

    #[test]
    fn special_workspace_at_tail_aligns_shell_and_logical_normal_ordinals() {
        let first = GUID::from_u128(1);
        let second = GUID::from_u128(2);
        let third = GUID::from_u128(3);
        let special = GUID::from_u128(99);
        let shell = [first, second, third, special];
        assert_eq!(
            numbered_desktop_ids(&shell, Some(special)),
            shell[..3].to_vec()
        );
    }

    #[test]
    fn special_workspace_return_prefers_explicit_then_history_then_first() {
        let first = GUID::from_u128(1);
        let second = GUID::from_u128(2);
        let stale = GUID::from_u128(9);
        let normal = [first, second];
        assert_eq!(
            choose_special_return_target(Some(second), Some(first), &normal),
            Some(second)
        );
        assert_eq!(
            choose_special_return_target(Some(stale), Some(first), &normal),
            Some(first)
        );
        assert_eq!(
            choose_special_return_target(Some(stale), Some(stale), &normal),
            Some(first)
        );
    }
}
