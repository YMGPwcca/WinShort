//! Overlay state transitions and frame planning. Native operations consume owned plans.

use super::backend::{OverlayGraphics, OverlayRenderData, OverlaySurface, SurfaceSpec};
use super::badge::BadgeMotion;
use super::layout::{presentation_geometry, window_region_for};
use super::model::OverlayModel;
use super::palette::{
    composition_blur_enabled, opaque_palette, palette_for, resolved_theme_mode, OverlayPalette,
};
use super::timeline::{
    motion_policy, timer_id_for_generation, timer_interval_for_card, timing_after_show,
    MotionPolicy, Phase, PositionTween, ShowMode, ShowPlan, TickPlan, APPEAR_MS, LEAVE_MS,
};
use crate::config::model::{OverlayBlur, OverlayCfg, OverlayPosition};
use crate::error::Result;
use crate::platform::visual::SystemVisualPreferences;
use std::time::{Duration, Instant, SystemTime};
use windows::Win32::Foundation::{POINT, SIZE};

pub(super) struct ShowRequest {
    pub(super) generation: u64,
    pub(super) presentation_started_at: Instant,
    pub(super) model: OverlayModel,
    pub(super) config: OverlayCfg,
    pub(super) dpi: u32,
    pub(super) target_monitor: Option<String>,
    pub(super) position: POINT,
    pub(super) expires_at: Option<Instant>,
    pub(super) mode: ShowMode,
    pub(super) collapsible_mute: bool,
    pub(super) layout_size: SIZE,
}

pub(super) struct OverlayState {
    pub(super) entry_id: u64,
    pub(super) generation: u64,
    pub(super) timer_id: usize,
    pub(super) graphics: OverlayGraphics,
    pub(super) surface: Option<OverlaySurface>,
    pub(super) surface_size: SIZE,
    pub(super) model: OverlayModel,
    pub(super) config: OverlayCfg,
    pub(super) preferences: SystemVisualPreferences,
    pub(super) palette: OverlayPalette,
    pub(super) backdrop_enabled: bool,
    pub(super) motion: MotionPolicy,
    pub(super) base_position: POINT,
    pub(super) position_tween: Option<PositionTween>,
    pub(super) badge: BadgeMotion,
    pub(super) hover: super::hover::HoverMotion,
    pub(super) layout_size: SIZE,
    pub(super) phase: Phase,
    pub(super) phase_started: Instant,
    /// Each card owns its own optional toast deadline.
    pub(super) expires_at: Option<Instant>,
    pub(super) dpi: u32,
    pub(super) last_target_monitor: Option<String>,
    pub(super) last_render_dpi: Option<u32>,
    pub(super) last_shown: Option<SystemTime>,
}

impl OverlayState {
    pub(super) fn new(graphics: OverlayGraphics, entry_id: u64) -> Self {
        let now = Instant::now();
        let preferences = SystemVisualPreferences::query();
        let config = crate::config::Config::default().overlay;
        Self {
            entry_id,
            generation: 0,
            timer_id: super::timeline::TIMER_ID,
            graphics,
            surface: None,
            surface_size: SIZE::default(),
            backdrop_enabled: false,
            model: OverlayModel::default(),
            palette: palette_for(config.appearance, preferences),
            config,
            preferences,
            motion: motion_policy(preferences),
            base_position: POINT::default(),
            position_tween: None,
            badge: BadgeMotion::default(),
            hover: super::hover::HoverMotion::default(),
            layout_size: SIZE::default(),
            phase: Phase::Hidden,
            phase_started: now,
            expires_at: None,
            dpi: 96,
            last_target_monitor: None,
            last_render_dpi: None,
            last_shown: None,
        }
    }

