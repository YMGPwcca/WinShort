//! Independent overlay-card scheduling and native-window ownership.
//!
//! The registry identity invariant is one active entry per semantic
//! OverlayKey; different keys are the only way cards stack.

use super::backend::{OverlayGraphics, RECT_FALLBACK};
use super::composition::CompositionRuntime;
#[cfg(test)]
use super::layout::surface_geometry;
use super::layout::{layout_cards, model_geometry, select_monitor, CardPlacement, LayoutInput};
use super::model::OverlayModel;
use super::palette::acceptance_forces_composition_failure;
use super::state::ShowRequest;
use super::timeline::{motion_policy, toast_deadline, MotionPolicy, ShowMode};
use super::window::{OverlayRuntimeStatus, OverlayWindow};
use crate::config::model::{OverlayCfg, OverlayPosition};
use crate::error::Result;
use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::RECT;

pub(super) const UNASSIGNED_ENTRY_ID: u64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum OverlayKey {
    MicrophonePermanent,
    MicrophoneToast,
    Speaker,
    CurrentAppAudio,
    CurrentAppAudioPermanent,
    CurrentAppVolume,
    Workspace,
    DisplayProfile,
    Status,
    Preview,
    InputDevice,
    OutputDevice,
}

impl OverlayKey {
    pub(crate) const ALL: [Self; 12] = [
        Self::MicrophonePermanent,
        Self::MicrophoneToast,
        Self::Speaker,
        Self::CurrentAppAudio,
        Self::CurrentAppAudioPermanent,
        Self::CurrentAppVolume,
        Self::InputDevice,
        Self::OutputDevice,
        Self::Workspace,
        Self::DisplayProfile,
        Self::Status,
        Self::Preview,
    ];

    fn permanent_rank(self) -> u8 {
        match self {
            Self::MicrophonePermanent => 0,
            Self::Speaker => 1,
            Self::CurrentAppAudioPermanent => 2,
            Self::CurrentAppAudio => 10,
            Self::CurrentAppVolume => 11,
            Self::Workspace => 3,
            Self::DisplayProfile => 4,
            Self::Status => 5,
            Self::Preview => 6,
            Self::InputDevice => 7,
            Self::OutputDevice => 8,
            Self::MicrophoneToast => 9,
        }
    }

