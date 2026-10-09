//! Pointer observation for click-through windows. Coalesced wake-only messages
//! keep the mouse callback short; it always forwards input to the next hook.

use super::timeline::{MotionPolicy, TIMER_MS};
use crate::error::{Error, Result};
use std::cell::Cell;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK,
    MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_MOUSEMOVE,
};

pub(super) const POINTER_MESSAGE: u32 = 0x8120;
const HOVER_MS: u64 = 140;
const MAX_CARDS: usize = super::manager::OverlayKey::ALL.len();

#[cfg(test)]
pub(super) fn observer_active() -> bool {
    HOOK.get() != 0
}

thread_local! {
    static TARGETS: Cell<[isize; MAX_CARDS]> = const { Cell::new([0; MAX_CARDS]) };
    static PENDING: Cell<u16> = const { Cell::new(0) };
    static POINTER: Cell<POINT> = const { Cell::new(POINT { x: 0, y: 0 }) };
    static HOOK: Cell<isize> = const { Cell::new(0) };
}

pub(super) fn register(hwnd: HWND, enabled: bool) -> Result<()> {
    if !enabled {
        unregister(hwnd);
        return Ok(());
    }
    let mut targets = TARGETS.get();
    let handle = hwnd.0 as isize;
    if targets.contains(&handle) {
        refresh(hwnd);
        return Ok(());
    }
    let Some(slot) = targets.iter_mut().find(|value| **value == 0) else {
        return Err(Error::internal("overlay hover target capacity exhausted"));
    };
    if HOOK.get() == 0 {
        // SAFETY: the executable module outlives this callback. The low-level
        // hook runs on this installing thread, which owns all TARGETS HWNDs and
        // pumps their messages; unregister releases the hook on the same thread.
        let module = unsafe { GetModuleHandleW(None) }
            .map_err(|error| Error::win("GetModuleHandleW(overlay hover)", &error))?;
        let hook =
            unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module.into()), 0) }
                .map_err(|error| Error::win("SetWindowsHookExW(overlay hover)", &error))?;
        HOOK.set(hook.0 as isize);
    }
    *slot = handle;
    TARGETS.set(targets);
    refresh(hwnd);
    Ok(())
}

pub(super) fn unregister(hwnd: HWND) {
    let mut targets = TARGETS.get();
    for (index, target) in targets.iter_mut().enumerate() {
        if *target == hwnd.0 as isize {
            *target = 0;
            PENDING.set(PENDING.get() & !(1 << index));
        }
    }
    TARGETS.set(targets);
    if targets.iter().all(|target| *target == 0) {
        let hook = HOOK.replace(0);
        if hook != 0 {
            // SAFETY: HOOK contains the hook created by this thread. Clear it
            // before release so repeated teardown cannot reuse a freed handle.
            let _ = unsafe { UnhookWindowsHookEx(HHOOK(hook as *mut _)) };
        }
    }
}

fn enqueue(index: usize, hwnd: isize) {
    let mask = 1 << index;
    if PENDING.get() & mask != 0 {
        return;
    }
    PENDING.set(PENDING.get() | mask);
    // SAFETY: targets are registered/unregistered by their owning thread. No
    // pointer payload crosses the queue; destroyed HWNDs simply reject the wake.
    if unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut _)),
            POINTER_MESSAGE,
            WPARAM(0),
            LPARAM(0),
        )
    }
    .is_err()
    {
        PENDING.set(PENDING.get() & !mask);
    }
}

pub(super) fn refresh(hwnd: HWND) {
    let mut point = POINT::default();
    // SAFETY: the API writes a POINT into this valid stack allocation.
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return;
    }
    POINTER.set(point);
    if let Some(index) = TARGETS
        .get()
        .iter()
        .position(|target| *target == hwnd.0 as isize)
    {
        enqueue(index, hwnd.0 as isize);
    }
}

pub(super) fn take_pointer(hwnd: HWND) -> POINT {
    if let Some(index) = TARGETS
        .get()
        .iter()
        .position(|target| *target == hwnd.0 as isize)
    {
        PENDING.set(PENDING.get() & !(1 << index));
    }
    POINTER.get()
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_MOUSEMOVE {
        // SAFETY: a nonnegative WH_MOUSE_LL notification supplies a live
        // MSLLHOOKSTRUCT for this synchronous callback. Only copy its POINT.
        let point = unsafe { (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt };
        POINTER.set(point);
        for (index, hwnd) in TARGETS.get().into_iter().enumerate() {
            if hwnd != 0 {
                enqueue(index, hwnd);
            }
        }
    }
    // SAFETY: forward the original callback arguments unmodified, including
    // negative codes, so observation never consumes another application's input.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[derive(Debug)]
pub(super) struct HoverMotion {
    from: f32,
    target: f32,
    started: Option<Instant>,
}

impl Default for HoverMotion {
    fn default() -> Self {
        Self {
            from: 1.0,
            target: 1.0,
            started: None,
        }
    }
}

impl HoverMotion {
    pub(super) fn is_animating(&self) -> bool {
        self.started.is_some()
    }
    pub(super) fn value(&self, now: Instant) -> f32 {
        let Some(started) = self.started else {
            return self.target;
        };
        let t = (now.saturating_duration_since(started).as_secs_f32() / (HOVER_MS as f32 / 1000.0))
            .clamp(0.0, 1.0);
        self.from + (self.target - self.from) * (1.0 - (1.0 - t).powi(3))
    }

    pub(super) fn set_target(&mut self, target: f32, motion: MotionPolicy, now: Instant) -> bool {
        let target = target.clamp(0.1, 1.0);
        if self.target == target {
            return false;
        }
        self.from = self.value(now);
        self.target = target;
        self.started = (motion == MotionPolicy::Animated).then_some(now);
        true
    }

    pub(super) fn tick(&mut self, now: Instant) {
        if self.started.is_some_and(|started| {
            now.saturating_duration_since(started) >= Duration::from_millis(HOVER_MS)
        }) {
            self.started = None;
        }
    }

    pub(super) fn timer_interval(&self) -> Option<u32> {
        self.started.map(|_| TIMER_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_fade_can_reverse_and_stops_when_settled() {
        let now = Instant::now();
        let mut hover = HoverMotion::default();
        assert!(hover.set_target(0.3, MotionPolicy::Animated, now));
        let mid = now + Duration::from_millis(70);
        let value = hover.value(mid);
        assert!(value > 0.3 && value < 1.0);
        assert!(hover.set_target(1.0, MotionPolicy::Animated, mid));
        assert_eq!(hover.value(mid), value);
        let end = mid + Duration::from_millis(HOVER_MS);
        hover.tick(end);
        assert_eq!(hover.value(end), 1.0);
        assert_eq!(hover.timer_interval(), None);
    }

    #[test]
    fn reduced_motion_is_immediate_and_off_does_not_arm_a_timer() {
        let now = Instant::now();
        let mut hover = HoverMotion::default();
        assert!(!hover.set_target(1.0, MotionPolicy::Animated, now));
        assert!(hover.set_target(0.3, MotionPolicy::Reduced, now));
        assert_eq!(hover.value(now), 0.3);
        assert_eq!(hover.timer_interval(), None);
    }
}
