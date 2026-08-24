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

#[derive(Default)]
pub struct Motion {
    tweens: HashMap<ElementId, Tween>,
}

impl Motion {
    pub fn value(&self, id: ElementId, fallback: f32) -> f32 {
        self.tweens.get(&id).map_or(fallback, |t| t.value)
    }

    pub fn animate_to(&mut self, id: ElementId, target: f32, duration_ms: u64) {
        let current = self.value(id, 1.0 - target);
        if (current - target).abs() < 0.001 {
            return;
        }
        self.tweens.insert(
            id,
            Tween {
                from: current,
                to: target,
                value: current,
                started: Instant::now(),
                duration: Duration::from_millis(duration_ms.max(1)),
            },
        );
    }

    /// Advance all tweens. Returns true while any animation remains active.
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let mut active = false;
        for tween in self.tweens.values_mut() {
            let t = (now.duration_since(tween.started).as_secs_f32()
                / tween.duration.as_secs_f32())
            .clamp(0.0, 1.0);
            // Fluent-style cubic ease out.
            let eased = 1.0 - (1.0 - t).powi(3);
            tween.value = tween.from + (tween.to - tween.from) * eased;
            if t < 1.0 {
                active = true;
            } else {
                tween.value = tween.to;
            }
        }
        active
    }

    pub fn has_active(&self) -> bool {
        let now = Instant::now();
        self.tweens
            .values()
            .any(|t| now.duration_since(t.started) < t.duration)
    }
}
