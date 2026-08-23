//! Window class registration and creation helpers.
//!
//! All windows in WinShort go through here so WndProc plumbing is uniform:
//! a class gets a `wndproc`, and instance state is recovered from the
//! `GWLP_USERDATA` slot set at creation.

use std::ffi::c_void;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassExW, SetWindowLongPtrW,
    CW_USEDEFAULT, WINDOW_EX_STYLE, WM_NCCREATE, WNDCLASSEXW, WNDPROC,
    CS_HREDRAW, CS_VREDRAW,
};
use windows::Win32::Graphics::Gdi::COLOR_WINDOW;

/// Extra data passed through `WM_NCCREATE` into `GWLP_USERDATA`.
pub struct WindowParams<T> {
    pub state: T,
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

/// Create a window of a registered class, storing `state: T` in GWLP_USERDATA.
///
/// # Safety contract
/// The window must never outlive `T`; the owner drops `T` on `WM_NCDESTROY`.
pub fn create_window<T>(
    class: &str,
    title: &str,
    style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    size: (i32, i32),
    state: Box<T>,
) -> Result<HWND, crate::error::Error> {
    let hinstance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| crate::error::Error::win("GetModuleHandleW", &e))?;

    let class_pc = PCWSTR(HSTRING::from(class).as_ptr());
    let title_pc = PCWSTR(HSTRING::from(title).as_ptr());

    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_pc,
            title_pc,
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            size.0,
            size.1,
            None,
            None,
            Some(hinstance.into()),
            Some(Box::into_raw(state) as *mut c_void),
        )
    };
    hwnd.map_err(|e| crate::error::Error::win("CreateWindowExW", &e))
}

#[allow(dead_code)]
fn unused_last_error() -> u32 {
    unsafe { windows::Win32::Foundation::GetLastError().0 }
}

/// Recover `T` from a window's user data slot.
///
/// # Safety
/// Only call inside that window's own WndProc for a window created via
/// [`create_window`] with the same `T`.
pub unsafe fn userdata<T>(hwnd: HWND) -> Option<&'static mut T> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut T;
    if ptr.is_null() {
        None
    } else {
        Some(&mut *ptr)
    }
}

use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWLP_USERDATA};

/// Standard default handling.
pub fn def_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// True when the message is WM_NCCREATE (used by generic procs to stash state).
pub fn is_nccreate(msg: u32) -> bool {
    msg == WM_NCCREATE
}
