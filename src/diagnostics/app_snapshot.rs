//! Diagnostics snapshot construction from cached application state.

use crate::audio::state::Aggregate;
use crate::desktop::{BackendAvailability, BackendKind};
use crate::diagnostics::snapshot::{
    aggregate_label, ApplicationDiagnostics, AudioDiagnostics, ConfigDiagnostics,
    DegradedSubsystem, DesktopDiagnostics, DiagnosticsSnapshot, ForegroundAudioDiagnostics, Health,
    KeyboardDiagnostics, LoggingDiagnostics, OverlayDiagnostics, StartupDiagnostics,
    WindowsDiagnostics,
};

pub(crate) struct SnapshotInputs {
    pub config: crate::config::Config,
    pub audio_runtime: crate::audio::AudioRuntimeSnapshot,
    pub microphone_state: crate::audio::AudioState,
    pub output_state: crate::audio::OutputState,
    pub foreground_state: crate::audio::AppAudioState,
    pub keyboard_installed: bool,
    pub keyboard_hook_active: bool,
    pub keyboard_capture_active: bool,
    pub suspended: bool,
    pub desktop_installed: bool,
    pub desktop_status: crate::desktop::BackendStatus,
    pub overlay_status: crate::ui::overlay::OverlayRuntimeStatus,
    pub degraded: Vec<(String, String)>,
}

pub(crate) fn build(inputs: SnapshotInputs) -> DiagnosticsSnapshot {
    let validation_messages = validation_messages(&inputs.config);
    let windows = build_windows();
    let audio = build_audio(&inputs);
    let keyboard = build_keyboard(&inputs, validation_messages.clone());
    let desktop = build_desktop(&inputs, &windows);
    let overlay = build_overlay(&inputs);
    let startup = build_startup();
    let logging = build_logging();
    let degraded = build_degraded(inputs.degraded);
    let config = build_config(inputs.config, validation_messages);
    DiagnosticsSnapshot {
        generated_at: std::time::SystemTime::now(),
        application: build_application(),
        windows,
        keyboard,
        audio,
        desktop,
        config,
        overlay,
        startup,
        logging,
        degraded,
    }
}

fn validation_messages(config: &crate::config::Config) -> Vec<String> {
    crate::config::validate(config)
        .iter()
        .map(|violation| format!("{}: {}", violation.field, violation.message))
        .collect()
}

fn build_application() -> ApplicationDiagnostics {
    ApplicationDiagnostics {
        version: env!("CARGO_PKG_VERSION").into(),
        profile: if cfg!(debug_assertions) {
            "debug".into()
        } else {
            "release".into()
        },
        architecture: std::env::consts::ARCH.into(),
    }
}

fn build_windows() -> WindowsDiagnostics {
    match crate::desktop::detect::detect() {
        Ok(value) => WindowsDiagnostics {
            architecture: std::env::consts::ARCH.into(),
            build: Some(value.build),
            update_revision: Some(value.update_revision),
            error: None,
        },
        Err(error) => WindowsDiagnostics {
            architecture: std::env::consts::ARCH.into(),
            build: None,
            update_revision: None,
            error: Some(error.to_string()),
        },
    }
}

fn build_audio(inputs: &SnapshotInputs) -> AudioDiagnostics {
    let input = crate::diagnostics::snapshot::endpoint_diagnostic(
        &inputs.config.audio.input_device,
        inputs.config.audio.input_role.label(),
        inputs.audio_runtime.capture.as_ref(),
        inputs.audio_runtime.capture_error.as_deref(),
    );
    let output = crate::diagnostics::snapshot::endpoint_diagnostic(
        &inputs.config.audio.output_device,
        inputs.config.audio.output_role.label(),
        inputs.audio_runtime.render.as_ref(),
        inputs.audio_runtime.render_error.as_deref(),
    );
    let foreground_health = match inputs.foreground_state.aggregate {
        Aggregate::Error => Health::Error,
        Aggregate::NoExternalApp | Aggregate::NoSession => Health::Warning,
        _ => Health::Healthy,
    };
    AudioDiagnostics {
        input,
        output,
        microphone_state: audio_state_label(&inputs.microphone_state),
        output_state: output_state_label(&inputs.output_state),
        foreground: ForegroundAudioDiagnostics {
            health: foreground_health,
            aggregate: aggregate_label(inputs.foreground_state.aggregate).into(),
            app_name: inputs.foreground_state.app_name.clone(),
            sessions: inputs.foreground_state.sessions,
            error: inputs.foreground_state.error.clone(),
        },
    }
}

