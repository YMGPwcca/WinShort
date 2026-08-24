//! Window class registration helpers and the per-window state slot.
//!
//! All windows in WinShort go through [`register_class`] so WndProc plumbing
//! is uniform: instance state lives in `GWLP_USERDATA` wrapped in
//! [`WindowState`], whose interior mutability makes reentrant Win32 dispatch
//! panic loudly instead of aliasing mutable state.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetWindowLongPtrW, RegisterClassExW, SetWindowLongPtrW, GWLP_USERDATA,
    WNDCLASSEXW, WNDPROC, WM_NCCREATE, CS_HREDRAW, CS_VREDRAW,
};

/// Interior-mutable per-window state stored (boxed) in `GWLP_USERDATA`.
///
/// The WndProc recovers the cell with [`state_cell`] and scopes a
/// `borrow_mut()` per message arm. A second borrow while one is live means
/// reentrant dispatch into the same WndProc — that panics by design.
pub struct WindowState<T> {
    pub cell: std::cell::RefCell<T>,
}

impl<T> WindowState<T> {
    pub fn new(state: T) -> Box<Self> {
        Box::new(Self {
            cell: std::cell::RefCell::new(state),
        })
    }
}

/// Register a window class. Returns the class atom. Idempotent per name is
/// NOT handled here — call once per class at startup.
///
/// `proc` must be a plain function; it recovers its state via
/// [`userdata`](Self::userdata).
pub fn register_class<T>(name: &str, wndproc: WNDPROC) -> Result<u16, crate::error::Error> {
    let hinstance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| crate::error::Error::win("GetModuleHandleW", &e))?;

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: wndproc,
        hInstance: hinstance.into(),
        hCursor: unsafe { windows::Win32::UI::WindowsAndMessaging::LoadCursorW(None, windows::Win32::UI::WindowsAndMessaging::IDC_ARROW) }
            .map_err(|e| crate::error::Error::win("LoadCursorW", &e))?,
        hbrBackground: Default::default(),
        lpszClassName: PCWSTR(HSTRING::from(name).as_ptr()),
        ..Default::default()
    };

    let atom = unsafe { RegisterClassExW(&wc) };
    if atom == 0 {
        return Err(crate::error::Error::os("RegisterClassExW", unsafe {
            windows::Win32::Foundation::GetLastError().0
        }));
    }
    Ok(atom)
}


/// Recover the state cell from a window created with a [`WindowState`] in
/// `GWLP_USERDATA` (stashed at `WM_NCCREATE`).
///
/// # Safety
/// Only call inside that window's own WndProc for a window whose slot holds
/// a `Box<WindowState<T>>`. The returned reference must not outlive the call;
/// take ownership with [`take_state`] on `WM_NCDESTROY`.
pub unsafe fn state_cell<'a, T>(hwnd: HWND) -> Option<&'a std::cell::RefCell<T>> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const std::cell::RefCell<T>;
    if ptr.is_null() {
        None
    } else {
        Some(&*ptr)
    }
}

/// Take ownership of a window's state during teardown. Clears
/// `GWLP_USERDATA` before dropping, so the drop runs exactly once even if
/// the WndProc sees further messages during destruction.
///
/// # Safety
/// Same contract as [`state_cell`]; call exactly once, from `WM_NCDESTROY`.
pub unsafe fn take_state<T>(hwnd: HWND) -> Option<Box<WindowState<T>>> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState<T>;
    if ptr.is_null() {
        None
    } else {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        Some(Box::from_raw(ptr))
    }
}


/// Standard default handling.
pub fn def_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// True when the message is WM_NCCREATE (used by generic procs to stash state).
pub fn is_nccreate(msg: u32) -> bool {
    msg == WM_NCCREATE
}
