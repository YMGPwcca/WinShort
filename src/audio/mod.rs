//! Core Audio subsystem. All COM interfaces stay on the dedicated MTA worker.

pub mod controller;
pub mod devices;
pub mod notifications;
pub mod state;
pub mod sessions;

pub use controller::{AudioCommand, AudioService};
pub use state::{Aggregate, AppAudioState, AudioState, DeviceId, OutputState};
