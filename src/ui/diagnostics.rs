//! Diagnostics window API. Report generation is separate from rendering and native dispatch.

mod interaction;
mod layout;
mod messages;
mod model;
mod native;
mod painting;
mod report;
mod state;
mod window;

pub(crate) use window::DiagnosticsWindow;
