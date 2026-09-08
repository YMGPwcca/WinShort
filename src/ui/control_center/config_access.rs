//! Application-owned capabilities supplied to the Control Center.

use crate::config::model::Config;
use crate::error::Result;
use std::path::PathBuf;
use std::sync::Arc;

/// The Control Center needs only a current snapshot and an atomic commit.
/// Function pointers suffice for the existing application boundary; no service
/// container, dynamic allocation, or generic UI type is needed.
#[derive(Clone, Copy)]
pub(crate) struct ConfigAccess {
    load: fn() -> Arc<Config>,
    commit: fn(Config) -> Result<()>,
}

impl ConfigAccess {
    pub(crate) fn new(load: fn() -> Arc<Config>, commit: fn(Config) -> Result<()>) -> Self {
        Self { load, commit }
    }

    pub(super) fn current(self) -> Arc<Config> {
        (self.load)()
    }
    pub(super) fn commit(self, candidate: Config) -> Result<()> {
        (self.commit)(candidate)
    }

    #[cfg(test)]
    pub(super) fn unavailable() -> Self {
        Self::new(
            || Arc::new(Config::default()),
            |_| {
                Err(crate::error::Error::config(
                    "configuration handle unavailable",
                ))
            },
        )
    }
}

/// Narrow side-effect boundary used by Control Center state.
///
/// The application injects one concrete capability bundle. Read-only values
/// are cached by `SettingsUi`; functions here are used only at construction,
/// refresh, or explicit action points.
#[derive(Clone, Copy)]
pub(crate) struct ControlCenterAccess {
    data_dir: fn() -> PathBuf,
    startup_enabled: fn() -> bool,
    set_startup_enabled: fn(bool) -> Result<()>,
    debug_logging_enabled: fn() -> bool,
    set_debug_logging: fn(bool),
    begin_capture: fn(),
    end_capture: fn(),
    take_captured_chord: fn() -> Option<crate::keyboard::hook::CapturedChord>,
}

impl ControlCenterAccess {
    pub(crate) const fn system() -> Self {
        Self {
            data_dir: crate::config::data_dir,
            startup_enabled: crate::platform::startup::is_enabled,
            set_startup_enabled: crate::platform::startup::set_enabled,
            debug_logging_enabled: crate::diagnostics::logging::debug_logging_enabled,
            set_debug_logging: crate::diagnostics::logging::set_debug_logging,
            begin_capture: crate::keyboard::hook::begin_capture,
            end_capture: crate::keyboard::hook::end_capture,
            take_captured_chord: crate::keyboard::hook::take_captured_chord,
        }
    }

    pub(super) fn data_dir(self) -> PathBuf {
        (self.data_dir)()
    }

    pub(super) fn startup_enabled(self) -> bool {
        (self.startup_enabled)()
    }

    pub(super) fn set_startup_enabled(self, enabled: bool) -> Result<()> {
        (self.set_startup_enabled)(enabled)
    }

    pub(super) fn debug_logging_enabled(self) -> bool {
        (self.debug_logging_enabled)()
    }

    pub(super) fn set_debug_logging(self, enabled: bool) {
        (self.set_debug_logging)(enabled);
    }

    pub(super) fn begin_capture(self) {
        (self.begin_capture)();
    }

    pub(super) fn end_capture(self) {
        (self.end_capture)();
    }

    pub(super) fn take_captured_chord(self) -> Option<crate::keyboard::hook::CapturedChord> {
        (self.take_captured_chord)()
    }
}
