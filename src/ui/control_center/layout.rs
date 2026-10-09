//! Layout for the control center.

use super::overlay_preview::overlay_preview_aspect;
use super::placement::client_size_dip;
use super::state::SettingsUi;
use crate::ui::layout::{ElementId, LayoutContext, SettingsLayout};
use crate::ui::renderer::Renderer;
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn layout_context(&self) -> LayoutContext {
        let rollback = self.display_rollback_status();
        LayoutContext {
            profile_count: self.draft.display_profiles.profiles.len(),
            selected_profile_index: self
                .draft
                .display_profiles
                .active_profile
                .as_ref()
                .and_then(|id| {
                    self.draft
                        .display_profiles
                        .profiles
                        .iter()
                        .position(|profile| profile.id.eq_ignore_ascii_case(id))
                }),
            display_output_count: self.display_route_candidates().len(),
            display_route_count: self
                .draft
                .display_profiles
                .active()
                .map_or(0, |profile| profile.routes.len()),
            display_editor_step: self.display.step(),
            display_profiles_enabled: self.draft.display_profiles.enabled,
            display_draft_dirty: self.display.is_dirty(),
            display_rollback_active: rollback.active(),
            display_keep_available: rollback.keep_available(),
            display_inventory_unknown: self.inventory.error().is_some()
                || !self.inventory.was_queried(),
            workspace_enabled: self.draft.virtual_desktops.enabled,
            paused: !self.draft.general.start_hotkeys_enabled,
            current_app_audio_available: !matches!(
                self.runtime.foreground.aggregate,
                crate::audio::Aggregate::NoExternalApp
            ),
            input_cycle_mode: self.audio_view().mode(crate::audio::DeviceCycleFlow::Input),
            output_cycle_mode: self
                .audio_view()
                .mode(crate::audio::DeviceCycleFlow::Output),
            input_device_count: self.devices.inputs.len(),
            output_device_count: self.devices.outputs.len(),
            overlay_preview_aspect: self.overlay_preview_aspect,
        }
    }

    pub(super) fn refresh_overlay_preview_aspect(&mut self) {
        self.overlay_preview_aspect = overlay_preview_aspect(&self.draft.overlay.monitor);
    }

    pub(super) fn rebuild_layout(&mut self, hwnd: HWND) {
        let size = self
            .renderer
            .as_ref()
            .map(Renderer::client_size_dip)
            .or_else(|| client_size_dip(hwnd, self.dpi));
        let (width, height) = size.unwrap_or_else(|| {
            crate::warn_!("settings client size query failed; using last valid layout size");
            (self.layout.width, self.layout.height)
        });
        self.layout = SettingsLayout::build_shell_with_context(
            width,
            height,
            self.scroll,
            self.page,
            &self.search_query,
            self.layout_context(),
            self.onboarding_step,
        );
        self.scroll = self.layout.scroll;
    }

    pub(super) fn visual_focus(&self, id: ElementId) -> bool {
        self.focus.indicator_visible() && self.focus.target() == Some(id)
    }
}
