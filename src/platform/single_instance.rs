//! Single-instance enforcement via a named mutex plus an activation event
//! (spec §7). A secondary instance signals the primary to open Settings and exits.

use windows::Win32::Foundation::{HANDLE, GetLastError, WAIT_TIMEOUT, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject,
};
use windows::core::{HSTRING, PCWSTR};

const MUTEX_NAME: &str = r"Local\WinShort.SingleInstance.Mutex";
const ACTIVATE_EVENT: &str = r"Local\WinShort.SingleInstance.Activate";

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
///
/// The primary creates both objects; a secondary opens the activation event
/// (creating it if racing), signals it, and reports `Secondary`.
pub fn acquire() -> Result<InstanceRole, crate::error::Error> {
    // SAFETY: named kernel objects; handles kept alive in PrimaryRole.
    unsafe {
        let mutex = CreateMutexW(None, false, PCWSTR(HSTRING::from(MUTEX_NAME).as_ptr()))
            .map_err(|e| crate::error::Error::win("CreateMutexW", &e))?;
        if GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS {
            // ERROR_ALREADY_EXISTS: signal the primary, then leave.
            let ev = CreateEventW(None, false, false, PCWSTR(HSTRING::from(ACTIVATE_EVENT).as_ptr()))
                .unwrap_or_default();
            if !ev.is_invalid() {
                let _ = SetEvent(ev);
            }
            return Ok(InstanceRole::Secondary);
        }
        let activate_event =
            CreateEventW(None, false, false, PCWSTR(HSTRING::from(ACTIVATE_EVENT).as_ptr()))
                .map_err(|e| crate::error::Error::win("CreateEventW", &e))?;
        Ok(InstanceRole::Primary(PrimaryRole { mutex, activate_event }))
    }
}

/// Kernel handle transferable to the watcher thread. Only ever waited on.
struct SendHandle(HANDLE);
// SAFETY: HANDLE is a raw kernel object reference; the watcher thread is the
// sole user after transfer and the process owns the object for its lifetime.
unsafe impl Send for SendHandle {}

/// Watch the activation event; on each signal invoke `on_activate` (which should
/// post ShowSettings to the main window). Exits when `stop` flips true.
///
/// Runs on a dedicated plain thread with no COM.
pub fn spawn_watcher(
    event: HANDLE,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    on_activate: impl Fn() + Send + 'static,
) -> std::thread::JoinHandle<()> {
    let event = SendHandle(event);
    // Plain function body: avoids edition-2021 field-wise capture of the raw
    // HANDLE inside the closure (the wrapper exists precisely to be opaque).
    std::thread::spawn(move || watch(event, stop, on_activate))
}

fn watch(
    event: SendHandle,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    on_activate: impl Fn() + Send + 'static,
) {
    loop {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        // SAFETY: handle valid for process lifetime (PrimaryRole).
        match unsafe { WaitForSingleObject(event.0, 250) } {
            WAIT_TIMEOUT => continue,
            WAIT_OBJECT_0 => on_activate(),
            _ => continue,
        }
    }
}