fn build_keyboard(inputs: &SnapshotInputs, conflicts: Vec<String>) -> KeyboardDiagnostics {
    let health = if !inputs.keyboard_installed {
        Health::Unavailable
    } else if inputs.suspended {
        Health::Warning
    } else if !inputs.keyboard_hook_active {
        Health::Error
    } else {
        Health::Healthy
    };
    let hotkeys = &inputs.config.hotkeys;
    let mut bindings = vec![
        ("Microphone".into(), hotkey_label(hotkeys.toggle_microphone)),
        ("Output".into(), hotkey_label(hotkeys.toggle_output)),
        (
            "Foreground app".into(),
            hotkey_label(hotkeys.toggle_foreground_audio),
        ),
        (
            "Cycle input device".into(),
            hotkey_label(hotkeys.cycle_input_device),
        ),
        (
            "Cycle output device".into(),
            hotkey_label(hotkeys.cycle_output_device),
        ),
        (
            "Foreground volume up".into(),
            hotkey_label(hotkeys.foreground_volume_up),
        ),
        (
            "Foreground volume down".into(),
            hotkey_label(hotkeys.foreground_volume_down),
        ),
    ];
    bindings.extend(hotkeys.display_profiles.iter().map(|binding| {
        (
            format!("Display profile {}", binding.profile_id),
            binding.hotkey.to_string(),
        )
    }));
    KeyboardDiagnostics {
        health,
        installed: inputs.keyboard_installed,
        hook_active: inputs.keyboard_hook_active,
        suspended: inputs.suspended,
        capture_active: inputs.keyboard_capture_active,
        bindings,
        conflicts,
        reserved_win_numbers: inputs.config.virtual_desktops.enabled
            && inputs.config.virtual_desktops.win_number_switching,
    }
}

fn hotkey_label(value: Option<crate::keyboard::binding::Hotkey>) -> String {
    value.map_or_else(|| "Not assigned".into(), |hotkey| hotkey.to_string())
}

fn build_desktop(inputs: &SnapshotInputs, windows: &WindowsDiagnostics) -> DesktopDiagnostics {
    let error = match &inputs.desktop_status.native {
        BackendAvailability::Available => None,
        BackendAvailability::Failed { reason } => Some(reason.clone()),
        BackendAvailability::UnsupportedBuild { build } => {
            Some(format!("unsupported build {build}"))
        }
    };
    let health = if !inputs.desktop_installed {
        Health::Unavailable
    } else if matches!(inputs.desktop_status.native, BackendAvailability::Available) {
        Health::Healthy
    } else {
        Health::Warning
    };
    DesktopDiagnostics {
        health,
        native: inputs.desktop_status.native.label(),
        fallback: inputs.desktop_status.fallback.label(),
        active: inputs.desktop_status.active.label().into(),
        last_served: inputs
            .desktop_status
            .last_served
            .map(BackendKind::label)
            .unwrap_or("none yet")
            .into(),
        desktop_count: inputs.desktop_status.desktop_count,
        build: windows.build,
        update_revision: windows.update_revision,
        error,
    }
}

fn build_overlay(inputs: &SnapshotInputs) -> OverlayDiagnostics {
    let status = &inputs.overlay_status;
    OverlayDiagnostics {
        health: if status.window_available {
            Health::Healthy
        } else {
            Health::Unavailable
        },
        enabled: inputs.config.overlay.enabled,
        active_card_count: status.active_card_count,
        permanent_card_count: status.permanent_card_count,
        toast_card_count: status.toast_card_count,
        active_monitor_summary: status.active_monitor_summary.clone(),
        blur: inputs.config.overlay.blur.as_str().into(),
        appearance: inputs.config.overlay.appearance.as_str().into(),
        resolved_appearance: status.resolved_appearance.clone(),
        notifications: inputs.config.overlay.notifications,
        animations_enabled: status.animations_enabled,
        high_contrast: status.high_contrast,
        disable_overlapped_content: status.disable_overlapped_content,
        position: inputs.config.overlay.position.label().into(),
        monitor_selector: inputs.config.overlay.monitor.as_str(),
        target_monitor: status.target_monitor.clone(),
        render_dpi: status.render_dpi,
        last_shown: status.last_shown,
        window_available: status.window_available,
    }
}

