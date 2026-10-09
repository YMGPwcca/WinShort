//! Config validation (spec §9, §40): range checks, hotkey conflicts.
//! Returns violations with field paths for inline UI display.

use std::collections::HashMap;

use crate::config::model::{
    normalize_overlay_duration, normalize_overlay_scale, Config, DeviceSelection,
    OVERLAY_DURATION_MAX_MS, OVERLAY_DURATION_MIN_MS,
};
use crate::keyboard::binding::{numbered_desktop_family, Hotkey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Dotted field path, e.g. `overlay.duration_ms` or `hotkeys.toggle_microphone`.
    pub field: String,
    pub message: String,
}

impl Violation {
    fn new(field: &str, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

pub fn validate(cfg: &Config) -> Vec<Violation> {
    let mut v = Vec::new();

    validate_overlay_values(cfg, &mut v);

    // Hotkey conflicts: same (modifiers, key) bound twice.
    let mut seen: HashMap<Hotkey, &'static str> = HashMap::new();
    for (name, hk) in [
        ("toggle_microphone", &cfg.hotkeys.toggle_microphone),
        ("toggle_output", &cfg.hotkeys.toggle_output),
        (
            "toggle_foreground_audio",
            &cfg.hotkeys.toggle_foreground_audio,
        ),
        ("cycle_input_device", &cfg.hotkeys.cycle_input_device),
        ("cycle_output_device", &cfg.hotkeys.cycle_output_device),
        ("foreground_volume_up", &cfg.hotkeys.foreground_volume_up),
        (
            "foreground_volume_down",
            &cfg.hotkeys.foreground_volume_down,
        ),
        ("previous_desktop", &cfg.virtual_desktops.previous_desktop),
        ("scratchpad_assign", &cfg.virtual_desktops.scratchpad_assign),
        ("scratchpad_toggle", &cfg.virtual_desktops.scratchpad_toggle),
    ] {
        if let Some(hk) = hk {
            if let Some(other) = seen.insert(*hk, name) {
                v.push(Violation::new(
                    &format!("hotkeys.{name}"),
                    format!("conflicts with `{other}` ({hk})"),
                ));
            }
        }
    }
    if cfg.hotkeys.display_profiles.len() > u8::MAX as usize + 1 {
        v.push(Violation::new(
            "hotkeys.display_profiles",
            "display profile hotkey count must be at most 256",
        ));
    }
    let mut profile_keys: HashMap<u16, usize> = HashMap::new();
    for (index, binding) in cfg.hotkeys.display_profiles.iter().enumerate() {
        let field = format!("hotkeys.display_profiles[{index}]");
        let profile_id = binding.profile_id.trim();
        if profile_id.is_empty() {
            v.push(Violation::new(
                &format!("{field}.profile_id"),
                "profile ID must not be empty",
            ));
        } else {
            if !cfg
                .display_profiles
                .profiles
                .iter()
                .any(|profile| profile.id.eq_ignore_ascii_case(profile_id))
            {
                v.push(Violation::new(
                    &format!("{field}.profile_id"),
                    format!("references unknown display profile `{profile_id}`"),
                ));
            }
            if let Some(previous) =
                profile_keys.insert(crate::display::profile_id_key(profile_id), index)
            {
                v.push(Violation::new(
                    &format!("{field}.profile_id"),
                    format!("collides with profile ID key at index {previous}"),
                ));
            }
        }
        if let Some(other) = seen.insert(binding.hotkey, "display_profiles") {
            v.push(Violation::new(
                &format!("{field}.hotkey"),
                format!("conflicts with `{other}` ({})", binding.hotkey),
            ));
        }
    }

    let mut disabled_actions: HashMap<String, usize> = HashMap::new();
    for (index, binding) in cfg.hotkeys.disabled.iter().enumerate() {
        let field = format!("hotkeys.disabled[{index}]");
        let action = binding.action.trim();
        if action.is_empty() {
            v.push(Violation::new(
                &format!("{field}.action"),
                "disabled shortcut action must not be empty",
            ));
            continue;
        }
        if cfg.canonical_disabled_action(action).is_none() {
            v.push(Violation::new(
                &format!("{field}.action"),
                format!("unknown disabled shortcut action `{action}`"),
            ));
            continue;
        }
        if let Some(previous) = disabled_actions.insert(action.to_ascii_lowercase(), index) {
            v.push(Violation::new(
                &format!("{field}.action"),
                format!("duplicates disabled shortcut action at index {previous}"),
            ));
        }
        if cfg.has_active_hotkey_action(action) {
            v.push(Violation::new(
                &format!("{field}.action"),
                "disabled shortcut must not also have an active binding",
            ));
        }
    }

    // Numbered families reserve every digit with their configured modifier.
    // Family collisions and explicit-hotkey collisions are rejected instead
    // of being silently shadowed by build_bindings.
    let number_modifier = cfg.virtual_desktops.number_modifier;
    if !number_modifier.is_valid() {
        v.push(Violation::new(
            "virtual_desktops.number_modifier",
            "contains unsupported modifier bits",
        ));
    }
    for (field, modifier) in [
        (
            "virtual_desktops.move_follow_modifier",
            cfg.virtual_desktops.move_follow_modifier,
        ),
        (
            "virtual_desktops.move_silent_modifier",
            cfg.virtual_desktops.move_silent_modifier,
        ),
    ] {
        if let Some(modifier) = modifier {
            if !modifier.is_valid() {
                v.push(Violation::new(field, "contains unsupported modifier bits"));
            } else if modifier.is_empty() {
                v.push(Violation::new(
                    field,
                    "optional modifier family must not be empty",
                ));
            }
        }
    }
    if cfg.virtual_desktops.enabled {
        let mut families: Vec<(&str, crate::keyboard::binding::ModifierMask)> = Vec::new();
        if cfg.virtual_desktops.win_number_switching
            && number_modifier.is_valid()
            && !number_modifier.is_empty()
        {
            families.push(("number_modifier", number_modifier));
        }
        if let Some(modifier) = cfg
            .virtual_desktops
            .move_follow_modifier
            .filter(|modifier| modifier.is_valid() && !modifier.is_empty())
        {
            families.push(("move_follow_modifier", modifier));
        }
        if let Some(modifier) = cfg
            .virtual_desktops
            .move_silent_modifier
            .filter(|modifier| modifier.is_valid() && !modifier.is_empty())
        {
            families.push(("move_silent_modifier", modifier));
        }
        if cfg.virtual_desktops.win_number_switching && number_modifier.is_empty() {
            v.push(Violation::new(
                "virtual_desktops.number_modifier",
                "numbered desktop switching requires at least one modifier",
            ));
        }
        let mut family_seen: HashMap<Hotkey, &str> = HashMap::new();
        for (family, modifier) in families {
            for hotkey in numbered_desktop_family(modifier) {
                if let Some(other) = seen.get(&hotkey) {
                    let message = if family == "number_modifier" {
                        format!("conflicts with reserved virtual-desktop shortcut {hotkey}")
                    } else {
                        format!("conflicts with virtual-desktop {family} shortcut {hotkey}")
                    };
                    v.push(Violation::new(&format!("hotkeys.{other}"), message));
                }
                if let Some(other) = family_seen.insert(hotkey, family) {
                    v.push(Violation::new(
                        &format!("virtual_desktops.{family}"),
                        format!("conflicts with virtual-desktop {other} family ({hotkey})"),
                    ));
                }
            }
        }
    }
    for (field, allowlist) in [
        (
            "audio.cycle_input_allowlist",
            &cfg.audio.cycle_input_allowlist,
        ),
        (
            "audio.cycle_output_allowlist",
            &cfg.audio.cycle_output_allowlist,
        ),
    ] {
        let Some(ids) = allowlist else {
            continue;
        };
        let mut seen = HashMap::new();
        for (index, id) in ids.iter().enumerate() {
            if id.trim().is_empty() {
                v.push(Violation::new(
                    &format!("{field}[{index}]"),
                    "endpoint ID must not be empty",
                ));
            } else if let Some(previous) = seen.insert(id, index) {
                v.push(Violation::new(
                    &format!("{field}[{index}]"),
                    format!("duplicates endpoint ID at index {previous}"),
                ));
            }
        }
    }

    let mut profile_ids: HashMap<String, usize> = HashMap::new();
    let mut profile_names: HashMap<String, usize> = HashMap::new();
    if cfg.display_profiles.profiles.len() > crate::display::MAX_PROFILES {
        v.push(Violation::new(
            "display_profiles.profiles",
            format!(
                "display profile count must be at most {}",
                crate::display::MAX_PROFILES
            ),
        ));
    }
    for (index, profile) in cfg.display_profiles.profiles.iter().enumerate() {
        let field = format!("display_profiles.profiles[{index}]");
        let id = profile.id.trim();
        if id.is_empty() {
            v.push(Violation::new(
                &format!("{field}.id"),
                "display profile ID must not be empty",
            ));
        } else if let Some(previous) = profile_ids.insert(id.to_ascii_lowercase(), index) {
            v.push(Violation::new(
                &format!("{field}.id"),
                format!("duplicates display profile at index {previous}"),
            ));
        }
        let name = profile.name.trim();
        if name.is_empty() {
            v.push(Violation::new(
                &format!("{field}.name"),
                "display profile name must not be empty",
            ));
        } else if let Some(previous) = profile_names.insert(name.to_ascii_lowercase(), index) {
            v.push(Violation::new(
                &format!("{field}.name"),
                format!("duplicates display profile name at index {previous}"),
            ));
        }
        if profile.routes.is_empty() {
            v.push(Violation::new(
                &format!("{field}.routes"),
                "display profile must contain at least one route",
            ));
        } else if profile.routes.len() > 32 {
            v.push(Violation::new(
                &format!("{field}.routes"),
                "display profile contains more than 32 routes",
            ));
        }
        let mut route_ids = HashMap::new();
        for (route_index, route) in profile.routes.iter().enumerate() {
            if route.target_path.trim().is_empty() {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].target_path"),
                    "display route target path must not be empty",
                ));
            }
            let route_id = format!(
                "{}|{}|{}|{}|{}",
                route.target_path.trim().to_ascii_lowercase(),
                route.source_adapter,
                route.source_id,
                route.target_adapter,
                route.target_id
            );
            if let Some(previous) = route_ids.insert(route_id, route_index) {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}]"),
                    format!("duplicates display route at index {previous}"),
                ));
            }
            if crate::display::route_has_any_mode(route)
                && !crate::display::route_has_complete_mode(route)
            {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].mode"),
                    "display output mode must be either unresolved or complete",
                ));
            } else if profile.confirmed && !crate::display::route_has_complete_mode(route) {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].mode"),
                    "confirmed display output must have a resolved mode",
                ));
            }
        }
        if profile.routes.len() > 1
            && profile
                .routes
                .iter()
                .all(crate::display::route_has_complete_mode)
        {
            let distinct_sources = profile
                .routes
                .iter()
                .map(|route| (route.source_adapter, route.source_id))
                .collect::<std::collections::HashSet<_>>()
                .len();
            let distinct_source_modes = profile
                .routes
                .iter()
                .map(|route| {
                    (
                        route.source_width,
                        route.source_height,
                        route.source_pixel_format,
                        route.source_position_x,
                        route.source_position_y,
                    )
                })
                .collect::<std::collections::HashSet<_>>()
                .len();
            match profile.topology {
                crate::display::DisplayTopology::Clone if distinct_source_modes != 1 => {
                    v.push(Violation::new(
                        &format!("{field}.topology"),
                        "duplicate display profile routes must share one source mode",
                    ));
                }
                crate::display::DisplayTopology::Extend
                    if distinct_sources != profile.routes.len() =>
                {
                    v.push(Violation::new(
                        &format!("{field}.topology"),
                        "extend display profile routes must use distinct sources",
                    ));
                }
                _ => {}
            }
        }
    }
    if let Some(active) = cfg.display_profiles.active_profile.as_deref() {
        if !profile_ids.contains_key(&active.to_ascii_lowercase()) {
            v.push(Violation::new(
                "display_profiles.active_profile",
                format!("references unknown display profile `{active}`"),
            ));
        }
    }

    for (field, dev) in [
        ("audio.input_device", &cfg.audio.input_device),
        ("audio.output_device", &cfg.audio.output_device),
    ] {
        if let DeviceSelection::Endpoint(s) = dev {
            if s.trim().is_empty() {
                v.push(Violation::new(field, format!("malformed device id `{s}`")));
            }
        }
    }

    v
}

