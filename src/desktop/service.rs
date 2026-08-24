//! STA-owned virtual desktop controller: build-pinned Shell backend with
//! automatic best-effort keyboard fallback.

use std::sync::mpsc::{self, Sender};

use crate::desktop::backend::{
    BackendAvailability, BackendKind, BackendStatus, VirtualDesktopBackend,
};
use crate::desktop::detect::{detect, OsBuild};
use crate::desktop::internal_api::InternalBackend;
use crate::desktop::keyboard_fallback::KeyboardFallback;
use crate::error::{Error, Result};
use crate::event::AppEvent;

/// Result of attempting the native backend for one switch (#20).
enum NativeOutcome {
    Served,
    /// Semantic refusal: log and never inject.
    Refused,
    MayFallback,
}

#[derive(Debug)]
pub enum DesktopCommand {
    SwitchTo(usize),
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
    /// Backend that actually completed the last switch (#21).
    last_served: Option<BackendKind>,
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
        let controller = Self {
            hwnd_raw,
            build,
            native,
            native_availability,
            fallback: KeyboardFallback::new(),
            last_served: None,
        };
        controller.publish_status();
        Ok(controller)
    }

    fn switch_to(&mut self, index: usize) {
        match self.try_native(index) {
            // Served or refused: no synthetic input.
            NativeOutcome::Served => {
                self.last_served = Some(BackendKind::NativeShell);
                self.publish_status();
                return;
            }
            NativeOutcome::Refused => {
                self.publish_status();
                return;
            }
            NativeOutcome::MayFallback => {}
        }
        match self.fallback.switch_to(index) {
            Ok(()) => {
                crate::info!(
                    "switched toward virtual desktop {} via keyboard fallback (best effort)",
                    index + 1
                );
                self.last_served = Some(BackendKind::KeyboardFallback);
            }
            Err(e) => crate::error_!(
                "keyboard desktop fallback failed: {e} — target not verified (possible UIPI block)"
            ),
        }
        self.publish_status();
    }

    /// Attempt the native backend. Only RPC-class failures return
    /// [`NativeOutcome::MayFallback`] — and only after one proxy rebuild
    /// (#20/#21).
    fn try_native(&mut self, index: usize) -> NativeOutcome {
        if self.native.is_none() && self.build.native_shell_supported() {
            // One bounded create attempt per action (#21): a transient startup
            // failure must not permanently disable the native backend.
            match InternalBackend::create(self.build) {
                Ok(backend) => {
                    self.native = Some(backend);
                    self.native_availability = BackendAvailability::Available;
                    crate::info!("native desktop backend recovered on demand");
                }
                Err(e) => {
                    self.native_availability = BackendAvailability::Failed {
                        reason: e.to_string(),
                    };
                    return NativeOutcome::MayFallback;
                }
            }
        }
        let Some(native) = &self.native else {
            return NativeOutcome::MayFallback;
        };
        match native.switch_to(index) {
            Ok(()) => {
                crate::info!("switched to virtual desktop {} via Native Shell", index + 1);
                self.last_served = Some(BackendKind::NativeShell);
                return NativeOutcome::Served;
            }
            Err(e) if e.permits_fallback() => {
                crate::warn_!("native desktop switch failed: {e}; rebuilding Shell proxy");
            }
            Err(e) => {
                // Semantic/ABI refusal (#20): never inject keystrokes for a
                // target that does not exist or a build we cannot serve.
                crate::error_!("native desktop switch refused: {e}");
                return NativeOutcome::Refused;
            }
        }
        // Explorer may have restarted; rebuild the STA proxy once.
        match InternalBackend::create(self.build) {
            Ok(rebuilt) => {
                self.native = Some(rebuilt);
                match self.native.as_ref().expect("just set").switch_to(index) {
                    Ok(()) => {
                        crate::info!(
                            "switched to desktop {} after Shell proxy rebuild",
                            index + 1
                        );
                        self.last_served = Some(BackendKind::NativeShell);
                        return NativeOutcome::Served;
                    }
                    Err(e) if !e.permits_fallback() => {
                        crate::error_!("native desktop switch refused after rebuild: {e}");
                        return NativeOutcome::Refused;
                    }
                    Err(_) => {} // still unavailable: fallback permitted
                }
            }
            Err(e) => {
                self.native_availability = BackendAvailability::Failed {
                    reason: e.to_string(),
                };
                self.native = None;
            }
        }
        NativeOutcome::MayFallback
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

    fn publish_status(&self) {
        let hwnd = windows::Win32::Foundation::HWND(self.hwnd_raw as *mut _);
        unsafe {
            let _ = crate::event::post_event(hwnd, AppEvent::DesktopBackendChanged(self.status()));
        }
    }
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
