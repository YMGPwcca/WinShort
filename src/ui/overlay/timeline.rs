//! Pure per-card animation timing policy.

use crate::platform::visual::SystemVisualPreferences;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, POINT, SIZE};

pub(super) const TIMER_MS: u32 = 16;

pub(super) const TIMER_ID: usize = 2;

pub(super) const APPEAR_MS: u64 = 140;

pub(super) const LEAVE_MS: u64 = 180;

pub(super) const POSITION_TWEEN_MS: u64 = 140;

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

/// The configured toast duration is the fully-visible hold interval. Animated
/// presentations therefore reserve the appear interval before the deadline;
/// reduced motion has no such prefix.
pub(super) fn toast_deadline(
    presentation_started: Instant,
    motion: MotionPolicy,
    hold_duration: Duration,
) -> Instant {
    let appear = match motion {
        MotionPolicy::Animated => Duration::from_millis(APPEAR_MS),
        MotionPolicy::Reduced => Duration::ZERO,
    };
    presentation_started + appear + hold_duration
}

fn deadline_interval_ms(now: Instant, deadline: Instant) -> u32 {
    deadline
        .saturating_duration_since(now)
        .as_millis()
        .clamp(1, u32::MAX as u128) as u32
}

/// Choose the next HWND timer interval without coupling the fully-visible hold
/// phase to the animation cadence.
///
/// `TIMER_MS` marks animation work in this pure policy; the native frame clock
/// replaces that marker with the target monitor's refresh cadence. A toast in
/// `Holding` arms directly to its expiry
/// deadline, while permanent cards stop their timer entirely.
pub(super) fn timer_interval_for_card(
    phase: Phase,
    motion: MotionPolicy,
    position_tween_active: bool,
    expires_at: Option<Instant>,
    now: Instant,
) -> Option<u32> {
    if phase == Phase::Hidden {
        return None;
    }
    if position_tween_active {
        return Some(TIMER_MS);
    }
    if motion == MotionPolicy::Reduced {
        return expires_at.map(|deadline| deadline_interval_ms(now, deadline));
    }
    match phase {
        Phase::Appearing | Phase::Leaving => Some(TIMER_MS),
        Phase::Holding => expires_at.map(|deadline| deadline_interval_ms(now, deadline)),
        Phase::Hidden => None,
    }
}

/// Give every entry generation a different HWND timer identity. A stale
/// WM_TIMER posted for a released card consequently cannot tick a reused
/// HWND's new assignment.
pub(super) fn timer_id_for_generation(generation: u64) -> usize {
    const TIMER_ID_OFFSET: usize = 0x1000;
    if generation == 0 {
        return TIMER_ID;
    }
    let id = (generation as usize).wrapping_add(TIMER_ID_OFFSET);
    if id == 0 || id == TIMER_ID {
        TIMER_ID_OFFSET + 1
    } else {
        id
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
    pub(super) compact: f32,
    pub(super) hover_alpha: f32,
    pub(super) layout_changed: bool,
    pub(super) timer_id: usize,
    pub(super) timer_interval: Option<u32>,
    pub(super) animation_active: bool,
    pub(super) content_width: f32,
    pub(super) text_alpha: f32,
    pub(super) cluster_extra: f32,
    pub(super) join_alpha: f32,
    pub(super) behind_badge: Option<HWND>,
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
    Frame(ShowPlan),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShowMode {
    Present,
    Relayout,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PositionTween {
    from: POINT,
    to: POINT,
    started_at: Instant,
}

impl PositionTween {
    pub(super) fn start(
        from: POINT,
        to: POINT,
        motion: MotionPolicy,
        started_at: Instant,
    ) -> Option<Self> {
        if motion == MotionPolicy::Reduced || from.x == to.x && from.y == to.y {
            return None;
        }
        Some(Self {
            from,
            to,
            started_at,
        })
    }

    pub(super) fn is_finished(self, now: Instant) -> bool {
        now.saturating_duration_since(self.started_at) >= Duration::from_millis(POSITION_TWEEN_MS)
    }

    pub(super) fn position_at(self, now: Instant) -> POINT {
        let elapsed = now.saturating_duration_since(self.started_at).as_secs_f32();
        let t = (elapsed / (POSITION_TWEEN_MS as f32 / 1000.0)).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        POINT {
            x: interpolate(self.from.x, self.to.x, eased),
            y: interpolate(self.from.y, self.to.y, eased),
        }
    }
}

fn interpolate(from: i32, to: i32, amount: f32) -> i32 {
    (from as f32 + (to - from) as f32 * amount).round() as i32
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
    ShowTiming {
        phase: Phase::Appearing,
        restart_phase: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Hidden,
    Appearing,
    Holding,
    Leaving,
}
