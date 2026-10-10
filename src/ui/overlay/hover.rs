//! Pointer observation for click-through windows. Coalesced wake-only messages
//! keep the mouse callback short; it always forwards input to the next hook.

use super::timeline::{MotionPolicy, TIMER_MS};
use crate::error::{Error, Result};
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK,
    MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_MOUSEMOVE,
};

pub(super) const POINTER_MESSAGE: u32 = 0x8120;
const HOVER_MS: u64 = 140;

#[cfg(test)]
pub(super) fn observer_active() -> bool {
    HOOK.get() != 0
}

thread_local! {
    static TARGETS: RefCell<Vec<HoverTarget>> = const { RefCell::new(Vec::new()) };
    static POINTER: Cell<POINT> = const { Cell::new(POINT { x: 0, y: 0 }) };
    static HOOK: Cell<isize> = const { Cell::new(0) };
}

struct HoverTarget {
    hwnd: isize,
    pending: bool,
}

pub(super) fn register(hwnd: HWND, enabled: bool) -> Result<()> {
    if !enabled {
        unregister(hwnd);
        return Ok(());
    }
    let handle = hwnd.0 as isize;
    if TARGETS.with_borrow(|targets| targets.iter().any(|target| target.hwnd == handle)) {
        refresh(hwnd);
        return Ok(());
    }
    if HOOK.get() == 0 {
        // This callback runs on the installing UI thread; it only queues wakes.
        let module = unsafe { GetModuleHandleW(None) }
            .map_err(|e| Error::win("GetModuleHandleW(overlay hover)", &e))?;
        let hook =
            unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module.into()), 0) }
                .map_err(|e| Error::win("SetWindowsHookExW(overlay hover)", &e))?;
        HOOK.set(hook.0 as isize);
    }
    TARGETS.with_borrow_mut(|targets| {
        targets.push(HoverTarget {
            hwnd: handle,
            pending: false,
        })
    });
    refresh(hwnd);
    Ok(())
}

pub(super) fn unregister(hwnd: HWND) {
    let empty = TARGETS.with_borrow_mut(|targets| {
        targets.retain(|target| target.hwnd != hwnd.0 as isize);
        targets.is_empty()
    });
    if empty {
        let hook = HOOK.replace(0);
        if hook != 0 {
            let _ = unsafe { UnhookWindowsHookEx(HHOOK(hook as *mut _)) };
        }
    }
}

fn enqueue(target: &mut HoverTarget) {
    if target.pending {
        return;
    }
    target.pending = true;
    if unsafe {
        PostMessageW(
            Some(HWND(target.hwnd as *mut _)),
            POINTER_MESSAGE,
            WPARAM(0),
            LPARAM(0),
        )
    }
    .is_err()
    {
        target.pending = false;
    }
}

pub(super) fn refresh(hwnd: HWND) {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return;
    }
    POINTER.set(point);
    TARGETS.with_borrow_mut(|targets| {
        if let Some(target) = targets
            .iter_mut()
            .find(|target| target.hwnd == hwnd.0 as isize)
        {
            enqueue(target);
        }
    });
}

pub(super) fn take_pointer(hwnd: HWND) -> POINT {
    TARGETS.with_borrow_mut(|targets| {
        if let Some(target) = targets
            .iter_mut()
            .find(|target| target.hwnd == hwnd.0 as isize)
        {
            target.pending = false;
        }
    });
    POINTER.get()
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_MOUSEMOVE {
        let point = unsafe { (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt };
        POINTER.set(point);
        TARGETS.with_borrow_mut(|targets| {
            for target in targets {
                enqueue(target);
            }
        });
    }
    // Observation must always forward input unmodified.
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