fn build_startup() -> StartupDiagnostics {
    let details = crate::platform::startup::details();
    let (state, health) = match &details.state {
        crate::platform::startup::StartupState::Enabled => ("Enabled".into(), Health::Healthy),
        crate::platform::startup::StartupState::Disabled => ("Disabled".into(), Health::Healthy),
        crate::platform::startup::StartupState::Stale { .. } => ("Stale".into(), Health::Warning),
    };
    StartupDiagnostics {
        health: if details.error.is_some() {
            Health::Error
        } else {
            health
        },
        state,
        registered_command: details.registered_command,
        current_command: details.current_command,
        error: details.error,
    }
}

fn build_logging() -> LoggingDiagnostics {
    let logger = crate::diagnostics::logging::info();
    LoggingDiagnostics {
        health: if logger
            .as_ref()
            .and_then(|value| value.directory.as_ref())
            .is_some()
        {
            Health::Healthy
        } else {
            Health::Unavailable
        },
        directory: logger.as_ref().and_then(|value| value.directory.clone()),
        current_file: logger
            .as_ref()
            .and_then(|value| value.current_file.clone())
            .or_else(crate::diagnostics::logging::current_log_path),
        level: logger.as_ref().map_or_else(
            || "unavailable".into(),
            |value| {
                if value.temporary_debug {
                    format!("{} (temporary)", value.level.as_str())
                } else {
                    value.level.as_str().into()
                }
            },
        ),
        default_level: logger.as_ref().map_or_else(
            || "unavailable".into(),
            |value| value.default_level.as_str().into(),
        ),
        temporary_debug: logger.as_ref().is_some_and(|value| value.temporary_debug),
        retention_days: logger
            .as_ref()
            .map_or(crate::diagnostics::logging::LOG_RETENTION_DAYS, |value| {
                value.retention_days
            }),
        buffering: logger.as_ref().map_or_else(
            || "unavailable".into(),
            |value| {
                if value.buffered {
                    format!(
                        "BufWriter; Warn/Error + {}s dirty flush",
                        crate::diagnostics::logging::FLUSH_TIMER_MS / 1000
                    )
                } else {
                    "unbuffered".into()
                }
            },
        ),
    }
}

fn build_config(config: crate::config::Config, validation: Vec<String>) -> ConfigDiagnostics {
    let load = crate::config::load_diagnostics();
    let read_only = crate::config::config_readonly();
    let health = if load
        .source_schema_version
        .is_some_and(|version| version > crate::config::model::CURRENT_SCHEMA_VERSION)
        || read_only
    {
        Health::Error
    } else if !load.warnings.is_empty() || !load.repaired_fields.is_empty() {
        Health::Warning
    } else if !validation.is_empty() {
        Health::Error
    } else {
        Health::Healthy
    };
    let hotkey_count = [
        config.hotkeys.toggle_microphone,
        config.hotkeys.toggle_output,
        config.hotkeys.toggle_foreground_audio,
        config.hotkeys.cycle_input_device,
        config.hotkeys.cycle_output_device,
        config.hotkeys.foreground_volume_up,
        config.hotkeys.foreground_volume_down,
    ]
    .into_iter()
    .flatten()
    .count();
    ConfigDiagnostics {
        health,
        path: if load.path.as_os_str().is_empty() {
            crate::config::load::config_path(&crate::config::data_dir())
        } else {
            load.path
        },
        source_schema_version: load.source_schema_version,
        effective_schema_version: load.effective_schema_version,
        read_only,
        warnings: load.warnings,
        repaired_fields: load.repaired_fields,
        migrations: load.migrations,
        validation,
        hotkey_count,
        raw: config,
    }
}

fn build_degraded(entries: Vec<(String, String)>) -> Vec<DegradedSubsystem> {
    entries
        .into_iter()
        .map(|(name, reason)| DegradedSubsystem { name, reason })
        .collect()
}

fn audio_state_label(state: &crate::audio::AudioState) -> String {
    match state {
        crate::audio::AudioState::Unavailable { reason } => format!("Unavailable — {reason}"),
        crate::audio::AudioState::Muted { volume_pct } => format!("Muted ({volume_pct}%)"),
        crate::audio::AudioState::Active { volume_pct } => format!("Active ({volume_pct}%)"),
    }
}

fn output_state_label(state: &crate::audio::OutputState) -> String {
    match state {
        crate::audio::OutputState::Unavailable { reason } => format!("Unavailable — {reason}"),
        crate::audio::OutputState::Current {
            device,
            muted,
            volume_pct,
        } => format!(
            "{} ({volume_pct}%) — {}",
            device.name,
            if *muted { "muted" } else { "active" }
        ),
    }
}
