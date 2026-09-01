//! Minimal UI-owned onboarding state.
//!
//! The marker is separate from `config.toml`: adding the Control Center never
//! makes an existing configuration look like a new installation.

use std::path::{Path, PathBuf};

const STATE_FILE: &str = "control-center-ui-state.txt";
const COMPLETED_LINE: &str = "onboarding_completed=1";

pub fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(STATE_FILE)
}

pub fn is_completed(data_dir: &Path) -> bool {
    std::fs::read_to_string(state_path(data_dir))
        .ok()
        .is_some_and(|text| text.lines().any(|line| line.trim() == COMPLETED_LINE))
}

/// Existing users are considered migrated when a real config file already
/// exists, even if it only contains defaults. This is intentionally stronger
/// than checking for a newly added marker field.
pub fn should_show(data_dir: &Path) -> bool {
    !is_completed(data_dir) && !data_dir.join("config.toml").exists()
}

pub fn mark_completed(data_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let path = state_path(data_dir);
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, format!("{COMPLETED_LINE}\n"))?;
    std::fs::rename(temp, path)
}

#[cfg(test)]
mod tests {
    use super::{is_completed, mark_completed, should_show};

    #[test]
    fn missing_state_and_config_is_new_user() {
        let dir = std::env::temp_dir().join(format!("winshort-ui-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test directory");
        assert!(should_show(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_config_is_never_re_onboarded() {
        let dir =
            std::env::temp_dir().join(format!("winshort-ui-config-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test directory");
        std::fs::write(dir.join("config.toml"), "schema_version = 10\n").expect("config");
        assert!(!should_show(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn completion_marker_is_durable_and_separate() {
        let dir =
            std::env::temp_dir().join(format!("winshort-ui-state-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test directory");
        assert!(!is_completed(&dir));
        mark_completed(&dir).expect("marker");
        assert!(is_completed(&dir));
        assert!(!dir.join("config.toml").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
