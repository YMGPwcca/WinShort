//! Interruptible width and text transitions for microphone state changes.

use super::model::{OverlayIcon, OverlayModel, OverlayTone};
use super::timeline::{MotionPolicy, TIMER_MS};
use std::time::{Duration, Instant};

pub(super) const CONTENT_MS: u64 = 160;

#[derive(Debug, Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    started: Instant,
}
impl Tween {
    fn value(self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / (CONTENT_MS as f32 / 1000.0))
            .clamp(0.0, 1.0);
        self.from + (self.to - self.from) * (1.0 - (1.0 - t).powi(3))
    }
    fn finished(self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= Duration::from_millis(CONTENT_MS)
    }
}

#[derive(Debug, Default)]
pub(super) struct ContentMotion {
    width: Option<Tween>,
    text: Option<Tween>,
    pub(super) previous: Option<OverlayModel>,
}

fn mic_state(model: &OverlayModel) -> Option<OverlayTone> {
    let [row] = model.rows.as_slice() else {
        return None;
    };
    (row.icon == OverlayIcon::Microphone
        && matches!(row.tone, OverlayTone::Muted | OverlayTone::Active))
    .then_some(row.tone)
}

impl ContentMotion {
    pub(super) fn width(&self, model: &OverlayModel, now: Instant) -> f32 {
        self.width
            .map_or(model.width_dip.unwrap_or(240.0), |tween| tween.value(now))
    }
    pub(super) fn text_alpha(&self, now: Instant) -> f32 {
        self.text.map_or(1.0, |tween| tween.value(now))
    }
    pub(super) fn is_animating(&self) -> bool {
        self.width.is_some() || self.text.is_some()
    }
    pub(super) fn timer_interval(&self) -> Option<u32> {
        self.is_animating().then_some(TIMER_MS)
    }

    pub(super) fn update(
        &mut self,
        previous: &OverlayModel,
        next: &OverlayModel,
        enabled: bool,
        reveal_only: bool,
        motion: MotionPolicy,
        now: Instant,
    ) {
        if !enabled || motion == MotionPolicy::Reduced {
            *self = Self::default();
            return;
        }
        let (old_state, new_state) = (mic_state(previous), mic_state(next));
        if old_state == new_state {
            return;
        }
        if old_state.is_none() || new_state.is_none() {
            *self = Self::default();
            return;
        }
        let from = self.width(previous, now);
        let to = next.width_dip.unwrap_or(240.0);
        self.width = ((from - to).abs() > 0.001).then_some(Tween {
            from,
            to,
            started: now,
        });
        self.retarget_text(previous, new_state, reveal_only, now);
    }

    fn retarget_text(
        &mut self,
        previous: &OverlayModel,
        new_state: Option<OverlayTone>,
        reveal_only: bool,
        now: Instant,
    ) {
        if reveal_only {
            // The badge already reveals the new label while expanding. Hidden
            // old text must not reappear as a second layer during that reveal.
            self.previous = None;
            self.text = None;
            return;
        }
        // A rapid reversal swaps the same two labels with their current
        // weights. No animation queue or extra outgoing layers accumulate.
        let alpha = self
            .previous
            .as_ref()
            .filter(|model| mic_state(model) == new_state)
            .map_or(0.0, |_| 1.0 - self.text_alpha(now));
        if alpha >= 0.999 {
            self.previous = None;
            self.text = None;
            return;
        }
        self.previous = Some(previous.clone());
        self.text = Some(Tween {
            from: alpha,
            to: 1.0,
            started: now,
        });
    }

