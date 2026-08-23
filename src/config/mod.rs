//! Configuration subsystem: typed model, TOML boundary, validation, snapshots.

pub mod load;
pub mod model;
pub mod save;
pub mod validate;

pub use model::{
    Config, DeviceSelection, EndpointRole, HotkeysCfg, MonitorChoice, OverlayCfg, OverlayPosition,
    VdCfg, DEFAULT_TOGGLE_FOREGROUND, DEFAULT_TOGGLE_MICROPHONE, DEFAULT_TOGGLE_OUTPUT,
};
pub use validate::validate;


/// Lock-free-read snapshot of the live configuration (spec §9, §10, §45).
///
/// Readers (keyboard hook thread, UI) clone the current `Arc` cheaply under a
/// short read lock; the only writer is Save on the main thread. The keyboard
/// hook is never reinstalled when this swaps.
pub struct ConfigHandle {
    value: std::sync::RwLock<std::sync::Arc<Config>>,
    revision: std::sync::atomic::AtomicU64,
}

impl ConfigHandle {
    pub fn new(cfg: Config) -> Self {
        Self {
            value: std::sync::RwLock::new(std::sync::Arc::new(cfg)),
            revision: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub fn get(&self) -> std::sync::Arc<Config> {
        self.value.read().expect("config lock").clone()
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Atomically replace the live snapshot; returns the previous one.
    pub fn replace(&self, cfg: Config) -> std::sync::Arc<Config> {
        let mut w = self.value.write().expect("config lock");
        let old = std::mem::replace(&mut *w, std::sync::Arc::new(cfg));
        self.revision.fetch_add(1, std::sync::atomic::Ordering::Release);
        old
    }
}

/// Process data root: `%LOCALAPPDATA%\WinShort`.
pub fn data_dir() -> std::path::PathBuf {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(local).join("WinShort")
}