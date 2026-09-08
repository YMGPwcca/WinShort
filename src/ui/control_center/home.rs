//! Home for the control center.

use super::state::SettingsUi;
use crate::config::validate::Violation;
use crate::ui::layout::Rect as UiRect;
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};

fn human_subsystem_name(name: &str) -> &str {
    match name {
        "audio" => "Audio",
        "desktop" => "Workspace",
        "overlay" => "Overlay",
        "keyboard" => "Shortcuts",
        "tray" => "Tray",
        "foreground" => "Current app audio",
        _ => "WinShort service",
    }
}

pub(super) fn friendly_violation(violation: &Violation) -> String {
    let message = violation.message.as_str();
    if violation.field == "Startup" {
        "WinShort couldn't update Windows startup. Try again.".into()
    } else if message.contains("configuration invalid") {
        "WinShort couldn't save this change. Check the setting and try again.".into()
    } else if message.contains("hotkey") || message.contains("modifier") {
        "That shortcut conflicts with another action. Choose a different key combination.".into()
    } else if message.contains("display") || message.contains("route") {
        "The selected display setup is not available. Check Displays and try again.".into()
    } else {
        message.to_string()
    }
}

impl SettingsUi {
    pub(super) fn draw_degraded_summary(&self, renderer: &Renderer) {
        let Some((name, _)) = self.runtime.degraded.first() else {
            return;
        };
        let rect = UiRect::new(
            self.layout.content_column.x,
            self.layout.content_clip.bottom() - 74.0,
            self.layout.content_column.w,
            58.0,
        );
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::Card);
        renderer.stroke_rounded(rect.d2d(), 10.0, BrushRole::Warning, 1.0);
        renderer.text(
            &format!("{} unavailable", human_subsystem_name(name)),
            UiRect::new(rect.x + 16.0, rect.y + 8.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            BrushRole::Warning,
        );
        renderer.text(
            "Open Diagnostics for the full technical reason.",
            UiRect::new(rect.x + 16.0, rect.y + 34.0, rect.w - 32.0, 18.0).d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }

    pub(super) fn special_workspace_summary(&self) -> (String, String, String) {
        if !self.draft.virtual_desktops.enabled {
            return ("Off".into(), "Workspaces are off".into(), "Enable".into());
        }
        if matches!(
            &self.runtime.desktop.native,
            crate::desktop::BackendAvailability::Available
        ) {
            (
                "Available".into(),
                "A dedicated workspace for windows kept out of the way".into(),
                "Open".into(),
            )
        } else {
            (
                "Unavailable".into(),
                "Windows workspace service is unavailable".into(),
                "Open".into(),
            )
        }
    }

    pub(super) fn display_profile_needs_attention(&self) -> bool {
        !self.draft.display_profiles.enabled
            || self
                .draft
                .display_profiles
                .active()
                .is_none_or(|profile| !self.inventory.profile_readiness(profile).is_ready())
    }

    pub(super) fn home_diagnostics_copy(&self) -> (String, String, String) {
        if !self.draft.display_profiles.enabled {
            return (
                "Display profiles are off".into(),
                "Turn on Display profiles".into(),
                "Open Displays to save and switch screen arrangements.".into(),
            );
        }
        if self.display_profile_needs_attention() {
            let (_, readiness) = self.display_summary();
            let detail = if self.inventory.error().is_some() {
                "Windows display information is currently unavailable.".into()
            } else if !self.inventory.was_queried() {
                "Open Displays to check connected screens.".into()
            } else {
                "Open Displays to test or repair this profile.".into()
            };
            return ("Display profile needs attention".into(), readiness, detail);
        }
        if self.runtime.degraded.is_empty() {
            (
                "System status".into(),
                "WinShort services ready".into(),
                "Technical details stay in Diagnostics.".into(),
            )
        } else {
            (
                "System status".into(),
                "Some services need attention".into(),
                "Open Diagnostics for details and recovery.".into(),
            )
        }
    }

    pub(super) fn shortcut_health_copy(&self) -> (String, String, String) {
        let (active, conflicts) = self.shortcut_health();
        if !self.draft.general.start_hotkeys_enabled {
            (
                "Shortcuts paused".into(),
                format!("{active} configured"),
                "Open".into(),
            )
        } else if conflicts > 0 {
            (
                format!("{conflicts} need attention"),
                format!("{active} configured"),
                "Open".into(),
            )
        } else {
            (
                format!("{active} active"),
                "No conflicts".into(),
                "Open".into(),
            )
        }
    }

    pub(super) fn shortcut_health(&self) -> (usize, usize) {
        let config = &self.draft;
        let bindings = [
            config.hotkeys.toggle_microphone,
            config.hotkeys.toggle_output,
            config.hotkeys.toggle_foreground_audio,
            config.hotkeys.cycle_input_device,
            config.hotkeys.cycle_output_device,
            config.hotkeys.foreground_volume_up,
            config.hotkeys.foreground_volume_down,
            config.virtual_desktops.previous_desktop,
            config.virtual_desktops.scratchpad_assign,
            config.virtual_desktops.scratchpad_toggle,
        ];
        let explicit = bindings.iter().filter(|binding| binding.is_some()).count()
            + config.hotkeys.display_profiles.len();
        let numbered = usize::from(
            config.virtual_desktops.enabled && config.virtual_desktops.win_number_switching,
        ) * 9;
        let move_follow = usize::from(
            config.virtual_desktops.enabled
                && config.virtual_desktops.move_follow_modifier.is_some(),
        ) * 9;
        let move_silent = usize::from(
            config.virtual_desktops.enabled
                && config.virtual_desktops.move_silent_modifier.is_some(),
        ) * 9;
        let active = explicit + numbered + move_follow + move_silent;
        let conflicts = crate::config::validate(config)
            .iter()
            .filter(|violation| violation.field.contains("hotkey"))
            .count();
        (active, conflicts)
    }
}
