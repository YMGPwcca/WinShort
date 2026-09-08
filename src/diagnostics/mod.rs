//! Diagnostics models, snapshot assembly, logging, and support artifacts.

#[macro_use]
pub mod logging;
// App supplies cached runtime state; diagnostics owns report construction.
pub(crate) mod app_snapshot;
pub mod snapshot;
pub mod support;