fn validate_overlay_values(cfg: &Config, violations: &mut Vec<Violation>) {
    if cfg.overlay.duration_ms != normalize_overlay_duration(cfg.overlay.duration_ms) {
        violations.push(Violation::new(
            "overlay.duration_ms",
            format!(
                "must be {OVERLAY_DURATION_MIN_MS}–{OVERLAY_DURATION_MAX_MS} in {step}ms steps (got {})",
                cfg.overlay.duration_ms,
                step = crate::config::model::OVERLAY_DURATION_STEP_MS,
            ),
        ));
    }
    let hover = super::model::normalize_hover_opacity(cfg.overlay.hover_opacity);
    if !cfg.overlay.hover_opacity.is_finite() || cfg.overlay.hover_opacity != hover {
        violations.push(Violation::new(
            "overlay.hover_opacity",
            "must be 0.1..=1.0 in 0.1 steps",
        ));
    }
    let normalized_scale = normalize_overlay_scale(cfg.overlay.scale);
    if !cfg.overlay.scale.is_finite() || cfg.overlay.scale != normalized_scale {
        violations.push(Violation::new(
            "overlay.scale",
            "must be 0.7–1.6 in 0.1 steps",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::*;

    #[test]
    fn defaults_are_valid() {
        assert!(validate(&Config::default()).is_empty());
    }

    #[test]
    fn overlay_duration_accepts_supported_boundaries_and_default() {
        for duration_ms in [OVERLAY_DURATION_MIN_MS, 1300, OVERLAY_DURATION_MAX_MS] {
            let mut config = Config::default();
            config.overlay.duration_ms = duration_ms;
            assert!(
                validate(&config).is_empty(),
                "duration {duration_ms} should be valid"
            );
        }
    }

    #[test]
    fn overlay_duration_rejects_values_outside_supported_range() {
        for duration_ms in [
            OVERLAY_DURATION_MIN_MS - 1,
            OVERLAY_DURATION_MAX_MS + 1,
            1051,
        ] {
            let mut config = Config::default();
            config.overlay.duration_ms = duration_ms;
            let violations = validate(&config);
            assert_eq!(violations.len(), 1, "duration {duration_ms}");
            assert_eq!(violations[0].field, "overlay.duration_ms");
        }
    }

    #[test]
    fn overlay_values_reject_off_grid_and_non_finite_values() {
        let mut config = Config::default();
        config.overlay.duration_ms = 1049;
        config.overlay.scale = 0.73;
        let violations = validate(&config);
        assert!(violations
            .iter()
            .any(|violation| violation.field == "overlay.duration_ms"));
        assert!(violations
            .iter()
            .any(|violation| violation.field == "overlay.scale"));

        config.overlay.scale = f32::INFINITY;
        assert!(validate(&config)
            .iter()
            .any(|violation| violation.field == "overlay.scale"));
    }

    #[test]
    fn range_violations_reported() {
        let mut c = Config::default();
        c.overlay.duration_ms = 10;
        c.overlay.scale = 9.0;
        let v = validate(&c);
        assert_eq!(v.len(), 2);
        assert!(v.iter().any(|x| x.field == "overlay.duration_ms"));
    }

    #[test]
    fn hotkey_conflict_detected() {
        let mut c = Config::default();
        c.hotkeys.toggle_output = c.hotkeys.toggle_microphone;
        let v = validate(&c);
        assert!(v
            .iter()
            .any(|x| x.field.contains("toggle_output") && x.message.contains("conflicts")));
    }

    #[test]
    fn opaque_endpoint_id_is_accepted_and_round_trips() {
        // Regression for #7: real MMDevice ids look like
        // "{0.0.0.00000000}.{guid}" — 36-char GUID checks rejected them.
        let raw = r#"
[general]
start_hotkeys_enabled = true
start_with_windows = false

[hotkeys]

[overlay]

[audio]
output_device = '{0.0.0.00000000}.{12345678-1234-1234-1234-123456789abc}'
"#;
        let t: ConfigToml = toml::from_str(raw).unwrap();
        let (cfg, warnings) = Config::from_toml(&t);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        match &cfg.audio.output_device {
            DeviceSelection::Endpoint(id) => {
                assert!(id.starts_with("{0.0.0.00000000}."));
            }
            other => panic!("expected Endpoint, got {other:?}"),
        }
        assert!(
            !validate(&cfg).iter().any(|x| x.field.contains("device")),
            "opaque id must validate"
        );
    }

    #[test]
    fn win_digit_hotkeys_rejected_when_reserved() {
        // #12: with win_number_switching on, Win+5 collides with the
        // reserved desktop shortcut.
        let mut c = Config::default();
        c.hotkeys.toggle_output = Some(Hotkey {
            modifiers: crate::keyboard::binding::ModifierMask::WIN,
            key: crate::keyboard::binding::VirtualKey(b'5' as u16),
        });
        let v = validate(&c);
        assert!(
            v.iter().any(|x| x.field == "hotkeys.toggle_output"
                && x.message
                    .contains("reserved virtual-desktop shortcut Win+5")),
            "got {v:?}"
        );

        // Same hotkey is fine when the reserved block is off.
        c.virtual_desktops.win_number_switching = false;
        assert!(validate(&c).is_empty());
    }

    #[test]
    fn ctrl_alt_digit_is_not_reserved() {
        let mut c = Config::default();
        c.hotkeys.toggle_output = Some(Hotkey {
            modifiers: crate::keyboard::binding::ModifierMask::CTRL
                .union(crate::keyboard::binding::ModifierMask::ALT),
            key: crate::keyboard::binding::VirtualKey(b'5' as u16),
        });
        assert!(validate(&c).is_empty());
    }
    #[test]
    fn new_hotkeys_participate_in_conflict_and_reserved_validation() {
        let mut c = Config::default();
        c.hotkeys.cycle_input_device = c.hotkeys.toggle_microphone;
        let conflicts = validate(&c);
        assert!(conflicts
            .iter()
            .any(|violation| violation.field == "hotkeys.cycle_input_device"));

        c.hotkeys.cycle_input_device = Some(Hotkey {
            modifiers: crate::keyboard::binding::ModifierMask::WIN,
            key: crate::keyboard::binding::VirtualKey(b'6' as u16),
        });
        c.hotkeys.toggle_microphone = None;
        let reserved = validate(&c);
        assert!(reserved.iter().any(|violation| {
            violation.field == "hotkeys.cycle_input_device"
                && violation
                    .message
                    .contains("reserved virtual-desktop shortcut Win+6")
        }));
    }

    #[test]
    fn configurable_number_family_reserves_custom_chord() {
        let mut config = Config::default();
        config.virtual_desktops.number_modifier = crate::keyboard::binding::ModifierMask::CTRL
            .union(crate::keyboard::binding::ModifierMask::WIN);
        config.hotkeys.toggle_output = Some(Hotkey {
            modifiers: config.virtual_desktops.number_modifier,
            key: crate::keyboard::binding::VirtualKey(b'5' as u16),
        });
        let violations = validate(&config);
        assert!(violations.iter().any(|violation| {
            violation.field == "hotkeys.toggle_output"
                && violation
                    .message
                    .contains("reserved virtual-desktop shortcut Ctrl+Win+5")
        }));
    }

    #[test]
    fn move_families_participate_in_conflict_validation() {
        let mut config = Config::default();
        config.virtual_desktops.move_follow_modifier =
            Some(crate::keyboard::binding::ModifierMask::ALT);
        config.virtual_desktops.move_silent_modifier =
            Some(crate::keyboard::binding::ModifierMask::ALT);
        let violations = validate(&config);
        assert!(violations
            .iter()
            .any(|violation| violation.field == "virtual_desktops.move_silent_modifier"));

        config.virtual_desktops.move_silent_modifier = None;
        config.hotkeys.toggle_output = Some(Hotkey {
            modifiers: crate::keyboard::binding::ModifierMask::ALT,
            key: crate::keyboard::binding::VirtualKey(b'5' as u16),
        });
        let violations = validate(&config);
        assert!(violations.iter().any(|violation| {
            violation.field == "hotkeys.toggle_output"
                && violation.message.contains("move_follow_modifier")
        }));
    }

    #[test]
    fn previous_desktop_hotkey_uses_shared_conflict_validation() {
        let mut config = Config::default();
        config.virtual_desktops.previous_desktop = config.hotkeys.toggle_microphone;
        let violations = validate(&config);
        assert!(violations
            .iter()
            .any(|violation| violation.field == "hotkeys.previous_desktop"));
    }
    #[test]
    fn empty_or_unknown_optional_modifier_families_are_rejected() {
        let mut config = Config::default();
        config.virtual_desktops.move_follow_modifier =
            Some(crate::keyboard::binding::ModifierMask::NONE);
        config.virtual_desktops.move_silent_modifier =
            Some(crate::keyboard::binding::ModifierMask::from_bits(0x80));

        let violations = validate(&config);
        assert!(violations.iter().any(|violation| {
            violation.field == "virtual_desktops.move_follow_modifier"
                && violation.message.contains("must not be empty")
        }));
        assert!(violations.iter().any(|violation| {
            violation.field == "virtual_desktops.move_silent_modifier"
                && violation.message.contains("unsupported modifier bits")
        }));
    }

    #[test]
    fn disabled_numbered_switching_does_not_reserve_custom_family() {
        let mut config = Config::default();
        config.virtual_desktops.win_number_switching = false;
        config.virtual_desktops.number_modifier = crate::keyboard::binding::ModifierMask::CTRL
            .union(crate::keyboard::binding::ModifierMask::ALT);
        config.hotkeys.toggle_output = Some(Hotkey {
            modifiers: config.virtual_desktops.number_modifier,
            key: crate::keyboard::binding::VirtualKey(b'5' as u16),
        });
        assert!(validate(&config).is_empty());
    }
    #[test]
    fn display_profiles_validate_and_repair_unknown_selection() {
        let mut config = Config::default();
        config.display_profiles.active_profile = Some("missing".into());
        config.display_profiles.profiles = vec![
            DisplayProfile {
                id: "Work".into(),
                name: "Work".into(),
                topology: DisplayTopology::Extend,
                confirmed: false,
                routes: vec![DisplayRoute {
                    target_path: "monitor-a".into(),
                    source_width: 1920,
                    source_height: 1080,
                    active_width: 1920,
                    active_height: 1080,
                    refresh_numerator: 60,
                    refresh_denominator: 1,
                    rotation: 1,
                    ..Default::default()
                }],
            },
            DisplayProfile {
                id: "work".into(),
                name: "Duplicate".into(),
                topology: DisplayTopology::Extend,
                confirmed: false,
                routes: vec![DisplayRoute {
                    target_path: "monitor-b".into(),
                    source_width: 1920,
                    source_height: 1080,
                    active_width: 1920,
                    active_height: 1080,
                    refresh_numerator: 60,
                    refresh_denominator: 1,
                    rotation: 1,
                    ..Default::default()
                }],
            },
        ];
        let violations = validate(&config);
        assert!(violations
            .iter()
            .any(|violation| violation.field == "display_profiles.active_profile"));
        assert!(violations
            .iter()
            .any(|violation| violation.message.contains("duplicates display profile")));

        config.repair(&violations);

        assert_eq!(config.display_profiles.profiles.len(), 1);
        assert!(config.display_profiles.active_profile.is_none());
        assert!(validate(&config).is_empty());
    }

    #[test]
    fn audio_allowlists_validate_duplicates_and_repair() {
        let mut config = Config::default();
        config.audio.cycle_input_allowlist =
            Some(vec!["capture-a".into(), "capture-a".into(), " ".into()]);
        config.audio.cycle_output_allowlist = Some(Vec::new());

        let violations = validate(&config);
        assert!(violations.iter().any(|violation| {
            violation
                .message
                .contains("duplicates endpoint ID at index 0")
        }));
        assert!(violations
            .iter()
            .any(|violation| violation.message.contains("endpoint ID must not be empty")));

        config.repair(&violations);

        assert_eq!(
            config.audio.cycle_input_allowlist,
            Some(vec!["capture-a".into()])
        );
        assert_eq!(config.audio.cycle_output_allowlist, Some(Vec::new()));
        assert!(validate(&config).is_empty());
    }
    #[test]
    fn profile_hotkeys_require_existing_ids_and_share_conflict_validator() {
        let mut config = Config::default();
        config.display_profiles.profiles = vec![DisplayProfile {
            id: "gaming-id".into(),
            name: "Gaming".into(),
            topology: DisplayTopology::Extend,
            confirmed: true,
            routes: vec![DisplayRoute {
                target_path: "monitor-a".into(),
                source_width: 1920,
                source_height: 1080,
                active_width: 1920,
                active_height: 1080,
                refresh_numerator: 60,
                refresh_denominator: 1,
                rotation: 1,
                ..Default::default()
            }],
        }];
        config.hotkeys.display_profiles = vec![
            DisplayProfileHotkey {
                profile_id: "gaming-id".into(),
                hotkey: config.hotkeys.toggle_microphone.unwrap(),
            },
            DisplayProfileHotkey {
                profile_id: "missing-id".into(),
                hotkey: Hotkey::parse("Ctrl+Alt+F12").unwrap(),
            },
        ];

        let violations = validate(&config);
        assert!(violations
            .iter()
            .any(|violation| violation.field == "hotkeys.display_profiles[0].hotkey"));
        assert!(violations.iter().any(|violation| {
            violation
                .message
                .contains("references unknown display profile")
        }));

        config.repair(&violations);

        assert!(config.hotkeys.display_profiles.is_empty());
        assert!(validate(&config).is_empty());
    }
    #[test]
    fn profile_hotkey_stable_id_key_collisions_are_repaired() {
        let mut config = Config::default();
        config.display_profiles.profiles = vec![DisplayProfile {
            id: "gaming-id".into(),
            name: "Gaming".into(),
            topology: DisplayTopology::Extend,
            confirmed: true,
            routes: vec![DisplayRoute {
                target_path: "monitor-a".into(),
                source_width: 1920,
                source_height: 1080,
                active_width: 1920,
                active_height: 1080,
                refresh_numerator: 60,
                refresh_denominator: 1,
                rotation: 1,
                ..Default::default()
            }],
        }];
        config.hotkeys.display_profiles = vec![
            DisplayProfileHotkey {
                profile_id: "gaming-id".into(),
                hotkey: Hotkey::parse("Ctrl+Alt+F12").unwrap(),
            },
            DisplayProfileHotkey {
                profile_id: "GAMING-ID".into(),
                hotkey: Hotkey::parse("Ctrl+Alt+F13").unwrap(),
            },
        ];

        let violations = validate(&config);
        assert!(violations.iter().any(|violation| {
            violation.field == "hotkeys.display_profiles[1].profile_id"
                && violation.message.contains("collides")
        }));

        config.repair(&violations);

        assert_eq!(config.hotkeys.display_profiles.len(), 1);
        assert!(validate(&config).is_empty());
    }
}
