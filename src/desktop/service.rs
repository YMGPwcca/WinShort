//! STA-owned virtual desktop controller: build-pinned Shell backend with
//! automatic best-effort keyboard fallback.

use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsWindow, IsWindowVisible, SetForegroundWindow, GWL_STYLE, WS_DISABLED,
};
use windows_core::GUID;

use crate::desktop::backend::{
    BackendAvailability, BackendKind, BackendStatus, DesktopError, VirtualDesktopBackend,
};
use crate::desktop::detect::{detect, OsBuild};
use crate::desktop::internal_api::InternalBackend;
use crate::desktop::keyboard_fallback::KeyboardFallback;
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
    RememberForeground {
        hwnd_raw: isize,
    },
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
        if ready_rx
            .recv_timeout(std::time::Duration::from_secs(8))
            .map_err(|_| Error::desktop("desktop startup timed out"))
            .and_then(|r| r)
            .is_err()
        {
            let _ = sender.send(DesktopCommand::Shutdown);
            let _ = join.join();
            return Err(Error::desktop("desktop startup timed out"));
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

    pub fn remember_foreground(&self, hwnd_raw: isize) {
        let _ = self
            .sender
            .send(DesktopCommand::RememberForeground { hwnd_raw });
    }

    pub fn switch_to(&self, index: usize) {
        let _ = self.sender.send(DesktopCommand::SwitchTo(index));
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
    /// Last eligible foreground window observed for each stable desktop ID.
    last_focused: HashMap<windows_core::GUID, HWND>,
    /// Stable desktop ID used by the previous-desktop action.
    previous_desktop: Option<windows_core::GUID>,
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
        let controller = Self {
            hwnd_raw,
            build,
            native,
            native_availability,
            fallback: KeyboardFallback::new(),
            known_count,
            last_served: None,
            last_focused: HashMap::new(),
            previous_desktop: None,
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
                if previous != Some(target) {
                    self.previous_desktop = previous;
                }
                if let Err(error) = self.restore_focus(target, false) {
                    crate::warn_!("desktop focus restoration failed: {error}");
                }
                self.last_served = Some(BackendKind::NativeShell);
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
                "move foreground window",
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
            if follow {
                native.switch_to(index)?;
                if !unsafe { SetForegroundWindow(hwnd) }.as_bool() {
                    return Err(DesktopError::FocusFailed(
                        "SetForegroundWindow rejected the moved window".into(),
                    ));
                }
                if previous != Some(target) {
                    self.previous_desktop = previous;
                }
                self.last_focused.insert(target, hwnd);
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.last_served = Some(BackendKind::NativeShell);
                self.publish_status();
            }
            Err(error) => self.publish_failure(
                if follow {
                    "move and follow foreground window"
                } else {
                    "move foreground window silently"
                },
                error,
            ),
        }
    }

    fn switch_previous(&mut self) {
        let result = (|| {
            let native = self.native.as_ref().ok_or_else(|| {
                DesktopError::NavigationUnavailable("native desktop identity is unavailable".into())
            })?;
            let current = native.current_desktop_id()?;
            let target = self.previous_desktop.ok_or_else(|| {
                DesktopError::NavigationUnavailable("no previous desktop is remembered".into())
            })?;
            let ids = native.desktop_ids()?;
            let index = ids.iter().position(|id| *id == target).ok_or_else(|| {
                DesktopError::NavigationUnavailable("remembered desktop was deleted".into())
            })?;
            native.switch_to(index)?;
            self.previous_desktop = Some(current);
            Ok(target)
        })();
        match result {
            Ok(target) => {
                if let Err(error) = self.restore_focus(target, false) {
                    crate::warn_!("previous-desktop focus restoration failed: {error}");
                }
                self.last_served = Some(BackendKind::NativeShell);
                self.publish_status();
            }
            Err(error) => {
                if matches!(error, DesktopError::NavigationUnavailable(_)) {
                    self.previous_desktop = None;
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
            self.last_focused.insert(desktop, hwnd);
        }
    }

    fn remember_current_foreground(&mut self) {
        let hwnd = unsafe { GetForegroundWindow() };
        self.remember_foreground(hwnd.0 as isize);
    }

    fn restore_focus(
        &mut self,
        target: windows_core::GUID,
        strict: bool,
    ) -> std::result::Result<(), DesktopError> {
        let native = self.native.as_ref().ok_or_else(|| {
            DesktopError::FocusFailed("native desktop identity is unavailable".into())
        })?;
        let mut candidate = self.last_focused.get(&target).copied();
        if candidate.is_some_and(|hwnd| {
            !eligible_window(hwnd) || native.window_desktop_id(hwnd).ok() != Some(target)
        }) {
            if let Some(hwnd) = candidate {
                self.last_focused.remove(&target);
                self.last_focused
                    .retain(|_, remembered| *remembered != hwnd);
            }
            candidate = None;
        }
        if candidate.is_none() {
            candidate = enumerate_windows().into_iter().find(|hwnd| {
                eligible_window(*hwnd) && native.window_desktop_id(*hwnd).ok() == Some(target)
            });
        }
        let Some(hwnd) = candidate else {
            return if strict {
                Err(DesktopError::FocusFailed(
                    "no visible foreground-capable window exists on the destination desktop".into(),
                ))
            } else {
                Ok(())
            };
        };
        if unsafe { SetForegroundWindow(hwnd) }.as_bool() {
            self.last_focused.insert(target, hwnd);
            Ok(())
        } else if strict {
            Err(DesktopError::FocusFailed(
                "SetForegroundWindow rejected the destination window".into(),
            ))
        } else {
            Ok(())
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
    HWND(raw as *mut _)
}

fn eligible_window(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool()
            || !IsWindowVisible(hwnd).as_bool()
            || GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_DISABLED.0 != 0
        {
            return false;
        }
        let mut pid = 0u32;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == std::process::id() {
            return false;
        }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return false;
        }
        let mut class = [0u16; 128];
        let length = GetClassNameW(hwnd, &mut class);
        if length <= 0 {
            return false;
        }
        let class = String::from_utf16_lossy(&class[..length as usize]);
        !matches!(
            class.as_str(),
            "Progman"
                | "WorkerW"
                | "Shell_TrayWnd"
                | "Shell_SecondaryTrayWnd"
                | "DV2ControlHost"
                | "Windows.UI.Core.CoreWindow"
        )
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
            DesktopCommand::RememberForeground { hwnd_raw } => {
                controller.remember_foreground(hwnd_raw)
            }
            DesktopCommand::Shutdown => break,
        }
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
}
