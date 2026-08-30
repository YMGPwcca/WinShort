//! Config load: read `%LOCALAPPDATA%\WinShort\config.toml`; missing or corrupt
//! file yields defaults with a warning (spec §8, §46) so the UI still opens.
//! Hardening (#15): unknown keys warn, newer schema versions refuse writes,
//! validation violations repair to defaults, and durability is guaranteed.

use std::path::Path;

use crate::config::model::{Config, ConfigToml, CURRENT_SCHEMA_VERSION};
pub fn config_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("config.toml")
}

/// Load config; returns (config, warnings). Never fails hard.
pub fn load(data_dir: &Path) -> (Config, Vec<String>) {
    let path = config_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            // Unknown top-level/section keys (double parse, #15c).
            let mut warnings = unknown_keys(&text);
            let source_schema = source_schema_version(&text);
            match toml::from_str::<ConfigToml>(&text) {
                Ok(toml) => {
                    // Versionless documents deserialize as the legacy v1
                    // baseline; newly serialized documents always include v9.
                    let parsed_schema = toml.schema_version;
                    if parsed_schema > CURRENT_SCHEMA_VERSION {
                        let msg = format!(
                            "config written by a newer WinShort (schema v{}); not overwriting",
                            parsed_schema
                        );
                        crate::error_!("{msg}");
                        crate::config::set_config_readonly(&msg);
                        warnings.push(msg.clone());
                        record_diagnostics(&path, source_schema, &warnings, &[], &[]);
                        return (Config::default(), warnings);
                    }
                    let (mut cfg, mut parsed_warnings) = Config::from_toml(&toml);
                    warnings.append(&mut parsed_warnings);
                    // Validate and repair in place (#15a).
                    let violations = crate::config::validate(&cfg);
                    let repaired_fields: Vec<String> = violations
                        .iter()
                        .map(|violation| violation.field.clone())
                        .collect();
                    if !violations.is_empty() {
                        for v in &violations {
                            crate::warn_!("config violation repaired: {} ({})", v.field, v.message);
                        }
                        cfg.repair(&violations);
                    }
                    if !warnings.is_empty() {
                        crate::warn_!(
                            "config {} loaded with warnings: {:?}",
                            path.display(),
                            warnings
                        );
                    }
                    let mut migrations = Vec::new();
                    if parsed_schema < CURRENT_SCHEMA_VERSION {
                        let detail = match parsed_schema {
                            1 => "v2 overlay defaults, v3 hotkey fields, v4 desktop controls, v5 scratchpad fields, v7 audio allowlists, v8 display profiles, and v9 profile hotkeys defaulted",
                            2 => "v3 hotkey fields, v4 desktop controls, v5 scratchpad fields, v7 audio allowlists, v8 display profiles, and v9 profile hotkeys defaulted",
                            3 => "v4 desktop workflow, v5 scratchpad fields, v7 audio allowlists, v8 display profiles, and v9 profile hotkeys defaulted",
                            4 => "v5 scratchpad fields, v7 audio allowlists, v8 display profiles, and v9 profile hotkeys defaulted",
                            5 => "v7 audio allowlists, v8 display profiles, and v9 profile hotkeys defaulted",
                            6 => "v7 audio allowlists, v8 display profiles, and v9 profile hotkeys defaulted",
                            7 => "v8 display profiles and v9 profile hotkeys defaulted",
                            8 => "v9 display profile hotkeys and confirmation state defaulted",
                            _ => "newer fields defaulted",
                        };
                        migrations.push(format!(
                            "schema v{parsed_schema} migrated to v{CURRENT_SCHEMA_VERSION}: {detail}"
                        ));
                    }
                    if text.to_ascii_lowercase().contains("monitor = \"index:") {
                        migrations.push("overlay.monitor index:N mapped to primary".into());
                    }
                    record_diagnostics(
                        &path,
                        source_schema,
                        &warnings,
                        &repaired_fields,
                        &migrations,
                    );
                    (cfg, warnings)
                }
                Err(e) => {
                    let msg = format!("config parse failed: {e}");
                    crate::error_!("{}", msg);
                    let warnings = vec![msg];
                    record_diagnostics(&path, None, &warnings, &[], &[]);
                    (Config::default(), warnings)
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            crate::info!("config not found; using defaults ({})", path.display());
            record_diagnostics(&path, None, &[], &[], &[]);
            (Config::default(), Vec::new())
        }
        Err(e) => {
            let msg = format!("config read failed: {e}");
            crate::error_!("{}", msg);
            let warnings = vec![msg];
            record_diagnostics(&path, None, &warnings, &[], &[]);
            (Config::default(), warnings)
        }
    }
}

