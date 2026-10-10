//! Overlay state transitions and frame planning. Native operations consume owned plans.

use super::backend::{OverlayGraphics, OverlayRenderData, OverlaySurface, SurfaceSpec};
use super::badge::BadgeMotion;
use super::layout::{geometry_with_width, window_region_for};
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
use windows::Win32::Foundation::{HWND, POINT, SIZE};

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
    pub(super) animate_content: bool,
    pub(super) layout_size: SIZE,
    pub(super) frame_period: Duration,
    pub(super) cluster: Option<super::group::ClusterRequest>,
    pub(super) join: Option<super::group::JoinRequest>,
    pub(super) behind_badge: Option<HWND>,
    pub(super) edge_entrance: bool,
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
    pub(super) content: super::content::ContentMotion,
    pub(super) cluster: super::group::ClusterMotion,
    pub(super) join: Option<super::group::JoinMotion>,
    pub(super) compacted_at: Option<Instant>,
    pub(super) collapse_origin: Option<POINT>,
    pub(super) parked: bool,
    pub(super) behind_badge: Option<HWND>,
    pub(super) edge_entrance: bool,
    pub(super) hover: super::hover::HoverMotion,
    pub(super) layout_size: SIZE,
    pub(super) phase: Phase,
    pub(super) phase_started: Instant,
    /// Opacity and slide at the start of an entrance or an interrupted exit.
    appearance_origin: (f32, f32),
    /// Each card owns its own optional toast deadline.
    pub(super) expires_at: Option<Instant>,
    pub(super) dpi: u32,
    pub(super) last_target_monitor: Option<String>,
    pub(super) last_render_dpi: Option<u32>,
    pub(super) last_shown: Option<SystemTime>,
    pub(super) frame_period: Duration,
    #[cfg(test)]
    pub(super) test_frame_times: Vec<Instant>,
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
            content: super::content::ContentMotion::default(),
            cluster: super::group::ClusterMotion::default(),
            join: None,
            compacted_at: None,
            collapse_origin: None,
            parked: false,
            behind_badge: None,
            edge_entrance: false,
            hover: super::hover::HoverMotion::default(),
            layout_size: SIZE::default(),
            phase: Phase::Hidden,
            phase_started: now,
            appearance_origin: (0.0, 12.0),
            expires_at: None,
            dpi: 96,
            last_target_monitor: None,
            last_render_dpi: None,
            last_shown: None,
            frame_period: Duration::from_secs_f64(1.0 / 60.0),
            #[cfg(test)]
            test_frame_times: Vec::new(),
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
            animate_content,
            layout_size,
            frame_period,
            cluster,
            join,
            behind_badge,
            edge_entrance,
        } = request;
        if model.rows.is_empty() || !config.enabled {
            return Ok(None);
        }
        let now = Instant::now();
        let previous_position = self.presentation_position(now, self.presentation_size(now));
        self.preferences = preferences;
        self.motion = motion_policy(preferences);
        self.parked = false;
        self.behind_badge = behind_badge;
        self.cluster.update(cluster, self.motion, now);
        self.update_join(join, previous_position);
        self.config = config;
        self.content.update(
            &self.model,
            &model,
            animate_content && self.phase != Phase::Hidden,
            self.badge.value(now) >= 1.0 / 3.0,
            self.motion,
            now,
        );
        self.badge
            .update(collapsible_mute, presentation_started_at, self.motion, now);
        if !self.badge.reserves_compact() {
            self.compacted_at = None;
        }
        if self.badge.collapse_started().is_none() {
            self.collapse_origin = None;
        }
        self.layout_size = layout_size;
        self.frame_period = frame_period;
        self.model = model;
        self.expires_at = expires_at;
        self.generation = generation;
        self.timer_id = timer_id_for_generation(generation);
        self.dpi = dpi.max(96);
        self.last_target_monitor = target_monitor;
        self.last_render_dpi = Some(self.dpi);
        if mode != ShowMode::Relayout {
            self.last_shown = Some(SystemTime::now());
        }
        self.refresh_palette();
        let offset = self.presentation_offset(self.presentation_size(now));
        let previous_anchor = POINT {
            x: previous_position.x - offset.x,
            y: previous_position.y - offset.y,
        };
        let hidden = self.phase == Phase::Hidden;
        self.update_show_timing(mode, presentation_started_at, edge_entrance, now);
        self.update_show_position(mode, hidden, previous_anchor, position, now);
        Ok(Some(self.frame_plan()))
    }

    fn update_show_timing(
        &mut self,
        mode: ShowMode,
        presentation_started_at: Instant,
        edge_entrance: bool,
        now: Instant,
    ) {
        let previous_phase = self.phase;
        let origin = self.frame_values(now);
        let timing = timing_after_show(previous_phase, self.motion, mode);
        self.phase = timing.phase;
        if !timing.restart_phase {
            return;
        }
        let resume_exit = mode == ShowMode::Refresh && previous_phase == Phase::Leaving;
        self.appearance_origin = if resume_exit { origin } else { (0.0, 12.0) };
        self.phase_started = if resume_exit {
            now
        } else {
            presentation_started_at
        };
        if mode != ShowMode::Refresh || previous_phase == Phase::Hidden {
            self.edge_entrance = edge_entrance;
        }
    }

    fn update_show_position(
        &mut self,
        mode: ShowMode,
        hidden: bool,
        from: POINT,
        to: POINT,
        now: Instant,
    ) {
        if mode == ShowMode::Present || hidden {
            self.position_tween = None;
        } else if mode != ShowMode::Refresh || self.base_position != to {
            self.position_tween = PositionTween::start(from, to, self.motion, now);
        }
        self.base_position = to;
    }

    pub(super) fn frame_plan(&self) -> ShowPlan {
        self.frame_plan_at(Instant::now())
    }

    fn frame_plan_at(&self, now: Instant) -> ShowPlan {
        let (alpha, slide_dip) = self.frame_values(now);
        let slide_px = (slide_dip * self.config.scale * self.dpi as f32 / 96.0).round() as i32;
        let slide = if self.edge_entrance {
            entrance_offset(self.config.position, slide_px)
        } else {
            stacked_entrance_offset(self.config.position, slide_px)
        };
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
            self.content.timer_interval(),
            self.cluster.timer_interval(),
            self.join
                .filter(|join| !join.finished(now))
                .map(|_| super::timeline::TIMER_MS),
        ]
        .into_iter()
        .flatten()
        .min();
        ShowPlan {
            position: POINT {
                x: position.x + slide.x,
                y: position.y + slide.y,
            },
            size,
            region: window_region_for(size, self.dpi),
            alpha,
            compact,
            hover_alpha: self.hover.value(now),
            layout_changed: false,
            timer_id: self.timer_id,
            timer_interval,
            content_width: self.content.width(&self.model, now),
            text_alpha: self.content.text_alpha(now),
            cluster_extra: self.cluster.extra(now) * compact,
            cluster_primary_offset: self.cluster.primary_offset(now) * compact,
            join_alpha: self.join.map_or(1.0, |join| join.alpha(now)),
            behind_badge: self.behind_badge,
            animation_active: self.motion == MotionPolicy::Animated
                && (matches!(self.phase, Phase::Appearing | Phase::Leaving)
                    || self
                        .position_tween
                        .is_some_and(|tween| !tween.is_finished(now))
                    || self.badge.is_animating()
                    || self.content.is_animating()
                    || self.cluster.timer_interval().is_some()
                    || self.join.is_some_and(|join| !join.finished(now))
                    || self.hover.is_animating()),
        }
    }

    fn presentation_position(&self, now: Instant, size: SIZE) -> POINT {
        if let Some(join) = self.join {
            return join.position(now);
        }
        let position = self.position_at(now);
        let offset = self.presentation_offset(size);
        POINT {
            x: position.x + offset.x,
            y: position.y + offset.y,
        }
    }

    fn presentation_size(&self, now: Instant) -> SIZE {
        let mut geometry = geometry_with_width(
            self.config.scale,
            self.model.rows.len(),
            self.content.width(&self.model, now),
            self.badge.value(now),
        );
        geometry.width += self.cluster.extra(now) * self.badge.value(now) * self.config.scale;
        geometry.body_right = geometry.width;
        geometry.pixel_size(self.dpi)
    }

    fn presentation_offset(&self, size: SIZE) -> POINT {
        let dx = self.layout_size.cx - size.cx;
        let dy = self.layout_size.cy - size.cy;
        let x = match self.cluster.side() {
            Some(super::group::Side::Left) => dx,
            Some(super::group::Side::Right) => 0,
            None => match self.config.position {
                OverlayPosition::TopRight
                | OverlayPosition::CenterRight
                | OverlayPosition::BottomRight => dx,
                OverlayPosition::TopCenter
                | OverlayPosition::Center
                | OverlayPosition::BottomCenter => dx / 2,
                _ => 0,
            },
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
        if self.phase == Phase::Hidden || self.parked {
            return None;
        }
        let old_collapse = self.badge.collapse_started();
        let collapse_position = self.presentation_position(now, self.presentation_size(now));
        let mut layout_changed = self.badge.tick(self.motion, now);
        if old_collapse != self.badge.collapse_started() {
            self.collapse_origin = Some(collapse_position);
        }
        if self.badge.reserves_compact() && self.compacted_at.is_none() {
            self.compacted_at = Some(now);
        }
        layout_changed |= self.cluster.tick(now);
        if let Some(join) = &mut self.join {
            layout_changed |= join.tick(now);
        }
        self.hover.tick(now);
        self.content.tick(now);
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
                (
                    self.appearance_origin.0 + (1.0 - self.appearance_origin.0) * eased,
                    self.appearance_origin.1 * (1.0 - eased),
                )
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
        let now = Instant::now();
        let mut model = self.model.clone();
        model.width_dip = Some(self.content.width(&model, now));
        OverlayRenderData {
            dwrite: self.graphics.dwrite.clone(),
            model,
            previous_text: self.content.previous.clone(),
            text_alpha: self.content.text_alpha(now),
            cluster_extra: self.cluster.extra(now) * compact,
            cluster_primary_offset: self.cluster.primary_offset(now) * compact,
            cluster_side: self.cluster.side(),
            cluster_peers: if compact >= 0.999 {
                self.cluster.icons(now)
            } else {
                Vec::new()
            },
            join_alpha: self.join.map_or(1.0, |join| join.alpha(now)),
            scale: self.config.scale,
            palette: self.palette,
            theme_mode: resolved_theme_mode(self.config.appearance, self.preferences),
            alpha,
            blur: self.config.blur,
            compact,
            hover_alpha: self.hover.value(Instant::now()),
        }
    }

    fn update_join(&mut self, request: Option<super::group::JoinRequest>, from: POINT) {
        match request {
            None => self.join = None,
            Some(request) => {
                if self.join.is_some_and(|join| {
                    join.request.started == request.started
                        && join.request.position == request.position
                }) {
                    return;
                }
                let from = if self.badge.collapse_started() == Some(request.started) {
                    self.collapse_origin.unwrap_or(from)
                } else {
                    from
                };
                self.join = Some(super::group::JoinMotion::new(from, request));
            }
        }
    }
}

