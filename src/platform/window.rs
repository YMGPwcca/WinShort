//! Window class registration helpers and the per-window state slot.
//!
//! All windows in WinShort go through [`register_class`] so WndProc plumbing
//! is uniform: instance state lives in `GWLP_USERDATA` wrapped in
//! [`WindowState`], whose interior mutability makes reentrant Win32 dispatch
//! panic loudly instead of aliasing mutable state.

mod creation;
pub(crate) use creation::{WindowConstructionGuard, WindowCreation};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetWindowLongPtrW, RegisterClassExW, SetWindowLongPtrW, CS_HREDRAW, CS_VREDRAW,
    GWLP_USERDATA, WNDCLASSEXW, WNDPROC,
};

pub(crate) fn apply_chrome(hwnd: HWND, theme: crate::ui::theme::Theme) {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    unsafe {
        let dark: u32 = if theme.mode == crate::ui::theme::ThemeMode::Dark {
            1
        } else {
            0
        };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let preference = DWMWCP_ROUND.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&preference as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        let color = theme.bg;
        let caption = COLORREF(color.r as u32 | ((color.g as u32) << 8) | ((color.b as u32) << 16));
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            (&caption as *const COLORREF).cast(),
            std::mem::size_of::<COLORREF>() as u32,
        );
    }
}

/// Give shell thumbnails and window switching the embedded application icon.
/// Call on the owning UI thread after constructing the window.
pub(crate) fn set_application_icon(hwnd: HWND) -> Result<(), crate::error::Error> {
    use windows::Win32::UI::WindowsAndMessaging::{
        LoadIconW, SendMessageW, ICON_BIG, ICON_SMALL, WM_SETICON,
    };
    const APPLICATION_ICON_RESOURCE_ID: u16 = 1;
    // SAFETY: the executable module and its resource 1 (assets/winshort.ico)
    // live for the process lifetime. LoadIconW returns a shared HICON, which
    // must not be destroyed while Windows still uses it.
    let icon = unsafe {
        let module = GetModuleHandleW(None)
            .map_err(|error| crate::error::Error::win("GetModuleHandleW(icon)", &error))?;
        LoadIconW(
            Some(module.into()),
            PCWSTR(usize::from(APPLICATION_ICON_RESOURCE_ID) as *const u16),
        )
    }
    .map_err(|error| crate::error::Error::win("LoadIconW(application)", &error))?;
    for kind in [ICON_SMALL, ICON_BIG] {
        // SAFETY: the caller owns this HWND on the current UI thread, no state
        // borrow spans dispatch, and the shared icon outlives the window.
        unsafe {
            SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(kind as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
        }
    }
    Ok(())
}

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
/// [`state_cell`].
pub fn register_class(name: &str, wndproc: WNDPROC) -> Result<u16, crate::error::Error> {
    let hinstance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| crate::error::Error::win("GetModuleHandleW", &e))?;

    // SAFETY: `class` owns the wide string that `wc.lpszClassName` points at;
    // it must outlive the RegisterClassExW call below.
    let class = HSTRING::from(name);
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: wndproc,
        hInstance: hinstance.into(),
        hCursor: unsafe {
            windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                None,
                windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
            )
        }
        .map_err(|e| crate::error::Error::win("LoadCursorW", &e))?,
        hbrBackground: Default::default(),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };

    // SAFETY: `wc` outlives the call and the class name owner keeps the
    // wide string alive for its duration.
    let atom = unsafe { RegisterClassExW(&wc) };
    if atom == 0 {
        return Err(crate::error::Error::os("RegisterClassExW", unsafe {
            windows::Win32::Foundation::GetLastError().0
        }));
    }
    Ok(atom)
}

/// Store a boxed [`WindowState`] pointer into GWLP_USERDATA with the right
/// integer width for the target (#28).
pub fn store_state_ptr<T>(hwnd: HWND, state: Box<WindowState<T>>) {
    use windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW;
    // SAFETY: caller owns window creation flow; ptr ownership moves to slot.
    #[cfg(target_pointer_width = "64")]
    let value = Box::into_raw(state) as isize;
    #[cfg(target_pointer_width = "32")]
    let value = Box::into_raw(state) as i32;
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, value);
    }
}

/// Recover the state cell from a window created with a [`WindowState`] in
/// `GWLP_USERDATA` (stashed at `WM_NCCREATE`).
///
/// # Safety
/// Only call inside that window's own WndProc for a window whose slot holds
/// a `Box<WindowState<T>>`. The returned reference must not outlive the call;
/// take ownership with [`take_state`] on `WM_NCDESTROY`.
pub unsafe fn state_cell<'a, T>(hwnd: HWND) -> Option<&'a std::cell::RefCell<T>> {
    // SAFETY: caller guarantees the slot holds a valid Box<WindowState<T>>;
    // the raw deref only reads the pointer.
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState<T>;
        if ptr.is_null() {
            None
        } else {
            Some(&(*ptr).cell)
        }
    }
}

/// Take ownership of a window's state during teardown. Clears
/// `GWLP_USERDATA` before dropping, so the drop runs exactly once even if
/// the WndProc sees further messages during destruction.
///
/// # Safety
/// Same contract as [`state_cell`]; call exactly once, from `WM_NCDESTROY`.
pub unsafe fn take_state<T>(hwnd: HWND) -> Option<Box<WindowState<T>>> {
    // SAFETY: caller guarantees exactly-once invocation from WM_NCDESTROY;
    // the slot is cleared before reconstructing the Box.
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState<T>;
        if ptr.is_null() {
            None
        } else {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            Some(Box::from_raw(ptr))
        }
    }
}

/// Standard default handling.
pub fn def_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Cache successful class registration without panicking on a recoverable OS error.
/// UI window constructors call this on their owning message-loop thread.
pub fn register_class_once(
    cache: &std::sync::OnceLock<u16>,
    name: &str,
    wndproc: WNDPROC,
) -> Result<u16, crate::error::Error> {
    if let Some(atom) = cache.get() {
        return Ok(*atom);
    }
    let atom = register_class(name, wndproc)?;
    let _ = cache.set(atom);
    Ok(atom)
}

mod paint;
pub(crate) use paint::PaintSession;
