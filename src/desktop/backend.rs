//! The virtual desktop backend abstraction (spec §18). The keyboard subsystem
//! and UI never touch COM GUIDs — they see this trait and status types only.

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

/// Typed desktop operation failures (#20). The error CLASS decides policy:
/// only Shell/RPC unavailability may trigger the keyboard fallback; semantic
/// refusals (target out of range, unsupported build, ABI mismatch, switch
/// rejection) never inject synthetic keystrokes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopError {
    /// Requested desktop does not exist. Never falls back.
    TargetOutOfRange { requested: usize, count: usize },
    /// Backend cannot operate right now (not activated, safety limit).
    BackendUnavailable(String),
    /// Build not whitelisted — fail closed (spec §20).
    UnsupportedBuild(u32),
    /// Shell RPC dropped (Explorer restart): proxy rebuild + fallback allowed.
    RpcDisconnected,
    /// Interface layout mismatch against the pinned build contract.
    AbiMismatch(String),
    /// Shell rejected the switch with this HRESULT.
    SwitchFailed(i32),
}

impl DesktopError {
    /// Only Shell/RPC-level unavailability permits the keyboard fallback
    /// (after one proxy rebuild). Everything else refuses without input (#20).
    pub fn permits_fallback(&self) -> bool {
        matches!(
            self,
            DesktopError::RpcDisconnected | DesktopError::BackendUnavailable(_)
        )
    }
}

impl std::fmt::Display for DesktopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DesktopError::TargetOutOfRange { requested, count } => write!(
                f,
                "desktop {} does not exist (count {count})",
                requested + 1
            ),
            DesktopError::BackendUnavailable(reason) => write!(f, "backend unavailable: {reason}"),
            DesktopError::UnsupportedBuild(build) => write!(f, "unsupported build {build}"),
            DesktopError::RpcDisconnected => write!(f, "Shell RPC disconnected"),
            DesktopError::AbiMismatch(reason) => write!(f, "ABI mismatch: {reason}"),
            DesktopError::SwitchFailed(hr) => write!(f, "SwitchDesktop failed 0x{hr:08X}"),
        }
    }
}

impl std::error::Error for DesktopError {}

/// Backend contract. Implementations must be safe to call from the desktop
/// worker thread only; they serialize internally.
pub trait VirtualDesktopBackend {
    fn availability(&self) -> BackendAvailability;
    fn desktop_count(&self) -> std::result::Result<usize, DesktopError>;
    /// 0-based index of the current desktop.
    fn current_desktop(&self) -> std::result::Result<usize, DesktopError>;
    /// Switch to 0-based `index`. Returns Err(TargetOutOfRange) when the
    /// target doesn't exist.
    fn switch_to(&self, index: usize) -> std::result::Result<(), DesktopError>;
    fn kind(&self) -> BackendKind;
}
