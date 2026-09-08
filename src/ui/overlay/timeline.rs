//! Pure per-card animation timing policy.

use crate::platform::visual::SystemVisualPreferences;
use windows::Win32::Foundation::{POINT, SIZE};

pub(super) const TIMER_MS: u32 = 16;

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
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ShowPlan {
    pub(super) position: POINT,
    pub(super) size: SIZE,
    pub(super) region: WindowRegion,
    pub(super) alpha: f32,
    pub(super) timer_interval: Option<u32>,
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
    StopTimer,
    Frame(ShowPlan),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShowMode {
    Present,
    Relayout,
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

pub(super) fn timing_after_show(phase: Phase, motion: MotionPolicy, mode: ShowMode) -> ShowTiming {
    if mode == ShowMode::Relayout && phase != Phase::Hidden {
        return ShowTiming {
            phase,
            restart_phase: false,
        };
    }
    if motion == MotionPolicy::Reduced {
        return ShowTiming {
            phase: Phase::Holding,
            restart_phase: true,
        };
    }
    if phase == Phase::Hidden {
        return ShowTiming {
            phase: Phase::Appearing,
            restart_phase: true,
        };
    }
    match phase {
        Phase::Appearing => ShowTiming {
            phase: Phase::Appearing,
            restart_phase: false,
        },
        Phase::Holding => ShowTiming {
            phase: Phase::Holding,
            restart_phase: false,
        },
        Phase::Leaving => ShowTiming {
            phase: Phase::Holding,
            restart_phase: true,
        },
        Phase::Hidden => unreachable!("hidden phase handled above"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Hidden,
    Appearing,
    Holding,
    Leaving,
}
