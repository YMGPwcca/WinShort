//! Config validation (spec §9, §40): range checks, hotkey conflicts.
//! Returns violations with field paths for inline UI display.

use std::collections::HashMap;

use crate::config::model::{Config, DeviceSelection};
use crate::keyboard::binding::Hotkey;

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

    if !(500..=10_000).contains(&cfg.overlay.duration_ms) {
        v.push(Violation::new(
            "overlay.duration_ms",
            format!("must be 500–10000 (got {})", cfg.overlay.duration_ms),
        ));
    }
    if !(0.7..=1.6).contains(&cfg.overlay.scale) {
        v.push(Violation::new("overlay.scale", "must be 0.7–1.6"));
    }
    if !(0.3..=1.0).contains(&cfg.overlay.opacity) {
        v.push(Violation::new("overlay.opacity", "must be 0.3–1.0"));
    }

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

    // Reserved virtual-desktop shortcuts (#12): when Win-number switching is
    // on, a user hotkey equal to Win+1..9 would be overwritten by the table
    // build. Reject the collision instead of silently shadowing it.
    if cfg.virtual_desktops.enabled && cfg.virtual_desktops.win_number_switching {
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
        ] {
            if let Some(hk) = hk {
                if hk.modifiers == crate::keyboard::binding::ModifierMask::WIN {
                    let vk = hk.key.code();
                    if (0x31..=0x39).contains(&vk) {
                        v.push(Violation::new(
                            &format!("hotkeys.{name}"),
                            format!(
                                "conflicts with reserved virtual-desktop shortcut Win+{}",
                                vk - 0x30
                            ),
                        ));
                    }
                }
            }
        }
    }
    // Endpoint IDs are opaque; only emptiness is malformed (parse already
    // routes empty to Default, so this is defense in depth) (#7).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::*;

    #[test]
    fn defaults_are_valid() {
        assert!(validate(&Config::default()).is_empty());
    }

    #[test]
    fn range_violations_reported() {
        let mut c = Config::default();
        c.overlay.duration_ms = 10;
        c.overlay.scale = 9.0;
        c.overlay.opacity = 0.1;
        let v = validate(&c);
        assert_eq!(v.len(), 3);
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
}
