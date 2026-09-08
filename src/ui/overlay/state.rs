//! Overlay state transitions and frame planning. Native operations consume owned plans.

use super::backend::{OverlayGraphics, OverlayRenderData, OverlaySurface, SurfaceSpec};
use super::layout::{surface_geometry, window_region_for};
use super::model::OverlayModel;
use super::palette::{
    composition_blur_enabled, opaque_palette, palette_for, resolved_theme_mode, OverlayPalette,
};
use super::timeline::{
    motion_policy, timing_after_show, MotionPolicy, Phase, ShowMode, ShowPlan, TickPlan, APPEAR_MS,
    LEAVE_MS, TIMER_MS,
};
use crate::config::model::{OverlayBlur, OverlayCfg};
use crate::error::Result;
use crate::platform::visual::SystemVisualPreferences;
use std::time::{Duration, Instant, SystemTime};
use windows::Win32::Foundation::{POINT, SIZE};

pub(super) struct ShowRequest {
    pub(super) model: OverlayModel,
    pub(super) config: OverlayCfg,
    pub(super) dpi: u32,
    pub(super) target_monitor: Option<String>,
    pub(super) position: POINT,
    pub(super) expires_at: Option<Instant>,
    pub(super) mode: ShowMode,
}

pub(super) struct OverlayState {
    pub(super) entry_id: u64,
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
            model,
            config,
            dpi,
            target_monitor,
            position,
            expires_at,
            mode,
        } = request;
        if model.rows.is_empty() || !config.enabled {
            return Ok(None);
        }
        self.preferences = preferences;
        self.motion = motion_policy(preferences);
        self.config = config;
        let now = Instant::now();
        self.model = model;
        self.expires_at = expires_at;
        self.dpi = dpi.max(96);
        self.last_target_monitor = target_monitor;
        self.last_render_dpi = Some(self.dpi);
        self.last_shown = Some(SystemTime::now());
        self.refresh_palette();
        self.surface_size =
            surface_geometry(self.config.scale, self.model.rows.len()).pixel_size(self.dpi);
        self.base_position = position;
        let timing = timing_after_show(self.phase, self.motion, mode);
        self.phase = timing.phase;
        if timing.restart_phase {
            self.phase_started = now;
        }
        Ok(Some(self.frame_plan()))
    }

    pub(super) fn frame_plan(&self) -> ShowPlan {
        let (alpha, slide_dip) = self.frame_values();
        let slide_px = (slide_dip * self.config.scale * self.dpi as f32 / 96.0).round() as i32;
        ShowPlan {
            position: POINT {
                x: self.base_position.x,
                y: self.base_position.y + slide_px,
            },
            size: self.surface_size,
            region: window_region_for(self.surface_size, self.dpi),
            alpha,
            timer_interval: self.timer_interval(),
        }
    }

    pub(super) fn timer_interval(&self) -> Option<u32> {
        if self.phase == Phase::Hidden {
            return None;
        }
        if self.motion == MotionPolicy::Reduced {
            return self.expires_at.map(|expires_at| {
                expires_at
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .clamp(1, u32::MAX as u128) as u32
            });
        }
        match self.phase {
            Phase::Appearing | Phase::Leaving => Some(TIMER_MS),
            Phase::Holding if self.expires_at.is_some() => Some(TIMER_MS),
            Phase::Holding | Phase::Hidden => None,
        }
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
        let now = Instant::now();
        if self.phase == Phase::Hidden {
            return None;
        }
        if self.motion == MotionPolicy::Reduced {
            if self.expires_at.is_some_and(|expires_at| now >= expires_at) {
                self.phase = Phase::Hidden;
                return Some(TickPlan::Hide);
            }
            return Some(TickPlan::Frame(self.frame_plan()));
        }
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
                } else if self.expires_at.is_none() {
                    return Some(TickPlan::StopTimer);
                }
            }
            Phase::Leaving => {
                if now.duration_since(self.phase_started) >= Duration::from_millis(LEAVE_MS) {
                    self.phase = Phase::Hidden;
                    return Some(TickPlan::Hide);
                }
            }
            Phase::Hidden => return None,
        }
        Some(TickPlan::Frame(self.frame_plan()))
    }

    pub(super) fn frame_values(&self) -> (f32, f32) {
        let elapsed = Instant::now()
            .duration_since(self.phase_started)
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

    pub(super) fn render_data(&self, alpha: f32) -> OverlayRenderData {
        OverlayRenderData {
            dwrite: self.graphics.dwrite.clone(),
            model: self.model.clone(),
            scale: self.config.scale,
            palette: self.palette,
            theme_mode: resolved_theme_mode(self.config.appearance, self.preferences),
            alpha,
            blur: self.config.blur,
        }
    }
}
