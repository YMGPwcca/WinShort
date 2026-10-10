//! Visual-only grouping. Audio entries keep their own identity and lifetime.

use super::model::OverlayRow;
use super::timeline::{MotionPolicy, TIMER_MS};
use crate::config::model::OverlayPosition;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::POINT;

pub(super) const GROUP_MS: u64 = 220;
pub(super) const GROUP_EXTRA: f32 = 44.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Right,
}
impl Side {
    pub(super) fn for_position(position: OverlayPosition) -> Self {
        if matches!(
            position,
            OverlayPosition::TopRight | OverlayPosition::CenterRight | OverlayPosition::BottomRight
        ) {
            Self::Left
        } else {
            Self::Right
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ClusterRequest {
    pub(super) peer_id: u64,
    pub(super) row: OverlayRow,
    pub(super) side: Side,
    pub(super) started: Instant,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct JoinRequest {
    pub(super) position: POINT,
    pub(super) started: Instant,
}

fn progress(started: Instant, now: Instant) -> f32 {
    (now.saturating_duration_since(started).as_secs_f32() / (GROUP_MS as f32 / 1000.0))
        .clamp(0.0, 1.0)
}
fn eased(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

#[derive(Debug, Default)]
pub(super) struct ClusterMotion {
    request: Option<ClusterRequest>,
    side: Option<Side>,
    from: f32,
    to: f32,
    started: Option<Instant>,
    revealing: bool,
}
impl ClusterMotion {
    pub(super) fn extra(&self, now: Instant) -> f32 {
        self.started.map_or(self.to, |start| {
            self.from + (self.to - self.from) * eased(progress(start, now))
        })
    }
    pub(super) fn side(&self) -> Option<Side> {
        self.side
    }
    pub(super) fn peer(&self, now: Instant) -> Option<&OverlayRow> {
        self.request
            .as_ref()
            .filter(|request| progress(request.started, now) >= 0.8)
            .map(|request| &request.row)
    }
    pub(super) fn peer_alpha(&self, now: Instant) -> f32 {
        self.request.as_ref().map_or(0.0, |r| {
            ((progress(r.started, now) - 0.8) / 0.2).clamp(0.0, 1.0)
        })
    }
    pub(super) fn update(
        &mut self,
        request: Option<ClusterRequest>,
        motion: MotionPolicy,
        now: Instant,
    ) {
        let unchanged = match (&self.request, &request) {
            (Some(old), Some(new)) => {
                old.peer_id == new.peer_id && old.side == new.side && old.started == new.started
            }
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            self.request = request;
        } else {
            self.from = self.extra(now);
            self.to = if request.is_some() { GROUP_EXTRA } else { 0.0 };
            if let Some(request) = &request {
                self.side = Some(request.side);
            }
            self.started = Some(request.as_ref().map_or(now, |request| request.started));
            self.revealing = request.is_some();
            self.request = request;
        }
        if motion == MotionPolicy::Reduced {
            self.started = None;
            self.revealing = false;
            if let Some(request) = &mut self.request {
                request.started = now
                    .checked_sub(Duration::from_millis(GROUP_MS))
                    .unwrap_or(now);
            }
            if self.to == 0.0 {
                self.side = None;
            }
        }
    }
    pub(super) fn tick(&mut self, now: Instant) -> bool {
        let width_done = self
            .started
            .is_some_and(|start| progress(start, now) >= 1.0);
        let reveal_done = self.revealing
            && self
                .request
                .as_ref()
                .is_some_and(|r| progress(r.started, now) >= 1.0);
        if width_done {
            self.started = None;
        }
        if reveal_done {
            self.revealing = false;
        }
        if self.to == 0.0 && self.started.is_none() {
            self.side = None;
        }
        width_done || reveal_done
    }
    pub(super) fn timer_interval(&self) -> Option<u32> {
        (self.started.is_some() || self.revealing).then_some(TIMER_MS)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct JoinMotion {
    from: POINT,
    pub(super) request: JoinRequest,
    notified: bool,
}
impl JoinMotion {
    pub(super) fn new(from: POINT, request: JoinRequest) -> Self {
        Self {
            from,
            request,
            notified: false,
        }
    }
    pub(super) fn position(&self, now: Instant) -> POINT {
        let t = eased(progress(self.request.started, now));
        POINT {
            x: (self.from.x as f32 + (self.request.position.x - self.from.x) as f32 * t).round()
                as i32,
            y: (self.from.y as f32 + (self.request.position.y - self.from.y) as f32 * t).round()
                as i32,
        }
    }
    pub(super) fn alpha(&self, now: Instant) -> f32 {
        if self.finished(now) {
            return 0.0;
        }
        1.0 - ((progress(self.request.started, now) - 0.8) / 0.2).clamp(0.0, 1.0)
    }
    pub(super) fn finished(&self, now: Instant) -> bool {
        progress(self.request.started, now) >= 1.0
    }
    pub(super) fn tick(&mut self, now: Instant) -> bool {
        if self.finished(now) && !self.notified {
            self.notified = true;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn join_keeps_one_icon_until_handoff_and_can_finish_without_polling() {
        let start = Instant::now();
        let mut join = JoinMotion::new(
            POINT { x: 10, y: 80 },
            JoinRequest {
                position: POINT { x: 54, y: 10 },
                started: start,
            },
        );
        assert_eq!(join.position(start), POINT { x: 10, y: 80 });
        assert_eq!(join.alpha(start + Duration::from_millis(100)), 1.0);
        let end = start + Duration::from_millis(GROUP_MS);
        assert_eq!(join.position(end), POINT { x: 54, y: 10 });
        assert_eq!(join.alpha(end), 0.0);
        assert!(join.tick(end));
        assert!(!join.tick(end));
    }
    #[test]
    fn background_reverses_from_current_width_and_reduced_motion_is_immediate() {
        let now = Instant::now();
        let request = ClusterRequest {
            peer_id: 2,
            row: OverlayRow::preview("peer", ""),
            side: Side::Left,
            started: now,
        };
        let mut group = ClusterMotion::default();
        group.update(Some(request.clone()), MotionPolicy::Animated, now);
        let middle = now + Duration::from_millis(60);
        let width = group.extra(middle);
        group.update(None, MotionPolicy::Animated, middle);
        assert!((group.extra(middle) - width).abs() < 0.001);
        group.tick(middle + Duration::from_millis(GROUP_MS));
        assert_eq!(group.extra(middle + Duration::from_millis(GROUP_MS)), 0.0);
        assert_eq!(group.timer_interval(), None);
        group.update(Some(request), MotionPolicy::Reduced, middle);
        assert_eq!(group.extra(middle), GROUP_EXTRA);
        assert!(group.peer(middle).is_some());
        assert_eq!(group.timer_interval(), None);
    }
}
