//! Audio state types shared across threads (main thread consumes; audio
//! worker produces). Pure data — no COM.

/// Identity of an audio endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceId {
    /// Windows endpoint id string.
    pub endpoint: String,
    /// Friendly name from the property store.
    pub name: String,
}

impl DeviceId {
    pub fn default_device() -> Self {
        Self {
            endpoint: "default".into(),
            name: "Default device".into(),
        }
    }
}

/// Microphone / capture state.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioState {
    Unavailable { reason: String },
    Muted { volume_pct: u8 },
    Active { volume_pct: u8 },
}

impl AudioState {
    pub fn short_label(&self) -> String {
        match self {
            AudioState::Unavailable { .. } => "Unavailable".into(),
            AudioState::Muted { .. } => "Muted".into(),
            AudioState::Active { volume_pct } => format!("{volume_pct}%"),
        }
    }

    pub fn is_muted(&self) -> bool {
        matches!(self, AudioState::Muted { .. })
    }
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
    pub fn no_session() -> Self {
        Self {
            app_name: None,
            aggregate: Aggregate::NoSession,
            sessions: 0,
            error: None,
        }
    }

    pub fn no_external() -> Self {
        Self {
            app_name: None,
            aggregate: Aggregate::NoExternalApp,
            sessions: 0,
            error: None,
        }
    }

    pub fn status_label(&self) -> &'static str {
        match self.aggregate {
            Aggregate::NoSession => "No audio session",
            Aggregate::AllMuted => "Muted",
            Aggregate::AllActive => "Active",
            Aggregate::Mixed => "Mixed",
            Aggregate::NoExternalApp => "No external application selected",
            Aggregate::Error => "Error",
        }
    }
}
