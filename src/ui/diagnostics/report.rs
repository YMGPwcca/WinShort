//! Report for the diagnostics.

use super::model::Line;
use super::state::DiagnosticsUi;
use crate::diagnostics::snapshot::Health;

fn endpoint_value(endpoint: &crate::diagnostics::snapshot::AudioEndpointDiagnostics) -> String {
    let description = endpoint
        .description
        .as_deref()
        .map_or_else(|| "unresolved".into(), compact);
    format!(
        "{} · {} · {description}",
        endpoint.health.label(),
        endpoint.role
    )
}

pub(super) fn compact(value: &str) -> String {
    const MAX: usize = 92;
    let mut chars = value.chars();
    let compact: String = chars.by_ref().take(MAX).collect();
    if chars.next().is_some() {
        format!("{compact}…")
    } else {
        compact
    }
}

impl DiagnosticsUi {
    pub(super) fn lines(&self) -> Vec<Line> {
        let mut lines = Vec::new();
        append_application(&self.snapshot, &mut lines);
        append_keyboard(&self.snapshot, &mut lines);
        append_audio(&self.snapshot, &mut lines);
        append_virtual_desktops(&self.snapshot, &mut lines);
        append_config(&self.snapshot, &mut lines);
        append_overlay(&self.snapshot, &mut lines);
        append_startup_and_logging(&self.snapshot, &mut lines);
        append_degraded_subsystems(&self.snapshot, &mut lines, self.self_test.as_ref());
        lines
    }
}

fn section(lines: &mut Vec<Line>, name: &str) {
    lines.push(Line {
        section: true,
        key: name.into(),
        value: String::new(),
        health: None,
    });
}
fn row(lines: &mut Vec<Line>, key: &str, value: String, health: Option<Health>) {
    lines.push(Line {
        section: false,
        key: key.into(),
        value,
        health,
    });
}
fn append_application(
    s: &crate::diagnostics::snapshot::DiagnosticsSnapshot,
    lines: &mut Vec<Line>,
) {
    section(lines, "Application");
    row(lines, "Version", s.application.version.clone(), None);
    row(lines, "Profile", s.application.profile.clone(), None);
    row(
        lines,
        "Architecture",
        s.application.architecture.clone(),
        None,
    );
    row(
        lines,
        "Windows",
        match (s.windows.build, s.windows.update_revision) {
            (Some(build), Some(ubr)) => format!("Build {build}.{ubr}"),
            (Some(build), None) => format!("Build {build}.?"),
            _ => s
                .windows
                .error
                .clone()
                .unwrap_or_else(|| "Unavailable".into()),
        },
        s.windows.error.as_ref().map(|_| Health::Unavailable),
    );
    row(
        lines,
        "Windows architecture",
        s.windows.architecture.clone(),
        None,
    );
}

fn append_keyboard(s: &crate::diagnostics::snapshot::DiagnosticsSnapshot, lines: &mut Vec<Line>) {
    section(lines, "Keyboard");
    row(
        lines,
        "Hook",
        if s.keyboard.hook_active {
            "Active"
        } else {
            "Not active"
        }
        .into(),
        Some(s.keyboard.health),
    );
    row(
        lines,
        "Hotkeys",
        if s.keyboard.suspended {
            "Suspended"
        } else {
            "Enabled"
        }
        .into(),
        Some(if s.keyboard.suspended {
            Health::Warning
        } else {
            s.keyboard.health
        }),
    );
    row(
        lines,
        "Recorder",
        if s.keyboard.capture_active {
            "Capturing"
        } else {
            "Idle"
        }
        .into(),
        None,
    );
    for (name, binding) in &s.keyboard.bindings {
        row(lines, name, binding.clone(), None);
    }
    row(
        lines,
        "Win+1..9",
        if s.keyboard.reserved_win_numbers {
            "Reserved by desktop switching".into()
        } else {
            "Not reserved".into()
        },
        None,
    );
    row(
        lines,
        "Validation",
        if s.keyboard.conflicts.is_empty() {
            "No conflicts".into()
        } else {
            compact(&s.keyboard.conflicts.join(" | "))
        },
        Some(if s.keyboard.conflicts.is_empty() {
            Health::Healthy
        } else {
            Health::Error
        }),
    );
}

