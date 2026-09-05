//! Configuration I/O is supplied by the application, not discovered by UI state.

use crate::config::model::Config;
use crate::error::Result;
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
