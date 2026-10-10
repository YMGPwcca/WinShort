//! Permanent mute cards' expanded-to-badge transition. No idle frame loop.

use super::timeline::{MotionPolicy, APPEAR_MS, TIMER_MS};
use std::time::{Duration, Instant};

pub(super) const BADGE_HOLD_MS: u64 = 1000;
pub(super) const BADGE_TWEEN_MS: u64 = 220;

#[derive(Debug, Clone, Copy)]
struct BadgeTween {
    from: f32,
    to: f32,
    started: Instant,
}

impl BadgeTween {
    fn value(self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / (BADGE_TWEEN_MS as f32 / 1000.0))
            .clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        self.from + (self.to - self.from) * eased
    }

    fn finished(self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= Duration::from_millis(BADGE_TWEEN_MS)
    }
}

#[derive(Debug, Default)]
pub(super) struct BadgeMotion {
    collapsible: bool,
    collapse_at: Option<Instant>,
    settled: f32,
    tween: Option<BadgeTween>,
    collapse_started: Option<Instant>,
}

impl BadgeMotion {
    pub(super) fn is_animating(&self) -> bool {
        self.tween.is_some()
    }
    pub(super) fn collapse_started(&self) -> Option<Instant> {
        self.collapse_started
    }
    pub(super) fn value(&self, now: Instant) -> f32 {
        self.tween.map_or(self.settled, |tween| tween.value(now))
    }

    pub(super) fn update(
        &mut self,
        collapsible: bool,
        shown_at: Instant,
        motion: MotionPolicy,
        now: Instant,
    ) {
        if self.collapsible != collapsible {
            self.collapse_started = None;
            self.collapsible = collapsible;
            if collapsible {
                let appear = if motion == MotionPolicy::Animated {
                    APPEAR_MS
                } else {
                    0
                };
                let fully_visible_at = (shown_at + Duration::from_millis(appear)).max(now);
                self.collapse_at = Some(fully_visible_at + Duration::from_millis(BADGE_HOLD_MS));
                self.animate_to(0.0, motion, now);
            } else {
                self.collapse_at = None;
                self.animate_to(0.0, motion, now);
            }
        }
        if motion == MotionPolicy::Reduced {
            if let Some(tween) = self.tween.take() {
                self.settled = tween.to;
            }
        }
    }

    fn animate_to(&mut self, to: f32, motion: MotionPolicy, now: Instant) {
        let from = self.value(now);
        self.tween = None;
        self.settled = to;
        if motion == MotionPolicy::Animated && (from - to).abs() > 0.001 {
            self.tween = Some(BadgeTween {
                from,
                to,
                started: now,
            });
        }
    }

    /// Return true once the stack can reclaim the expanded card's height.
    pub(super) fn tick(&mut self, motion: MotionPolicy, now: Instant) -> bool {
        let was_compact = self.reserves_compact();
        let mut started = false;
        if self.collapse_at.is_some_and(|deadline| now >= deadline) {
            self.collapse_at = None;
            self.collapse_started = Some(now);
            started = true;
            self.animate_to(1.0, motion, now);
        }
        if self.tween.is_some_and(|tween| tween.finished(now)) {
            self.tween = None;
        }
        started || was_compact != self.reserves_compact()
    }

    pub(super) fn reserves_compact(&self) -> bool {
        self.collapsible && self.tween.is_none() && self.settled == 1.0
    }

    pub(super) fn timer_interval(&self, now: Instant) -> Option<u32> {
        if self.tween.is_some() {
            return Some(TIMER_MS);
        }
        self.collapse_at.map(|deadline| {
            deadline
                .saturating_duration_since(now)
                .as_millis()
                .clamp(1, u32::MAX as u128) as u32
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_holds_then_collapses_and_stops_its_timer() {
        let now = Instant::now();
        let mut badge = BadgeMotion::default();
        badge.update(true, now, MotionPolicy::Animated, now);
        let collapse = now + Duration::from_millis(APPEAR_MS + BADGE_HOLD_MS);
        assert_eq!(badge.value(collapse - Duration::from_millis(1)), 0.0);
        assert!(badge.tick(MotionPolicy::Animated, collapse));
        assert_eq!(badge.timer_interval(collapse), Some(TIMER_MS));
        let mid = collapse + Duration::from_millis(BADGE_TWEEN_MS / 2);
        assert!(badge.value(mid) > 0.0 && badge.value(mid) < 1.0);
        let complete = collapse + Duration::from_millis(BADGE_TWEEN_MS);
        assert!(badge.tick(MotionPolicy::Animated, complete));
        assert!(badge.reserves_compact());
        assert_eq!(badge.value(complete), 1.0);
        assert_eq!(badge.timer_interval(complete), None);
    }

    #[test]
    fn unmute_expands_from_current_shape_and_remute_cancels_expansion() {
        let now = Instant::now();
        let mut badge = BadgeMotion::default();
        badge.update(true, now, MotionPolicy::Reduced, now);
        let collapsed = now + Duration::from_millis(BADGE_HOLD_MS);
        badge.tick(MotionPolicy::Reduced, collapsed);
        badge.update(false, collapsed, MotionPolicy::Animated, collapsed);
        assert_eq!(badge.value(collapsed), 1.0);
        assert!(!badge.reserves_compact());
        let mid = collapsed + Duration::from_millis(BADGE_TWEEN_MS / 2);
        let interrupted = badge.value(mid);
        badge.update(true, mid, MotionPolicy::Animated, mid);
        assert_eq!(badge.value(mid), interrupted);
        assert!(badge.timer_interval(mid).is_some());
    }

    #[test]
    fn passive_relayout_does_not_restart_the_one_second_hold() {
        let now = Instant::now();
        let mut badge = BadgeMotion::default();
        badge.update(true, now, MotionPolicy::Reduced, now);
        let refresh = now + Duration::from_millis(800);
        badge.update(true, refresh, MotionPolicy::Reduced, refresh);
        assert_eq!(badge.timer_interval(refresh), Some(200));
        assert!(badge.tick(MotionPolicy::Reduced, now + Duration::from_secs(1)));
        assert_eq!(badge.timer_interval(now + Duration::from_secs(1)), None);
    }
}
