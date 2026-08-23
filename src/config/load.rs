//! Config load: read `%LOCALAPPDATA%\WinShort\config.toml`; missing or corrupt
//! file yields defaults with a warning (spec §8, §46) so the UI still opens.

use std::path::Path;

use crate::config::model::{Config, ConfigToml};
use crate::error::Result;

pub fn config_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("config.toml")
}

/// Load config; returns (config, warnings). Never fails hard.
pub fn load(data_dir: &Path) -> (Config, Vec<String>) {
    let path = config_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<ConfigToml>(&text) {
            Ok(toml) => {
                let (cfg, warnings) = Config::from_toml(&toml);
                if !warnings.is_empty() {
                    crate::warn_!("config {} loaded with warnings: {:?}", path.display(), warnings);
                }
                (cfg, warnings)
            }
            Err(e) => {
                let msg = format!("config parse failed: {e}");
                crate::error_!("{}", msg);
                (Config::default(), vec![msg])
            }
        },
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

/// Strict variant used by Save: parse errors are hard failures.
pub fn parse_strict(text: &str) -> Result<Config> {
    let toml: ConfigToml =
        toml::from_str(text).map_err(|e| crate::error::Error::config(format!("{e}")))?;
    let (cfg, warnings) = Config::from_toml(&toml);
    if !warnings.is_empty() {
        return Err(crate::error::Error::config(warnings.join("; ")));
    }
    Ok(cfg)
}
