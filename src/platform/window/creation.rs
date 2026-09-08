//! Transfer native window state exactly once, and roll back incomplete HWND construction.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;

/// Keeps state Rust-owned until the synchronous WM_NCCREATE callback accepts it.
/// A failure before that callback simply drops the unconsumed value.
pub(crate) struct WindowCreation<T> {
    state: Option<T>,
}

impl<T> WindowCreation<T> {
    pub(crate) fn new(state: T) -> Self {
        Self { state: Some(state) }
    }

    /// The pointer is only valid while this value remains at its current address.
    /// Pass it directly to the synchronous CreateWindowExW call, never retain it.
    pub(crate) fn parameter(&mut self) -> *const c_void {
        (self as *mut Self).cast()
    }

    /// # Safety
    /// `parameter` must be the pointer supplied by `WindowCreation<T>::parameter`
    /// to this window's currently executing CreateWindowExW call. That call must
    /// not have returned. Only WM_NCCREATE may consume it, on the creating thread.
    pub(crate) unsafe fn take_from(parameter: *mut c_void) -> Option<T> {
        // SAFETY: the caller guarantees the live creation slot and matching T.
        unsafe { parameter.cast::<Self>().as_mut()?.state.take() }
    }
}

/// Owns a newly created window until all fallible constructor steps succeed.
/// It is intentionally thread-bound: DestroyWindow belongs to the creating thread.
pub(crate) struct WindowConstructionGuard {
    hwnd: HWND,
    _thread_bound: PhantomData<*mut ()>,
}

impl WindowConstructionGuard {
    /// # Safety
    /// The caller must exclusively own this newly created HWND on its creating
    /// thread. No other owner may destroy it until this guard is completed.
    pub(crate) unsafe fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            _thread_bound: PhantomData,
        }
    }

    pub(crate) fn complete(self) -> HWND {
        let guard = ManuallyDrop::new(self);
        guard.hwnd
    }
}

impl Drop for WindowConstructionGuard {
    fn drop(&mut self) {
        // SAFETY: construction transfers one live, thread-owned HWND to this guard.
        // DestroyWindow also tears down any child controls created so far.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}
