//! Immutable diagnostics data collected on the main thread from cached runtime state.
//!
//! This module deliberately contains no Win32 drawing code. UI rendering and support
//! export consume the same copied snapshot, while worker-thread COM interfaces remain
//! owned by their original apartments.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::audio::state::Aggregate;
use crate::config::model::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Healthy,
    Warning,
    Unavailable,
    Error,
}

impl Health {
    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "Healthy",
            Self::Warning => "Warning",
            Self::Unavailable => "Unavailable",
            Self::Error => "Error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApplicationDiagnostics {
    pub version: String,
    pub profile: String,
    pub architecture: String,
}

#[derive(Debug, Clone)]
pub struct WindowsDiagnostics {
    pub architecture: String,
    pub build: Option<u32>,
    pub update_revision: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KeyboardDiagnostics {
    pub health: Health,
    pub installed: bool,
    pub hook_active: bool,
    pub suspended: bool,
    pub capture_active: bool,
    pub bindings: Vec<(String, String)>,
    pub conflicts: Vec<String>,
    pub reserved_win_numbers: bool,
}

#[derive(Debug, Clone)]
pub struct AudioEndpointDiagnostics {
    pub selector: String,
    pub role: String,
    pub health: Health,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ForegroundAudioDiagnostics {
    pub health: Health,
    pub aggregate: String,
    pub app_name: Option<String>,
    pub sessions: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AudioDiagnostics {
    pub input: AudioEndpointDiagnostics,
    pub output: AudioEndpointDiagnostics,
    pub microphone_state: String,
    pub output_state: String,
    pub foreground: ForegroundAudioDiagnostics,
}

#[derive(Debug, Clone)]
pub struct DesktopDiagnostics {
    pub health: Health,
    pub native: String,
    pub fallback: String,
    pub active: String,
    pub last_served: String,
    pub desktop_count: Option<usize>,
    pub build: Option<u32>,
    pub update_revision: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConfigDiagnostics {
    pub health: Health,
    pub path: PathBuf,
    pub schema_version: u8,
    pub read_only: bool,
    pub warnings: Vec<String>,
    pub repaired_fields: Vec<String>,
    pub migrations: Vec<String>,
    pub validation: Vec<String>,
    pub hotkey_count: usize,
    pub(crate) raw: Config,
}

#[derive(Debug, Clone)]
pub struct OverlayDiagnostics {
    pub health: Health,
    pub enabled: bool,
    pub position: String,
    pub monitor_selector: String,
    pub target_monitor: Option<String>,
    pub render_dpi: Option<u32>,
    pub last_shown: Option<SystemTime>,
    pub window_available: bool,
}

#[derive(Debug, Clone)]
pub struct StartupDiagnostics {
    pub health: Health,
    pub state: String,
    pub registered_command: Option<String>,
    pub current_command: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LoggingDiagnostics {
    pub health: Health,
    pub directory: Option<PathBuf>,
    pub current_file: Option<PathBuf>,
    pub level: String,
}

#[derive(Debug, Clone)]
pub struct DegradedSubsystem {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct DiagnosticsSnapshot {
    pub generated_at: SystemTime,
    pub application: ApplicationDiagnostics,
    pub windows: WindowsDiagnostics,
    pub keyboard: KeyboardDiagnostics,
    pub audio: AudioDiagnostics,
    pub desktop: DesktopDiagnostics,
    pub config: ConfigDiagnostics,
    pub overlay: OverlayDiagnostics,
    pub startup: StartupDiagnostics,
    pub logging: LoggingDiagnostics,
    pub degraded: Vec<DegradedSubsystem>,
}

#[derive(Debug, Clone)]
pub struct SelfTestCheck {
    pub name: String,
    pub health: Health,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct SelfTestReport {
    pub checks: Vec<SelfTestCheck>,
}
impl SelfTestReport {
    pub fn summary(&self) -> String {
        let failed = self
            .checks
            .iter()
            .filter(|check| matches!(check.health, Health::Error | Health::Unavailable))
            .count();
        let warnings = self
            .checks
            .iter()
            .filter(|check| check.health == Health::Warning)
            .count();
        if failed > 0 {
            format!("Self-test: {failed} failed, {warnings} warning(s)")
        } else if warnings > 0 {
            format!("Self-test: {warnings} warning(s), no failures")
        } else {
            format!("Self-test: {} checks passed", self.checks.len())
        }
    }
}

pub fn run_self_test(snapshot: &DiagnosticsSnapshot) -> SelfTestReport {
    let mut checks = Vec::with_capacity(7);
    checks.push(SelfTestCheck {
        name: "Keyboard hook".into(),
        health: snapshot.keyboard.health,
        detail: if snapshot.keyboard.installed && snapshot.keyboard.hook_active {
            if snapshot.keyboard.suspended {
                "installed; currently suspended".into()
            } else {
                "installed and active".into()
            }
        } else {
            "keyboard service is not active".into()
        },
    });
    checks.push(SelfTestCheck {
        name: "Configuration".into(),
        health: snapshot.config.health,
        detail: if snapshot.config.read_only {
            "future-schema read-only latch is active".into()
        } else if !snapshot.config.warnings.is_empty() {
            format!("{} load warning(s)", snapshot.config.warnings.len())
        } else {
            "loaded without warnings".into()
        },
    });
    checks.push(SelfTestCheck {
        name: "Input endpoint".into(),
        health: snapshot.audio.input.health,
        detail: snapshot
            .audio
            .input
            .description
            .clone()
            .unwrap_or_else(|| "no resolved capture endpoint".into()),
    });
    checks.push(SelfTestCheck {
        name: "Output endpoint".into(),
        health: snapshot.audio.output.health,
        detail: snapshot
            .audio
            .output
            .description
            .clone()
            .unwrap_or_else(|| "no resolved render endpoint".into()),
    });
    checks.push(SelfTestCheck {
        name: "Virtual desktops".into(),
        health: snapshot.desktop.health,
        detail: format!(
            "{}; active {}; {}",
            snapshot.desktop.native,
            snapshot.desktop.active,
            snapshot.desktop.desktop_count.map_or_else(
                || "count unavailable".into(),
                |count| format!("{count} desktops")
            )
        ),
    });
    checks.push(SelfTestCheck {
        name: "Overlay window".into(),
        health: snapshot.overlay.health,
        detail: if snapshot.overlay.window_available {
            "overlay window is available; no overlay was shown".into()
        } else {
            "overlay window is unavailable".into()
        },
    });
    let log_detail = match &snapshot.logging.directory {
        Some(directory) if directory.is_dir() => "log directory is readable".into(),
        Some(directory) => format!("log directory is unavailable: {}", directory.display()),
        None => "logger has no writable directory".into(),
    };
    checks.push(SelfTestCheck {
        name: "Logging".into(),
        health: snapshot.logging.health,
        detail: log_detail,
    });
    SelfTestReport { checks }
}

pub fn aggregate_label(aggregate: Aggregate) -> &'static str {
    match aggregate {
        Aggregate::NoSession => "No session",
        Aggregate::AllMuted => "All muted",
        Aggregate::AllActive => "All active",
        Aggregate::Mixed => "Mixed",
        Aggregate::NoExternalApp => "No external app",
        Aggregate::Error => "Error",
    }
}