    fn is_compact_mute(self) -> bool {
        matches!(
            self,
            Self::MicrophonePermanent | Self::CurrentAppAudioPermanent
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayLifetime {
    Permanent,
    Toast,
}

impl OverlayLifetime {
    pub(crate) fn is_permanent(self) -> bool {
        matches!(self, Self::Permanent)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OverlayRequest {
    pub(crate) key: OverlayKey,
    pub(crate) model: OverlayModel,
    pub(crate) lifetime: OverlayLifetime,
    pub(crate) replace_key: Option<OverlayKey>,
}

impl OverlayRequest {
    pub(crate) fn permanent(key: OverlayKey, model: OverlayModel) -> Self {
        Self {
            key,
            model,
            lifetime: OverlayLifetime::Permanent,
            replace_key: None,
        }
    }

    pub(crate) fn toast(key: OverlayKey, model: OverlayModel) -> Self {
        Self {
            key,
            model,
            lifetime: OverlayLifetime::Toast,
            replace_key: None,
        }
    }

    /// Explicit state transitions can keep a card's HWND and presentation.
    pub(crate) fn replacing(mut self, key: OverlayKey) -> Self {
        self.replace_key = Some(key);
        self
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OverlayEntry {
    id: u64,
    generation: u64,
    key: OverlayKey,
    model: OverlayModel,
    lifetime: OverlayLifetime,
    expires_at: Option<Instant>,
    presented_at: Instant,
    sequence: u64,
    placement: ResolvedPlacement,
    lifecycle: EntryLifecycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryLifecycle {
    Active,
    Expiring,
}

impl OverlayEntry {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn key(&self) -> OverlayKey {
        self.key
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn model(&self) -> &OverlayModel {
        &self.model
    }

    pub(crate) fn lifetime(&self) -> OverlayLifetime {
        self.lifetime
    }

    pub(crate) fn expires_at(&self) -> Option<Instant> {
        self.expires_at
    }

    pub(crate) fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) fn presented_at(&self) -> Instant {
        self.presented_at
    }

    pub(crate) fn is_permanent(&self) -> bool {
        self.lifetime.is_permanent()
    }

    pub(crate) fn is_expired(&self, now: Instant) -> bool {
        self.expires_at.is_some_and(|expires_at| now >= expires_at)
    }

    fn is_expiring(&self) -> bool {
        self.lifecycle == EntryLifecycle::Expiring
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresentOutcome {
    pub(crate) id: u64,
    pub(crate) inserted: bool,
    pub(crate) restart_appearance: bool,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct OverlayRegistry {
    entries: Vec<OverlayEntry>,
    next_id: u64,
    next_sequence: u64,
    next_generation: u64,
}

impl OverlayRegistry {
    pub(crate) fn entries(&self) -> &[OverlayEntry] {
        &self.entries
    }

    #[cfg(test)]
    pub(crate) fn present(
        &mut self,
        request: OverlayRequest,
        duration: Duration,
        now: Instant,
    ) -> PresentOutcome {
        self.present_with_placement(
            request,
            duration,
            now,
            MotionPolicy::Reduced,
            ResolvedPlacement::default(),
        )
    }

    fn present_with_placement(
        &mut self,
        request: OverlayRequest,
        duration: Duration,
        now: Instant,
        motion: MotionPolicy,
        placement: ResolvedPlacement,
    ) -> PresentOutcome {
        let mut expires_at = match request.lifetime {
            OverlayLifetime::Permanent => None,
            OverlayLifetime::Toast => Some(toast_deadline(now, motion, duration)),
        };
        let sequence = self.next_sequence();
        if let Some(index) = self.replacement_index(&request) {
            let generation = self.next_generation();
            let entry = &mut self.entries[index];
            let transition = entry.key != request.key;
            let restart_appearance = !entry.lifetime.is_permanent() && !transition;
            let preserve_placement = entry.lifetime.is_permanent() || transition;
            if transition
                && request.lifetime == OverlayLifetime::Toast
                && motion == MotionPolicy::Animated
            {
                expires_at =
                    Some(now + Duration::from_millis(super::badge::BADGE_TWEEN_MS) + duration);
            }
            entry.key = request.key;
            entry.model = request.model;
            entry.lifetime = request.lifetime;
            entry.expires_at = expires_at;
            entry.presented_at = now;
            entry.generation = generation;
            entry.sequence = sequence;
            entry.lifecycle = EntryLifecycle::Active;
            if !preserve_placement {
                entry.placement = placement;
            }
            return PresentOutcome {
                id: entry.id,
                inserted: false,
                restart_appearance,
            };
        }

        let id = self.next_id();
        let generation = self.next_generation();
        self.entries.push(OverlayEntry {
            id,
            generation,
            key: request.key,
            model: request.model,
            lifetime: request.lifetime,
            expires_at,
            presented_at: now,
            sequence,
            placement,
            lifecycle: EntryLifecycle::Active,
        });
        debug_assert!(self.has_unique_keys());
        PresentOutcome {
            id,
            inserted: true,
            restart_appearance: true,
        }
    }

    pub(crate) fn remove_id(&mut self, id: u64) -> Option<OverlayEntry> {
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        Some(self.entries.remove(index))
    }

    #[cfg(test)]
    pub(crate) fn remove_expired(&mut self, now: Instant) -> Vec<OverlayEntry> {
        let mut active = Vec::with_capacity(self.entries.len());
        let mut expired = Vec::new();
        for entry in self.entries.drain(..) {
            if entry.is_expired(now) {
                expired.push(entry);
            } else {
                active.push(entry);
            }
        }
        self.entries = active;
        expired
    }

    pub(crate) fn filter_notifications(
        &mut self,
        notifications: crate::config::model::OverlayNotifications,
    ) -> Vec<u64> {
        let mut active = Vec::with_capacity(self.entries.len());
        let mut removed = Vec::new();
        for mut entry in self.entries.drain(..) {
            entry.model = entry.model.clone().filter_enabled(notifications);
            if entry.model.rows.is_empty() {
                removed.push(entry.id);
            } else {
                active.push(entry);
            }
        }
        self.entries = active;
        removed
    }

    pub(crate) fn remove_key(&mut self, key: OverlayKey) -> Vec<u64> {
        let mut removed = Vec::new();
        self.entries.retain(|entry| {
            if entry.key == key {
                removed.push(entry.id);
                false
            } else {
                true
            }
        });
        removed
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    fn mark_expiring(&mut self, now: Instant) {
        for entry in &mut self.entries {
            if !entry.is_permanent() && entry.is_expired(now) {
                entry.lifecycle = EntryLifecycle::Expiring;
            }
        }
    }

    fn remove_expired_generation(
        &mut self,
        id: u64,
        generation: u64,
        now: Instant,
    ) -> Option<OverlayEntry> {
        let index = self.entries.iter().position(|entry| {
            entry.id == id
                && entry.generation == generation
                && !entry.is_permanent()
                && entry.is_expired(now)
        })?;
        Some(self.entries.remove(index))
    }

    pub(crate) fn has_unique_keys(&self) -> bool {
        let mut keys = std::collections::HashSet::new();
        self.entries.iter().all(|entry| keys.insert(entry.key))
    }

    fn replacement_index(&self, request: &OverlayRequest) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.key == request.key)
            .or_else(|| {
                request
                    .replace_key
                    .and_then(|key| self.entries.iter().position(|entry| entry.key == key))
            })
    }

    fn next_id(&mut self) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        if self.next_id == UNASSIGNED_ENTRY_ID {
            self.next_id = 1;
        }
        self.next_id
    }

    fn next_sequence(&mut self) -> u64 {
        self.next_sequence = self.next_sequence.wrapping_add(1);
        if self.next_sequence == 0 {
            self.next_sequence = 1;
        }
        self.next_sequence
    }

    fn next_generation(&mut self) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1);
        if self.next_generation == 0 {
            self.next_generation = 1;
        }
        self.next_generation
    }
}

struct ManagedWindow {
    id: u64,
    window: OverlayWindow,
}

#[derive(Clone)]
struct PresentationStateSnapshot {
    registry: OverlayRegistry,
    render_configs: HashMap<u64, OverlayCfg>,
}

impl PresentationStateSnapshot {
    fn capture(registry: &OverlayRegistry, render_configs: &HashMap<u64, OverlayCfg>) -> Self {
        Self {
            registry: registry.clone(),
            render_configs: render_configs.clone(),
        }
    }

    fn restore(
        self,
        registry: &mut OverlayRegistry,
        render_configs: &mut HashMap<u64, OverlayCfg>,
    ) {
        *registry = self.registry;
        *render_configs = self.render_configs;
    }
}

pub(crate) struct OverlayManager {
    graphics: OverlayGraphics,
    registry: OverlayRegistry,
    render_configs: HashMap<u64, OverlayCfg>,
    spare: Option<OverlayWindow>,
    windows: Vec<ManagedWindow>,
    // Drop after every window so the shared dispatcher/compositor/device graph
    // outlives all per-card Composition targets.
    composition_runtime: Option<CompositionRuntime>,
    last_shown: Option<std::time::SystemTime>,
}

impl OverlayManager {
    #[cfg(test)]
    pub(super) fn test_hwnds(&self) -> Vec<windows::Win32::Foundation::HWND> {
        self.windows
            .iter()
            .map(|managed| managed.window.hwnd)
            .collect()
    }

    pub(crate) fn create() -> Result<Self> {
        let graphics = OverlayGraphics::create()?;
        let composition_runtime = if acceptance_forces_composition_failure() {
            crate::warn_!(
                "overlay Composition unavailable; using opaque D2D fallback: forced acceptance Composition failure"
            );
            None
        } else {
            match CompositionRuntime::create(&graphics) {
                Ok(runtime) => Some(runtime),
                Err(error) => {
                    crate::warn_!(
                        "overlay shared Composition runtime unavailable; using opaque D2D fallback for all cards: {error}"
                    );
                    None
                }
            }
        };
        let spare = Some(OverlayWindow::create_with_graphics(
            UNASSIGNED_ENTRY_ID,
            graphics.clone(),
            composition_runtime.clone(),
        )?);
        Ok(Self {
            graphics,
            registry: OverlayRegistry::default(),
            render_configs: HashMap::new(),
            spare,
            windows: Vec::new(),
            composition_runtime,
            last_shown: None,
        })
    }

    pub(crate) fn present(
        &mut self,
        mut request: OverlayRequest,
        config: OverlayCfg,
    ) -> Result<()> {
        if !config.enabled {
            self.clear();
            return Ok(());
        }
        if request.model.rows.is_empty() {
            return Ok(());
        }
        super::drawing::measure_model(&self.graphics.dwrite, &mut request.model)?;
        let snapshot = PresentationStateSnapshot::capture(&self.registry, &self.render_configs);
        let presentation_started_at = Instant::now();
        let motion = motion_policy(crate::platform::visual::SystemVisualPreferences::query());
        let placement = self.placement_for_presentation(&request, &config);
        let outcome = self.registry.present_with_placement(
            request,
            Duration::from_millis(config.duration_ms as u64),
            presentation_started_at,
            motion,
            placement,
        );
        if let Err(error) = self.ensure_window(outcome.id) {
            self.rollback_presentation(snapshot);
            return Err(error);
        }
        self.render_configs.insert(outcome.id, config);
        if let Err(error) = self.sync_layout(Some(outcome)) {
            self.rollback_presentation(snapshot);
            return Err(error);
        }
        if self
            .registry
            .entries()
            .iter()
            .any(|entry| entry.id() == outcome.id)
        {
            self.last_shown = Some(std::time::SystemTime::now());
        }
        Ok(())
    }

    pub(crate) fn remove_key(&mut self, key: OverlayKey, config: &OverlayCfg) -> Result<()> {
        if !config.enabled {
            self.clear();
            return Ok(());
        }
        let ids = self.registry.remove_key(key);
        for id in ids {
            self.render_configs.remove(&id);
            self.release_window(id);
        }
        self.sync_layout(None)
    }

    pub(crate) fn apply_config(&mut self, config: &OverlayCfg) -> Result<()> {
        if !config.enabled {
            self.clear();
            return Ok(());
        }

        let remove_ids = self.registry.filter_notifications(config.notifications);
        for id in remove_ids {
            self.render_configs.remove(&id);
            self.release_window(id);
        }
        for entry in &mut self.registry.entries {
            super::drawing::measure_model(&self.graphics.dwrite, &mut entry.model)?;
        }
        self.adopt_runtime_config(config);
        self.sync_layout(None)
    }

    pub(crate) fn refresh_visuals(&mut self) -> Result<()> {
        self.sync_layout(None)
    }

    pub(crate) fn card_expired(
        &mut self,
        id: u64,
        generation: u64,
        config: &OverlayCfg,
    ) -> Result<()> {
        if !config.enabled {
            self.clear();
            return Ok(());
        }
        let now = Instant::now();
        self.registry.mark_expiring(now);
        if self
            .registry
            .remove_expired_generation(id, generation, now)
            .is_none()
        {
            return Ok(());
        }
        self.render_configs.remove(&id);
        self.release_window(id);
        self.sync_layout(None)
    }

    fn adopt_runtime_config(&mut self, config: &OverlayCfg) {
        let placement = resolve_placement(config);
        adopt_runtime_configs(
            &mut self.registry,
            &mut self.render_configs,
            config,
            placement,
        );
    }

    fn placement_for_presentation(
        &self,
        request: &OverlayRequest,
        config: &OverlayCfg,
    ) -> ResolvedPlacement {
        if request.lifetime.is_permanent() {
            if let Some(entry) = self
                .registry
                .entries()
                .iter()
                .find(|entry| entry.key() == request.key && entry.is_permanent())
            {
                return entry.placement.clone();
            }
        }
        resolve_placement(config)
    }

    pub(crate) fn clear(&mut self) {
        self.registry.clear();
        self.render_configs.clear();
        while let Some(managed) = self.windows.pop() {
            self.release_window_value(managed.window);
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.registry.clear();
        self.render_configs.clear();
        for managed in self.windows.drain(..) {
            managed.window.destroy();
        }
        if let Some(spare) = self.spare.take() {
            spare.destroy();
        }
    }

    pub(crate) fn status(&self) -> OverlayRuntimeStatus {
        debug_assert!(self.registry.has_unique_keys());
        debug_assert!(self.registry.entries().len() <= OverlayKey::ALL.len());
        let window_statuses = self
            .windows
            .iter()
            .map(|managed| managed.window.status())
            .collect::<Vec<_>>();
        let active_monitor_summary = active_monitor_summary(&self.registry);
        OverlayRuntimeStatus {
            window_available: self.spare.is_some() || !self.windows.is_empty(),
            active_card_count: self.registry.entries().len(),
            permanent_card_count: self
                .registry
                .entries()
                .iter()
                .filter(|entry| entry.is_permanent())
                .count(),
            toast_card_count: self
                .registry
                .entries()
                .iter()
                .filter(|entry| !entry.is_permanent())
                .count(),
            target_monitor: active_monitor_summary.clone(),
            active_monitor_summary,
            resolved_appearance: consistent_string(
                window_statuses
                    .iter()
                    .map(|status| status.resolved_appearance.clone()),
            ),
            animations_enabled: consistent_value(
                window_statuses
                    .iter()
                    .map(|status| status.animations_enabled),
            ),
            high_contrast: consistent_value(
                window_statuses.iter().map(|status| status.high_contrast),
            ),
            disable_overlapped_content: consistent_value(
                window_statuses
                    .iter()
                    .map(|status| status.disable_overlapped_content),
            ),
            render_dpi: consistent_value(window_statuses.iter().map(|status| status.render_dpi)),
            last_shown: self.last_shown,
        }
    }

    fn rollback_presentation(&mut self, snapshot: PresentationStateSnapshot) {
        snapshot.restore(&mut self.registry, &mut self.render_configs);

        let active_ids = self
            .registry
            .entries()
            .iter()
            .map(OverlayEntry::id)
            .collect::<BTreeSet<_>>();
        let stale_window_ids = self
            .windows
            .iter()
            .filter(|managed| !active_ids.contains(&managed.id))
            .map(|managed| managed.id)
            .collect::<Vec<_>>();
        for id in stale_window_ids {
            self.release_window(id);
        }

        if let Err(error) = self.sync_layout(None) {
            crate::warn_!("overlay presentation rollback redraw failed: {error}");
        }
    }

    fn ensure_window(&mut self, id: u64) -> Result<()> {
        if self.windows.iter().any(|managed| managed.id == id) {
            return Ok(());
        }
        let generation = self
            .registry
            .entries()
            .iter()
            .find(|entry| entry.id() == id)
            .map_or(0, OverlayEntry::generation);
        let window = if let Some(window) = self.spare.take() {
            if let Err(error) = window.set_entry_id(id, generation) {
                window.destroy();
                return Err(error);
            }
            window
        } else {
            OverlayWindow::create_with_graphics(
                id,
                self.graphics.clone(),
                self.composition_runtime.clone(),
            )?
        };
        self.windows.push(ManagedWindow { id, window });
        Ok(())
    }

    fn release_window(&mut self, id: u64) {
        let Some(index) = self.windows.iter().position(|managed| managed.id == id) else {
            return;
        };
        let managed = self.windows.swap_remove(index);
        self.release_window_value(managed.window);
    }

    fn release_window_value(&mut self, window: OverlayWindow) {
        window.hide();
        if self.spare.is_none() && window.set_entry_id(UNASSIGNED_ENTRY_ID, 0).is_ok() {
            self.spare = Some(window);
        } else {
            window.destroy();
        }
    }

    fn sync_layout(&mut self, presentation: Option<PresentOutcome>) -> Result<()> {
        if self.registry.entries().is_empty() {
            return Ok(());
        }

        let now = Instant::now();
        self.registry.mark_expiring(now);
        refresh_registry_placements(&mut self.registry);
        let layout = plan_layout_with_evictions(self.build_layout_entries());
        for id in layout.evicted_ids {
            self.render_configs.remove(&id);
            self.registry.remove_id(id);
            self.release_window(id);
        }
        let plans = layout.entries;

        for plan in &plans {
            self.ensure_window(plan.entry.id)?;
        }

        let mut first_error = None;
        for plan in plans {
            if let Err(error) = self.show_planned_entry(plan, presentation) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn build_layout_entries(&self) -> Vec<LayoutEntry> {
        self.registry
            .entries()
            .iter()
            .filter_map(|entry| {
                if entry.is_expiring() {
                    return None;
                }
                let render_config = self.render_configs.get(&entry.id()).cloned()?;
                let placement = entry.placement.clone();
                let mut plan = make_layout_entry(entry, render_config, placement);
                if entry.key().is_compact_mute()
                    && entry.is_permanent()
                    && self.windows.iter().any(|managed| {
                        managed.id == entry.id() && managed.window.reserves_compact()
                    })
                {
                    plan.compact = true;
                    plan.size = model_geometry(plan.render_config.scale, &plan.model, 1.0)
                        .pixel_size(plan.placement.dpi);
                }
                Some(plan)
            })
            .collect()
    }

    fn show_planned_entry(
        &self,
        plan: PlannedEntry,
        presentation: Option<PresentOutcome>,
    ) -> Result<()> {
        let Some(managed) = self
            .windows
            .iter()
            .find(|managed| managed.id == plan.entry.id)
        else {
            return Ok(());
        };
        let mode = if presentation
            .is_some_and(|value| value.id == plan.entry.id && value.restart_appearance)
        {
            ShowMode::Present
        } else {
            ShowMode::Relayout
        };
        managed.window.show_at(ShowRequest {
            generation: plan.entry.generation,
            presentation_started_at: plan.entry.presented_at,
            model: plan.entry.model,
            config: plan.entry.render_config,
            dpi: plan.entry.placement.dpi,
            target_monitor: plan.entry.placement.key.monitor.clone(),
            position: plan.card.position,
            expires_at: plan.entry.expires_at,
            mode,
            collapsible_mute: plan.entry.key.is_compact_mute()
                && plan.entry.lifetime.is_permanent(),
            layout_size: plan.entry.size,
            frame_period: crate::platform::monitor::refresh_period(
                plan.entry.placement.key.monitor.as_deref(),
            ),
        })
    }
}

fn consistent_value<T: Copy + Eq>(values: impl Iterator<Item = Option<T>>) -> Option<T> {
    let mut result = None;
    let mut missing = false;
    for value in values {
        match value {
            None => missing = true,
            Some(value) => match result {
                None => result = Some(value),
                Some(existing) if existing != value => return None,
                Some(_) => {}
            },
        }
    }
    if missing {
        None
    } else {
        result
    }
}

fn consistent_string(values: impl Iterator<Item = Option<String>>) -> Option<String> {
    let mut result = None;
    let mut missing = false;
    for value in values {
        match value {
            None => missing = true,
            Some(value) => match &result {
                None => result = Some(value),
                Some(existing) if existing != &value => return None,
                Some(_) => {}
            },
        }
    }
    if missing {
        None
    } else {
        result
    }
}

fn active_monitor_summary(registry: &OverlayRegistry) -> Option<String> {
    let mut monitors = BTreeSet::new();
    for entry in registry.entries() {
        monitors.insert(
            entry
                .placement
                .key
                .monitor
                .as_deref()
                .unwrap_or("<unresolved>"),
        );
    }
    match monitors.len() {
        0 => None,
        1 => monitors.into_iter().next().map(str::to_owned),
        count => Some(format!("multiple ({count})")),
    }
}

impl Drop for OverlayManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Clone)]
struct LayoutEntry {
    id: u64,
    generation: u64,
    key: OverlayKey,
    model: OverlayModel,
    lifetime: OverlayLifetime,
    expires_at: Option<Instant>,
    presented_at: Instant,
    sequence: u64,
    render_config: OverlayCfg,
    placement: ResolvedPlacement,
    size: windows::Win32::Foundation::SIZE,
    compact: bool,
}

/// Stable identity for one independently laid out monitor/position group.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PlacementGroupKey {
    monitor: Option<String>,
    position: OverlayPosition,
}

/// Resolved monitor geometry captured by one active overlay entry.
#[derive(Debug, Clone)]
struct ResolvedPlacement {
    key: PlacementGroupKey,
    work: RECT,
    dpi: u32,
}

#[cfg(test)]
impl Default for ResolvedPlacement {
    fn default() -> Self {
        Self {
            key: PlacementGroupKey {
                monitor: None,
                position: OverlayPosition::BottomCenter,
            },
            work: RECT_FALLBACK,
            dpi: 96,
        }
    }
}

struct LayoutGroup {
    placement: ResolvedPlacement,
    entries: Vec<LayoutEntry>,
}

struct PlannedEntry {
    entry: LayoutEntry,
    card: CardPlacement,
}

struct LayoutPlan {
    entries: Vec<PlannedEntry>,
    evicted_ids: Vec<u64>,
}

fn resolve_placement(config: &OverlayCfg) -> ResolvedPlacement {
    resolve_placement_for_monitor(select_monitor(config.monitor.clone()), config.position)
}

fn resolve_placement_for_monitor(
    monitor: Option<crate::platform::monitor::MonitorGeometry>,
    position: OverlayPosition,
) -> ResolvedPlacement {
    let target_monitor = monitor.as_ref().map(|value| value.device_name.clone());
    let work = monitor
        .as_ref()
        .map(|value| value.work)
        .unwrap_or(RECT_FALLBACK);
    let dpi = crate::platform::dpi::effective_render_dpi(monitor.as_ref().map(|value| value.dpi));
    ResolvedPlacement {
        key: PlacementGroupKey {
            monitor: target_monitor.clone(),
            position,
        },
        work,
        dpi,
    }
}

fn adopt_runtime_configs(
    registry: &mut OverlayRegistry,
    render_configs: &mut HashMap<u64, OverlayCfg>,
    config: &OverlayCfg,
    placement: ResolvedPlacement,
) {
    for entry in &mut registry.entries {
        if !entry.model().bypass_categories {
            render_configs.insert(entry.id(), config.clone());
            entry.placement = placement.clone();
        }
    }
}

#[cfg(test)]
fn refresh_placement_snapshot_for_monitor(
    snapshot: &mut ResolvedPlacement,
    monitor: Option<crate::platform::monitor::MonitorGeometry>,
) {
    let position = snapshot.key.position;
    *snapshot = resolve_placement_for_monitor(monitor, position);
}

fn refresh_registry_placements(registry: &mut OverlayRegistry) {
    let mut refreshed = HashMap::new();
    for entry in &mut registry.entries {
        let group_key = entry.placement.key.clone();
        entry.placement = refreshed
            .entry(group_key)
            .or_insert_with(|| resolve_stored_placement(&entry.placement))
            .clone();
    }
}

fn resolve_stored_placement(snapshot: &ResolvedPlacement) -> ResolvedPlacement {
    let monitor = snapshot
        .key
        .monitor
        .as_deref()
        .and_then(|name| {
            crate::platform::monitor::all()
                .into_iter()
                .find(|monitor| monitor.device_name == name)
        })
        .or_else(crate::platform::monitor::primary);
    resolve_placement_for_monitor(monitor, snapshot.key.position)
}

fn make_layout_entry(
    entry: &OverlayEntry,
    render_config: OverlayCfg,
    placement: ResolvedPlacement,
) -> LayoutEntry {
    let size = model_geometry(render_config.scale, entry.model(), 0.0).pixel_size(placement.dpi);
    LayoutEntry {
        id: entry.id(),
        generation: entry.generation(),
        key: entry.key(),
        model: entry.model().clone(),
        lifetime: entry.lifetime(),
        expires_at: entry.expires_at(),
        presented_at: entry.presented_at(),
        sequence: entry.sequence(),
        render_config,
        placement,
        size,
        compact: false,
    }
}

#[cfg(test)]
fn plan_layout(entries: Vec<LayoutEntry>) -> Vec<PlannedEntry> {
    plan_layout_with_evictions(entries).entries
}

fn plan_layout_with_evictions(entries: Vec<LayoutEntry>) -> LayoutPlan {
    let mut layout = LayoutPlan {
        entries: Vec::new(),
        evicted_ids: Vec::new(),
    };
    for mut group in group_layout_entries(entries) {
        group.entries.sort_by_key(layout_order);
        let stack_scale = group
            .entries
            .iter()
            .map(|entry| entry.render_config.scale.clamp(0.7, 1.6))
            .fold(0.7, f32::max);
        let visible_count = visible_entry_count(
            &group.entries,
            group.placement.work,
            group.placement.key.position,
            group.placement.dpi,
            stack_scale,
        );
        let inputs = group.entries[..visible_count]
            .iter()
            .map(|entry| LayoutInput { size: entry.size })
            .collect::<Vec<_>>();
        let placements = layout_cards(
            group.placement.work,
            group.placement.key.position,
            group.placement.dpi,
            stack_scale,
            &inputs,
        );
        let mut entries = group.entries;
        let visible_entries = entries.drain(..visible_count);
        layout.entries.extend(
            visible_entries
                .zip(placements)
                .map(|(entry, card)| PlannedEntry { entry, card }),
        );
        layout.evicted_ids.extend(
            entries
                .into_iter()
                .filter(|entry| !entry.lifetime.is_permanent())
                .map(|entry| entry.id),
        );
    }
    layout
}

fn visible_entry_count(
    entries: &[LayoutEntry],
    work: RECT,
    position: OverlayPosition,
    dpi: u32,
    scale: f32,
) -> usize {
    let permanent_count = entries
        .iter()
        .take_while(|entry| entry.lifetime.is_permanent())
        .count();
    let mut visible_count = permanent_count;
    for index in permanent_count..entries.len() {
        let fits = index == 0 && permanent_count == 0
            || stack_fits(work, position, dpi, scale, &entries[..=index]);
        if !fits {
            break;
        }
        visible_count += 1;
    }
    visible_count
}

fn stack_fits(
    work: RECT,
    position: OverlayPosition,
    dpi: u32,
    scale: f32,
    entries: &[LayoutEntry],
) -> bool {
    let down = !matches!(
        position,
        OverlayPosition::BottomLeft | OverlayPosition::BottomCenter | OverlayPosition::BottomRight
    );
    let gap = super::layout::stack_gap_px(scale, dpi);
    let mut previous_edge: Option<i64> = None;
    for entry in entries {
        let anchor = super::layout::position_for(work, entry.size, position, dpi);
        let y = match previous_edge {
            None => i64::from(anchor.y),
            Some(edge) if down => edge + i64::from(gap),
            Some(edge) => edge - i64::from(gap) - i64::from(entry.size.cy),
        };
        if y < i64::from(work.top)
            || y.saturating_add(i64::from(entry.size.cy)) > i64::from(work.bottom)
        {
            return false;
        }
        previous_edge = Some(if down {
            y + i64::from(entry.size.cy)
        } else {
            y
        });
    }
    true
}

fn group_layout_entries(entries: Vec<LayoutEntry>) -> Vec<LayoutGroup> {
    let mut groups = Vec::new();
    for mut entry in entries {
        let Some(group) = groups
            .iter_mut()
            .find(|group: &&mut LayoutGroup| group.placement.key == entry.placement.key)
        else {
            groups.push(LayoutGroup {
                placement: entry.placement.clone(),
                entries: vec![entry],
            });
            continue;
        };
        let placement = group.placement.clone();
        entry.placement = placement.clone();
        if !entry.model.rows.is_empty() {
            entry.size = model_geometry(
                entry.render_config.scale,
                &entry.model,
                if entry.compact { 1.0 } else { 0.0 },
            )
            .pixel_size(placement.dpi);
        }
        group.entries.push(entry);
    }
    groups
}

fn layout_order(entry: &LayoutEntry) -> (u8, u8, u64, u64) {
    if entry.lifetime.is_permanent() {
        (0, entry.key.permanent_rank(), 0, entry.id)
    } else {
        (1, 0, u64::MAX - entry.sequence, entry.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::MonitorChoice;
    use crate::platform::monitor::MonitorGeometry;
    use crate::ui::overlay::{position_for, OverlayRow};
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{RECT, SIZE};

    fn work(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    fn monitor(name: &str, work: RECT) -> MonitorGeometry {
        MonitorGeometry {
            device_name: name.into(),
            work,
            dpi: 96,
        }
    }

    fn placement(name: &str, work: RECT, position: OverlayPosition) -> ResolvedPlacement {
        resolve_placement_for_monitor(Some(monitor(name, work)), position)
    }

    fn layout_entry(
        id: u64,
        key: OverlayKey,
        lifetime: OverlayLifetime,
        sequence: u64,
        scale: f32,
        placement: ResolvedPlacement,
        size: SIZE,
    ) -> LayoutEntry {
        let mut config = crate::config::Config::default().overlay;
        config.scale = scale;
        LayoutEntry {
            id,
            generation: id,
            key,
            model: OverlayModel::default(),
            lifetime,
            expires_at: None,
            presented_at: Instant::now(),
            sequence,
            render_config: config,
            placement,
            size,
            compact: false,
        }
    }

    fn planned(plans: &[PlannedEntry], id: u64) -> &PlannedEntry {
        plans
            .iter()
            .find(|plan| plan.entry.id == id)
            .unwrap_or_else(|| panic!("missing planned entry {id}"))
    }

    fn preview_registry_entry(registry: &mut OverlayRegistry, now: Instant) -> OverlayEntry {
        registry.present(
            OverlayRequest::toast(
                OverlayKey::Preview,
                OverlayModel::preview(OverlayRow::preview("preview", "detail")),
            ),
            Duration::from_millis(1300),
            now,
        );
        registry.entries()[0].clone()
    }

    fn present_at(
        registry: &mut OverlayRegistry,
        request: OverlayRequest,
        placement: ResolvedPlacement,
        now: Instant,
    ) -> PresentOutcome {
        registry.present_with_placement(
            request,
            Duration::from_millis(1300),
            now,
            MotionPolicy::Reduced,
            placement,
        )
    }

    fn registry_entry(registry: &OverlayRegistry, id: u64) -> &OverlayEntry {
        registry
            .entries()
            .iter()
            .find(|entry| entry.id() == id)
            .unwrap_or_else(|| panic!("missing registry entry {id}"))
    }

    #[test]
    fn presentation_snapshot_restores_same_key_model_generation_deadline_and_config() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let mut render_configs = HashMap::new();
        let placement = ResolvedPlacement::default();

        let original = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Speaker,
                OverlayModel::single(OverlayRow::preview("original", "before failure")),
            ),
            placement.clone(),
            now,
        );
        let mut original_config = crate::config::Config::default().overlay;
        original_config.duration_ms = 1300;
        render_configs.insert(original.id, original_config.clone());

        let original_entry = registry_entry(&registry, original.id).clone();
        let snapshot = PresentationStateSnapshot::capture(&registry, &render_configs);

        let replacement = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Speaker,
                OverlayModel::single(OverlayRow::preview("replacement", "failed render")),
            ),
            placement,
            now + Duration::from_millis(100),
        );
        assert_eq!(replacement.id, original.id);
        assert_ne!(
            registry_entry(&registry, original.id).generation(),
            original_entry.generation()
        );
        let mut replacement_config = original_config.clone();
        replacement_config.duration_ms = 5000;
        render_configs.insert(original.id, replacement_config);

        snapshot.restore(&mut registry, &mut render_configs);

        let restored = registry_entry(&registry, original.id);
        assert_eq!(restored.generation(), original_entry.generation());
        assert_eq!(restored.expires_at(), original_entry.expires_at());
        assert_eq!(restored.presented_at(), original_entry.presented_at());
        assert_eq!(restored.model().rows[0].title, "original");
        assert_eq!(
            render_configs
                .get(&original.id)
                .map(|config| config.duration_ms),
            Some(1300)
        );
    }

    #[test]
    fn presentation_snapshot_removes_failed_new_entry_and_config() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let mut render_configs = HashMap::new();
        let snapshot = PresentationStateSnapshot::capture(&registry, &render_configs);

        let inserted = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Workspace,
                OverlayModel::single(OverlayRow::preview("workspace", "failed render")),
            ),
            ResolvedPlacement::default(),
            now,
        );
        render_configs.insert(inserted.id, crate::config::Config::default().overlay);
        assert_eq!(registry.entries().len(), 1);
        assert_eq!(render_configs.len(), 1);

        snapshot.restore(&mut registry, &mut render_configs);

        assert!(registry.entries().is_empty());
        assert!(render_configs.is_empty());
    }

    #[test]
    fn permanent_and_preview_at_different_positions_use_different_groups() {
        let display = work(0, 0, 1000, 800);
        let permanent_placement = placement("DISPLAY1", display, OverlayPosition::TopLeft);
        let preview_placement = placement("DISPLAY1", display, OverlayPosition::BottomRight);
        assert_ne!(
            permanent_placement.key, preview_placement.key,
            "position is part of the placement group key"
        );

        let plans = plan_layout(vec![
            layout_entry(
                1,
                OverlayKey::MicrophonePermanent,
                OverlayLifetime::Permanent,
                1,
                1.0,
                permanent_placement,
                SIZE { cx: 120, cy: 80 },
            ),
            layout_entry(
                2,
                OverlayKey::Preview,
                OverlayLifetime::Toast,
                2,
                1.0,
                preview_placement,
                SIZE { cx: 120, cy: 80 },
            ),
        ]);

        assert_eq!(
            planned(&plans, 1).card.position,
            position_for(
                display,
                SIZE { cx: 120, cy: 80 },
                OverlayPosition::TopLeft,
                96
            )
        );
        assert_eq!(
            planned(&plans, 2).card.position,
            position_for(
                display,
                SIZE { cx: 120, cy: 80 },
                OverlayPosition::BottomRight,
                96
            )
        );
        assert_ne!(
            planned(&plans, 1).card.position,
            planned(&plans, 2).card.position
        );
    }

    #[test]
    fn preview_uses_its_own_draft_monitor() {
        let runtime_work = work(0, 0, 1000, 800);
        let draft_work = work(1000, 0, 1800, 900);
        let plans = plan_layout(vec![
            layout_entry(
                1,
                OverlayKey::MicrophonePermanent,
                OverlayLifetime::Permanent,
                1,
                1.0,
                placement("DISPLAY1", runtime_work, OverlayPosition::TopLeft),
                SIZE { cx: 120, cy: 80 },
            ),
            layout_entry(
                2,
                OverlayKey::Preview,
                OverlayLifetime::Toast,
                2,
                1.0,
                placement("DISPLAY2", draft_work, OverlayPosition::TopLeft),
                SIZE { cx: 120, cy: 80 },
            ),
        ]);

        assert_eq!(
            planned(&plans, 1).card.position,
            position_for(
                runtime_work,
                SIZE { cx: 120, cy: 80 },
                OverlayPosition::TopLeft,
                96
            )
        );
        assert_eq!(
            planned(&plans, 2).card.position,
            position_for(
                draft_work,
                SIZE { cx: 120, cy: 80 },
                OverlayPosition::TopLeft,
                96
            )
        );
    }

    #[test]
    fn preview_uses_its_own_draft_position() {
        let display = work(0, 0, 1000, 800);
        let plans = plan_layout(vec![
            layout_entry(
                1,
                OverlayKey::MicrophonePermanent,
                OverlayLifetime::Permanent,
                1,
                1.0,
                placement("DISPLAY1", display, OverlayPosition::TopLeft),
                SIZE { cx: 120, cy: 80 },
            ),
            layout_entry(
                2,
                OverlayKey::Preview,
                OverlayLifetime::Toast,
                2,
                1.0,
                placement("DISPLAY1", display, OverlayPosition::BottomRight),
                SIZE { cx: 120, cy: 80 },
            ),
        ]);

        assert_eq!(
            planned(&plans, 2).card.position,
            position_for(
                display,
                SIZE { cx: 120, cy: 80 },
                OverlayPosition::BottomRight,
                96
            )
        );
        assert_ne!(
            planned(&plans, 1).card.position,
            planned(&plans, 2).card.position
        );
    }

    #[test]
    fn preview_uses_its_own_draft_scale_for_surface_size_and_anchor() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let preview = preview_registry_entry(&mut registry, now);
        let display = work(0, 0, 1000, 800);
        let mut draft = crate::config::Config::default().overlay;
        draft.scale = 1.5;
        draft.position = OverlayPosition::BottomRight;
        let preview_layout = make_layout_entry(
            &preview,
            draft,
            placement("DISPLAY1", display, OverlayPosition::BottomRight),
        );
        let expected_size = surface_geometry(1.5, 1).pixel_size(96);
        assert_eq!(preview_layout.size, expected_size);

        let plans = plan_layout(vec![preview_layout]);
        assert_eq!(
            planned(&plans, preview.id()).card.position,
            position_for(display, expected_size, OverlayPosition::BottomRight, 96)
        );
    }

    #[test]
    fn same_anchor_preview_stacks_after_permanent_without_overlap() {
        let display = work(0, 0, 1000, 800);
        let placement = placement("DISPLAY1", display, OverlayPosition::TopLeft);
        let plans = plan_layout(vec![
            layout_entry(
                1,
                OverlayKey::MicrophonePermanent,
                OverlayLifetime::Permanent,
                1,
                1.0,
                placement.clone(),
                SIZE { cx: 120, cy: 80 },
            ),
            layout_entry(
                2,
                OverlayKey::Preview,
                OverlayLifetime::Toast,
                2,
                1.0,
                placement,
                SIZE { cx: 120, cy: 80 },
            ),
        ]);
        let permanent = planned(&plans, 1);
        let preview = planned(&plans, 2);

        assert_eq!(
            permanent.card.position,
            position_for(display, permanent.entry.size, OverlayPosition::TopLeft, 96)
        );
        assert_eq!(
            preview.card.position.y,
            permanent.card.position.y + permanent.entry.size.cy + 10
        );
        assert!(preview.card.position.y >= permanent.card.position.y + permanent.entry.size.cy);
    }

    #[test]
    fn placement_groups_compute_gaps_independently() {
        let first_display = work(0, 0, 1000, 800);
        let second_display = work(1000, 0, 2000, 800);
        let first_placement = placement("DISPLAY1", first_display, OverlayPosition::TopLeft);
        let second_placement = placement("DISPLAY2", second_display, OverlayPosition::TopLeft);
        let first_group = vec![
            layout_entry(
                1,
                OverlayKey::MicrophonePermanent,
                OverlayLifetime::Permanent,
                1,
                1.0,
                first_placement.clone(),
                SIZE { cx: 120, cy: 80 },
            ),
            layout_entry(
                2,
                OverlayKey::Workspace,
                OverlayLifetime::Toast,
                2,
                1.0,
                first_placement,
                SIZE { cx: 120, cy: 80 },
            ),
        ];
        let all_plans = plan_layout(vec![
            first_group[0].clone(),
            first_group[1].clone(),
            layout_entry(
                3,
                OverlayKey::Status,
                OverlayLifetime::Permanent,
                3,
                1.6,
                second_placement.clone(),
                SIZE { cx: 120, cy: 80 },
            ),
            layout_entry(
                4,
                OverlayKey::DisplayProfile,
                OverlayLifetime::Toast,
                4,
                1.6,
                second_placement,
                SIZE { cx: 120, cy: 80 },
            ),
        ]);
        let first_only = plan_layout(first_group);

        let first_gap = planned(&all_plans, 2).card.position.y
            - planned(&all_plans, 1).card.position.y
            - planned(&all_plans, 1).entry.size.cy;
        let second_gap = planned(&all_plans, 4).card.position.y
            - planned(&all_plans, 3).card.position.y
            - planned(&all_plans, 3).entry.size.cy;
        assert_eq!(first_gap, 10);
        assert_eq!(second_gap, 16);
        assert_eq!(
            planned(&all_plans, 1).card.position,
            planned(&first_only, 1).card.position
        );
        assert_eq!(
            planned(&all_plans, 2).card.position,
            planned(&first_only, 2).card.position
        );
    }

    #[test]
    fn preview_expiry_does_not_relayout_unrelated_permanent_group() {
        let display = work(0, 0, 1000, 800);
        let permanent_placement = placement("DISPLAY1", display, OverlayPosition::TopLeft);
        let preview_placement = placement("DISPLAY1", display, OverlayPosition::BottomRight);
        let permanent = layout_entry(
            1,
            OverlayKey::MicrophonePermanent,
            OverlayLifetime::Permanent,
            1,
            1.0,
            permanent_placement,
            SIZE { cx: 120, cy: 80 },
        );
        let preview = layout_entry(
            2,
            OverlayKey::Preview,
            OverlayLifetime::Toast,
            2,
            1.0,
            preview_placement,
            SIZE { cx: 120, cy: 80 },
        );
        let before_expiry = plan_layout(vec![permanent.clone(), preview]);
        let after_expiry = plan_layout(vec![permanent]);

        assert_eq!(
            planned(&before_expiry, 1).card.position,
            planned(&after_expiry, 1).card.position
        );
    }

    #[test]
    fn applying_runtime_config_moves_permanent_and_preserves_preview_snapshot() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let old_runtime_placement =
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft);
        let draft_placement = placement(
            "DISPLAY3",
            work(2000, 0, 3000, 800),
            OverlayPosition::BottomRight,
        );
        let permanent = present_at(
            &mut registry,
            OverlayRequest::permanent(
                OverlayKey::MicrophonePermanent,
                OverlayModel::single(OverlayRow::preview("muted", "detail")),
            ),
            old_runtime_placement.clone(),
            now,
        );
        let preview = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Preview,
                OverlayModel::preview(OverlayRow::preview("preview", "detail")),
            ),
            draft_placement.clone(),
            now,
        );
        let mut old_runtime = crate::config::Config::default().overlay;
        old_runtime.monitor = MonitorChoice::Device("DISPLAY1".into());
        old_runtime.position = OverlayPosition::TopLeft;
        let mut draft = crate::config::Config::default().overlay;
        draft.monitor = MonitorChoice::Device("DISPLAY3".into());
        draft.position = OverlayPosition::BottomRight;
        draft.scale = 1.4;
        let mut new_runtime = crate::config::Config::default().overlay;
        new_runtime.monitor = MonitorChoice::Device("DISPLAY2".into());
        new_runtime.position = OverlayPosition::CenterRight;
        new_runtime.scale = 1.2;
        let mut render_configs = HashMap::from([
            (permanent.id, old_runtime.clone()),
            (preview.id, draft.clone()),
        ]);
        let new_runtime_placement = placement(
            "DISPLAY2",
            work(1000, 0, 2000, 800),
            OverlayPosition::CenterRight,
        );
        let preview_snapshot = registry.entries()[1].placement.clone();

        adopt_runtime_configs(
            &mut registry,
            &mut render_configs,
            &new_runtime,
            new_runtime_placement,
        );

        assert_eq!(render_configs.get(&permanent.id), Some(&new_runtime));
        assert_eq!(render_configs.get(&preview.id), Some(&draft));
        assert_eq!(
            registry.entries()[0].placement.key.monitor,
            Some("DISPLAY2".into())
        );
        assert_eq!(
            registry.entries()[0].placement.key.position,
            OverlayPosition::CenterRight
        );
        assert_eq!(registry.entries()[1].placement.key, preview_snapshot.key);

        let old_layout =
            make_layout_entry(&registry.entries()[0], old_runtime, old_runtime_placement);
        let new_layout = make_layout_entry(
            &registry.entries()[0],
            render_configs[&permanent.id].clone(),
            registry.entries()[0].placement.clone(),
        );
        let preview_layout = make_layout_entry(
            &registry.entries()[1],
            render_configs[&preview.id].clone(),
            registry.entries()[1].placement.clone(),
        );
        let old_plan = plan_layout(vec![old_layout]);
        let new_plan = plan_layout(vec![new_layout]);
        let expected_preview_position = position_for(
            preview_layout.placement.work,
            preview_layout.size,
            preview_layout.placement.key.position,
            preview_layout.placement.dpi,
        );
        let preview_plan = plan_layout(vec![preview_layout]);

        assert_ne!(
            planned(&old_plan, permanent.id).card.position,
            planned(&new_plan, permanent.id).card.position
        );
        assert_eq!(
            planned(&preview_plan, preview.id).card.position,
            expected_preview_position
        );
    }

    #[test]
    fn cursor_monitor_snapshot_survives_unrelated_toast_presentation() {
        let now = Instant::now();
        let display1 = work(0, 0, 1000, 800);
        let display2 = work(1000, 0, 2000, 800);
        let mut registry = OverlayRegistry::default();
        let microphone = present_at(
            &mut registry,
            OverlayRequest::permanent(
                OverlayKey::MicrophonePermanent,
                OverlayModel::single(OverlayRow::preview("muted", "detail")),
            ),
            placement("DISPLAY1", display1, OverlayPosition::TopLeft),
            now,
        );
        let before = registry_entry(&registry, microphone.id).placement.clone();

        present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Speaker,
                OverlayModel::single(OverlayRow::preview("speaker", "detail")),
            ),
            placement("DISPLAY2", display2, OverlayPosition::TopLeft),
            now + Duration::from_millis(100),
        );

        let after = registry_entry(&registry, microphone.id).placement.clone();
        assert_eq!(after.key, before.key);
        assert_eq!(after.work, before.work);
        assert_eq!(after.dpi, before.dpi);
    }

    #[test]
    fn toast_expiry_does_not_move_a_permanent_cursor_snapshot() {
        let now = Instant::now();
        let display1 = work(0, 0, 1000, 800);
        let mut registry = OverlayRegistry::default();
        let microphone = present_at(
            &mut registry,
            OverlayRequest::permanent(
                OverlayKey::MicrophonePermanent,
                OverlayModel::single(OverlayRow::preview("muted", "detail")),
            ),
            placement("DISPLAY1", display1, OverlayPosition::TopLeft),
            now,
        );
        let before = registry_entry(&registry, microphone.id).placement.clone();
        let speaker = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Speaker,
                OverlayModel::single(OverlayRow::preview("speaker", "detail")),
            ),
            placement("DISPLAY1", display1, OverlayPosition::TopLeft),
            now,
        );

        let cursor_now = placement(
            "DISPLAY2",
            work(1000, 0, 2000, 800),
            OverlayPosition::TopLeft,
        );
        assert_eq!(cursor_now.key.monitor, Some("DISPLAY2".into()));
        let expired = registry.remove_expired(now + Duration::from_millis(1300));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id(), speaker.id);
        let after = registry_entry(&registry, microphone.id).placement.clone();
        assert_eq!(after.key, before.key);
        assert_eq!(after.work, before.work);
    }

    #[test]
    fn different_toasts_keep_their_own_cursor_monitor_snapshots() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let first = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Status,
                OverlayModel::single(OverlayRow::preview("first", "detail")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            now,
        );
        let second = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Workspace,
                OverlayModel::single(OverlayRow::preview("second", "detail")),
            ),
            placement(
                "DISPLAY2",
                work(1000, 0, 2000, 800),
                OverlayPosition::TopLeft,
            ),
            now + Duration::from_millis(100),
        );

        assert_ne!(first.id, second.id);
        assert_eq!(
            registry_entry(&registry, first.id).placement.key.monitor,
            Some("DISPLAY1".into())
        );
        assert_eq!(
            registry_entry(&registry, second.id).placement.key.monitor,
            Some("DISPLAY2".into())
        );
    }

    #[test]
    fn replace_same_key_toast_moves_only_the_replaced_toast_snapshot() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let microphone = present_at(
            &mut registry,
            OverlayRequest::permanent(
                OverlayKey::MicrophonePermanent,
                OverlayModel::single(OverlayRow::preview("muted", "detail")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            now,
        );
        let first_speaker = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Speaker,
                OverlayModel::single(OverlayRow::preview("old", "detail")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            now,
        );
        let replacement = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Speaker,
                OverlayModel::single(OverlayRow::preview("new", "detail")),
            ),
            placement(
                "DISPLAY2",
                work(1000, 0, 2000, 800),
                OverlayPosition::BottomRight,
            ),
            now + Duration::from_millis(100),
        );

        assert!(!replacement.inserted);
        assert_eq!(replacement.id, first_speaker.id);
        assert_eq!(
            registry_entry(&registry, replacement.id)
                .placement
                .key
                .monitor,
            Some("DISPLAY2".into())
        );
        assert_eq!(
            registry_entry(&registry, replacement.id)
                .placement
                .key
                .position,
            OverlayPosition::BottomRight
        );
        assert_eq!(
            registry_entry(&registry, microphone.id)
                .placement
                .key
                .monitor,
            Some("DISPLAY1".into())
        );
    }

    #[test]
    fn permanent_replacement_preserves_its_existing_cursor_snapshot() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let first = present_at(
            &mut registry,
            OverlayRequest::permanent(
                OverlayKey::MicrophonePermanent,
                OverlayModel::single(OverlayRow::preview("muted", "first")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            now,
        );
        let updated = present_at(
            &mut registry,
            OverlayRequest::permanent(
                OverlayKey::MicrophonePermanent,
                OverlayModel::single(OverlayRow::preview("muted", "updated")),
            ),
            placement(
                "DISPLAY2",
                work(1000, 0, 2000, 800),
                OverlayPosition::BottomRight,
            ),
            now + Duration::from_millis(100),
        );

        assert_eq!(updated.id, first.id);
        let entry = registry_entry(&registry, first.id);
        assert_eq!(entry.model().rows[0].detail, "updated");
        assert_eq!(entry.placement.key.monitor, Some("DISPLAY1".into()));
        assert_eq!(entry.placement.key.position, OverlayPosition::TopLeft);
    }

    #[test]
    fn visual_refresh_updates_same_monitor_geometry_without_using_cursor_monitor() {
        let mut snapshot = placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft);
        let cursor_now = placement(
            "DISPLAY2",
            work(1000, 0, 2000, 800),
            OverlayPosition::BottomRight,
        );
        refresh_placement_snapshot_for_monitor(
            &mut snapshot,
            Some(monitor("DISPLAY1", work(0, 0, 1000, 760))),
        );

        assert_eq!(snapshot.key.monitor, Some("DISPLAY1".into()));
        assert_eq!(snapshot.key.position, OverlayPosition::TopLeft);
        assert_eq!(snapshot.work.bottom, 760);
        assert_ne!(snapshot.key, cursor_now.key);
    }

    #[test]
    fn visual_refresh_uses_a_deterministic_fallback_when_snapshot_monitor_disappears() {
        let mut snapshot = placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft);
        refresh_placement_snapshot_for_monitor(
            &mut snapshot,
            Some(monitor("DISPLAY2", work(1000, 0, 2000, 800))),
        );

        assert_eq!(snapshot.key.monitor, Some("DISPLAY2".into()));
        assert_eq!(snapshot.key.position, OverlayPosition::TopLeft);
        assert_eq!(snapshot.work.left, 1000);
    }

    #[test]
    fn preview_replacement_uses_latest_draft_placement_snapshot() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let first = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Preview,
                OverlayModel::preview(OverlayRow::preview("first", "detail")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            now,
        );
        let replacement = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Preview,
                OverlayModel::preview(OverlayRow::preview("second", "detail")),
            ),
            placement(
                "DISPLAY2",
                work(1000, 0, 2000, 800),
                OverlayPosition::BottomRight,
            ),
            now + Duration::from_millis(100),
        );

        assert_eq!(replacement.id, first.id);
        assert_eq!(
            registry_entry(&registry, first.id).placement.key.monitor,
            Some("DISPLAY2".into())
        );
        assert_eq!(
            registry_entry(&registry, first.id).placement.key.position,
            OverlayPosition::BottomRight
        );
    }

    #[test]
    fn primary_and_device_monitor_choices_resolve_to_the_same_primary_snapshot() {
        let Some(primary) = crate::platform::monitor::primary() else {
            return;
        };
        let mut primary_config = crate::config::Config::default().overlay;
        primary_config.monitor = MonitorChoice::Primary;
        let primary_snapshot = resolve_placement(&primary_config);
        assert_eq!(
            primary_snapshot.key.monitor,
            Some(primary.device_name.clone())
        );

        let mut device_config = crate::config::Config::default().overlay;
        device_config.monitor = MonitorChoice::Device(primary.device_name.clone());
        let device_snapshot = resolve_placement(&device_config);
        assert_eq!(device_snapshot.key.monitor, Some(primary.device_name));
    }

    #[test]
    fn preview_replacement_keeps_its_timer_independent() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let first = registry.present(
            OverlayRequest::toast(
                OverlayKey::Preview,
                OverlayModel::preview(OverlayRow::preview("first", "detail")),
            ),
            Duration::from_millis(1000),
            now,
        );
        let other = registry.present(
            OverlayRequest::toast(
                OverlayKey::Workspace,
                OverlayModel::single(OverlayRow::preview("workspace", "detail")),
            ),
            Duration::from_millis(2000),
            now,
        );
        let replaced = registry.present(
            OverlayRequest::toast(
                OverlayKey::Preview,
                OverlayModel::preview(OverlayRow::preview("second", "detail")),
            ),
            Duration::from_millis(3000),
            now + Duration::from_millis(500),
        );

        assert_eq!(replaced.id, first.id);
        assert_eq!(
            registry.entries()[0].expires_at(),
            Some(now + Duration::from_millis(3500))
        );
        assert_eq!(
            registry
                .entries()
                .iter()
                .find(|entry| entry.id() == other.id)
                .and_then(OverlayEntry::expires_at),
            Some(now + Duration::from_millis(2000))
        );
    }

    #[test]
    fn expired_entries_are_marked_expiring_before_unrelated_layout() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let expired = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Speaker,
                OverlayModel::single(OverlayRow::preview("old", "detail")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            now,
        );
        registry.mark_expiring(now + Duration::from_millis(1300));
        present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Workspace,
                OverlayModel::single(OverlayRow::preview("new", "detail")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            now + Duration::from_millis(1301),
        );

        let entry = registry_entry(&registry, expired.id);
        assert!(entry.is_expiring());
        let visible = registry
            .entries()
            .iter()
            .filter(|entry| !entry.is_expiring())
            .map(|entry| {
                make_layout_entry(
                    entry,
                    crate::config::Config::default().overlay,
                    entry.placement.clone(),
                )
            })
            .collect::<Vec<_>>();
        let plans = plan_layout(visible);
        assert!(plans.iter().all(|plan| plan.entry.id != expired.id));
    }

    #[test]
    fn layout_evicts_oldest_toasts_by_work_area_capacity() {
        let display = work(0, 0, 400, 370);
        let placement = placement("DISPLAY1", display, OverlayPosition::TopLeft);
        let entries = vec![
            layout_entry(
                1,
                OverlayKey::MicrophonePermanent,
                OverlayLifetime::Permanent,
                1,
                1.0,
                placement.clone(),
                SIZE { cx: 120, cy: 100 },
            ),
            layout_entry(
                2,
                OverlayKey::Workspace,
                OverlayLifetime::Toast,
                2,
                1.0,
                placement.clone(),
                SIZE { cx: 120, cy: 100 },
            ),
            layout_entry(
                3,
                OverlayKey::Speaker,
                OverlayLifetime::Toast,
                3,
                1.0,
                placement.clone(),
                SIZE { cx: 120, cy: 100 },
            ),
            layout_entry(
                4,
                OverlayKey::OutputDevice,
                OverlayLifetime::Toast,
                4,
                1.0,
                placement,
                SIZE { cx: 120, cy: 100 },
            ),
        ];

        let result = plan_layout_with_evictions(entries);
        assert_eq!(result.evicted_ids, vec![2]);
        assert!(result.entries.iter().any(|plan| plan.entry.id == 1));
        assert!(result.entries.iter().any(|plan| plan.entry.id == 4));
        assert!(result.entries.iter().any(|plan| plan.entry.id == 3));
        for plan in &result.entries {
            assert!(plan.card.position.x >= display.left);
            assert!(plan.card.position.y >= display.top);
            assert!(plan.card.position.x + plan.entry.size.cx <= display.right);
            assert!(plan.card.position.y + plan.entry.size.cy <= display.bottom);
        }
    }

    #[test]
    fn toast_order_is_newest_first_and_permanent_slot_is_stable() {
        let display = work(0, 0, 600, 600);
        let placement = placement("DISPLAY1", display, OverlayPosition::TopLeft);
        let mut registry = OverlayRegistry::default();
        let permanent = present_at(
            &mut registry,
            OverlayRequest::permanent(
                OverlayKey::MicrophonePermanent,
                OverlayModel::single(OverlayRow::preview("muted", "detail")),
            ),
            placement.clone(),
            Instant::now(),
        );
        let workspace = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Workspace,
                OverlayModel::single(OverlayRow::preview("workspace", "detail")),
            ),
            placement.clone(),
            Instant::now(),
        );
        let speaker = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Speaker,
                OverlayModel::single(OverlayRow::preview("speaker", "detail")),
            ),
            placement.clone(),
            Instant::now(),
        );
        let replacement = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Workspace,
                OverlayModel::single(OverlayRow::preview("workspace latest", "detail")),
            ),
            placement.clone(),
            Instant::now(),
        );
        let entries = registry
            .entries()
            .iter()
            .map(|entry| {
                make_layout_entry(
                    entry,
                    crate::config::Config::default().overlay,
                    entry.placement.clone(),
                )
            })
            .collect::<Vec<_>>();
        let plans = plan_layout(entries);
        let ordered_ids = plans.iter().map(|plan| plan.entry.id).collect::<Vec<_>>();
        assert_eq!(ordered_ids[0], permanent.id);
        assert_eq!(ordered_ids[1], replacement.id);
        assert_eq!(ordered_ids[2], speaker.id);
        assert_eq!(workspace.id, replacement.id);
    }

    #[test]
    fn monitor_diagnostics_use_a_deterministic_aggregate_summary() {
        let mut registry = OverlayRegistry::default();
        assert_eq!(active_monitor_summary(&registry), None);
        present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Status,
                OverlayModel::single(OverlayRow::preview("status", "detail")),
            ),
            placement(
                "DISPLAY2",
                work(1000, 0, 2000, 800),
                OverlayPosition::TopLeft,
            ),
            Instant::now(),
        );
        assert_eq!(active_monitor_summary(&registry), Some("DISPLAY2".into()));
        present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::Workspace,
                OverlayModel::single(OverlayRow::preview("workspace", "detail")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            Instant::now(),
        );
        assert_eq!(
            active_monitor_summary(&registry),
            Some("multiple (2)".into())
        );
    }

    #[test]
    fn placement_group_normalizes_member_geometry_to_one_snapshot() {
        let first = placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft);
        let mut second = first.clone();
        second.work = work(0, 0, 900, 700);
        second.dpi = 144;
        let plans = plan_layout(vec![
            layout_entry(
                1,
                OverlayKey::Status,
                OverlayLifetime::Toast,
                1,
                1.0,
                first,
                SIZE { cx: 120, cy: 80 },
            ),
            layout_entry(
                2,
                OverlayKey::Workspace,
                OverlayLifetime::Toast,
                2,
                1.0,
                second,
                SIZE { cx: 120, cy: 80 },
            ),
        ]);
        assert_eq!(
            planned(&plans, 1).entry.placement.work,
            planned(&plans, 2).entry.placement.work
        );
        assert_eq!(
            planned(&plans, 1).entry.placement.dpi,
            planned(&plans, 2).entry.placement.dpi
        );
    }

    #[test]
    fn overflow_never_evicts_permanent_entries() {
        let display = work(0, 0, 300, 180);
        let placement = placement("DISPLAY1", display, OverlayPosition::TopLeft);
        let result = plan_layout_with_evictions(vec![
            layout_entry(
                1,
                OverlayKey::MicrophonePermanent,
                OverlayLifetime::Permanent,
                1,
                1.0,
                placement.clone(),
                SIZE { cx: 120, cy: 140 },
            ),
            layout_entry(
                2,
                OverlayKey::Status,
                OverlayLifetime::Permanent,
                2,
                1.0,
                placement.clone(),
                SIZE { cx: 120, cy: 140 },
            ),
            layout_entry(
                3,
                OverlayKey::Workspace,
                OverlayLifetime::Toast,
                3,
                1.0,
                placement,
                SIZE { cx: 120, cy: 140 },
            ),
        ]);

        assert_eq!(result.evicted_ids, vec![3]);
        assert!(result.entries.iter().any(|plan| plan.entry.id == 1));
        assert!(result.entries.iter().any(|plan| plan.entry.id == 2));
    }

    #[test]
    fn stale_expiry_generation_cannot_remove_same_key_replacement() {
        let now = Instant::now();
        let mut registry = OverlayRegistry::default();
        let first = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::OutputDevice,
                OverlayModel::single(OverlayRow::preview("Device A", "detail")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            now,
        );
        let old_generation = registry_entry(&registry, first.id).generation();
        registry.mark_expiring(now + Duration::from_millis(1300));
        let replacement = present_at(
            &mut registry,
            OverlayRequest::toast(
                OverlayKey::OutputDevice,
                OverlayModel::single(OverlayRow::preview("Device B", "detail")),
            ),
            placement("DISPLAY1", work(0, 0, 1000, 800), OverlayPosition::TopLeft),
            now + Duration::from_millis(1301),
        );

        assert_eq!(replacement.id, first.id);
        assert_ne!(
            registry_entry(&registry, first.id).generation(),
            old_generation
        );
        assert!(registry
            .remove_expired_generation(first.id, old_generation, now + Duration::from_millis(2000))
            .is_none());
        assert_eq!(
            registry_entry(&registry, first.id).model().rows[0].title,
            "Device B"
        );
        assert!(!registry_entry(&registry, first.id).is_expiring());
    }
}
