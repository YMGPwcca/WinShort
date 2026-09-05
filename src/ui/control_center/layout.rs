//! Layout for the control center.

use super::overlay_preview::overlay_preview_aspect;
use super::placement::client_size_dip;
use super::state::SettingsUi;
use crate::ui::layout::{ElementId, LayoutContext, SettingsLayout};
use crate::ui::presentation::allowlist_mode;
use crate::ui::renderer::Renderer;
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn layout_context(&self) -> LayoutContext {
        LayoutContext {
            profile_count: self.draft.display_profiles.profiles.len(),
            display_output_count: self.display_route_candidates().len(),
            display_route_count: self
                .draft
                .display_profiles
                .active()
                .map_or(0, |profile| profile.routes.len()),
            display_editor_step: self.display_editor.map(|editor| editor.step),
            display_profiles_enabled: self.draft.display_profiles.enabled,
            display_draft_dirty: self.display_draft_dirty,
            display_rollback_active: self.display_rollback_active,
            display_keep_available: self.display_keep_available,
            display_inventory_unknown: self.inventory.error().is_some()
                || !self.inventory.was_queried(),
            workspace_enabled: self.draft.virtual_desktops.enabled,
            paused: !self.draft.general.start_hotkeys_enabled,
            current_app_audio_available: !matches!(
                self.runtime.foreground.aggregate,
                crate::audio::Aggregate::NoExternalApp
            ),
            input_cycle_mode: allowlist_mode(self.draft.audio.cycle_input_allowlist.as_deref()),
            output_cycle_mode: allowlist_mode(self.draft.audio.cycle_output_allowlist.as_deref()),
            input_device_count: self.devices.inputs.len(),
            output_device_count: self.devices.outputs.len(),
            overlay_preview_aspect: self.overlay_preview_aspect,
        }
    }

    pub(super) fn refresh_overlay_preview_aspect(&mut self) {
        self.overlay_preview_aspect = overlay_preview_aspect(&self.draft.overlay.monitor);
    }

    pub(super) fn rebuild_layout(&mut self, hwnd: HWND) {
        let (width, height) = self
            .renderer
            .as_ref()
            .map(Renderer::client_size_dip)
            .unwrap_or_else(|| client_size_dip(hwnd, self.dpi));
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
