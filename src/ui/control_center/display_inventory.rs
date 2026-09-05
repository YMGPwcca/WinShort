//! Display inventory state and presentation for the Control Center.

use super::display_routes::rotation_degrees;
use super::state::SettingsUi;
use crate::display::DisplayOutput;
use crate::ui::presentation::display_output_label;

#[derive(Debug, Default)]
pub(super) enum DisplayInventory {
    #[default]
    Unqueried,
    Available(Vec<DisplayOutput>),
    Failed(String),
}

impl DisplayInventory {
    pub(super) fn outputs(&self) -> &[DisplayOutput] {
        match self {
            Self::Available(outputs) => outputs,
            Self::Unqueried | Self::Failed(_) => &[],
        }
    }

    pub(super) fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message),
            Self::Unqueried | Self::Available(_) => None,
        }
    }

    pub(super) fn was_queried(&self) -> bool {
        !matches!(self, Self::Unqueried)
    }

    pub(super) fn profile_readiness(
        &self,
        profile: &crate::display::DisplayProfile,
    ) -> crate::ui::presentation::ProfileReadiness {
        use crate::ui::presentation::ProfileReadiness;

        let outputs = match self {
            Self::Unqueried => return ProfileReadiness::Unchecked,
            Self::Failed(_) => return ProfileReadiness::Unknown,
            Self::Available(outputs) => outputs,
        };
        let missing = profile
            .routes
            .iter()
            .filter(|route| {
                !outputs
                    .iter()
                    .any(|output| crate::display::same_output(&output.route, route))
            })
            .count();

        if let Some(count) = std::num::NonZeroUsize::new(missing) {
            ProfileReadiness::MissingScreens(count)
        } else if profile.routes.is_empty() {
            ProfileReadiness::Empty
        } else if !profile.confirmed {
            ProfileReadiness::NeedsTest
        } else {
            ProfileReadiness::Ready
        }
    }
}

impl SettingsUi {
    pub(super) fn selected_display_route(
        &self,
    ) -> Option<(
        &crate::display::DisplayProfile,
        &crate::display::DisplayRoute,
    )> {
        let profile = self.draft.display_profiles.active()?;
        profile
            .routes
            .get(self.selected_display_route)
            .map(|route| (profile, route))
    }

    pub(super) fn refresh_display_outputs(&mut self) {
        self.inventory = match crate::display::output_inventory() {
            Ok(outputs) => DisplayInventory::Available(outputs),
            Err(error) => {
                crate::warn_!("display output inventory unavailable: {error}");
                DisplayInventory::Failed(error.to_string())
            }
        };
    }

    pub(super) fn output_label(&self, route: &crate::display::DisplayRoute) -> String {
        if self.inventory.error().is_some() {
            return "Screen status unknown".into();
        }
        if !self.inventory.was_queried() {
            return "Screen status not checked".into();
        }
        self.inventory
            .outputs()
            .iter()
            .find(|output| crate::display::same_output(&output.route, route))
            .map(|output| {
                display_output_label(
                    &output.monitor_name,
                    &output.adapter_name,
                    &output.connector_name,
                    output.active,
                )
                .compact()
            })
            .unwrap_or_else(|| "Configured screen unavailable".into())
    }

    pub(super) fn display_route_candidates(
        &self,
    ) -> Vec<(String, String, crate::display::DisplayRoute, bool)> {
        let mut candidates = self
            .inventory
            .outputs()
            .iter()
            .map(|output| {
                let label = display_output_label(
                    &output.monitor_name,
                    &output.adapter_name,
                    &output.connector_name,
                    output.active,
                );
                (
                    label.primary,
                    label.detail.unwrap_or_else(|| "Connected screen".into()),
                    output.route.clone(),
                    true,
                )
            })
            .collect::<Vec<_>>();
        if let Some(profile) = self.draft.display_profiles.active() {
            for (index, route) in profile.routes.iter().enumerate() {
                if !self
                    .inventory
                    .outputs()
                    .iter()
                    .any(|output| crate::display::same_output(&output.route, route))
                {
                    candidates.push((
                        format!("Saved screen {}", index + 1),
                        if self.inventory.error().is_some() {
                            "Status unknown".into()
                        } else {
                            "Unavailable — reconnect this screen".into()
                        },
                        route.clone(),
                        false,
                    ));
                }
            }
        }
        candidates
    }

    pub(super) fn display_output_card_data(
        &self,
        index: usize,
    ) -> Option<crate::ui::presentation::DisplayOutputCard> {
        let (primary, detail, route, available) =
            self.display_route_candidates().into_iter().nth(index)?;
        let selected = self.draft.display_profiles.active().is_some_and(|profile| {
            profile
                .routes
                .iter()
                .any(|configured| crate::display::same_output(configured, &route))
        });
        Some(crate::ui::presentation::DisplayOutputCard {
            primary,
            detail,
            selected,
            available,
        })
    }

    pub(super) fn display_outputs_label(&self) -> String {
        let Some(profile) = self.draft.display_profiles.active() else {
            return "No profile selected".into();
        };
        match profile.routes.as_slice() {
            [] => "No screens selected".into(),
            [route] => self.profile_route_label(0, route),
            routes => format!("{} screens selected", routes.len()),
        }
    }

    pub(super) fn selected_display_route_label(&self) -> String {
        let Some((_, route)) = self.selected_display_route() else {
            return "No screen selected".into();
        };
        self.output_label(route)
    }

    pub(super) fn selected_display_route_edit_value(&self) -> Option<String> {
        let (_, route) = self.selected_display_route()?;
        let refresh = if route.refresh_denominator == 1 {
            route.refresh_numerator.to_string()
        } else {
            format!("{}/{}", route.refresh_numerator, route.refresh_denominator)
        };
        Some(format!(
            "{},{},{},{},{},{}",
            route.source_position_x,
            route.source_position_y,
            route.source_width,
            route.source_height,
            refresh,
            rotation_degrees(route.rotation)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{DisplayProfile, DisplayRoute, DisplayTopology};
    use crate::ui::presentation::ProfileReadiness;

    #[test]
    fn readiness_distinguishes_unknown_empty_missing_unconfirmed_and_ready() {
        let route = DisplayRoute {
            target_path: "screen-a".into(),
            ..Default::default()
        };
        let mut profile = DisplayProfile {
            id: "profile".into(),
            name: "Display".into(),
            topology: DisplayTopology::Extend,
            routes: vec![route.clone()],
            confirmed: false,
        };
        assert_eq!(
            DisplayInventory::Unqueried.profile_readiness(&profile),
            ProfileReadiness::Unchecked
        );
        assert_eq!(
            DisplayInventory::Failed("offline".into()).profile_readiness(&profile),
            ProfileReadiness::Unknown
        );
        assert!(matches!(
            DisplayInventory::Available(Vec::new()).profile_readiness(&profile),
            ProfileReadiness::MissingScreens(_)
        ));
        let inventory = DisplayInventory::Available(vec![DisplayOutput {
            route,
            monitor_name: "A".into(),
            adapter_name: "GPU".into(),
            connector_name: "DP".into(),
            active: true,
        }]);
        assert_eq!(
            inventory.profile_readiness(&profile),
            ProfileReadiness::NeedsTest
        );
        profile.confirmed = true;
        assert_eq!(
            inventory.profile_readiness(&profile),
            ProfileReadiness::Ready
        );
        profile.routes.clear();
        assert_eq!(
            inventory.profile_readiness(&profile),
            ProfileReadiness::Empty
        );
    }
}