fn entrance_offset(position: OverlayPosition, distance: i32) -> POINT {
    let (x, y) = match position {
        OverlayPosition::TopLeft => (-1, -1),
        OverlayPosition::TopCenter => (0, -1),
        OverlayPosition::TopRight => (1, -1),
        OverlayPosition::CenterLeft => (-1, 0),
        OverlayPosition::CenterRight => (1, 0),
        OverlayPosition::BottomLeft => (-1, 1),
        OverlayPosition::BottomCenter => (0, 1),
        OverlayPosition::BottomRight => (1, 1),
        OverlayPosition::Center => (0, 0),
    };
    POINT {
        x: x * distance,
        y: y * distance,
    }
}

fn stacked_entrance_offset(position: OverlayPosition, distance: i32) -> POINT {
    let (x, y) = match position {
        OverlayPosition::TopLeft | OverlayPosition::TopCenter | OverlayPosition::TopRight => {
            (0, -1)
        }
        OverlayPosition::CenterLeft => (-1, 0),
        OverlayPosition::CenterRight => (1, 0),
        OverlayPosition::BottomLeft
        | OverlayPosition::BottomCenter
        | OverlayPosition::BottomRight => (0, 1),
        OverlayPosition::Center => (0, 0),
    };
    POINT {
        x: x * distance,
        y: y * distance,
    }
}

