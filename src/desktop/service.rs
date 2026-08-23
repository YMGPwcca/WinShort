//! STA-owned virtual desktop controller: build-pinned Shell backend with
//! automatic best-effort keyboard fallback.

use std::sync::mpsc::{self, Sender};

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

use crate::desktop::backend::{
    BackendAvailability, BackendKind, BackendStatus, VirtualDesktopBackend,
};
use crate::desktop::detect::{detect, OsBuild};
use crate::desktop::internal_api::InternalBackend;
use crate::desktop::keyboard_fallback::KeyboardFallback;
use crate::error::{Error, Result};
use crate::event::AppEvent;

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
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(8))
            .map_err(|_| Error::desktop("desktop startup timed out"))??;
        Ok(Self { sender, join: Some(join) })
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
        };
        controller.publish_status();
        Ok(controller)
    }

    fn switch_to(&mut self, index: usize) {
        if let Some(native) = &self.native {
            match native.switch_to(index) {
                Ok(()) => {
                    crate::info!("switched to virtual desktop {} via Native Shell", index + 1);
                    self.publish_status();
                    return;
                }
                Err(e) => {
                    crate::warn_!("native desktop switch failed: {e}; rebuilding Shell proxy");
                }
            }
            // Explorer may have restarted; rebuild the STA proxy once.
            match InternalBackend::create(self.build) {
                Ok(rebuilt) => {
                    self.native = Some(rebuilt);
                    if self.native.as_ref().is_some_and(|n| n.switch_to(index).is_ok()) {
                        crate::info!("switched to desktop {} after Shell proxy rebuild", index + 1);
                        self.publish_status();
                        return;
                    }
                }
                Err(e) => {
                    self.native_availability = BackendAvailability::Failed {
                        reason: e.to_string(),
                    };
                    self.native = None;
                }
            }
        }

        match self.fallback.switch_to(index) {
            Ok(()) => crate::info!(
                "switched toward virtual desktop {} via keyboard fallback (best effort)",
                index + 1
            ),
            Err(e) => crate::error_!("keyboard desktop fallback failed: {e}"),
        }
        self.publish_status();
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
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if hr.0 < 0 {
        let _ = ready.send(Err(Error::os("CoInitializeEx(desktop)", hr.0 as u32)));
        return;
    }
    let mut controller = match DesktopController::create(hwnd_raw) {
        Ok(controller) => controller,
        Err(e) => {
            let _ = ready.send(Err(e));
            unsafe { CoUninitialize(); }
            return;
        }
    };
    if let Some(native) = controller.native.as_ref() {
        match (native.desktop_count(), native.current_desktop()) {
            (Ok(count), Ok(current)) => crate::info!(
                "virtual desktops: count={count}, current={}",
                current + 1
            ),
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
    unsafe { CoUninitialize(); }
    crate::info!("virtual desktop controller stopped");
}
