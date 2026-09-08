//! Pure overlay timing and coalescing policy.

use super::model::OverlayLifetime;
use crate::platform::visual::SystemVisualPreferences;
use windows::Win32::Foundation::{POINT, SIZE};

pub(super) const TIMER_MS: u32 = 16;

pub(super) const COALESCE_WINDOW_MS: u64 = 180;

pub(super) const APPEAR_MS: u64 = 140;

pub(super) const LEAVE_MS: u64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MotionPolicy {
    Animated,
    Reduced,
}

pub(super) fn motion_policy(preferences: SystemVisualPreferences) -> MotionPolicy {
    if preferences.animations_enabled {
        MotionPolicy::Animated
    } else {
        MotionPolicy::Reduced
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ShowTiming {
    pub(super) phase: Phase,
    pub(super) restart_phase: bool,
    pub(super) hold_after_now_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ShowPlan {
    pub(super) position: POINT,
    pub(super) size: SIZE,
    pub(super) region: WindowRegion,
    pub(super) alpha: f32,
    pub(super) timer_interval: u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct WindowRegion {
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) inset: i32,
    pub(super) corner_diameter: i32,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TickPlan {
    Hide,
    PruneToStickyOwner,
    Frame(ShowPlan),
}

/// Return an owned plan before any caller performs HWND work.
///
/// Keeping this boundary in one helper makes it impossible for the state
/// borrow used to prepare a plan to accidentally span a reentrant call.
pub(super) fn prepare_state_plan<State, Plan, Prepare>(
    cell: &std::cell::RefCell<State>,
    prepare: Prepare,
) -> Plan
where
    Prepare: FnOnce(&mut State) -> Plan,
{
    let mut state = cell.borrow_mut();
    prepare(&mut state)
}

pub(super) fn timing_after_show(
    phase: Phase,
    appearance_elapsed_ms: u64,
    motion: MotionPolicy,
    coalesced: bool,
    lifetime: OverlayLifetime,
    has_transient_rows: bool,
    duration_ms: u64,
) -> ShowTiming {
    if motion == MotionPolicy::Reduced {
        return ShowTiming {
            phase: Phase::Holding,
            restart_phase: true,
            hold_after_now_ms: hold_after(lifetime, has_transient_rows, duration_ms),
        };
    }
    if !coalesced {
        return ShowTiming {
            phase: Phase::Appearing,
            restart_phase: true,
            hold_after_now_ms: hold_after(lifetime, has_transient_rows, APPEAR_MS + duration_ms),
        };
    }
    match phase {
        Phase::Appearing => ShowTiming {
            phase: Phase::Appearing,
            restart_phase: false,
            hold_after_now_ms: hold_after(
                lifetime,
                has_transient_rows,
                APPEAR_MS.saturating_sub(appearance_elapsed_ms) + duration_ms,
            ),
        },
        Phase::Holding => ShowTiming {
            phase: Phase::Holding,
            restart_phase: false,
            hold_after_now_ms: hold_after(lifetime, has_transient_rows, duration_ms),
        },
        Phase::Leaving | Phase::Hidden => ShowTiming {
            phase: Phase::Holding,
            restart_phase: true,
            hold_after_now_ms: hold_after(lifetime, has_transient_rows, duration_ms),
        },
    }
}

fn hold_after(
    lifetime: OverlayLifetime,
    has_transient_rows: bool,
    duration_ms: u64,
) -> Option<u64> {
    if lifetime.is_sticky() && !has_transient_rows {
        None
    } else {
        Some(duration_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Hidden,
    Appearing,
    Holding,
    Leaving,
}