#[cfg(test)]
mod entrance_tests {
    use super::*;
    #[test]
    fn stacked_cards_enter_and_exit_along_their_screen_edge() {
        let _com = crate::platform::com::ComApartment::init_sta();
        let mut state =
            OverlayState::new(super::super::backend::OverlayGraphics::create().unwrap(), 0);
        state.model = OverlayModel::single(super::super::model::OverlayRow::preview(
            "Second popup",
            "Direction",
        ));
        state.motion = MotionPolicy::Animated;
        state.edge_entrance = false;
        state.dpi = 144;
        state.base_position = POINT { x: 300, y: 300 };
        let start = state.phase_started;
        state.layout_size = state.presentation_size(start);
        for (position, (x, y)) in [
            (OverlayPosition::TopLeft, (0, -1)),
            (OverlayPosition::TopCenter, (0, -1)),
            (OverlayPosition::TopRight, (0, -1)),
            (OverlayPosition::CenterLeft, (-1, 0)),
            (OverlayPosition::CenterRight, (1, 0)),
            (OverlayPosition::BottomLeft, (0, 1)),
            (OverlayPosition::BottomCenter, (0, 1)),
            (OverlayPosition::BottomRight, (0, 1)),
        ] {
            state.config.position = position;
            state.phase = Phase::Appearing;
            let plan = state.frame_plan_at(start);
            assert_eq!(
                plan.position,
                POINT {
                    x: 300 + 18 * x,
                    y: 300 + 18 * y
                }
            );
            assert_eq!(
                state
                    .frame_plan_at(start + Duration::from_millis(APPEAR_MS))
                    .position,
                state.base_position
            );
            state.phase = Phase::Leaving;
            let plan = state.frame_plan_at(start + Duration::from_millis(LEAVE_MS / 2));
            assert_eq!(
                plan.position,
                POINT {
                    x: 300 + 6 * x,
                    y: 300 + 6 * y
                }
            );
            assert!(plan.alpha > 0.0 && plan.alpha < 1.0);
            state.motion = MotionPolicy::Reduced;
            assert_eq!(state.frame_plan_at(start).position, state.base_position);
            state.motion = MotionPolicy::Animated;
        }
    }

