//! Durable identity for the dedicated Special Desktop.
//!
//! The GUID is runtime state, not user configuration. Windows preserves Virtual
//! Desktop GUIDs across a reboot, so keeping the identity in LocalAppData lets a
//! new WinShort process reclaim the same desktop after an abrupt process kill or
//! OS shutdown instead of creating an orphan + replacement pair.

use std::io::Write;
use std::path::{Path, PathBuf};

use windows_core::GUID;

use crate::error::{Error, Result};

const FILE_NAME: &str = "special-workspace.guid";

fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

pub fn load() -> Result<Option<GUID>> {
    load_from(&crate::config::data_dir())
}

pub fn store(id: GUID) -> Result<()> {
    store_in(&crate::config::data_dir(), id)
}

pub fn clear() -> Result<()> {
    clear_in(&crate::config::data_dir())
}

fn load_from(data_dir: &Path) -> Result<Option<GUID>> {
    let path = state_path(data_dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Error::desktop(format!(
                "read Special Desktop identity {}: {error}",
                path.display()
            )))
        }
    };
    let value = text.trim();
    if value.len() != 32 {
        return Err(Error::desktop(format!(
            "Special Desktop identity has invalid length {}",
            value.len()
        )));
    }
    let raw = u128::from_str_radix(value, 16)
        .map_err(|error| Error::desktop(format!("parse Special Desktop identity: {error}")))?;
    if raw == 0 {
        return Err(Error::desktop(
            "Special Desktop identity cannot be the zero GUID",
        ));
    }
    Ok(Some(GUID::from_u128(raw)))
}

fn store_in(data_dir: &Path, id: GUID) -> Result<()> {
    if id == GUID::zeroed() {
        return Err(Error::desktop(
            "refusing to persist the zero GUID as a Special Desktop identity",
        ));
    }
    std::fs::create_dir_all(data_dir).map_err(|error| {
        Error::desktop(format!(
            "create Special Desktop state directory {}: {error}",
            data_dir.display()
        ))
    })?;
    let path = state_path(data_dir);
    let tmp = path.with_extension("guid.tmp");
    let text = format!("{:032x}\n", id.to_u128());
    {
        let mut file = std::fs::File::create(&tmp).map_err(|error| {
            Error::desktop(format!(
                "create Special Desktop temp state {}: {error}",
                tmp.display()
            ))
        })?;
        file.write_all(text.as_bytes()).map_err(|error| {
            Error::desktop(format!(
                "write Special Desktop temp state {}: {error}",
                tmp.display()
            ))
        })?;
        file.flush().map_err(|error| {
            Error::desktop(format!(
                "flush Special Desktop temp state {}: {error}",
                tmp.display()
            ))
        })?;
        file.sync_all().map_err(|error| {
            Error::desktop(format!(
                "sync Special Desktop temp state {}: {error}",
                tmp.display()
            ))
        })?;
    }
    std::fs::rename(&tmp, &path).map_err(|error| {
        let _ = std::fs::remove_file(&tmp);
        Error::desktop(format!(
            "commit Special Desktop identity {}: {error}",
            path.display()
        ))
    })?;
    Ok(())
}

fn clear_in(data_dir: &Path) -> Result<()> {
    let path = state_path(data_dir);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::desktop(format!(
            "remove Special Desktop identity {}: {error}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "winshort-special-workspace-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn identity_round_trip_is_exact() {
        let dir = temp_dir();
        let id = GUID::from_u128(0xe82fadd6_9821_4fad_b0f2_f376d97b1991);
        store_in(&dir, id).unwrap();
        assert_eq!(load_from(&dir).unwrap(), Some(id));
        clear_in(&dir).unwrap();
        assert_eq!(load_from(&dir).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_identity_is_rejected() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(state_path(&dir), "not-a-guid\n").unwrap();
        assert!(load_from(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zero_guid_is_never_persisted() {
        let dir = temp_dir();
        assert!(store_in(&dir, GUID::zeroed()).is_err());
        assert_eq!(load_from(&dir).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