    pub(super) fn prepare_show(
        &mut self,
        request: ShowRequest,
        preferences: SystemVisualPreferences,
    ) -> Result<Option<ShowPlan>> {
        let ShowRequest {
            generation,
            presentation_started_at,
            model,
            config,
            dpi,
            target_monitor,
            position,
            expires_at,
            mode,
            collapsible_mute,
            layout_size,
        } = request;
        if model.rows.is_empty() || !config.enabled {
            return Ok(None);
        }
        let now = Instant::now();
        let previous_position = self.presentation_position(now, self.presentation_size(now));
        self.preferences = preferences;
        self.motion = motion_policy(preferences);
        self.config = config;
        self.badge
            .update(collapsible_mute, presentation_started_at, self.motion, now);
        self.layout_size = layout_size;
        self.model = model;
        self.expires_at = expires_at;
        self.generation = generation;
        self.timer_id = timer_id_for_generation(generation);
        self.dpi = dpi.max(96);
        self.last_target_monitor = target_monitor;
        self.last_render_dpi = Some(self.dpi);
        if mode == ShowMode::Present {
            self.last_shown = Some(SystemTime::now());
        }
        self.refresh_palette();
        let offset = self.presentation_offset(self.presentation_size(now));
        let previous_anchor = POINT {
            x: previous_position.x - offset.x,
            y: previous_position.y - offset.y,
        };
        let timing = timing_after_show(self.phase, self.motion, mode);
        self.phase = timing.phase;
        if timing.restart_phase {
            self.phase_started = presentation_started_at;
            self.position_tween = None;
        } else if mode == ShowMode::Relayout {
            self.position_tween = PositionTween::start(previous_anchor, position, self.motion, now);
        } else {
            self.position_tween = None;
        }
        self.base_position = position;
        Ok(Some(self.frame_plan()))
    }

    pub(super) fn frame_plan(&self) -> ShowPlan {
        self.frame_plan_at(Instant::now())
    }

    fn frame_plan_at(&self, now: Instant) -> ShowPlan {
        let (alpha, slide_dip) = self.frame_values(now);
        let slide_px = (slide_dip * self.config.scale * self.dpi as f32 / 96.0).round() as i32;
        let compact = self.badge.value(now);
        let size = self.presentation_size(now);
        let position = self.presentation_position(now, size);
        let card_timer = timer_interval_for_card(
            self.phase,
            self.motion,
            self.position_tween
                .is_some_and(|tween| !tween.is_finished(now)),
            self.expires_at,
            now,
        );
        let timer_interval = [
            card_timer,
            self.badge.timer_interval(now),
            self.hover.timer_interval(),
        ]
        .into_iter()
        .flatten()
        .min();
        ShowPlan {
            position: POINT {
                x: position.x,
                y: position.y + slide_px,
            },
            size,
            region: window_region_for(size, self.dpi),
            alpha,
            compact,
            hover_alpha: self.hover.value(now),
            layout_changed: false,
            timer_id: self.timer_id,
            timer_interval,
        }
    }

    fn presentation_position(&self, now: Instant, size: SIZE) -> POINT {
        let position = self.position_at(now);
        let offset = self.presentation_offset(size);
        POINT {
            x: position.x + offset.x,
            y: position.y + offset.y,
        }
    }

    fn presentation_size(&self, now: Instant) -> SIZE {
        presentation_geometry(
            self.config.scale,
            self.model.rows.len(),
            self.badge.value(now),
        )
        .pixel_size(self.dpi)
    }

    fn presentation_offset(&self, size: SIZE) -> POINT {
        let dx = self.layout_size.cx - size.cx;
        let dy = self.layout_size.cy - size.cy;
        let x = match self.config.position {
            OverlayPosition::TopRight
            | OverlayPosition::CenterRight
            | OverlayPosition::BottomRight => dx,
            OverlayPosition::TopCenter
            | OverlayPosition::Center
            | OverlayPosition::BottomCenter => dx / 2,
            _ => 0,
        };
        let y = if matches!(
            self.config.position,
            OverlayPosition::BottomLeft
                | OverlayPosition::BottomCenter
                | OverlayPosition::BottomRight
        ) {
            dy
        } else if matches!(
            self.config.position,
            OverlayPosition::CenterLeft | OverlayPosition::Center | OverlayPosition::CenterRight
        ) {
            dy / 2
        } else {
            0
        };
        POINT { x, y }
    }

