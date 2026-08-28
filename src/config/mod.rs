//! Configuration subsystem: typed model, TOML boundary, validation, snapshots.

pub mod load;
pub mod model;
pub mod save;
pub mod validate;

pub use model::Config;
pub use validate::validate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLoadDiagnostics {
    pub path: std::path::PathBuf,
    /// Schema explicitly present in the loaded document, if any.
    pub source_schema_version: Option<u8>,
    /// Schema of the runtime model and serializer used by this executable.
    pub effective_schema_version: u8,
    pub warnings: Vec<String>,
    pub repaired_fields: Vec<String>,
    pub migrations: Vec<String>,
}

impl Default for ConfigLoadDiagnostics {
    fn default() -> Self {
        Self {
            path: std::path::PathBuf::new(),
            source_schema_version: None,
            effective_schema_version: crate::config::model::CURRENT_SCHEMA_VERSION,
            warnings: Vec::new(),
            repaired_fields: Vec::new(),
            migrations: Vec::new(),
        }
    }
}

static LOAD_DIAGNOSTICS: std::sync::OnceLock<std::sync::RwLock<ConfigLoadDiagnostics>> =
    std::sync::OnceLock::new();

pub fn set_load_diagnostics(diagnostics: ConfigLoadDiagnostics) {
    let lock =
        LOAD_DIAGNOSTICS.get_or_init(|| std::sync::RwLock::new(ConfigLoadDiagnostics::default()));
    if let Ok(mut current) = lock.write() {
        *current = diagnostics;
    }
}

pub fn load_diagnostics() -> ConfigLoadDiagnostics {
    LOAD_DIAGNOSTICS
        .get()
        .and_then(|lock| lock.read().ok().map(|value| value.clone()))
        .unwrap_or_default()
}

/// Lock-free-read snapshot of the live configuration (spec §9, §10, §45).
///
/// Readers clone the current `Arc` cheaply under a short read lock; the only
/// writer is a main-thread config commit (Settings Save or a device-cycle
/// hotkey). Binding lookups in the keyboard hook go through
/// [`ConfigHandle::bindings`] — an arc-swap snapshot rebuilt here on every
/// `replace`, so the hook never takes a lock or rebuilds a table (#10). The
/// hook itself is never reinstalled when this swaps.
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
    pub fn bindings(
        &self,
    ) -> arc_swap::Guard<std::sync::Arc<crate::keyboard::binding::BindingTable>> {
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
        self.bindings
            .store(std::sync::Arc::new(crate::keyboard::hook::build_bindings(
                new,
            )));
        self.revision
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        old
    }

    /// Persist a candidate before publishing it to the live snapshot.
    ///
    /// The callback must perform the durable atomic write. A failed callback
    /// leaves both the live value and its revision untouched.
    pub fn replace_after_save<F>(
        &self,
        cfg: Config,
        save: F,
    ) -> crate::error::Result<std::sync::Arc<Config>>
    where
        F: FnOnce(&Config) -> crate::error::Result<()>,
    {
        save(&cfg)?;
        Ok(self.replace(cfg))
    }
}

/// Authoritative process data root via the Known Folder API (#15e): no "."
/// fallback — callers decide whether failure is fatal.
pub fn try_data_dir() -> crate::error::Result<std::path::PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath};
    let path = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_LocalAppData,
            windows::Win32::UI::Shell::KNOWN_FOLDER_FLAG(0),
            None,
        )
        .map_err(|e| crate::error::Error::win("SHGetKnownFolderPath(LocalAppData)", &e))?
    };
    // SAFETY: PWSTR::display is unsafe (validity contract); we own the
    // allocation returned by SHGetKnownFolderPath here.
    let display = unsafe { path.display().to_string() };
    let mut root = std::path::PathBuf::from(display);
    windows_cow_free(path);
    root.push("WinShort");
    Ok(root)
}

// Free the CoTaskMem-allocated PWSTR returned by SHGetKnownFolderPath.
fn windows_cow_free(p: windows_core::PWSTR) {
    unsafe {
        windows::Win32::System::Com::CoTaskMemFree(Some(p.as_ptr().cast()));
    }
}

/// Convenience wrapper: Known Folder first, `%LOCALAPPDATA%` second; never
/// returns "." (a silent wrong-directory is worse than a loud temp dir).
pub fn data_dir() -> std::path::PathBuf {
    if let Ok(dir) = try_data_dir() {
        return dir;
    }
    match std::env::var("LOCALAPPDATA") {
        Ok(local) => std::path::PathBuf::from(local).join("WinShort"),
        Err(_) => {
            crate::warn_!("config data_dir falling back to temp; LocalAppData unavailable");
            std::env::temp_dir().join("WinShort")
        }
    }
}

#[cfg(test)]
pub(crate) static LATCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static CONFIG_READONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set when a newer schema version was detected on load (#15b).
pub fn set_config_readonly(reason: &str) {
    CONFIG_READONLY.store(true, std::sync::atomic::Ordering::Release);
    crate::warn_!("config marked read-only: {reason}");
}

pub fn config_readonly() -> bool {
    CONFIG_READONLY.load(std::sync::atomic::Ordering::Acquire)
}

/// Test-only: the read-only latch is process-global; tests that trip it must
/// clear it afterwards so unrelated tests are not poisoned.
#[cfg(test)]
pub(crate) fn latch_guard() -> std::sync::MutexGuard<'static, ()> {
    match LATCH_LOCK.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
pub fn clear_config_readonly() {
    CONFIG_READONLY.store(false, std::sync::atomic::Ordering::Release);
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
    #[test]
    fn failed_config_persist_does_not_replace_live_snapshot() {
        let handle = ConfigHandle::new(Config::default());
        let mut candidate = Config::default();
        candidate.general.start_hotkeys_enabled = false;
        let result = handle.replace_after_save(candidate, |_| {
            Err(crate::error::Error::config("simulated persistence failure"))
        });
        assert!(result.is_err());
        assert_eq!(*handle.get(), Config::default());
        assert_eq!(handle.revision(), 1);
    }

    #[test]
    fn successful_config_persist_replaces_once_after_callback() {
        use std::cell::Cell;

        let handle = ConfigHandle::new(Config::default());
        let mut candidate = Config::default();
        candidate.general.start_hotkeys_enabled = false;
        let writes = Cell::new(0);
        handle
            .replace_after_save(candidate.clone(), |_| {
                writes.set(writes.get() + 1);
                Ok(())
            })
            .unwrap();
        assert_eq!(writes.get(), 1);
        assert_eq!(*handle.get(), candidate);
        assert_eq!(handle.revision(), 2);
    }

    #[test]
    fn read_only_config_rejects_transaction_before_publication() {
        let _guard = latch_guard();
        clear_config_readonly();
        set_config_readonly("test future schema");
        let handle = ConfigHandle::new(Config::default());
        let mut candidate = Config::default();
        candidate.general.start_hotkeys_enabled = false;
        let dir = std::env::temp_dir().join(format!("winshort-readonly-{}", std::process::id()));
        let result =
            handle.replace_after_save(candidate, |config| crate::config::save::save(&dir, config));
        assert!(result.is_err());
        assert_eq!(*handle.get(), Config::default());
        assert_eq!(handle.revision(), 1);
        clear_config_readonly();
    }
}

#[cfg(test)]
mod config_props;
