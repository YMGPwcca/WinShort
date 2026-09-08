//! Display routes for the control center.

use crate::display::{DisplayRoute, DisplayTopology};
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rotation {
    Identity,
    Quarter,
    Half,
    ThreeQuarters,
}

impl Rotation {
    fn native(self) -> i32 {
        match self {
            Self::Identity => 1,
            Self::Quarter => 2,
            Self::Half => 3,
            Self::ThreeQuarters => 4,
        }
    }
}

/// Validated editor input; invalid dimensions and rational rates cannot escape
/// the parsing boundary into the persisted display transport model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DisplayRouteEdit {
    x: i32,
    y: i32,
    width: NonZeroU32,
    height: NonZeroU32,
    refresh: NonZeroU32,
    refresh_denominator: NonZeroU32,
    rotation: Rotation,
}

impl DisplayRouteEdit {
    pub(super) fn apply_to(self, route: &mut DisplayRoute) {
        route.source_position_x = self.x;
        route.source_position_y = self.y;
        route.source_width = self.width.get();
        route.source_height = self.height.get();
        route.active_width = self.width.get();
        route.active_height = self.height.get();
        route.total_width = route.total_width.max(self.width.get());
        route.total_height = route.total_height.max(self.height.get());
        route.refresh_numerator = self.refresh.get();
        route.refresh_denominator = self.refresh_denominator.get();
        route.rotation = self.rotation.native();
    }
}

pub(super) fn topology_choice(index: u8) -> Option<DisplayTopology> {
    match index {
        0 => Some(DisplayTopology::Extend),
        1 => Some(DisplayTopology::Clone),
        _ => None,
    }
}

pub(super) fn parse_display_route_values(
    value: &str,
) -> std::result::Result<DisplayRouteEdit, &'static str> {
    let mut parts = value.split(',').map(str::trim);
    let x = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse()
        .map_err(|_| "route x must be an integer")?;
    let y = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse()
        .map_err(|_| "route y must be an integer")?;
    let width = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse::<u32>()
        .map_err(|_| "route width must be a positive integer")?;
    let height = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse::<u32>()
        .map_err(|_| "route height must be a positive integer")?;
    let refresh = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?;
    let (refresh, refresh_denominator) = parse_display_refresh(refresh)?;
    let rotation = match parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse::<i32>()
        .map_err(|_| "route rotation must be 0, 90, 180, or 270 degrees")?
    {
        0 => Rotation::Identity,
        90 => Rotation::Quarter,
        180 => Rotation::Half,
        270 => Rotation::ThreeQuarters,
        _ => return Err("route rotation must be 0, 90, 180, or 270 degrees"),
    };
    if parts.next().is_some() {
        return Err("route edit has too many comma-separated values");
    }
    let width = NonZeroU32::new(width).ok_or("route width and height must be positive")?;
    let height = NonZeroU32::new(height).ok_or("route width and height must be positive")?;
    Ok(DisplayRouteEdit {
        x,
        y,
        width,
        height,
        refresh,
        refresh_denominator,
        rotation,
    })
}

fn parse_display_refresh(
    value: &str,
) -> std::result::Result<(NonZeroU32, NonZeroU32), &'static str> {
    let mut values = value.split('/');
    let numerator = values
        .next()
        .ok_or("route refresh must be a positive integer or numerator/denominator")?
        .parse::<u32>()
        .map_err(|_| "route refresh must be a positive integer or numerator/denominator")?;
    let denominator = values.next().map_or(Ok(1), |value| {
        value
            .parse::<u32>()
            .map_err(|_| "route refresh denominator must be positive")
    })?;
    if values.next().is_some() {
        return Err("route refresh must be a positive integer or numerator/denominator");
    }
    let numerator = NonZeroU32::new(numerator).ok_or("route refresh must be positive")?;
    let denominator = NonZeroU32::new(denominator).ok_or("route refresh must be positive")?;
    Ok((numerator, denominator))
}

pub(super) fn rotation_degrees(value: i32) -> i32 {
    match value {
        2 => 90,
        3 => 180,
        4 => 270,
        _ => 0,
    }
}

pub(super) fn next_display_profile_identity(
    profiles: &[crate::display::DisplayProfile],
) -> Option<(String, String)> {
    // A profile can occupy at most two candidate numbers (its id and name).
    // Search a finite bound without an overflow-prone unbounded iterator.
    let limit = profiles.len().checked_mul(2)?.checked_add(1)?;
    for number in 1..=limit {
        let id = format!("profile-{number}");
        let name = format!("Profile {number}");
        if profiles.iter().all(|profile| {
            !profile.id.eq_ignore_ascii_case(&id) && !profile.name.eq_ignore_ascii_case(&name)
        }) {
            return Some((id, name));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_preserves_rational_rate_and_rotation_without_permitting_zero() {
        let edit =
            parse_display_route_values("-1920,0,1920,1080,60000/1001,90").expect("valid edit");
        assert_eq!(edit.refresh.get(), 60000);
        assert_eq!(edit.refresh_denominator.get(), 1001);
        assert_eq!(edit.rotation, Rotation::Quarter);
        assert_eq!(edit.x, -1920);
        for value in [
            "0,0,0,1080,60,0",
            "0,0,1920,1080,60/0,0",
            "0,0,1920,1080,60,45",
        ] {
            assert!(parse_display_route_values(value).is_err());
        }
        assert!(topology_choice(2).is_none());
    }
    #[test]
    fn identity_search_has_a_free_candidate_when_names_and_ids_differ() {
        let profile = crate::display::DisplayProfile {
            id: "profile-1".into(),
            name: "Profile 2".into(),
            topology: DisplayTopology::Extend,
            routes: Vec::new(),
            confirmed: false,
        };
        assert_eq!(
            next_display_profile_identity(&[profile]),
            Some(("profile-3".into(), "Profile 3".into()))
        );
    }
}
