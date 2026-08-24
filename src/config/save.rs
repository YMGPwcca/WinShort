//! Atomic config persistence (spec §9): write temp file in the same directory,
//! fsync, rename over the target. Never leaves partial files.

use std::path::Path;

use crate::config::load::config_path;
use crate::config::model::Config;
use crate::error::{Error, Result};

pub fn save(data_dir: &Path, cfg: &Config) -> Result<()> {
    // Read-only guard (#15b): a config from a newer WinShort must never be
    // silently overwritten.
    if crate::config::config_readonly() {
        return Err(Error::config(
            "config written by a newer WinShort; not overwriting",
        ));
    }
    let path = config_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::config(format!("create config dir: {e}")))?;
    }

    let toml = cfg.to_toml();
    let text =
        toml::to_string_pretty(&toml).map_err(|e| Error::config(format!("serialize: {e}")))?;

    // Durable atomic commit (#15d): create -> write -> flush -> sync on the
    // SAME handle, then rename over the target.
    let tmp = path.with_extension("toml.tmp");
    {
        use std::io::Write;
        let mut file =
            std::fs::File::create(&tmp).map_err(|e| Error::config(format!("create temp: {e}")))?;
        file.write_all(text.as_bytes())
            .map_err(|e| Error::config(format!("write temp: {e}")))?;
        file.flush()
            .map_err(|e| Error::config(format!("flush temp: {e}")))?;
        file.sync_all()
            .map_err(|e| Error::config(format!("sync temp: {e}")))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::config(format!("rename: {e}"))
    })?;
    crate::info!("config saved to {}", path.display());
    Ok(())
}

/// Serialize without writing (used by tests and the "reset" flow preview).
pub fn to_text(cfg: &Config) -> Result<String> {
    toml::to_string_pretty(&cfg.to_toml()).map_err(|e| Error::config(format!("serialize: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load::load;
    use crate::config::model::Config;

    #[test]
    fn round_trip_preserves_config() {
        let dir = std::env::temp_dir().join(format!("winshort-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = Config::default();
        save(&dir, &cfg).unwrap();
        let (loaded, warnings) = load(&dir);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(loaded, cfg);
        // No temp residue.
        assert!(!dir.join("config.toml.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = std::env::temp_dir().join(format!("winshort-missing-{}", std::process::id()));
        let (cfg, w) = load(&dir);
        assert!(w.is_empty());
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn corrupt_file_falls_back() {
        let dir = std::env::temp_dir().join(format!("winshort-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "[overlay\nduration_ms = ===").unwrap();
        let (cfg, warnings) = load(&dir);
        assert_eq!(cfg, Config::default());
        assert!(!warnings.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