    pub(super) fn refresh_palette(&mut self) {
        self.backdrop_enabled = composition_blur_enabled(
            self.preferences,
            self.surface
                .as_ref()
                .is_some_and(OverlaySurface::is_composition),
        );
        let palette = palette_for(self.config.appearance, self.preferences);
        self.palette = if self.backdrop_enabled && !matches!(self.config.blur, OverlayBlur::Solid) {
            palette
        } else {
            opaque_palette(palette)
        };
    }

    pub(super) fn prepare_tick(&mut self) -> Option<TickPlan> {
        self.prepare_tick_at(Instant::now())
    }

    pub(super) fn prepare_tick_at(&mut self, now: Instant) -> Option<TickPlan> {
        if self.phase == Phase::Hidden {
            return None;
        }
        let layout_changed = self.badge.tick(self.motion, now);
        self.hover.tick(now);
        let tween_finished = self
            .position_tween
            .is_some_and(|tween| tween.is_finished(now));
        if tween_finished {
            self.position_tween = None;
        }
        if self.motion == MotionPolicy::Animated {
            self.advance_phase(now);
        } else if self.expires_at.is_some_and(|expires_at| now >= expires_at) {
            self.phase = Phase::Hidden;
        }
        if self.phase == Phase::Hidden {
            return Some(TickPlan::Hide);
        }
        let mut plan = self.frame_plan_at(now);
        plan.layout_changed = layout_changed;
        Some(TickPlan::Frame(plan))
    }

    fn advance_phase(&mut self, now: Instant) {
        match self.phase {
            Phase::Appearing => {
                if now.duration_since(self.phase_started) >= Duration::from_millis(APPEAR_MS) {
                    self.phase = Phase::Holding;
                    self.phase_started = now;
                }
            }
            Phase::Holding => {
                if self.expires_at.is_some_and(|expires_at| now >= expires_at) {
                    self.phase = Phase::Leaving;
                    self.phase_started = now;
                }
            }
            Phase::Leaving => {
                if now.duration_since(self.phase_started) >= Duration::from_millis(LEAVE_MS) {
                    self.phase = Phase::Hidden;
                }
            }
            Phase::Hidden => {}
        }
    }

    pub(super) fn frame_values(&self, now: Instant) -> (f32, f32) {
        let elapsed = now
            .saturating_duration_since(self.phase_started)
            .as_secs_f32();
        if self.motion == MotionPolicy::Reduced {
            return (1.0, 0.0);
        }
        match self.phase {
            Phase::Appearing => {
                let t = (elapsed / (APPEAR_MS as f32 / 1000.0)).clamp(0.0, 1.0);
                let eased = 1.0 - (1.0 - t).powi(3);
                (eased, 12.0 * (1.0 - eased))
            }
            Phase::Holding => (1.0, 0.0),
            Phase::Leaving => {
                let t = (elapsed / (LEAVE_MS as f32 / 1000.0)).clamp(0.0, 1.0);
                (1.0 - t * t, 8.0 * t)
            }
            Phase::Hidden => (0.0, 0.0),
        }
    }

    fn position_at(&self, now: Instant) -> POINT {
        self.position_tween
            .map_or(self.base_position, |tween| tween.position_at(now))
    }

    pub(super) fn surface_spec(&self, size: SIZE) -> SurfaceSpec {
        SurfaceSpec {
            size,
            dpi: self.dpi,
            blur_enabled: self.backdrop_enabled,
        }
    }

    pub(super) fn requires_window_region(&self) -> bool {
        self.surface
            .as_ref()
            .is_some_and(|surface| !surface.is_composition())
    }

    pub(super) fn render_data(&self, alpha: f32, compact: f32) -> OverlayRenderData {
        OverlayRenderData {
            dwrite: self.graphics.dwrite.clone(),
            model: self.model.clone(),
            scale: self.config.scale,
            palette: self.palette,
            theme_mode: resolved_theme_mode(self.config.appearance, self.preferences),
            alpha,
            blur: self.config.blur,
            compact,
            hover_alpha: self.hover.value(Instant::now()),
        }
    }
}