    #[test]
    fn output_refresh_finishes_entrance_without_restarting_and_reverses_exit_continuously() {
        let _com = crate::platform::com::ComApartment::init_sta();
        let mut state =
            OverlayState::new(super::super::backend::OverlayGraphics::create().unwrap(), 0);
        state.motion = MotionPolicy::Animated;
        state.phase = Phase::Appearing;
        state.edge_entrance = true;
        let start = state.phase_started;
        for milliseconds in [20, 40, 80, 120] {
            let now = start + Duration::from_millis(milliseconds);
            let before = state.frame_values(now);
            state.update_show_timing(ShowMode::Refresh, now, false, now);
            assert_eq!(state.phase_started, start);
            assert_eq!(state.frame_values(now), before);
            assert!(state.edge_entrance);
        }
        state.advance_phase(start + Duration::from_millis(APPEAR_MS));
        assert_eq!(state.phase, Phase::Holding);
        let now = start + Duration::from_millis(200);
        state.update_show_timing(ShowMode::Refresh, now, false, now);
        assert_eq!(state.phase, Phase::Holding);
        assert_eq!(state.frame_values(now), (1.0, 0.0));

        for milliseconds in [1, LEAVE_MS / 2, LEAVE_MS - 1] {
            state.phase = Phase::Leaving;
            state.phase_started = start;
            let now = start + Duration::from_millis(milliseconds);
            let before = state.frame_values(now);
            state.update_show_timing(ShowMode::Refresh, now, false, now);
            assert_eq!(state.phase, Phase::Appearing);
            assert_eq!(
                state.frame_values(now),
                before,
                "exit refresh must not flash or jump"
            );
            let middle = state.frame_values(now + Duration::from_millis(APPEAR_MS / 2));
            assert!(middle.0 > before.0);
            assert!(middle.1 < before.1);
            assert_eq!(
                state.frame_values(now + Duration::from_millis(APPEAR_MS)),
                (1.0, 0.0)
            );
        }
        state.motion = MotionPolicy::Reduced;
        state.phase = Phase::Leaving;
        state.update_show_timing(ShowMode::Refresh, now, false, now);
        assert_eq!(state.phase, Phase::Holding);
    }

    #[test]
    fn output_refresh_does_not_restart_an_existing_position_tween() {
        let _com = crate::platform::com::ComApartment::init_sta();
        let mut state =
            OverlayState::new(super::super::backend::OverlayGraphics::create().unwrap(), 0);
        state.motion = MotionPolicy::Animated;
        let start = state.phase_started;
        let from = POINT { x: 300, y: 300 };
        let to = POINT { x: 300, y: 400 };
        state.base_position = to;
        state.position_tween = PositionTween::start(from, to, state.motion, start);
        let reference = state.position_tween.unwrap();
        for milliseconds in [20, 40, 60, 80, 100, 120] {
            let now = start + Duration::from_millis(milliseconds);
            state.update_show_position(ShowMode::Refresh, false, state.position_at(now), to, now);
            assert_eq!(state.position_at(now), reference.position_at(now));
        }
        assert!(state
            .position_tween
            .unwrap()
            .is_finished(start + Duration::from_millis(super::super::timeline::POSITION_TWEEN_MS)));
    }

