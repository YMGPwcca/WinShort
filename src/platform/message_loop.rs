//! Standard Win32 message loop.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

/// Run GetMessageW until WM_QUIT. Returns the exit code.
pub fn run() -> i32 {
    let mut msg = MSG::default();
    loop {
        // SAFETY: msg is a valid out param; no filter window = all messages.
        let res = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if res.0 == -1 {
            // GetMessageW failing repeatedly means the thread's queue is dead:
            // terminate with a nonzero exit code instead of spinning (#24).
            crate::error_!(
                "GetMessageW failed: {}; terminating message loop",
                unsafe { windows::Win32::Foundation::GetLastError().0 }
            );
            return 1;
        }
        if res.0 == 0 {
            return msg.wParam.0 as i32; // WM_QUIT carries the exit code
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Post WM_QUIT with a code (thread-safe).
pub fn quit(code: i32) {
    use windows::Win32::UI::WindowsAndMessaging::PostQuitMessage;
    unsafe { PostQuitMessage(code) }
}

// Re-exported for WndProc signatures used across app modules.
#[allow(unused)]
pub type ProcArgs = (HWND, u32, WPARAM, LPARAM);
#[allow(dead_code)]
fn unused(_: LPARAM) {}
