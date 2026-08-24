//! Config load: read `%LOCALAPPDATA%\WinShort\config.toml`; missing or corrupt
//! file yields defaults with a warning (spec §8, §46) so the UI still opens.
//! Hardening (#15): unknown keys warn, newer schema versions refuse writes,
//! validation violations repair to defaults, and durability is guaranteed.

use std::path::Path;

use crate::config::model::{Config, ConfigToml};
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
            match toml::from_str::<ConfigToml>(&text) {
                Ok(toml) => {
                    if toml.schema_version > 1 {
                        let msg = format!(
                            "config written by a newer WinShort (schema v{}); not overwriting",
                            toml.schema_version
                        );
                        crate::error_!("{msg}");
                        crate::config::set_config_readonly(&msg);
                        warnings.push(msg.clone());
                        return (Config::default(), warnings);
                    }
                    let (mut cfg, mut ws) = Config::from_toml(&toml);
                    warnings.append(&mut ws);
                    // Validate and repair in place (#15a).
                    let violations = crate::config::validate(&cfg);
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
                    (cfg, warnings)
                }
                Err(e) => {
                    let msg = format!("config parse failed: {e}");
                    crate::error_!("{}", msg);
                    (Config::default(), vec![msg])
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            crate::info!("config not found; using defaults ({})", path.display());
            (Config::default(), Vec::new())
        }
        Err(e) => {
            let msg = format!("config read failed: {e}");
            crate::error_!("{}", msg);
            (Config::default(), vec![msg])
        }
    }
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

    #[test]
    fn newer_schema_version_refuses_overwrite() {
        // #15b: schema v2 must mark the store read-only.
        let raw = "schema_version = 9\n";
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
        assert!(crate::config::config_readonly());
        // Save must refuse while read-only.
        let err = crate::config::save::save(&dir, &Config::default());
        assert!(err.is_err());
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