fn source_schema_version(text: &str) -> Option<u8> {
    let table = text.parse::<toml::Table>().ok()?;
    let value = table.get("schema_version")?.as_integer()?;
    u8::try_from(value).ok()
}

fn record_diagnostics(
    path: &Path,
    source_schema_version: Option<u8>,
    warnings: &[String],
    repaired_fields: &[String],
    migrations: &[String],
) {
    crate::config::set_load_diagnostics(crate::config::ConfigLoadDiagnostics {
        path: path.to_path_buf(),
        source_schema_version,
        effective_schema_version: CURRENT_SCHEMA_VERSION,
        warnings: warnings.to_vec(),
        repaired_fields: repaired_fields.to_vec(),
        migrations: migrations.to_vec(),
    });
}

fn unknown_keys(text: &str) -> Vec<String> {
    use crate::config::model::known_keys;
    // NOTE: toml::Value::FromStr parses a single VALUE; documents need Table.
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new(); // parse errors are reported by from_toml
    };
    let mut out = Vec::new();
    for (section, entries) in &table {
        match entries.as_table() {
            None => {
                if known_keys(section).is_none() && section != "schema_version" {
                    out.push(format!("unknown key `{section}`"));
                }
            }
            Some(entries) => match known_keys(section) {
                Some(known) => {
                    for key in entries.keys() {
                        if !known.contains(&key.as_str()) {
                            out.push(format!("unknown key `{section}.{key}`"));
                        }
                    }
                }
                None => out.push(format!("unknown section `{section}`")),
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::*;
    use crate::keyboard::binding::{Hotkey, ModifierMask};

    #[test]
    fn newer_schema_version_refuses_overwrite() {
        let _guard = crate::config::latch_guard();
        // #15b: a future schema must mark the store read-only.
        let future_schema = CURRENT_SCHEMA_VERSION.saturating_add(1);
        let raw = format!("schema_version = {future_schema}\n");
        let dir = std::env::temp_dir().join(format!("ws_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(config_path(&dir), raw).unwrap();

        let (cfg, warnings) = load(&dir);
        // A future schema must NOT be partially applied: every section falls
        // back to its default representation.
        assert_eq!(cfg.overlay, Config::default().overlay);
        assert_eq!(cfg.hotkeys, Config::default().hotkeys);
        assert_eq!(cfg.virtual_desktops, Config::default().virtual_desktops);
        assert!(
            warnings.iter().any(|w| w.contains("newer WinShort")),
            "warnings: {warnings:?}"
        );
        let diagnostics = crate::config::load_diagnostics();
        assert_eq!(diagnostics.source_schema_version, Some(future_schema));
        assert_eq!(diagnostics.effective_schema_version, CURRENT_SCHEMA_VERSION);
        assert!(crate::config::config_readonly());
        // Save must refuse while read-only.
        let err = crate::config::save::save(&dir, &Config::default());
        assert!(err.is_err());
        crate::config::clear_config_readonly();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn schema_v1_gets_overlay_defaults_and_records_migration() {
        let _guard = crate::config::latch_guard();
        crate::config::clear_config_readonly();
        let dir = std::env::temp_dir().join(format!("ws_schema_v1_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            config_path(&dir),
            "schema_version = 1\n[overlay]\nenabled = true\nduration_ms = 900\n[audio]\ninput_role = \"console\"\noutput_role = \"console\"\n",
        )
        .unwrap();

        let (cfg, warnings) = load(&dir);
        let diagnostics = crate::config::load_diagnostics();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(cfg.overlay.appearance, OverlayAppearance::System);
        assert!(cfg.overlay.show_external_audio_changes);
        assert_eq!(diagnostics.source_schema_version, Some(1));
        assert_eq!(diagnostics.effective_schema_version, CURRENT_SCHEMA_VERSION);
        assert!(diagnostics
            .migrations
            .iter()
            .any(|value| value.contains("schema v1 migrated")));

        crate::config::clear_config_readonly();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn schema_v2_defaults_new_hotkeys_and_records_v8_migration() {
        let _guard = crate::config::latch_guard();
        crate::config::clear_config_readonly();
        let dir = std::env::temp_dir().join(format!("ws_schema_v2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            config_path(&dir),
            "schema_version = 2\n[general]\nstart_hotkeys_enabled = false\n[hotkeys]\ntoggle_microphone = \"Ctrl+Alt+F1\"\ntoggle_output = \"Ctrl+Alt+F2\"\ntoggle_foreground_audio = \"Ctrl+Alt+F3\"\n",
        )
        .unwrap();

        let (cfg, warnings) = load(&dir);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!cfg.general.start_hotkeys_enabled);
        assert_eq!(
            cfg.hotkeys.toggle_microphone,
            Some(crate::keyboard::binding::Hotkey::parse("Ctrl+Alt+F1").unwrap())
        );
        assert!(cfg.hotkeys.cycle_input_device.is_none());
        assert!(cfg.hotkeys.cycle_output_device.is_none());
        assert!(cfg.hotkeys.foreground_volume_up.is_none());
        assert!(cfg.hotkeys.foreground_volume_down.is_none());
        let diagnostics = crate::config::load_diagnostics();
        assert_eq!(diagnostics.source_schema_version, Some(2));
        assert!(diagnostics
            .migrations
            .iter()
            .any(|value| value.contains("schema v2 migrated") && value.contains("v8")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn versionless_document_is_legacy_v1_with_current_effective_schema() {
        let _guard = crate::config::latch_guard();
        crate::config::clear_config_readonly();
        let dir = std::env::temp_dir().join(format!("ws_schema_absent_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            config_path(&dir),
            "[audio]\ninput_role = \"console\"\noutput_role = \"console\"\n",
        )
        .unwrap();

        let (cfg, warnings) = load(&dir);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(cfg, Config::default());
        let diagnostics = crate::config::load_diagnostics();
        assert_eq!(diagnostics.source_schema_version, None);
        assert_eq!(diagnostics.effective_schema_version, CURRENT_SCHEMA_VERSION);
        assert!(diagnostics
            .migrations
            .iter()
            .any(|value| value.contains("schema v1 migrated")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_config_reports_no_source_and_current_effective_schema() {
        let _guard = crate::config::latch_guard();
        crate::config::clear_config_readonly();
        let dir = std::env::temp_dir().join(format!("ws_schema_missing_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (cfg, warnings) = load(&dir);
        assert_eq!(cfg, Config::default());
        assert!(warnings.is_empty());
        let diagnostics = crate::config::load_diagnostics();
        assert_eq!(diagnostics.source_schema_version, None);
        assert_eq!(diagnostics.effective_schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn corrupt_config_reports_no_source_schema() {
        let _guard = crate::config::latch_guard();
        crate::config::clear_config_readonly();
        let dir = std::env::temp_dir().join(format!("ws_schema_corrupt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            config_path(&dir),
            "schema_version = 1\n[overlay]\nduration_ms = \"bad\"\n",
        )
        .unwrap();

        let (cfg, warnings) = load(&dir);
        assert_eq!(cfg, Config::default());
        assert!(!warnings.is_empty());
        let diagnostics = crate::config::load_diagnostics();
        assert_eq!(diagnostics.source_schema_version, None);
        assert_eq!(diagnostics.effective_schema_version, CURRENT_SCHEMA_VERSION);
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]

    fn saving_migrated_config_updates_diagnostics_to_v8() {
        let _guard = crate::config::latch_guard();
        crate::config::clear_config_readonly();
        let dir = std::env::temp_dir().join(format!("ws_schema_save_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            config_path(&dir),
            "schema_version = 1\n[audio]\ninput_role = \"console\"\noutput_role = \"console\"\n",
        )
        .unwrap();
        let _ = load(&dir);
        assert!(crate::config::save::save(&dir, &Config::default()).is_ok());
        let text = std::fs::read_to_string(config_path(&dir)).unwrap();
        let parsed: ConfigToml = toml::from_str(&text).unwrap();
        assert_eq!(parsed.schema_version, CURRENT_SCHEMA_VERSION);
        let diagnostics = crate::config::load_diagnostics();
        assert_eq!(
            diagnostics.source_schema_version,
            Some(CURRENT_SCHEMA_VERSION)
        );
        assert_eq!(diagnostics.effective_schema_version, CURRENT_SCHEMA_VERSION);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_routing_is_warned_ignored_and_stripped_on_save() {
        let _guard = crate::config::latch_guard();
        crate::config::clear_config_readonly();
        let dir =
            std::env::temp_dir().join(format!("ws_schema_legacy_routing_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            config_path(&dir),
            r#"schema_version = 6
[virtual_desktops]
enabled = false
win_number_switching = false
number_modifier = "Ctrl+Alt"
move_follow_modifier = "Shift"
move_silent_modifier = "Ctrl+Shift"
previous_desktop = "Ctrl+Alt+F12"
scratchpad_assign = "Ctrl+Alt+F13"
scratchpad_toggle = "Ctrl+Alt+F14"
[[virtual_desktops.routing_rules]]
executable = "notepad.exe"
desktop = 2
"#,
        )
        .unwrap();

        let (config, warnings) = load(&dir);
        assert!(warnings.iter().any(|warning| {
            warning.contains("virtual_desktops.routing_rules")
                && warning.contains("removed")
                && warning.contains("ignored")
        }));
        assert_eq!(
            config.virtual_desktops.number_modifier,
            ModifierMask::CTRL.union(ModifierMask::ALT)
        );
        assert_eq!(
            config.virtual_desktops.move_follow_modifier,
            Some(ModifierMask::SHIFT)
        );
        assert_eq!(
            config.virtual_desktops.move_silent_modifier,
            Some(ModifierMask::CTRL.union(ModifierMask::SHIFT))
        );
        assert_eq!(
            config.virtual_desktops.previous_desktop,
            Some(Hotkey::parse("Ctrl+Alt+F12").unwrap())
        );
        let diagnostics = crate::config::load_diagnostics();
        assert_eq!(diagnostics.source_schema_version, Some(6));
        assert!(diagnostics
            .warnings
            .iter()
            .any(|warning| { warning.contains("routing_rules") && warning.contains("ignored") }));

        crate::config::save::save(&dir, &config).unwrap();
        let saved_text = std::fs::read_to_string(config_path(&dir)).unwrap();
        assert!(!saved_text.contains("routing_rules"), "{saved_text}");
        let (saved, saved_warnings) = load(&dir);
        assert!(saved_warnings.is_empty(), "{saved_warnings:?}");
        assert_eq!(saved, config);
        assert_eq!(
            crate::config::load_diagnostics().source_schema_version,
            Some(CURRENT_SCHEMA_VERSION)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_keys_are_reported() {
        let keys = unknown_keys("[overlay]\nnope = 1\n");
        assert!(keys.iter().any(|k| k.contains("overlay.nope")), "{keys:?}");
        let keys = unknown_keys("[totally_bogus]\nx = 1\n");
        assert!(
            keys.iter().any(|k| k.contains("unknown section")),
            "{keys:?}"
        );
    }
}
