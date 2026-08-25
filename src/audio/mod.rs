//! Core Audio subsystem. All COM interfaces stay on the dedicated MTA worker.

pub mod controller;
pub mod devices;
pub mod notifications;
pub mod sessions;
pub mod state;

pub use controller::{AudioCommand, AudioService};
pub use state::{
    Aggregate, AppAudioState, AudioRuntimeSnapshot, AudioState, DeviceId, OutputState,
};
