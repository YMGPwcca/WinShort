//! Overlay state transitions and frame planning. Native operations consume owned plans.

use super::backend::{
    OverlayGraphics, OverlayRenderData, OverlaySurface, SurfaceSpec, RECT_FALLBACK,
};
use super::layout::{position_for, surface_geometry, window_region_for};
use super::model::{merge_overlay_models, OverlayModel};
use super::palette::{
    composition_blur_enabled, opaque_palette, palette_for, resolved_theme_mode, OverlayPalette,
};
use super::timeline::{
    motion_policy, timing_after_show, MotionPolicy, Phase, ShowPlan, TickPlan, APPEAR_MS,
    COALESCE_WINDOW_MS, LEAVE_MS, TIMER_MS,
};
use crate::config::model::{OverlayBlur, OverlayCfg};
use crate::error::Result;
use crate::platform::visual::SystemVisualPreferences;
use std::time::{Duration, Instant, SystemTime};
use windows::Win32::Foundation::{POINT, SIZE};

pub(super) struct OverlayState {
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
    pub(super) hold_until: Instant,
    pub(super) last_presented: Instant,
    pub(super) dpi: u32,
    pub(super) last_target_monitor: Option<String>,
    pub(super) last_render_dpi: Option<u32>,
    pub(super) last_shown: Option<SystemTime>,
}

impl OverlayState {
    pub(super) fn new(graphics: OverlayGraphics) -> Self {
        let now = Instant::now();
        let preferences = SystemVisualPreferences::query();
        let config = crate::config::Config::default().overlay;
        Self {
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
            hold_until: now,
            last_presented: now,
            dpi: 96,
            last_target_monitor: None,
            last_render_dpi: None,
            last_shown: None,
        }
    }

    pub(super) fn prepare_show(
        &mut self,
        model: OverlayModel,
        config: OverlayCfg,
        preferences: SystemVisualPreferences,
        monitor: Option<crate::platform::monitor::MonitorGeometry>,
    ) -> Result<Option<ShowPlan>> {
        if model.rows.is_empty() || !config.enabled {
            return Ok(None);
        }
        self.preferences = preferences;
        self.motion = motion_policy(preferences);
        let notifications = config.notifications;
        self.config = config;
        let now = Instant::now();
        let coalesce = self.phase != Phase::Hidden
            && now.duration_since(self.last_presented) <= Duration::from_millis(COALESCE_WINDOW_MS);
        self.model = if model.bypass_categories {
            model
        } else if coalesce {
            let current = self.model.clone().filter_enabled(notifications);
            merge_overlay_models(&current, &model)
        } else {
            model
        };
        self.last_presented = now;
        self.dpi =
            crate::platform::dpi::effective_render_dpi(monitor.as_ref().map(|value| value.dpi));
        self.last_target_monitor = monitor.as_ref().map(|value| value.device_name.clone());
        self.last_render_dpi = Some(self.dpi);
        self.last_shown = Some(SystemTime::now());
        self.refresh_palette();
        self.surface_size =
            surface_geometry(self.config.scale, self.model.rows.len()).pixel_size(self.dpi);
        self.base_position = position_for(
            monitor.map(|value| value.work).unwrap_or(RECT_FALLBACK),
            self.surface_size,
            self.config.position,
            self.dpi,
        );
        let appearance_elapsed_ms = now
            .duration_since(self.phase_started)
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let timing = timing_after_show(
            self.phase,
            appearance_elapsed_ms,
            self.motion,
            coalesce,
            self.config.duration_ms as u64,
        );
        self.phase = timing.phase;
        if timing.restart_phase {
            self.phase_started = now;
        }
        self.hold_until = now + Duration::from_millis(timing.hold_after_now_ms);
        Ok(Some(self.frame_plan()))
    }

    pub(super) fn frame_plan(&self) -> ShowPlan {
        let (alpha, slide_dip) = self.frame_values();
        let slide_px = (slide_dip * self.dpi as f32 / 96.0).round() as i32;
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

    pub(super) fn timer_interval(&self) -> u32 {
        if self.motion == MotionPolicy::Reduced {
            self.hold_until
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, u32::MAX as u128) as u32
        } else {
            TIMER_MS
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

    pub(super) fn prepare_visual_refresh(
        &mut self,
        preferences: SystemVisualPreferences,
        monitor: Option<crate::platform::monitor::MonitorGeometry>,
    ) -> Result<Option<ShowPlan>> {
        let backdrop_enabled = composition_blur_enabled(
            preferences,
            self.surface
                .as_ref()
                .is_some_and(OverlaySurface::is_composition),
        );
        if preferences == self.preferences && backdrop_enabled == self.backdrop_enabled {
            return Ok(None);
        }
        self.preferences = preferences;
        self.motion = motion_policy(preferences);
        self.dpi =
            crate::platform::dpi::effective_render_dpi(monitor.as_ref().map(|value| value.dpi));
        self.last_target_monitor = monitor.as_ref().map(|value| value.device_name.clone());
        self.last_render_dpi = Some(self.dpi);
        self.last_shown = Some(SystemTime::now());
        self.refresh_palette();
        if self.phase == Phase::Hidden {
            return Ok(None);
        }
        self.surface_size =
            surface_geometry(self.config.scale, self.model.rows.len()).pixel_size(self.dpi);
        self.base_position = position_for(
            monitor.map(|value| value.work).unwrap_or(RECT_FALLBACK),
            self.surface_size,
            self.config.position,
            self.dpi,
        );
        if self.motion == MotionPolicy::Reduced {
            self.phase = Phase::Holding;
            self.phase_started = Instant::now();
        }
        Ok(Some(self.frame_plan()))
    }

    pub(super) fn prepare_tick(&mut self) -> Option<TickPlan> {
        let now = Instant::now();
        if self.motion == MotionPolicy::Reduced {
            if now >= self.hold_until {
                self.phase = Phase::Hidden;
                return Some(TickPlan::Hide);
            }
            return None;
        }
        match self.phase {
            Phase::Appearing => {
                if now.duration_since(self.phase_started) >= Duration::from_millis(APPEAR_MS) {
                    self.phase = Phase::Holding;
                    self.phase_started = now;
                }
            }
            Phase::Holding => {
                if now >= self.hold_until {
                    self.phase = Phase::Leaving;
                    self.phase_started = now;
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
