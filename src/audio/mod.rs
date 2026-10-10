//! Core Audio subsystem. All COM interfaces stay on the dedicated MTA worker.

mod applications;
pub mod controller;
pub mod devices;
pub mod notifications;
mod policy;
pub mod sessions;
pub mod state;

pub use controller::{AudioCommand, AudioService};
pub use state::{
    Aggregate, AppAudioState, AppVolumeState, AudioRuntimeSnapshot, AudioState, DeviceCycleFlow,
    DeviceCycleResult, DeviceId, OutputState,
};
