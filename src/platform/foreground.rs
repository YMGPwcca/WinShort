//! Foreground application tracking via SetWinEventHook, never polling.

use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

use crate::error::{Error, Result};

static OWN_PID: AtomicU32 = AtomicU32::new(0);
static LAST_EXTERNAL_PID: AtomicU32 = AtomicU32::new(0);
static LAST_EXTERNAL_HWND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Last non-WinShort top-level window seen in the foreground (#26): lets the
/// overlay target "Foreground monitor" from a tray click, where the true
/// foreground is WinShort itself.
pub fn last_external_hwnd() -> Option<windows::Win32::Foundation::HWND> {
    let raw = LAST_EXTERNAL_HWND.load(std::sync::atomic::Ordering::Acquire);
    (raw != 0).then(|| windows::Win32::Foundation::HWND(raw as *mut _))
}

pub struct ForegroundTracker {
    hook: HWINEVENTHOOK,
}

impl ForegroundTracker {
    pub fn install() -> Result<Self> {
        OWN_PID.store(std::process::id(), Ordering::Release);
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if hook.is_invalid() {
            return Err(Error::os("SetWinEventHook(EVENT_SYSTEM_FOREGROUND)", 0));
        }
        // Seed from current foreground before the first transition.
        remember_if_external(unsafe { GetForegroundWindow() });
        Ok(Self { hook })
    }

    /// Current external foreground PID, falling back to the last valid external
    /// window when WinShort itself is active.
    pub fn target_pid(&self) -> Option<u32> {
        let hwnd = unsafe { GetForegroundWindow() };
        let current = pid_for_window(hwnd);
        let own = OWN_PID.load(Ordering::Acquire);
        if current != 0 && current != own {
            LAST_EXTERNAL_PID.store(current, Ordering::Release);
            Some(current)
        } else {
            let last = LAST_EXTERNAL_PID.load(Ordering::Acquire);
            (last != 0).then_some(last)
        }
    }
}

impl Drop for ForegroundTracker {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWinEvent(self.hook);
        }
    }
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    remember_if_external(hwnd);
}

fn remember_if_external(hwnd: HWND) {
    let pid = pid_for_window(hwnd);
    if pid != 0 && pid != OWN_PID.load(Ordering::Acquire) {
        LAST_EXTERNAL_PID.store(pid, Ordering::Release);
        // #26: remember the window itself for monitor targeting.
        if !hwnd.0.is_null() {
            LAST_EXTERNAL_HWND.store(hwnd.0 as usize, std::sync::atomic::Ordering::Release);
        }
    }
}

pub fn pid_for_window(hwnd: HWND) -> u32 {
    if hwnd.0.is_null() {
        return 0;
    }
    let mut pid = 0u32;
    unsafe {
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

/// Full executable path for `pid`, e.g. `C:\\Apps\\player.exe` (#46).
/// Unlike [`process_name`] this keeps the directory, so same-basename
/// installs at different locations remain distinguishable.
pub fn process_image_path(pid: u32) -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; 1024];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        Some(String::from_utf16_lossy(&buffer[..size as usize]))
    }
}

pub fn process_name(pid: u32) -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; 1024];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
    }
}
