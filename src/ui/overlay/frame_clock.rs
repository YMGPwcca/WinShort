//! One high-resolution wake source for all overlay windows. The worker never
//! renders or borrows HWND state; generation-tagged messages return to the UI.

use crate::error::{Error, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::Threading::{
    CancelWaitableTimer, CreateEventW, CreateWaitableTimerExW, SetEvent, SetWaitableTimerEx,
    WaitForMultipleObjects, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub(super) const FRAME_MESSAGE: u32 = 0x8121;

struct OwnedHandle(HANDLE);
// SAFETY: these unnamed synchronization handles are usable across threads;
// the Arc/join ownership below keeps them open through every wait and signal.
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this is the only owning wrapper for this kernel handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WakeMode {
    Frame(Duration),
    Deadline(Duration),
}
impl WakeMode {
    fn delay(self) -> Duration {
        match self {
            Self::Frame(value) | Self::Deadline(value) => value,
        }
    }
}

struct Target {
    hwnd: isize,
    id: usize,
    mode: WakeMode,
    due: Instant,
    pending: bool,
}
impl Target {
    fn update(&mut self, id: usize, mode: WakeMode) {
        // Frame mode retains its phase grid. Deadline delays are relative to
        // the newest plan and must rearm even when the delay is unchanged.
        if self.id == id && self.mode == mode && matches!(mode, WakeMode::Frame(_)) {
            return;
        }
        let pending = self.id == id && self.pending;
        *self = Self {
            hwnd: self.hwnd,
            id,
            mode,
            due: Instant::now() + mode.delay(),
            pending,
        };
    }
}
struct Shared {
    targets: Mutex<Vec<Target>>,
    changed: OwnedHandle,
    stopping: AtomicBool,
    failed: AtomicBool,
}

pub(super) struct FrameClock {
    shared: Arc<Shared>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl FrameClock {
    pub(super) fn create() -> Result<Arc<Self>> {
        // SAFETY: unnamed, non-inheritable event/timer handles have one owner.
        let changed = OwnedHandle(
            unsafe { CreateEventW(None, false, false, None) }
                .map_err(|e| Error::win("CreateEventW(overlay clock)", &e))?,
        );
        let timer = OwnedHandle(
            unsafe {
                CreateWaitableTimerExW(
                    None,
                    None,
                    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                    TIMER_ALL_ACCESS.0,
                )
            }
            .or_else(|_| unsafe { CreateWaitableTimerExW(None, None, 0, TIMER_ALL_ACCESS.0) })
            .map_err(|e| Error::win("CreateWaitableTimerExW(overlay clock)", &e))?,
        );
        let shared = Arc::new(Shared {
            targets: Mutex::new(Vec::new()),
            changed,
            stopping: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        });
        let thread_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("overlay-frame-clock".into())
            .spawn(move || {
                if let Err(error) = run(&thread_shared, &timer) {
                    thread_shared.failed.store(true, Ordering::Release);
                    crate::warn_!("overlay frame clock failed: {error}");
                }
            })
            .map_err(|e| Error::internal(format!("overlay clock worker: {e}")))?;
        Ok(Arc::new(Self {
            shared,
            worker: Some(worker),
        }))
    }

    pub(super) fn arm(&self, hwnd: HWND, id: usize, mode: Option<WakeMode>) -> Result<()> {
        if self.shared.failed.load(Ordering::Acquire) {
            return Err(Error::internal("overlay clock worker unavailable"));
        }
        let mut targets = self.shared.targets.lock().expect("overlay clock targets");
        let handle = hwnd.0 as isize;
        let index = targets.iter().position(|target| target.hwnd == handle);
        match (index, mode) {
            (Some(index), None) => {
                targets.swap_remove(index);
            }
            (Some(index), Some(mode)) => {
                // Keep the frame deadline on its previous cadence; UI render
                // time must not get added to every refresh interval.
                targets[index].update(id, mode);
            }
            (None, Some(mode)) => {
                targets.push(Target {
                    hwnd: handle,
                    id,
                    mode,
                    due: Instant::now() + mode.delay(),
                    pending: false,
                });
            }
            (None, None) => return Ok(()),
        }
        drop(targets);
        // SAFETY: shared owns the event throughout this synchronous signal.
        unsafe { SetEvent(self.shared.changed.0) }
            .map_err(|e| Error::win("SetEvent(overlay clock)", &e))
    }

    pub(super) fn acknowledge(&self, hwnd: HWND, id: usize) -> bool {
        let mut targets = self.shared.targets.lock().expect("overlay clock targets");
        let Some(target) = targets
            .iter_mut()
            .find(|t| t.hwnd == hwnd.0 as isize && t.id == id)
        else {
            return false;
        };
        if !target.pending {
            return false;
        }
        target.pending = false;
        true
    }
    #[cfg(test)]
    pub(super) fn active_targets(&self) -> usize {
        self.shared.targets.lock().unwrap().len()
    }
}

impl Drop for FrameClock {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        // SAFETY: the event remains owned until after the worker has joined.
        let _ = unsafe { SetEvent(self.shared.changed.0) };
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn next_frame(due: Instant, now: Instant, period: Duration) -> Instant {
    let period_ns = period.as_nanos().max(1);
    let skipped = now.saturating_duration_since(due).as_nanos() / period_ns + 1;
    due + Duration::from_nanos((period_ns * skipped).min(u64::MAX as u128) as u64)
}

fn dispatch_due(shared: &Shared, now: Instant) -> Option<Instant> {
    let mut targets = shared.targets.lock().expect("overlay clock targets");
    for target in targets.iter_mut().filter(|t| t.due <= now) {
        if !target.pending {
            target.pending = true;
            // SAFETY: no HWND state is dereferenced here. The receiver validates
            // this assignment's generation ID; removal and handle reuse are safe.
            if unsafe {
                PostMessageW(
                    Some(HWND(target.hwnd as *mut _)),
                    FRAME_MESSAGE,
                    WPARAM(target.id),
                    LPARAM(0),
                )
            }
            .is_err()
            {
                target.hwnd = 0;
            }
        }
        target.due = match target.mode {
            WakeMode::Frame(period) => next_frame(target.due, now, period),
            // A deadline is a one-shot wake; only the UI can arm the next phase.
            WakeMode::Deadline(_) => now + Duration::from_secs(86400),
        };
    }
    targets.retain(|target| target.hwnd != 0);
    targets
        .iter()
        .filter(|t| !(t.pending && matches!(t.mode, WakeMode::Deadline(_))))
        .map(|t| t.due)
        .min()
}

fn run(shared: &Shared, timer: &OwnedHandle) -> Result<()> {
    while !shared.stopping.load(Ordering::Acquire) {
        let now = Instant::now();
        if let Some(deadline) = dispatch_due(shared, now) {
            let ticks = deadline
                .saturating_duration_since(Instant::now())
                .as_nanos()
                .div_ceil(100)
                .clamp(1, i64::MAX as u128) as i64;
            let due = -ticks;
            // SAFETY: timer is worker-owned, due is a live stack i64, and no
            // callback/APC or pointer is retained by this one-shot timer.
            unsafe { SetWaitableTimerEx(timer.0, &due, 0, None, None, None, 0) }
                .map_err(|e| Error::win("SetWaitableTimerEx(overlay clock)", &e))?;
        } else {
            // SAFETY: only this worker arms/cancels the timer.
            let _ = unsafe { CancelWaitableTimer(timer.0) };
        }
        // SAFETY: both handles stay owned for the full wait. No lock is held,
        // and shutdown signals changed before joining this worker.
        let result =
            unsafe { WaitForMultipleObjects(&[shared.changed.0, timer.0], false, INFINITE) };
        if result != WAIT_OBJECT_0 && result.0 != WAIT_OBJECT_0.0 + 1 {
            return Err(Error::internal("overlay clock wait failed"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_app_targets_are_not_limited_to_the_fixed_notification_keys() {
        let clock = FrameClock::create().unwrap();
        for id in 1..=40 {
            clock
                .arm(
                    HWND(id as *mut _),
                    id,
                    Some(WakeMode::Deadline(Duration::from_secs(86400))),
                )
                .unwrap();
        }
        assert_eq!(clock.active_targets(), 40);
        for id in 1..=40 {
            clock.arm(HWND(id as *mut _), id, None).unwrap();
        }
        assert_eq!(clock.active_targets(), 0);
    }

    #[test]
    fn repeated_deadline_plan_rearms_after_a_one_shot_wake() {
        let now = Instant::now();
        let mode = WakeMode::Deadline(Duration::from_millis(1));
        let mut target = Target {
            hwnd: 1,
            id: 2,
            mode,
            due: now + Duration::from_secs(86400),
            pending: false,
        };
        target.update(2, mode);
        assert!(target.due < now + Duration::from_secs(1));
    }
    #[test]
    fn missed_frames_skip_forward_without_a_catch_up_burst_or_drift() {
        let start = Instant::now();
        let period = Duration::from_nanos(1_000_000_000 / 240);
        assert_eq!(
            next_frame(
                start,
                start + period * 3 + Duration::from_micros(20),
                period
            ),
            start + period * 4
        );
    }
    #[test]
    fn stale_assignment_cannot_acknowledge_reused_window_and_cancel_removes_idle_work() {
        let clock = FrameClock::create().unwrap();
        // Use no real HWND: the long deadline never fires during this test.
        let hwnd = HWND(std::ptr::dangling_mut());
        clock
            .arm(
                hwnd,
                100,
                Some(WakeMode::Deadline(Duration::from_secs(3600))),
            )
            .unwrap();
        clock
            .arm(
                hwnd,
                101,
                Some(WakeMode::Deadline(Duration::from_secs(3600))),
            )
            .unwrap();
        assert!(!clock.acknowledge(hwnd, 100));
        clock.arm(hwnd, 101, None).unwrap();
        assert!(clock.shared.targets.lock().unwrap().is_empty());
    }
}
