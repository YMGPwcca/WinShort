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
/// Readers clone the current `Arc` cheaply under a short read lock; the only
/// writer is Save on the main thread. Binding lookups in the keyboard hook go
/// through [`ConfigHandle::bindings`] — an arc-swap snapshot rebuilt here on
/// every `replace`, so the hook never takes a lock or rebuilds a table
/// (#10). The hook itself is never reinstalled when this swaps.
pub struct ConfigHandle {
    value: std::sync::RwLock<std::sync::Arc<Config>>,
    bindings: arc_swap::ArcSwap<crate::keyboard::binding::BindingTable>,
    revision: std::sync::atomic::AtomicU64,
}

impl ConfigHandle {
    pub fn new(cfg: Config) -> Self {
        let bindings = arc_swap::ArcSwap::from_pointee(crate::keyboard::hook::build_bindings(&cfg));
        Self {
            value: std::sync::RwLock::new(std::sync::Arc::new(cfg)),
            bindings,
            revision: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub fn get(&self) -> std::sync::Arc<Config> {
        self.value.read().expect("config lock").clone()
    }

    /// Snapshot of the active hotkey table; lock-free, wait-free read.
    pub fn bindings(&self) -> arc_swap::Guard<std::sync::Arc<crate::keyboard::binding::BindingTable>> {
        self.bindings.load()
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Atomically replace the live snapshot; returns the previous one.
    pub fn replace(&self, cfg: Config) -> std::sync::Arc<Config> {
        let mut w = self.value.write().expect("config lock");
        let old = std::mem::replace(&mut *w, std::sync::Arc::new(cfg));
        let new = &*w;
        self.bindings.store(std::sync::Arc::new(crate::keyboard::hook::build_bindings(new)));
        self.revision.fetch_add(1, std::sync::atomic::Ordering::Release);
        old
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::HotkeyAction;
    use crate::keyboard::binding::{Hotkey, ModifierMask, VirtualKey};

    #[test]
    fn replace_publishes_new_binding_snapshot_immediately() {
        // Regression for #10: the hook must see new bindings without taking
        // the config lock or rebuilding the table inside the callback.
        let handle = ConfigHandle::new(Config::default());
        let mut cfg = Config::default();
        cfg.hotkeys.toggle_microphone = Some(Hotkey {
            modifiers: ModifierMask::CTRL.union(ModifierMask::ALT),
            key: VirtualKey(0x7A), // VK_F13 — never a default binding
        });
        handle.replace(cfg);
        let hit = handle.bindings().lookup(
            ModifierMask::CTRL.union(ModifierMask::ALT),
            VirtualKey(0x7A),
        );
        assert_eq!(hit, Some(HotkeyAction::ToggleMicrophone));
    }
}

/// Process data root: `%LOCALAPPDATA%\WinShort`.
pub fn data_dir() -> std::path::PathBuf {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(local).join("WinShort")
}