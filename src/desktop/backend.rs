//! The virtual desktop backend abstraction (spec §18). The keyboard subsystem
//! and UI never touch COM GUIDs — they see this trait and status types only.

use crate::error::Result;

/// Which backend implementation is serving switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Undocumented Shell COM interfaces (build-pinned layout).
    NativeShell,
    /// SendInput Ctrl+Win+Arrow relative walking.
    KeyboardFallback,
}

impl BackendKind {
    pub fn label(self) -> &'static str {
        match self {
            BackendKind::NativeShell => "Native Shell",
            BackendKind::KeyboardFallback => "Keyboard fallback",
        }
    }
}

/// Availability of a backend, with a human-readable reason when unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendAvailability {
    Available,
    /// Known build but the Shell refused/dropped the interface.
    Failed { reason: String },
    /// Build not on the compatibility whitelist — fail closed (spec §20).
    UnsupportedBuild { build: u32 },
}

impl BackendAvailability {
    pub fn label(&self) -> String {
        match self {
            BackendAvailability::Available => "Available".into(),
            BackendAvailability::Failed { reason } => format!("Failed: {reason}"),
            BackendAvailability::UnsupportedBuild { build } => {
                format!("Unsupported build {build}")
            }
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, BackendAvailability::Available)
    }
}

/// Published status for the settings Advanced page and tray diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendStatus {
    pub native: BackendAvailability,
    pub fallback: BackendAvailability,
    pub active: BackendKind,
    pub desktop_count: Option<usize>,
}

/// Backend contract. Implementations must be safe to call from the desktop
/// worker thread only; they serialize internally.
pub trait VirtualDesktopBackend {
    fn availability(&self) -> BackendAvailability;
    fn desktop_count(&self) -> Result<usize>;
    /// 0-based index of the current desktop.
    fn current_desktop(&self) -> Result<usize>;
    /// Switch to 0-based `index`. Returns Err when the target doesn't exist.
    fn switch_to(&self, index: usize) -> Result<()>;
    fn kind(&self) -> BackendKind;
}
