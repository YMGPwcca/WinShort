//! Audio state types shared across threads (main thread consumes; audio
//! worker produces). Pure data — no COM.

use crate::config::model::DeviceSelection;
/// Identity of an audio endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceId {
    /// Windows endpoint id string.
    pub endpoint: String,
    /// Friendly name from the property store.
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceCycleFlow {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCycleResult {
    Changed {
        flow: DeviceCycleFlow,
        previous: DeviceSelection,
        selection: DeviceSelection,
        device: Option<DeviceId>,
    },
    NoDevices {
        flow: DeviceCycleFlow,
        previous: DeviceSelection,
    },
}

/// Worker-published endpoint binding identity for diagnostics. This is the
/// result of `GetDefaultAudioEndpoint` or explicit endpoint resolution; it is
/// never inferred from the sorted inventory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioRuntimeSnapshot {
    pub capture: Option<DeviceId>,
    pub render: Option<DeviceId>,
    pub capture_error: Option<String>,
    pub render_error: Option<String>,
}

/// Microphone / capture state.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioState {
    Unavailable { reason: String },
    Muted { volume_pct: u8 },
    Active { volume_pct: u8 },
}

/// Output / render state. Transient "output changed" presentation is carried
/// by the dedicated [`crate::event::AppEvent::DefaultOutputChanged`] event
/// instead of a persistent-state variant (#17b).
#[derive(Debug, Clone, PartialEq)]
pub enum OutputState {
    Unavailable {
        reason: String,
    },
    Current {
        device: DeviceId,
        muted: bool,
        volume_pct: u8,
    },
}

/// Aggregate mute state across all sessions of one process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    /// Process has no live audio session.
    NoSession,
    /// Every matched session is muted.
    AllMuted,
    /// Every matched session is unmuted.
    AllActive,
    /// Mixed: toggle deterministically mutes everything.
    Mixed,
    /// No external foreground application tracked.
    NoExternalApp,
    /// Operational failure (query/toggle error) — never reported as
    /// NoSession (#17d).
    Error,
}

/// Foreground application audio result for the overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct AppAudioState {
    pub app_name: Option<String>,
    pub aggregate: Aggregate,
    /// Session count that was toggled (for diagnostics display).
    pub sessions: usize,
    /// Reason when [`Aggregate::Error`] (#17d).
    pub error: Option<String>,
}

impl AppAudioState {
    pub fn no_external() -> Self {
        Self {
            app_name: None,
            aggregate: Aggregate::NoExternalApp,
            sessions: 0,
            error: None,
        }
    }
}

/// Foreground application volume after a volume adjustment or query.
///
/// `min_volume_pct` and `max_volume_pct` are absent when no matching session
/// volume could be read; a range is retained instead of inventing an average.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppVolumeState {
    pub app_name: Option<String>,
    pub sessions: usize,
    pub min_volume_pct: Option<u8>,
    pub max_volume_pct: Option<u8>,
    pub error: Option<String>,
}

impl AppVolumeState {
    pub fn no_external() -> Self {
        Self {
            app_name: None,
            sessions: 0,
            min_volume_pct: None,
            max_volume_pct: None,
            error: None,
        }
    }

    pub fn no_session(app_name: Option<String>) -> Self {
        Self {
            app_name,
            sessions: 0,
            min_volume_pct: None,
            max_volume_pct: None,
            error: None,
        }
    }

    pub fn error(app_name: Option<String>, reason: impl Into<String>) -> Self {
        Self {
            app_name,
            sessions: 0,
            min_volume_pct: None,
            max_volume_pct: None,
            error: Some(reason.into()),
        }
    }
}
