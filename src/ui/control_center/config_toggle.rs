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
    OverlayMicrophone,
    OverlaySpeaker,
    OverlayCurrentAppAudio,
    OverlayWorkspace,
    OverlayDisplayProfile,
}

impl ConfigToggle {
    pub(super) fn from_element(id: ElementId) -> Option<Self> {
        match id {
            ElementId::StartHotkeysEnabled => Some(Self::PauseShortcuts),
            ElementId::DesktopsEnabled => Some(Self::Workspaces),
            ElementId::WinNumberEnabled => Some(Self::WorkspaceNumbers),
            ElementId::DisplayProfilesEnabled => Some(Self::DisplayProfiles),
            ElementId::OverlayEnabled => Some(Self::Overlay),
            ElementId::OverlayMicrophone => Some(Self::OverlayMicrophone),
            ElementId::OverlaySpeaker => Some(Self::OverlaySpeaker),
            ElementId::OverlayCurrentAppAudio => Some(Self::OverlayCurrentAppAudio),
            ElementId::OverlayWorkspace => Some(Self::OverlayWorkspace),
            ElementId::OverlayDisplayProfile => Some(Self::OverlayDisplayProfile),
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
            Self::OverlayMicrophone => config.overlay.notifications.microphone,
            Self::OverlaySpeaker => config.overlay.notifications.speaker,
            Self::OverlayCurrentAppAudio => config.overlay.notifications.current_app_audio,
            Self::OverlayWorkspace => config.overlay.notifications.workspace,
            Self::OverlayDisplayProfile => config.overlay.notifications.display_profile,
        }
    }

    pub(super) fn toggle(self, config: &mut Config) {
        if matches!(self, Self::OverlayCurrentAppAudio) {
            let value = !config.overlay.notifications.current_app_audio;
            config.overlay.notifications.current_app_audio = value;
            // Explicit use of the v11 master switch adopts v11 semantics and
            // clears any narrower migrated external-change policy.
            config.overlay.notifications.external_current_app_audio = value;
            return;
        }
        let value = match self {
            Self::PauseShortcuts => &mut config.general.start_hotkeys_enabled,
            Self::Workspaces => &mut config.virtual_desktops.enabled,
            Self::WorkspaceNumbers => &mut config.virtual_desktops.win_number_switching,
            Self::DisplayProfiles => &mut config.display_profiles.enabled,
            Self::Overlay => &mut config.overlay.enabled,
            Self::OverlayMicrophone => &mut config.overlay.notifications.microphone,
            Self::OverlaySpeaker => &mut config.overlay.notifications.speaker,
            Self::OverlayWorkspace => &mut config.overlay.notifications.workspace,
            Self::OverlayDisplayProfile => &mut config.overlay.notifications.display_profile,
            Self::OverlayCurrentAppAudio => unreachable!(),
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
            ConfigToggle::OverlayMicrophone,
            ConfigToggle::OverlaySpeaker,
            ConfigToggle::OverlayCurrentAppAudio,
            ConfigToggle::OverlayWorkspace,
            ConfigToggle::OverlayDisplayProfile,
        ] {
            let before = toggle.selected(&config);
            toggle.toggle(&mut config);
            assert_eq!(toggle.selected(&config), !before);
            toggle.toggle(&mut config);
            assert_eq!(toggle.selected(&config), before);
        }
        assert!(ConfigToggle::from_element(ElementId::Search).is_none());
    }

    #[test]
    fn current_app_toggle_reconciles_legacy_external_policy() {
        let mut config = Config::default();
        config.overlay.notifications.current_app_audio = true;
        config.overlay.notifications.external_current_app_audio = false;

        ConfigToggle::OverlayCurrentAppAudio.toggle(&mut config);
        assert!(!config.overlay.notifications.current_app_audio);
        assert!(!config.overlay.notifications.external_current_app_audio);

        ConfigToggle::OverlayCurrentAppAudio.toggle(&mut config);
        assert!(config.overlay.notifications.current_app_audio);
        assert!(config.overlay.notifications.external_current_app_audio);
    }
}
