//! Config-backed toggles share one transaction and one source of visible state.

use crate::config::model::Config;
use crate::ui::layout::ElementId;

#[derive(Clone, Copy)]
pub(super) enum ConfigToggle {
    PauseShortcuts,
    Workspaces,
    WorkspaceNumbers,
    DisplayProfiles,
    Overlay,
    ExternalAudio,
}

impl ConfigToggle {
    pub(super) fn from_element(id: ElementId) -> Option<Self> {
        match id {
            ElementId::StartHotkeysEnabled => Some(Self::PauseShortcuts),
            ElementId::DesktopsEnabled => Some(Self::Workspaces),
            ElementId::WinNumberEnabled => Some(Self::WorkspaceNumbers),
            ElementId::DisplayProfilesEnabled => Some(Self::DisplayProfiles),
            ElementId::OverlayEnabled => Some(Self::Overlay),
            ElementId::OverlayExternalChanges => Some(Self::ExternalAudio),
            _ => None,
        }
    }

    pub(super) fn selected(self, config: &Config) -> bool {
        match self {
            Self::PauseShortcuts => !config.general.start_hotkeys_enabled,
            Self::Workspaces => config.virtual_desktops.enabled,
            Self::WorkspaceNumbers => config.virtual_desktops.win_number_switching,
            Self::DisplayProfiles => config.display_profiles.enabled,
            Self::Overlay => config.overlay.enabled,
            Self::ExternalAudio => config.overlay.show_external_audio_changes,
        }
    }

    pub(super) fn toggle(self, config: &mut Config) {
        let value = match self {
            Self::PauseShortcuts => &mut config.general.start_hotkeys_enabled,
            Self::Workspaces => &mut config.virtual_desktops.enabled,
            Self::WorkspaceNumbers => &mut config.virtual_desktops.win_number_switching,
            Self::DisplayProfiles => &mut config.display_profiles.enabled,
            Self::Overlay => &mut config.overlay.enabled,
            Self::ExternalAudio => &mut config.overlay.show_external_audio_changes,
        };
        *value = !*value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_label_is_inverted_but_toggle_round_trips_each_setting() {
        let mut config = Config::default();
        assert_eq!(
            ConfigToggle::PauseShortcuts.selected(&config),
            !config.general.start_hotkeys_enabled
        );
        for toggle in [
            ConfigToggle::PauseShortcuts,
            ConfigToggle::Workspaces,
            ConfigToggle::WorkspaceNumbers,
            ConfigToggle::DisplayProfiles,
            ConfigToggle::Overlay,
            ConfigToggle::ExternalAudio,
        ] {
            let before = toggle.selected(&config);
            toggle.toggle(&mut config);
            assert_eq!(toggle.selected(&config), !before);
            toggle.toggle(&mut config);
            assert_eq!(toggle.selected(&config), before);
        }
        assert!(ConfigToggle::from_element(ElementId::Search).is_none());
    }
}
