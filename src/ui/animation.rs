//! Small event-driven animation registry. A WM_TIMER runs only while one of
//! these tweens is active; idle UI consumes no render loop (spec §35, §50).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ui::layout::ElementId;

#[derive(Debug, Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    value: f32,
    started: Instant,
    duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionChannel {
    Hover,
    ToggleState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MotionKey {
    element: ElementId,
    channel: MotionChannel,
}

#[derive(Default)]
pub struct Motion {
    tweens: HashMap<MotionKey, Tween>,
}

impl Motion {
    pub fn value(&self, id: ElementId, channel: MotionChannel, fallback: f32) -> f32 {
        let key = MotionKey {
            element: id,
            channel,
        };
        let Some(tween) = self.tweens.get(&key) else {
            return fallback;
        };
        // A model replacement outranks an animation aimed at the old value.
        if (tween.to - fallback).abs() >= 0.001 {
            return fallback;
        }
        if Instant::now().saturating_duration_since(tween.started) < tween.duration {
            tween.value
        } else {
            fallback
        }
    }
    pub fn animate_to(
        &mut self,
        id: ElementId,
        channel: MotionChannel,
        target: f32,
        duration_ms: u64,
    ) {
        let from = self.value(id, channel, 1.0 - target);
        if (from - target).abs() < 0.001 {
            return;
        }
        self.tweens.insert(
            MotionKey {
                element: id,
                channel,
            },
            Tween {
                from,
                to: target,
                value: from,
                started: Instant::now(),
                duration: Duration::from_millis(duration_ms.max(1)),
            },
        );
    }

    pub fn clear(&mut self, id: ElementId, channel: MotionChannel) {
        self.tweens.remove(&MotionKey {
            element: id,
            channel,
        });
    }

    pub fn clear_channel(&mut self, channel: MotionChannel) {
        self.tweens.retain(|key, _| key.channel != channel);
    }

    /// Advance active tweens. Completed entries no longer shadow model state.
    pub fn tick(&mut self) -> bool {
        self.tick_at(Instant::now())
    }

    fn tick_at(&mut self, now: Instant) -> bool {
        let mut active = false;
        self.tweens.retain(|_, tween| {
            let t = (now.saturating_duration_since(tween.started).as_secs_f32()
                / tween.duration.as_secs_f32())
            .clamp(0.0, 1.0);
            if t < 1.0 {
                let eased = 1.0 - (1.0 - t).powi(3);
                tween.value = tween.from + (tween.to - tween.from) * eased;
                active = true;
                true
            } else {
                false
            }
        });
        active
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Motion, MotionChannel, MotionKey};
    use crate::ui::layout::ElementId;

    fn key(element: ElementId, channel: MotionChannel) -> MotionKey {
        MotionKey { element, channel }
    }

    #[test]
    fn false_toggle_hover_enter_and_leave_keep_state_channel_at_zero() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::Hover, 1.0, 140);
        assert_eq!(motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
        motion.animate_to(id, MotionChannel::Hover, 0.0, 140);
        assert_eq!(motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
        assert!(!motion
            .tweens
            .contains_key(&key(id, MotionChannel::ToggleState)));
    }
    #[test]
    fn false_toggle_hover_leave_keeps_state_channel_at_zero() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::Hover, 0.0, 140);
        assert_eq!(motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
    }

    #[test]
    fn true_toggle_hover_enter_and_leave_keep_state_channel_at_one() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::Hover, 1.0, 140);
        assert_eq!(motion.value(id, MotionChannel::ToggleState, 1.0), 1.0);
        motion.animate_to(id, MotionChannel::Hover, 0.0, 140);
        assert_eq!(motion.value(id, MotionChannel::ToggleState, 1.0), 1.0);
    }

    #[test]
    fn toggle_activation_animates_state_channel_from_false_to_true() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::ToggleState, 1.0, 160);
        let tween = motion
            .tweens
            .get(&key(id, MotionChannel::ToggleState))
            .expect("toggle state tween");
        assert_eq!(tween.from, 0.0);
        assert_eq!(tween.to, 1.0);
    }

    #[test]
    fn hover_and_toggle_state_channels_progress_independently() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::ToggleState, 1.0, 160);
        motion.animate_to(id, MotionChannel::Hover, 1.0, 140);
        assert_eq!(motion.tweens.len(), 2);
        assert_eq!(
            motion
                .tweens
                .get(&key(id, MotionChannel::ToggleState))
                .expect("toggle state tween")
                .to,
            1.0
        );
        assert_eq!(
            motion
                .tweens
                .get(&key(id, MotionChannel::Hover))
                .expect("hover tween")
                .to,
            1.0
        );
    }

    #[test]
    fn hovering_an_element_does_not_replace_active_state_tween() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::ToggleState, 1.0, 160);
        motion.animate_to(id, MotionChannel::Hover, 1.0, 140);
        let state = motion
            .tweens
            .get(&key(id, MotionChannel::ToggleState))
            .expect("toggle state tween");
        assert_eq!(state.from, 0.0);
        assert_eq!(state.to, 1.0);
    }

    #[test]
    fn hovering_many_toggles_never_creates_false_state_channels() {
        let ids = [
            ElementId::StartWithWindows,
            ElementId::StartHotkeysEnabled,
            ElementId::DesktopsEnabled,
            ElementId::WinNumberEnabled,
            ElementId::OverlayEnabled,
            ElementId::OverlayMicrophone,
            ElementId::OverlaySpeaker,
            ElementId::OverlayCurrentAppAudio,
            ElementId::OverlayWorkspace,
            ElementId::OverlayDisplayProfile,
            ElementId::DebugLogging,
        ];
        let mut motion = Motion::default();
        for id in ids {
            motion.animate_to(id, MotionChannel::Hover, 1.0, 140);
            motion.animate_to(id, MotionChannel::Hover, 0.0, 140);
            assert_eq!(motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
            assert!(!motion
                .tweens
                .contains_key(&key(id, MotionChannel::ToggleState)));
        }
    }
    fn complete(motion: &mut Motion, element: ElementId, channel: MotionChannel) {
        let now = Instant::now();
        motion
            .tweens
            .get_mut(&key(element, channel))
            .expect("tween")
            .started = now - Duration::from_secs(1);
        assert!(!motion.tick_at(now));
    }

    #[test]
    fn completed_toggle_tween_no_longer_overrides_fallback() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::ToggleState, 1.0, 160);
        complete(&mut motion, id, MotionChannel::ToggleState);
        assert_eq!(motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
        assert!(!motion
            .tweens
            .contains_key(&key(id, MotionChannel::ToggleState)));
    }

    #[test]
    fn false_fallback_wins_after_false_to_true_animation_completes() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::ToggleState, 1.0, 160);
        complete(&mut motion, id, MotionChannel::ToggleState);
        assert_eq!(motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
    }

    #[test]
    fn true_fallback_wins_after_true_to_false_animation_completes() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::ToggleState, 0.0, 160);
        complete(&mut motion, id, MotionChannel::ToggleState);
        assert_eq!(motion.value(id, MotionChannel::ToggleState, 1.0), 1.0);
    }

    #[test]
    fn active_toggle_tween_cannot_override_replaced_model_value() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::ToggleState, 1.0, 10_000);
        assert_eq!(motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
        assert!(motion
            .tweens
            .contains_key(&key(id, MotionChannel::ToggleState)));
    }

    #[test]
    fn clearing_a_hover_tween_removes_residual_hover_immediately() {
        let id = ElementId::OverlayEnabled;
        let mut motion = Motion::default();
        motion.animate_to(id, MotionChannel::Hover, 1.0, 140);
        motion.clear(id, MotionChannel::Hover);
        assert_eq!(motion.value(id, MotionChannel::Hover, 0.0), 0.0);
        assert!(!motion.tweens.contains_key(&key(id, MotionChannel::Hover)));
    }
}