fn append_audio(s: &crate::diagnostics::snapshot::DiagnosticsSnapshot, lines: &mut Vec<Line>) {
    section(lines, "Audio");
    row(
        lines,
        "Input",
        endpoint_value(&s.audio.input),
        Some(s.audio.input.health),
    );
    row(
        lines,
        "Output",
        endpoint_value(&s.audio.output),
        Some(s.audio.output.health),
    );
    row(lines, "Microphone", s.audio.microphone_state.clone(), None);
    row(lines, "Output state", s.audio.output_state.clone(), None);
    row(
        lines,
        "Foreground audio",
        format!(
            "{} · {} sessions{}",
            s.audio.foreground.aggregate,
            s.audio.foreground.sessions,
            s.audio
                .foreground
                .app_name
                .as_deref()
                .map_or_else(String::new, |name| format!(" · {name}"))
        ),
        Some(s.audio.foreground.health),
    );
    if let Some(error) = &s.audio.foreground.error {
        row(
            lines,
            "Foreground result",
            compact(error),
            Some(Health::Error),
        );
    }
}

fn append_virtual_desktops(
    s: &crate::diagnostics::snapshot::DiagnosticsSnapshot,
    lines: &mut Vec<Line>,
) {
    section(lines, "Virtual desktops");
    row(
        lines,
        "Native backend",
        s.desktop.native.clone(),
        Some(s.desktop.health),
    );
    row(lines, "Fallback", s.desktop.fallback.clone(), None);
    row(lines, "Active backend", s.desktop.active.clone(), None);
    row(lines, "Last served", s.desktop.last_served.clone(), None);
    row(
        lines,
        "Desktop count",
        s.desktop
            .desktop_count
            .map_or_else(|| "Unavailable".into(), |count| count.to_string()),
        None,
    );
    row(
        lines,
        "Detected build",
        match (s.desktop.build, s.desktop.update_revision) {
            (Some(build), Some(ubr)) => format!("{build}.{ubr}"),
            (Some(build), None) => format!("{build}.?"),
            _ => "Unavailable".into(),
        },
        None,
    );
    if let Some(error) = &s.desktop.error {
        row(
            lines,
            "Native detail",
            compact(error),
            Some(Health::Warning),
        );
    }
}

fn append_config(s: &crate::diagnostics::snapshot::DiagnosticsSnapshot, lines: &mut Vec<Line>) {
    section(lines, "Config");
    row(
        lines,
        "Status",
        s.config.health.label().into(),
        Some(s.config.health),
    );
    row(
        lines,
        "Path",
        compact(&s.config.path.to_string_lossy()),
        None,
    );
    row(
        lines,
        "Source schema",
        s.config
            .source_schema_version
            .map_or_else(|| "Not present".into(), |version| version.to_string()),
        None,
    );
    row(
        lines,
        "Effective schema",
        s.config.effective_schema_version.to_string(),
        None,
    );
    row(
        lines,
        "Future-schema latch",
        if s.config.read_only {
            "Read-only"
        } else {
            "Not active"
        }
        .into(),
        Some(if s.config.read_only {
            Health::Error
        } else {
            Health::Healthy
        }),
    );
    row(
        lines,
        "Configured hotkeys",
        s.config.hotkey_count.to_string(),
        None,
    );
    row(
        lines,
        "Load warnings",
        s.config.warnings.len().to_string(),
        None,
    );
    row(
        lines,
        "Repaired fields",
        s.config.repaired_fields.len().to_string(),
        None,
    );
    row(
        lines,
        "Migrations",
        s.config.migrations.len().to_string(),
        None,
    );
    row(
        lines,
        "Validation",
        if s.config.validation.is_empty() {
            "Valid".into()
        } else {
            compact(&s.config.validation.join(" | "))
        },
        Some(if s.config.validation.is_empty() {
            Health::Healthy
        } else {
            Health::Error
        }),
    );
}