    #[test]
    fn frame_plan_applies_direction_only_to_the_first_card_and_honors_dpi_and_reduced_motion() {
        let _com = crate::platform::com::ComApartment::init_sta();
        let graphics = super::super::backend::OverlayGraphics::create().unwrap();
        let mut state = OverlayState::new(graphics, 0);
        state.model = OverlayModel::single(super::super::model::OverlayRow::preview(
            "Preview",
            "Direction",
        ));
        state.motion = MotionPolicy::Animated;
        state.phase = Phase::Appearing;
        state.edge_entrance = true;
        state.dpi = 144;
        state.base_position = POINT { x: 300, y: 300 };
        let start = state.phase_started;
        state.layout_size = state.presentation_size(start);
        for (position, expected) in [
            (OverlayPosition::TopLeft, (-18, -18)),
            (OverlayPosition::TopCenter, (0, -18)),
            (OverlayPosition::TopRight, (18, -18)),
            (OverlayPosition::CenterLeft, (-18, 0)),
            (OverlayPosition::CenterRight, (18, 0)),
            (OverlayPosition::BottomLeft, (-18, 18)),
            (OverlayPosition::BottomCenter, (0, 18)),
            (OverlayPosition::BottomRight, (18, 18)),
        ] {
            state.config.position = position;
            let plan = state.frame_plan_at(start);
            assert_eq!((plan.position.x - 300, plan.position.y - 300), expected);
            assert_eq!(
                state
                    .frame_plan_at(start + Duration::from_millis(APPEAR_MS))
                    .position,
                state.base_position
            );
        }
        state.edge_entrance = false;
        assert_eq!(
            state.frame_plan_at(start).position,
            POINT { x: 300, y: 318 }
        );
        state.edge_entrance = true;
        state.motion = MotionPolicy::Reduced;
        assert_eq!(state.frame_plan_at(start).position, state.base_position);
    }

    #[test]
    fn first_card_exits_back_toward_its_entrance_at_every_corner_and_edge() {
        let _com = crate::platform::com::ComApartment::init_sta();
        let graphics = super::super::backend::OverlayGraphics::create().unwrap();
        let mut state = OverlayState::new(graphics, 0);
        state.model = OverlayModel::single(super::super::model::OverlayRow::preview(
            "Preview",
            "Exit direction",
        ));
        state.motion = MotionPolicy::Animated;
        state.phase = Phase::Leaving;
        state.edge_entrance = true;
        state.dpi = 144;
        state.base_position = POINT { x: 300, y: 300 };
        let start = state.phase_started;
        state.layout_size = state.presentation_size(start);
        let middle = start + Duration::from_millis(LEAVE_MS / 2);
        for (position, expected) in [
            (OverlayPosition::TopLeft, (-6, -6)),
            (OverlayPosition::TopCenter, (0, -6)),
            (OverlayPosition::TopRight, (6, -6)),
            (OverlayPosition::CenterLeft, (-6, 0)),
            (OverlayPosition::CenterRight, (6, 0)),
            (OverlayPosition::BottomLeft, (-6, 6)),
            (OverlayPosition::BottomCenter, (0, 6)),
            (OverlayPosition::BottomRight, (6, 6)),
        ] {
            state.config.position = position;
            assert_eq!(state.frame_plan_at(start).position, state.base_position);
            let plan = state.frame_plan_at(middle);
            assert_eq!((plan.position.x - 300, plan.position.y - 300), expected);
            assert!(plan.alpha > 0.0 && plan.alpha < 1.0);
        }
        state.edge_entrance = false;
        assert_eq!(
            state.frame_plan_at(middle).position,
            POINT { x: 300, y: 306 }
        );
        state.edge_entrance = true;
        state.motion = MotionPolicy::Reduced;
        assert_eq!(state.frame_plan_at(middle).position, state.base_position);
    }

    #[test]
    fn first_card_comes_from_each_named_corner_or_edge_and_settles_at_anchor() {
        for (position, expected) in [
            (OverlayPosition::TopLeft, (-12, -12)),
            (OverlayPosition::TopCenter, (0, -12)),
            (OverlayPosition::TopRight, (12, -12)),
            (OverlayPosition::CenterLeft, (-12, 0)),
            (OverlayPosition::CenterRight, (12, 0)),
            (OverlayPosition::BottomLeft, (-12, 12)),
            (OverlayPosition::BottomCenter, (0, 12)),
            (OverlayPosition::BottomRight, (12, 12)),
        ] {
            let offset = entrance_offset(position, 12);
            assert_eq!((offset.x, offset.y), expected);
            assert_eq!(entrance_offset(position, 0), POINT::default());
        }
    }
}
