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
    Failed {
        reason: String,
    },
    /// Build not on the compatibility whitelist — fail closed (spec §20).
    UnsupportedBuild {
        build: u32,
    },
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
}

/// Published status for the settings Advanced page and tray diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendStatus {
    pub native: BackendAvailability,
    pub fallback: BackendAvailability,
    pub active: BackendKind,
    pub desktop_count: Option<usize>,
    /// Backend that actually completed the most recent switch, including via
    /// fallback (#21). `None` until the first successful switch.
    pub last_served: Option<BackendKind>,
}

/// Typed desktop operation failures (#20). The error CLASS decides policy:
/// only Shell/RPC unavailability may trigger the keyboard fallback; semantic
/// refusals (target out of range, unsupported build, ABI mismatch, switch
/// rejection, creation/move/navigation limitations) never inject synthetic
/// keystrokes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopError {
    /// Requested desktop does not exist after a bounded ensure attempt.
    TargetOutOfRange { requested: usize, count: usize },
    /// Backend cannot operate right now (not activated, safety limit).
    BackendUnavailable(String),
    /// Build not whitelisted — fail closed (spec §20). Produced by
    /// `detect()` gating; constructed directly only in policy_tests.
    #[allow(dead_code)]
    UnsupportedBuild(u32),
    /// Shell RPC dropped (Explorer restart): proxy rebuild + fallback allowed.
    RpcDisconnected,
    /// Interface layout mismatch against the pinned build contract.
    /// Currently surfaced via `BackendUnavailable`; retained for typed
    /// matching and exercised by policy_tests.
    #[allow(dead_code)]
    AbiMismatch(String),
    /// Shell rejected the switch with this HRESULT.
    SwitchFailed(i32),
    /// Creating missing desktops requires the native backend.
    CreationUnavailable(String),
    /// Moving a top-level window requires the native window manager.
    MoveUnavailable(String),
    /// A remembered or requested HWND is no longer eligible.
    WindowUnavailable(String),
    /// Foreground activation was rejected after a move.
    FocusFailed(String),
    /// A native operation completed part of a multi-step action, but a later
    /// step failed. This is never eligible for keyboard fallback.
    Partial { completed: String, failure: String },
    /// Previous-desktop state cannot be resolved safely.
    NavigationUnavailable(String),
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
            DesktopError::CreationUnavailable(reason) => {
                write!(f, "desktop creation unavailable: {reason}")
            }
            DesktopError::MoveUnavailable(reason) => {
                write!(f, "desktop move unavailable: {reason}")
            }
            DesktopError::WindowUnavailable(reason) => write!(f, "window unavailable: {reason}"),
            DesktopError::FocusFailed(reason) => {
                write!(f, "foreground activation failed: {reason}")
            }
            DesktopError::Partial { completed, failure } => {
                write!(f, "partial desktop action: {completed}; {failure}")
            }
            DesktopError::NavigationUnavailable(reason) => {
                write!(f, "desktop navigation unavailable: {reason}")
            }
        }
    }
}

impl std::error::Error for DesktopError {}

/// Backend contract. Implementations must be safe to call from the desktop
/// worker thread only; they serialize internally.
pub trait VirtualDesktopBackend {
    fn desktop_count(&self) -> std::result::Result<usize, DesktopError>;
    /// 0-based index of the current desktop.
    fn current_desktop(&self) -> std::result::Result<usize, DesktopError>;
    /// Switch to 0-based `index`. Returns Err(TargetOutOfRange) when the
    /// target doesn't exist.
    fn switch_to(&self, index: usize) -> std::result::Result<(), DesktopError>;
}