    pub(super) fn tick(&mut self, now: Instant) {
        if self.width.is_some_and(|tween| tween.finished(now)) {
            self.width = None;
        }
        if self.text.is_some_and(|tween| tween.finished(now)) {
            self.text = None;
            self.previous = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model(tone: OverlayTone, width: f32) -> OverlayModel {
        let state = if tone == OverlayTone::Muted {
            crate::audio::AudioState::Muted { volume_pct: 100 }
        } else {
            crate::audio::AudioState::Active { volume_pct: 100 }
        };
        let mut model = OverlayModel::single(super::super::microphone_row(&state));
        model.width_dip = Some(width);
        model
    }
    #[test]
    fn expanded_toggle_tweens_width_and_crossfades_then_stops() {
        let now = Instant::now();
        let muted = model(OverlayTone::Muted, 200.0);
        let active = model(OverlayTone::Active, 218.0);
        let mut motion = ContentMotion::default();
        motion.update(&muted, &active, true, false, MotionPolicy::Animated, now);
        assert_eq!(motion.width(&active, now), 200.0);
        assert_eq!(motion.text_alpha(now), 0.0);
        let middle = now + Duration::from_millis(50);
        assert!(motion.width(&active, middle) > 200.0 && motion.width(&active, middle) < 218.0);
        assert!(motion.text_alpha(middle) > 0.0 && motion.text_alpha(middle) < 1.0);
        motion.tick(now + Duration::from_millis(CONTENT_MS));
        assert_eq!(
            motion.width(&active, now + Duration::from_millis(CONTENT_MS)),
            218.0
        );
        assert_eq!(motion.timer_interval(), None);
        assert!(motion.previous.is_none());
    }
    #[test]
    fn rapid_reversal_preserves_width_and_each_labels_visible_weight() {
        let now = Instant::now();
        let muted = model(OverlayTone::Muted, 200.0);
        let active = model(OverlayTone::Active, 218.0);
        let mut motion = ContentMotion::default();
        motion.update(&muted, &active, true, false, MotionPolicy::Animated, now);
        let next = now + Duration::from_millis(35);
        let width = motion.width(&active, next);
        let active_alpha = motion.text_alpha(next);
        motion.update(&active, &muted, true, false, MotionPolicy::Animated, next);
        assert!((motion.width(&muted, next) - width).abs() < 0.0001);
        assert!((motion.text_alpha(next) - (1.0 - active_alpha)).abs() < 0.0001);
        let later = next + Duration::from_millis(20);
        let width = motion.width(&muted, later);
        let muted_alpha = motion.text_alpha(later);
        motion.update(&muted, &active, true, false, MotionPolicy::Animated, later);
        assert!((motion.width(&active, later) - width).abs() < 0.0001);
        assert!((motion.text_alpha(later) - (1.0 - muted_alpha)).abs() < 0.0001);
        assert_eq!(
            mic_state(motion.previous.as_ref().unwrap()),
            Some(OverlayTone::Muted)
        );
    }
    #[test]
    fn relayout_does_not_restart_and_badge_reveal_does_not_resurrect_old_text() {
        let now = Instant::now();
        let muted = model(OverlayTone::Muted, 200.0);
        let active = model(OverlayTone::Active, 218.0);
        let mut motion = ContentMotion::default();
        motion.update(&muted, &active, true, false, MotionPolicy::Animated, now);
        let later = now + Duration::from_millis(40);
        let alpha = motion.text_alpha(later);
        motion.update(&active, &active, true, false, MotionPolicy::Animated, later);
        assert_eq!(motion.text_alpha(later), alpha);
        motion.update(&active, &muted, true, true, MotionPolicy::Animated, later);
        assert!(motion.previous.is_none());
        assert_eq!(motion.text_alpha(later), 1.0);
        motion.update(&muted, &active, true, false, MotionPolicy::Reduced, later);
        assert!(!motion.is_animating());
        assert_eq!(motion.width(&active, later), 218.0);
    }

    #[test]
    fn immediate_round_trip_cancels_invisible_text_without_idle_frame_work() {
        let now = Instant::now();
        let muted = model(OverlayTone::Muted, 200.0);
        let active = model(OverlayTone::Active, 218.0);
        let mut motion = ContentMotion::default();
        motion.update(&muted, &active, true, false, MotionPolicy::Animated, now);
        motion.update(&active, &muted, true, false, MotionPolicy::Animated, now);
        assert_eq!(motion.width(&muted, now), 200.0);
        assert_eq!(motion.text_alpha(now), 1.0);
        assert!(motion.previous.is_none());
        assert_eq!(motion.timer_interval(), None);
    }
}
