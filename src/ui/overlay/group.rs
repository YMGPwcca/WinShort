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
    pub(super) peers: Vec<ClusterPeer>,
    pub(super) side: Side,
    pub(super) started: Instant,
}
#[derive(Debug, Clone)]
pub(super) struct ClusterPeer {
    pub(super) id: u64,
    pub(super) row: OverlayRow,
    pub(super) offset: f32,
    pub(super) started: Instant,
}
#[derive(Debug, Clone)]
pub(super) struct ClusterIcon {
    pub(super) id: u64,
    pub(super) row: OverlayRow,
    pub(super) offset: f32,
    pub(super) alpha: f32,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct JoinRequest {
    pub(super) position: POINT,
    pub(super) started: Instant,
    pub(super) side: Side,
    pub(super) source_width: i32,
    pub(super) badge_width: i32,
}

fn progress(started: Instant, now: Instant) -> f32 {
    (now.saturating_duration_since(started).as_secs_f32() / (GROUP_MS as f32 / 1000.0))
        .clamp(0.0, 1.0)
}
fn eased(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn same_cluster(old: Option<&ClusterRequest>, new: Option<&ClusterRequest>) -> bool {
    match (old, new) {
        (Some(old), Some(new)) => {
            old.peers
                .iter()
                .map(|peer| peer.id)
                .eq(new.peers.iter().map(|peer| peer.id))
                && old.side == new.side
                && old.started == new.started
        }
        (None, None) => true,
        _ => false,
    }
}

#[derive(Debug, Default)]
pub(super) struct ClusterMotion {
    request: Option<ClusterRequest>,
    side: Option<Side>,
    from: f32,
    to: f32,
    started: Option<Instant>,
    revealing: bool,
    from_offsets: Vec<(u64, f32)>,
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
    pub(super) fn icons(&self, now: Instant) -> Vec<ClusterIcon> {
        let Some(request) = &self.request else {
            return Vec::new();
        };
        let t = self
            .started
            .map_or(1.0, |start| eased(progress(start, now)));
        request
            .peers
            .iter()
            .map(|peer| {
                let from = self
                    .from_offsets
                    .iter()
                    .find(|(id, _)| *id == peer.id)
                    .map_or(peer.offset, |(_, offset)| *offset);
                ClusterIcon {
                    id: peer.id,
                    row: peer.row.clone(),
                    offset: from + (peer.offset - from) * t,
                    alpha: ((progress(peer.started, now) - 0.8) / 0.2).clamp(0.0, 1.0),
                }
            })
            .collect()
    }
    pub(super) fn update(
        &mut self,
        request: Option<ClusterRequest>,
        motion: MotionPolicy,
        now: Instant,
    ) {
        let unchanged = same_cluster(self.request.as_ref(), request.as_ref());
        if unchanged {
            self.request = request;
        } else {
            self.from_offsets = self
                .icons(now)
                .iter()
                .map(|icon| (icon.id, icon.offset))
                .collect();
            self.from = self.extra(now);
            self.to = request
                .as_ref()
                .map_or(0.0, |request| GROUP_EXTRA * request.peers.len() as f32);
            if let Some(request) = &request {
                self.side = Some(request.side);
            }
            self.started = Some(if self.to < self.from {
                now
            } else {
                request.as_ref().map_or(now, |request| request.started)
            });
            self.revealing = request.is_some();
            self.request = request;
        }
        if motion == MotionPolicy::Reduced {
            self.finish_immediately(now);
        }
    }

    fn finish_immediately(&mut self, now: Instant) {
        self.started = None;
        self.revealing = false;
        if let Some(request) = &mut self.request {
            let settled = now
                .checked_sub(Duration::from_millis(GROUP_MS))
                .unwrap_or(now);
            request.started = settled;
            for peer in &mut request.peers {
                peer.started = settled;
            }
        }
        if self.to == 0.0 {
            self.side = None;
        }
    }
    pub(super) fn tick(&mut self, now: Instant) -> bool {
        let width_done = self
            .started
            .is_some_and(|start| progress(start, now) >= 1.0);
        let reveal_done = self.revealing
            && self.request.as_ref().is_some_and(|r| {
                r.peers
                    .iter()
                    .all(|peer| progress(peer.started, now) >= 1.0)
            });
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
        let t = progress(self.request.started, now);
        // Reach the side column before rising alongside the host. A straight
        // diagonal makes the moving card cover the badge that is staying put.
        let horizontal = 1.0 - (1.0 - t).powi(8);
        let vertical = eased(t * t * t);
        let x = match self.request.side {
            Side::Right => {
                self.from.x as f32 + (self.request.position.x - self.from.x) as f32 * horizontal
            }
            Side::Left => {
                let from_right = self.from.x + self.request.source_width;
                let to_right = self.request.position.x + self.request.badge_width;
                let right = from_right as f32 + (to_right - from_right) as f32 * horizontal;
                let width = self.request.source_width as f32
                    + (self.request.badge_width - self.request.source_width) as f32 * eased(t);
                right - width
            }
        };
        POINT {
            x: x.round() as i32,
            y: (self.from.y as f32 + (self.request.position.y - self.from.y) as f32 * vertical)
                .round() as i32,
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
                side: Side::Right,
                source_width: 200,
                badge_width: 52,
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
    fn joining_card_keeps_clear_of_the_primary_icon_at_each_frame() {
        let start = Instant::now();
        for side in [Side::Left, Side::Right] {
            let (from_x, target_x) = match side {
                Side::Left => (-148, -44),
                Side::Right => (-74, 44),
            };
            let join = JoinMotion::new(
                POINT { x: from_x, y: 62 },
                JoinRequest {
                    position: POINT { x: target_x, y: 0 },
                    started: start,
                    side,
                    source_width: 200,
                    badge_width: 52,
                },
            );
            // Host's coloured icon lies inside its 52 DIP badge at (10,10)-(42,42).
            for ms in 0..=GROUP_MS {
                let now = start + Duration::from_millis(ms);
                let pos = join.position(now);
                let width = 200.0 + (52.0 - 200.0) * eased(progress(start, now));
                assert!(
                    pos.y >= 42 || pos.x >= 42 || pos.x as f32 + width <= 10.5,
                    "moving card occludes host icon: side={side:?}, ms={ms}, pos={pos:?}"
                );
            }
        }
    }
    #[test]
    fn background_reverses_from_current_width_and_reduced_motion_is_immediate() {
        let now = Instant::now();
        let request = ClusterRequest {
            peers: vec![ClusterPeer {
                id: 2,
                row: OverlayRow::preview("peer", ""),
                offset: -44.0,
                started: now,
            }],
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
        assert!(!group.icons(middle).is_empty());
        assert_eq!(group.timer_interval(), None);
    }
}
