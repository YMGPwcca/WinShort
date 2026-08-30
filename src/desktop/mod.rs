//! Virtual desktop subsystem. Undocumented COM layouts are isolated and
//! build-pinned; callers see only the backend trait/service/status types.

pub mod backend;
pub mod detect;
pub mod internal_api;
pub mod keyboard_fallback;
pub mod service;
mod state;
mod workspace_state;

pub use backend::{BackendAvailability, BackendKind, BackendStatus};
pub use service::DesktopService;

#[cfg(test)]
mod policy_props;