fn append_overlay(s: &crate::diagnostics::snapshot::DiagnosticsSnapshot, lines: &mut Vec<Line>) {
    section(lines, "Overlay");
    row(
        lines,
        "Status",
        s.overlay.health.label().into(),
        Some(s.overlay.health),
    );
    row(
        lines,
        "Enabled",
        if s.overlay.enabled { "Yes" } else { "No" }.into(),
        None,
    );
    row(
        lines,
        "Active cards",
        s.overlay.active_card_count.to_string(),
        None,
    );
    row(
        lines,
        "Permanent cards",
        s.overlay.permanent_card_count.to_string(),
        None,
    );
    row(
        lines,
        "Toast cards",
        s.overlay.toast_card_count.to_string(),
        None,
    );
    row(lines, "Blur", s.overlay.blur.clone(), None);
    row(
        lines,
        "Resolved appearance",
        s.overlay
            .resolved_appearance
            .clone()
            .unwrap_or_else(|| "Unknown".into()),
        None,
    );
    row(
        lines,
        "Microphone notifications",
        if s.overlay.notifications.microphone {
            "Shown"
        } else {
            "Hidden"
        }
        .into(),
        None,
    );
    row(
        lines,
        "Speaker notifications",
        if s.overlay.notifications.speaker {
            "Shown"
        } else {
            "Hidden"
        }
        .into(),
        None,
    );
    row(
        lines,
        "Current app audio notifications",
        if s.overlay.notifications.current_app_audio {
            "Shown"
        } else {
            "Hidden"
        }
        .into(),
        None,
    );
    row(
        lines,
        "Workspace notifications",
        if s.overlay.notifications.workspace {
            "Shown"
        } else {
            "Hidden"
        }
        .into(),
        None,
    );
    row(
        lines,
        "Display profile notifications",
        if s.overlay.notifications.display_profile {
            "Shown"
        } else {
            "Hidden"
        }
        .into(),
        None,
    );
    row(
        lines,
        "Animations",
        s.overlay.animations_enabled.map_or_else(
            || "Unknown".into(),
            |enabled| {
                if enabled {
                    "Enabled".into()
                } else {
                    "Reduced".into()
                }
            },
        ),
        None,
    );
    row(
        lines,
        "High contrast",
        s.overlay.high_contrast.map_or_else(
            || "Unknown".into(),
            |enabled| {
                if enabled {
                    "Enabled".into()
                } else {
                    "Off".into()
                }
            },
        ),
        None,
    );
    row(
        lines,
        "Overlapped content",
        s.overlay.disable_overlapped_content.map_or_else(
            || "Unknown".into(),
            |disabled| {
                if disabled {
                    "Disabled".into()
                } else {
                    "Allowed".into()
                }
            },
        ),
        None,
    );
    row(lines, "Position", s.overlay.position.clone(), None);
    row(
        lines,
        "Monitor selector",
        s.overlay.monitor_selector.clone(),
        None,
    );
    row(
        lines,
        "Last target monitor",
        s.overlay
            .target_monitor
            .clone()
            .unwrap_or_else(|| "Never".into()),
        None,
    );
    row(
        lines,
        "Active monitor summary",
        s.overlay
            .active_monitor_summary
            .clone()
            .unwrap_or_else(|| "None".into()),
        None,
    );
    row(
        lines,
        "Last render DPI",
        s.overlay
            .render_dpi
            .map_or_else(|| "Never".into(), |dpi| dpi.to_string()),
        None,
    );
    row(
        lines,
        "Last shown",
        if s.overlay.last_shown.is_some() {
            "Available".into()
        } else {
            "Never".into()
        },
        None,
    );
}

fn append_startup_and_logging(
    s: &crate::diagnostics::snapshot::DiagnosticsSnapshot,
    lines: &mut Vec<Line>,
) {
    section(lines, "Startup and logging");
    row(
        lines,
        "Startup registration",
        s.startup.state.clone(),
        Some(s.startup.health),
    );
    if let Some(command) = &s.startup.registered_command {
        row(lines, "Registered command", compact(command), None);
    }
    row(
        lines,
        "Logging",
        s.logging.health.label().into(),
        Some(s.logging.health),
    );
    row(
        lines,
        "Log directory",
        s.logging.directory.as_ref().map_or_else(
            || "Unavailable".into(),
            |path| compact(&path.to_string_lossy()),
        ),
        None,
    );
    row(lines, "Log level", s.logging.level.clone(), None);
    row(
        lines,
        "Default log level",
        s.logging.default_level.clone(),
        None,
    );
    row(
        lines,
        "Temporary debug",
        if s.logging.temporary_debug {
            "Enabled"
        } else {
            "Off"
        }
        .into(),
        None,
    );
    row(
        lines,
        "Retention",
        format!("{} days", s.logging.retention_days),
        None,
    );
    row(lines, "Buffering", s.logging.buffering.clone(), None);
    if let Some(error) = &s.startup.error {
        row(lines, "Startup detail", compact(error), Some(Health::Error));
    }
    row(
        lines,
        "Current log file",
        s.logging.current_file.as_ref().map_or_else(
            || "Unavailable".into(),
            |path| compact(&path.to_string_lossy()),
        ),
        None,
    );
}

fn append_degraded_subsystems(
    s: &crate::diagnostics::snapshot::DiagnosticsSnapshot,
    lines: &mut Vec<Line>,
    self_test: Option<&crate::diagnostics::snapshot::SelfTestReport>,
) {
    section(lines, "Degraded subsystems");
    if s.degraded.is_empty() {
        row(
            lines,
            "Runtime",
            "No degraded subsystems".into(),
            Some(Health::Healthy),
        );
    } else {
        for degraded in &s.degraded {
            row(
                lines,
                &degraded.name,
                compact(&degraded.reason),
                Some(Health::Unavailable),
            );
        }
    }

    if let Some(report) = self_test {
        section(lines, "Self-test");
        row(lines, "Summary", report.summary(), None);
        for check in &report.checks {
            row(
                lines,
                &check.name,
                compact(&check.detail),
                Some(check.health),
            );
        }
    }
}
