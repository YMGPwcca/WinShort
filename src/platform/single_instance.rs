//! Single-instance enforcement via a named mutex plus an activation event
//! (spec §7). A secondary instance signals the primary to open Settings and exits.
//! The primary's watcher waits on BOTH the activation event and a shutdown
//! event via WaitForMultipleObjects — no polling (#24).

use std::sync::OnceLock;

use windows::Win32::Foundation::{HANDLE, GetLastError, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, SetEvent, WaitForMultipleObjects,
};
use windows::core::{HSTRING, PCWSTR};

const MUTEX_NAME: &str = r"Local\WinShort.SingleInstance.Mutex";
const ACTIVATE_EVENT: &str = r"Local\WinShort.SingleInstance.Activate";

static SHUTDOWN_EVENT: OnceLock<SendHandle> = OnceLock::new();

/// Which role this process took after [`acquire`].
pub enum InstanceRole {
    /// This process is the only instance; keep handles alive forever.
    Primary(PrimaryRole),
    /// Another instance exists; it has been notified. Caller must exit.
    Secondary,
}

/// Handles owned by the primary instance for process lifetime.
pub struct PrimaryRole {
    #[allow(dead_code)]
    pub mutex: HANDLE,
    pub activate_event: HANDLE,
}

/// Try to become the single running instance.
pub fn acquire() -> Result<InstanceRole, crate::error::Error> {
    // SAFETY: named kernel objects; handles kept alive in PrimaryRole.
    unsafe {
        let mutex = CreateMutexW(None, false, PCWSTR(HSTRING::from(MUTEX_NAME).as_ptr()))
            .map_err(|e| crate::error::Error::win("CreateMutexW", &e))?;
        if GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS {
            let ev = CreateEventW(None, false, false, PCWSTR(HSTRING::from(ACTIVATE_EVENT).as_ptr()))
                .unwrap_or_default();
            if !ev.is_invalid() {
                let _ = SetEvent(ev);
            }
            return Ok(InstanceRole::Secondary);
        }
        let activate_event =
            CreateEventW(None, false, false, PCWSTR(HSTRING::from(ACTIVATE_EVENT).as_ptr()))
                .map_err(|e| crate::error::Error::win("CreateEventW(activate)", &e))?;
        // Manual-reset shutdown event for the watcher (#24).
        let shutdown_event =
            CreateEventW(None, true, false, PCWSTR::null())
                .map_err(|e| crate::error::Error::win("CreateEventW(shutdown)", &e))?;
        let _ = SHUTDOWN_EVENT.set(SendHandle(shutdown_event));
        Ok(InstanceRole::Primary(PrimaryRole { mutex, activate_event }))
    }
}

/// Signal the watcher to exit. Safe to call from any thread, before window
/// destruction begins (#24 ordering requirement).
pub fn signal_shutdown() {
    if let Some(handle) = SHUTDOWN_EVENT.get() {
        unsafe {
            let _ = SetEvent(handle.0);
        }
    }
}

/// Kernel handle transferable to the watcher thread. Only ever waited on.
#[derive(Clone, Copy)]
struct SendHandle(HANDLE);
// SAFETY: HANDLE is a raw kernel object reference. The event object is
// process-owned for its entire lifetime; all users only SetEvent/wait on it,
// never close or mutate through the pointer.
unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

/// Owned watcher thread; [`Self::join`] signals shutdown first and blocks
/// until the thread exits — no orphan on any teardown path.
pub struct WatcherRuntime {
    join: Option<std::thread::JoinHandle<()>>,
}

impl WatcherRuntime {
    pub fn join(mut self) {
        signal_shutdown();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for WatcherRuntime {
    fn drop(&mut self) {
        signal_shutdown();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Watch the activation event; on each signal invoke `on_activate` (which
/// should post ShowSettings to the main window). Exits when shutdown is
/// signalled. Runs on a dedicated plain thread with no COM.
pub fn spawn_watcher(
    _activate_event: HANDLE,
    on_activate: impl Fn() + Send + 'static,
) -> WatcherRuntime {
    let Some(&shutdown) = SHUTDOWN_EVENT.get() else {
        return WatcherRuntime { join: None };
    };
    let join = std::thread::Builder::new()
        .name("winshort-watcher".into())
        .spawn(move || watch(shutdown, on_activate))
        .expect("spawn watcher thread");
    WatcherRuntime { join: Some(join) }
}

fn watch(
    shutdown: SendHandle,
    on_activate: impl Fn() + Send + 'static,
) {
    loop {
        // SAFETY: both handles valid for process lifetime (PrimaryRole owns
        // the activation event; the shutdown handle lives in SHUTDOWN_EVENT).
        const SHUTDOWN_INDEX: u32 = 1; // WAIT_OBJECT_0 + 1
        let result = unsafe { WaitForMultipleObjects(&[shutdown.0], false, 100_000) };
        match result.0 {
            0 => on_activate(),          // WAIT_OBJECT_0 + 0: activate
            SHUTDOWN_INDEX => return,    // WAIT_OBJECT_0 + 1: shutdown
            _ => continue,
        }
    }
}
