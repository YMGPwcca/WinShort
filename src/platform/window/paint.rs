//! A WM_PAINT validation pair cannot be abandoned by an early return.

use std::marker::PhantomData;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, HDC, PAINTSTRUCT};

pub(crate) struct PaintSession {
    hwnd: HWND,
    paint: PAINTSTRUCT,
    _owning_thread: PhantomData<*mut ()>,
}

impl PaintSession {
    /// Call while processing WM_PAINT on the window's message-loop thread.
    pub(crate) fn begin(hwnd: HWND) -> Self {
        let mut paint = PAINTSTRUCT::default();
        // SAFETY: BeginPaint validates HWND; the output storage lives with this guard.
        unsafe { BeginPaint(hwnd, &mut paint) };
        Self {
            hwnd,
            paint,
            _owning_thread: PhantomData,
        }
    }

    pub(crate) fn dc(&self) -> HDC {
        self.paint.hdc
    }
}

impl Drop for PaintSession {
    fn drop(&mut self) {
        // SAFETY: the guard retains the matching HWND/PAINTSTRUCT and cannot move
        // to another thread. Rendering returns before the validation pair closes.
        let _ = unsafe { EndPaint(self.hwnd, &self.paint) };
    }
}
