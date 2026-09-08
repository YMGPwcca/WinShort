//! Privacy projection, clipboard export, log collection, and local ZIP support bundles.
//!
//! The sanitizer is deliberately report-local: endpoint pseudonyms and path tokens are
//! stable within one report, but no identity is preserved across reports.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::config::model::{Config, DeviceSelection, MonitorChoice};
use crate::diagnostics::snapshot::{DiagnosticsSnapshot, SelfTestReport};
use crate::error::{Error, Result};

pub const MAX_LOG_FILES: usize = 3;
pub const MAX_LOG_BYTES_PER_FILE: usize = 512 * 1024;
pub const MAX_LOG_BYTES_TOTAL: usize = 2 * 1024 * 1024;
pub const SUPPORT_FORMAT_VERSION: &str = "1";

#[derive(Debug, Default)]
pub struct Sanitizer {
    endpoints: Vec<(String, String)>,
    paths: Vec<(String, String)>,
}

impl Sanitizer {
    pub fn endpoint_token(&mut self, raw: &str) -> String {
        if raw.is_empty() || raw.eq_ignore_ascii_case("default") {
            return "default".into();
        }
        if let Some((_, token)) = self.endpoints.iter().find(|(value, _)| value == raw) {
            return token.clone();
        }
        let token = format!("endpoint#{:02}", self.endpoints.len() + 1);
        self.endpoints.push((raw.into(), token.clone()));
        token
    }

    fn path_token(&mut self, raw: &str) -> String {
        let normalized = normalize_path(raw);
        if let Some((_, token)) = self
            .paths
            .iter()
            .find(|(value, _)| value.eq_ignore_ascii_case(&normalized))
        {
            return token.clone();
        }
        let basename = normalized
            .rsplit('\\')
            .find(|part| !part.is_empty())
            .unwrap_or("path");
        let basename = if basename.is_empty() {
            "path"
        } else {
            basename
        };
        let token = format!("{} [path#{:02}]", basename, self.paths.len() + 1);
        self.paths.push((normalized, token.clone()));
        token
    }

    /// Redact known endpoint identifiers, then replace every absolute Windows path with
    /// basename + a report-local token. This is intentionally stronger than replacing only
    /// the current username: arbitrary private project directories are still protected.
    pub fn sanitize_text(&mut self, input: &str) -> String {
        let mut output = input.to_string();
        for (raw, token) in &self.endpoints {
            if !raw.is_empty() {
                output = output.replace(raw, token);
            }
        }
        replace_absolute_paths(&output, self)
    }

    pub fn safe_known_path(&mut self, path: &Path) -> String {
        let raw = path.to_string_lossy();
        let roots = [
            ("USERPROFILE", std::env::var_os("USERPROFILE")),
            ("LOCALAPPDATA", std::env::var_os("LOCALAPPDATA")),
            ("TEMP", std::env::var_os("TEMP")),
        ];
        for (label, root) in roots {
            let Some(root) = root else { continue };
            let root = root.to_string_lossy();
            if raw.len() >= root.len() && raw[..root.len()].eq_ignore_ascii_case(&root) {
                let suffix = raw[root.len()..].replace('/', "\\");
                return format!("%{label}%{suffix}");
            }
        }
        self.path_token(&raw)
    }
}

fn normalize_path(raw: &str) -> String {
    let mut value = raw.trim_matches(['"', '\'']).replace('/', "\\");
    if value
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\UNC\"))
    {
        value.replace_range(..8, r"\\");
    } else if value
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\"))
    {
        value.replace_range(..4, "");
    }
    value
}

fn replace_absolute_paths(input: &str, sanitizer: &mut Sanitizer) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut last = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let Some((replace_start, replace_end, path_start, path_end, quote)) =
            path_match_at(input, index)
        else {
            index += 1;
            continue;
        };
        output.push_str(&input[last..replace_start]);
        if let Some(quote) = quote {
            output.push(quote as char);
        }
        output.push_str(&sanitizer.path_token(&input[path_start..path_end]));
        if let Some(quote) = quote {
            output.push(quote as char);
        }
        last = replace_end;
        index = replace_end;
    }
    output.push_str(&input[last..]);
    output
}

fn path_match_at(input: &str, index: usize) -> Option<(usize, usize, usize, usize, Option<u8>)> {
    let bytes = input.as_bytes();
    if index >= bytes.len() {
        return None;
    }
    if matches!(bytes[index], b'"' | b'\'') {
        let path_start = index + 1;
        if !is_absolute_path_start(bytes, path_start) {
            return None;
        }
        let quote = bytes[index];
        let path_end = bytes[path_start..]
            .iter()
            .position(|value| *value == quote)
            .map(|offset| path_start + offset)?;
        return (path_end > path_start).then_some((
            index,
            path_end + 1,
            path_start,
            path_end,
            Some(quote),
        ));
    }
    if !is_absolute_path_start(bytes, index) {
        return None;
    }
    let context = path_context(input, index);
    let mut path_end = scan_path_end(bytes, index, context);
    while path_end > index && bytes[path_end - 1].is_ascii_whitespace() {
        path_end -= 1;
    }
    while path_end > index && matches!(bytes[path_end - 1], b'.' | b',' | b';' | b':' | b'!' | b'?')
    {
        path_end -= 1;
    }
    (path_end > index + 2).then_some((index, path_end, index, path_end, None))
}

fn is_absolute_path_start(bytes: &[u8], index: usize) -> bool {
    if index + 2 < bytes.len()
        && bytes[index].is_ascii_alphabetic()
        && bytes[index + 1] == b':'
        && matches!(bytes[index + 2], b'\\' | b'/')
    {
        return true;
    }
    if index + 3 >= bytes.len() || bytes[index] != b'\\' || bytes[index + 1] != b'\\' {
        return false;
    }
    !(bytes[index + 2] == b'.' && bytes[index + 3] == b'\\')
}

fn path_context(input: &str, index: usize) -> bool {
    let line_start = input[..index].rfind('\n').map_or(0, |offset| offset + 1);
    let prefix = input[line_start..index].to_ascii_lowercase();
    [
        "path:",
        "command_line",
        "command line",
        "registered command",
        "current command",
    ]
    .iter()
    .any(|marker| prefix.contains(marker))
}

fn scan_path_end(bytes: &[u8], start: usize, context: bool) -> usize {
    let mut end = start;
    while end < bytes.len() {
        let value = bytes[end];
        if value == b'\n'
            || value == b'\r'
            || value == b'"'
            || value == b'\''
            || value == b'|'
            || value == b';'
        {
            break;
        }
        if !context
            && (value.is_ascii_whitespace() || matches!(value, b',' | b';' | b')' | b']' | b'}'))
        {
            break;
        }
        end += 1;
    }
    if context {
        return end;
    }
    loop {
        let mut probe = end;
        while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if is_absolute_path_start(bytes, probe) {
            break;
        }
        let mut component_end = probe;
        while component_end < bytes.len()
            && !bytes[component_end].is_ascii_whitespace()
            && !matches!(bytes[component_end], b',' | b';' | b')' | b']' | b'}')
        {
            component_end += 1;
        }
        if probe < component_end
            && bytes[probe..component_end]
                .iter()
                .any(|value| matches!(value, b'\\' | b'/'))
        {
            end = component_end;
        } else {
            break;
        }
    }
    end
}

#[derive(Debug, Clone)]
pub struct SupportLog {
    pub name: String,
    pub contents: String,
}

#[derive(Debug, Clone)]
pub struct SupportReport {
    pub diagnostics: String,
    pub config: String,
    pub logs: Vec<SupportLog>,
    pub manifest: String,
}

pub fn diagnostics_text(snapshot: &DiagnosticsSnapshot) -> String {
    let mut sanitizer = Sanitizer::default();
    let report = build_report(snapshot, None, &mut sanitizer);
    report.diagnostics
}

pub fn build_report(
    snapshot: &DiagnosticsSnapshot,
    self_test: Option<&SelfTestReport>,
    sanitizer: &mut Sanitizer,
) -> SupportReport {
    // Register all currently known endpoint IDs before formatting logs so repeated values
    // receive identical pseudonyms across diagnostics.txt and config.sanitized.toml.
    register_endpoint_ids(snapshot, sanitizer);
    let diagnostics = format_diagnostics(snapshot, self_test, sanitizer);
    let config = format_config(
        &snapshot.config.raw,
        snapshot.config.effective_schema_version,
        sanitizer,
    );
    let (logs, mut notes) = collect_logs(snapshot.logging.directory.as_deref(), sanitizer);
    if snapshot.logging.directory.is_none() {
        notes.push("log directory unavailable; no raw log files were included".into());
    }
    let manifest = format_manifest(snapshot, &logs, &notes, sanitizer);
    SupportReport {
        diagnostics,
        config,
        logs,
        manifest,
    }
}

fn register_endpoint_ids(snapshot: &DiagnosticsSnapshot, sanitizer: &mut Sanitizer) {
    for selection in [
        &snapshot.config.raw.audio.input_device,
        &snapshot.config.raw.audio.output_device,
    ] {
        if let DeviceSelection::Endpoint(value) = selection {
            sanitizer.endpoint_token(value);
        }
    }
    for ids in [
        &snapshot.config.raw.audio.cycle_input_allowlist,
        &snapshot.config.raw.audio.cycle_output_allowlist,
    ]
    .into_iter()
    .flatten()
    {
        for id in ids {
            sanitizer.endpoint_token(id);
        }
    }
    for endpoint in [
        &snapshot.audio.input.selector,
        &snapshot.audio.output.selector,
    ] {
        sanitizer.endpoint_token(endpoint);
    }
}

fn format_diagnostics(
    snapshot: &DiagnosticsSnapshot,
    self_test: Option<&SelfTestReport>,
    sanitizer: &mut Sanitizer,
) -> String {
    let mut out = String::new();
    line(&mut out, "WinShort Diagnostics");
    line(&mut out, "====================");
    line(
        &mut out,
        &format!("Generated: {}", format_time(snapshot.generated_at)),
    );
    line(
        &mut out,
        &format!("WinShort: {}", snapshot.application.version),
    );
    line(
        &mut out,
        &format!("Profile: {}", snapshot.application.profile),
    );
    line(
        &mut out,
        &format!("Architecture: {}", snapshot.application.architecture),
    );
    let windows = match (snapshot.windows.build, snapshot.windows.update_revision) {
        (Some(build), Some(ubr)) => format!("Build {build}.{ubr}"),
        (Some(build), None) => format!("Build {build}.?"),
        _ => "Unavailable".into(),
    };
    line(&mut out, &format!("Windows: {windows}"));
    if let Some(error) = &snapshot.windows.error {
        line(
            &mut out,
            &format!("Windows query: {}", sanitizer.sanitize_text(error)),
        );
    }

    section(&mut out, "Keyboard");
    line(
        &mut out,
        &format!("Status: {}", snapshot.keyboard.health.label()),
    );
    line(
        &mut out,
        &format!("Hook: {}", yes_no(snapshot.keyboard.hook_active)),
    );
    line(
        &mut out,
        &format!("Suspended: {}", yes_no(snapshot.keyboard.suspended)),
    );
    line(
        &mut out,
        &format!(
            "Recorder capture: {}",
            yes_no(snapshot.keyboard.capture_active)
        ),
    );
    line(
        &mut out,
        &format!(
            "Win+1..9 reserved: {}",
            yes_no(snapshot.keyboard.reserved_win_numbers)
        ),
    );
    for (name, binding) in &snapshot.keyboard.bindings {
        line(&mut out, &format!("{name} binding: {binding}"));
    }
    line(
        &mut out,
        &format!(
            "Conflicts/validation: {}",
            if snapshot.keyboard.conflicts.is_empty() {
                "none".into()
            } else {
                snapshot
                    .keyboard
                    .conflicts
                    .iter()
                    .map(|value| sanitizer.sanitize_text(value))
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
        ),
    );

    section(&mut out, "Audio");
    format_endpoint(&mut out, "Input", &snapshot.audio.input, sanitizer);
    format_endpoint(&mut out, "Output", &snapshot.audio.output, sanitizer);
    line(
        &mut out,
        &format!("Microphone: {}", snapshot.audio.microphone_state),
    );
    line(
        &mut out,
        &format!("Output state: {}", snapshot.audio.output_state),
    );
    line(
        &mut out,
        &format!(
            "Foreground status: {}",
            snapshot.audio.foreground.health.label()
        ),
    );
    line(
        &mut out,
        &format!(
            "Foreground aggregate: {}",
            snapshot.audio.foreground.aggregate
        ),
    );
    line(
        &mut out,
        &format!(
            "Foreground sessions: {}",
            snapshot.audio.foreground.sessions
        ),
    );
    if let Some(error) = &snapshot.audio.foreground.error {
        line(
            &mut out,
            &format!("Foreground result: {}", sanitizer.sanitize_text(error)),
        );
    }

    section(&mut out, "Virtual desktop");
    line(
        &mut out,
        &format!("Status: {}", snapshot.desktop.health.label()),
    );
    line(&mut out, &format!("Native: {}", snapshot.desktop.native));
    line(
        &mut out,
        &format!("Fallback: {}", snapshot.desktop.fallback),
    );
    line(&mut out, &format!("Active: {}", snapshot.desktop.active));
    line(
        &mut out,
        &format!("Last served: {}", snapshot.desktop.last_served),
    );
    line(
        &mut out,
        &format!(
            "Desktop count: {}",
            snapshot
                .desktop
                .desktop_count
                .map_or_else(|| "unavailable".into(), |value| value.to_string())
        ),
    );
    if let Some(error) = &snapshot.desktop.error {
        line(
            &mut out,
            &format!("Last error: {}", sanitizer.sanitize_text(error)),
        );
    }

    section(&mut out, "Config");
    line(
        &mut out,
        &format!("Status: {}", snapshot.config.health.label()),
    );
    line(
        &mut out,
        &format!("Path: {}", sanitizer.safe_known_path(&snapshot.config.path)),
    );
    line(
        &mut out,
        &format!(
            "Source schema: {}",
            snapshot
                .config
                .source_schema_version
                .map_or_else(|| "not present".into(), |version| version.to_string())
        ),
    );
    line(
        &mut out,
        &format!(
            "Effective schema: {}",
            snapshot.config.effective_schema_version
        ),
    );
    line(
        &mut out,
        &format!(
            "Future-schema read-only: {}",
            yes_no(snapshot.config.read_only)
        ),
    );
    line(
        &mut out,
        &format!("Configured hotkeys: {}", snapshot.config.hotkey_count),
    );
    line(
        &mut out,
        &format!("Load warnings: {}", snapshot.config.warnings.len()),
    );
    line(
        &mut out,
        &format!("Repaired fields: {}", snapshot.config.repaired_fields.len()),
    );
    line(
        &mut out,
        &format!("Migrations: {}", snapshot.config.migrations.len()),
    );

    section(&mut out, "Overlay");
    line(
        &mut out,
        &format!("Status: {}", snapshot.overlay.health.label()),
    );
    line(
        &mut out,
        &format!("Enabled: {}", yes_no(snapshot.overlay.enabled)),
    );
    line(
        &mut out,
        &format!("Appearance: {}", snapshot.overlay.appearance),
    );
    line(&mut out, &format!("Blur: {}", snapshot.overlay.blur));
    line(
        &mut out,
        &format!(
            "Microphone notifications: {}",
            yes_no(snapshot.overlay.notifications.microphone)
        ),
    );
    line(
        &mut out,
        &format!(
            "Speaker notifications: {}",
            yes_no(snapshot.overlay.notifications.speaker)
        ),
    );
    line(
        &mut out,
        &format!(
            "Current app audio notifications: {}",
            yes_no(snapshot.overlay.notifications.current_app_audio)
        ),
    );
    line(
        &mut out,
        &format!(
            "Workspace notifications: {}",
            yes_no(snapshot.overlay.notifications.workspace)
        ),
    );
    line(
        &mut out,
        &format!(
            "Display profile notifications: {}",
            yes_no(snapshot.overlay.notifications.display_profile)
        ),
    );
    line(
        &mut out,
        &format!(
            "Animations enabled: {}",
            snapshot.overlay.animations_enabled.map_or_else(
                || String::from("unknown"),
                |value| String::from(yes_no(value))
            )
        ),
    );
    line(
        &mut out,
        &format!(
            "High contrast: {}",
            snapshot.overlay.high_contrast.map_or_else(
                || String::from("unknown"),
                |value| String::from(yes_no(value))
            )
        ),
    );
    line(
        &mut out,
        &format!(
            "Disable overlapped content: {}",
            snapshot.overlay.disable_overlapped_content.map_or_else(
                || String::from("unknown"),
                |value| String::from(yes_no(value))
            )
        ),
    );
    line(
        &mut out,
        &format!("Position: {}", snapshot.overlay.position),
    );
    line(
        &mut out,
        &format!("Monitor selector: {}", snapshot.overlay.monitor_selector),
    );
    line(
        &mut out,
        &format!(
            "Last target monitor: {}",
            snapshot
                .overlay
                .target_monitor
                .as_deref()
                .unwrap_or("never")
        ),
    );
    line(
        &mut out,
        &format!(
            "Last render DPI: {}",
            snapshot
                .overlay
                .render_dpi
                .map_or_else(|| "never".into(), |value| value.to_string())
        ),
    );

    section(&mut out, "Startup and logging");
    line(&mut out, &format!("Startup: {}", snapshot.startup.state));
    if let Some(command) = &snapshot.startup.registered_command {
        line(
            &mut out,
            &format!("Registered command: {}", sanitizer.sanitize_text(command)),
        );
    }
    if let Some(command) = &snapshot.startup.current_command {
        line(
            &mut out,
            &format!("Current command: {}", sanitizer.sanitize_text(command)),
        );
    }
    if let Some(error) = &snapshot.startup.error {
        line(
            &mut out,
            &format!("Startup query: {}", sanitizer.sanitize_text(error)),
        );
    }
    line(
        &mut out,
        &format!("Logging: {}", snapshot.logging.health.label()),
    );
    line(
        &mut out,
        &format!(
            "Log directory: {}",
            snapshot
                .logging
                .directory
                .as_deref()
                .map(|path| sanitizer.safe_known_path(path))
                .unwrap_or_else(|| "unavailable".into())
        ),
    );
    line(&mut out, &format!("Log level: {}", snapshot.logging.level));
    line(
        &mut out,
        &format!("Default log level: {}", snapshot.logging.default_level),
    );
    line(
        &mut out,
        &format!(
            "Temporary debug: {}",
            if snapshot.logging.temporary_debug {
                "enabled"
            } else {
                "off"
            }
        ),
    );
    line(
        &mut out,
        &format!("Retention days: {}", snapshot.logging.retention_days),
    );
    line(
        &mut out,
        &format!("Buffering: {}", snapshot.logging.buffering),
    );
    line(
        &mut out,
        &format!(
            "Current log file: {}",
            snapshot
                .logging
                .current_file
                .as_deref()
                .map(|path| sanitizer.safe_known_path(path))
                .unwrap_or_else(|| "unavailable".into())
        ),
    );

    section(&mut out, "Degraded subsystems");
    if snapshot.degraded.is_empty() {
        line(&mut out, "None");
    } else {
        for degraded in &snapshot.degraded {
            line(
                &mut out,
                &format!(
                    "{}: {}",
                    degraded.name,
                    sanitizer.sanitize_text(&degraded.reason)
                ),
            );
        }
    }

    if let Some(report) = self_test {
        section(&mut out, "Self-test");
        line(&mut out, &report.summary());
        for check in &report.checks {
            line(
                &mut out,
                &format!(
                    "{}: {} — {}",
                    check.name,
                    check.health.label(),
                    sanitizer.sanitize_text(&check.detail)
                ),
            );
        }
    }

    section(&mut out, "Privacy");
    line(&mut out, "Raw key history: not collected");
    line(&mut out, "Window titles: omitted");
    line(&mut out, "Command lines: omitted");
    line(
        &mut out,
        "Full executable paths: redacted to basename + report-local token",
    );
    line(&mut out, "Endpoint IDs: pseudonymized within this report");
    out
}

fn format_endpoint(
    out: &mut String,
    label: &str,
    endpoint: &crate::diagnostics::snapshot::AudioEndpointDiagnostics,
    sanitizer: &mut Sanitizer,
) {
    line(
        out,
        &format!(
            "{label}: {} ({})",
            endpoint.health.label(),
            sanitizer.endpoint_token(&endpoint.selector)
        ),
    );
    line(
        out,
        &format!("{label} role: {}", sanitizer.sanitize_text(&endpoint.role)),
    );
    if let Some(description) = &endpoint.description {
        line(
            out,
            &format!("{label} device: {}", sanitizer.sanitize_text(description)),
        );
    }
}

fn format_config(config: &Config, schema_version: u8, sanitizer: &mut Sanitizer) -> String {
    let safe = SafeConfig {
        schema_version,
        general: SafeGeneral {
            start_hotkeys_enabled: config.general.start_hotkeys_enabled,
        },
        overlay: SafeOverlay {
            enabled: config.overlay.enabled,
            duration_ms: config.overlay.duration_ms,
            position: config.overlay.position.as_str().into(),
            monitor: safe_monitor(&config.overlay.monitor),
            scale: config.overlay.scale,
            blur: config.overlay.blur.as_str().into(),
            appearance: config.overlay.appearance.as_str().into(),
            show_microphone: config.overlay.notifications.microphone,
            show_speaker: config.overlay.notifications.speaker,
            show_current_app_audio: config.overlay.notifications.current_app_audio,
            show_workspace: config.overlay.notifications.workspace,
            show_display_profile: config.overlay.notifications.display_profile,
        },
        audio: SafeAudio {
            input_role: config.audio.input_role.as_str().into(),
            output_role: config.audio.output_role.as_str().into(),
            input_device: safe_selection(&config.audio.input_device, sanitizer),
            output_device: safe_selection(&config.audio.output_device, sanitizer),
            cycle_input_allowlist: safe_allowlist(
                config.audio.cycle_input_allowlist.as_deref(),
                sanitizer,
            ),
            cycle_output_allowlist: safe_allowlist(
                config.audio.cycle_output_allowlist.as_deref(),
                sanitizer,
            ),
        },
        hotkeys: SafeHotkeys {
            toggle_microphone: config
                .hotkeys
                .toggle_microphone
                .map(|value| value.to_string())
                .unwrap_or_default(),
            toggle_output: config
                .hotkeys
                .toggle_output
                .map(|value| value.to_string())
                .unwrap_or_default(),
            toggle_foreground_audio: config
                .hotkeys
                .toggle_foreground_audio
                .map(|value| value.to_string())
                .unwrap_or_default(),
            cycle_input_device: config
                .hotkeys
                .cycle_input_device
                .map(|value| value.to_string())
                .unwrap_or_default(),
            cycle_output_device: config
                .hotkeys
                .cycle_output_device
                .map(|value| value.to_string())
                .unwrap_or_default(),
            foreground_volume_up: config
                .hotkeys
                .foreground_volume_up
                .map(|value| value.to_string())
                .unwrap_or_default(),
            foreground_volume_down: config
                .hotkeys
                .foreground_volume_down
                .map(|value| value.to_string())
                .unwrap_or_default(),
            display_profiles: config
                .hotkeys
                .display_profiles
                .iter()
                .map(|binding| SafeDisplayProfileHotkey {
                    profile_id: binding.profile_id.clone(),
                    hotkey: binding.hotkey.to_string(),
                })
                .collect(),
            disabled: config
                .hotkeys
                .disabled
                .iter()
                .map(|binding| SafeDisabledHotkey {
                    action: binding.action.clone(),
                    hotkey: binding.hotkey.to_string(),
                })
                .collect(),
        },
        virtual_desktops: SafeVirtualDesktops {
            enabled: config.virtual_desktops.enabled,
            win_number_switching: config.virtual_desktops.win_number_switching,
            number_modifier: config.virtual_desktops.number_modifier.to_string(),
            move_follow_modifier: config
                .virtual_desktops
                .move_follow_modifier
                .map_or(String::new(), |modifier| modifier.to_string()),
            move_silent_modifier: config
                .virtual_desktops
                .move_silent_modifier
                .map_or(String::new(), |modifier| modifier.to_string()),
            previous_desktop: config
                .virtual_desktops
                .previous_desktop
                .map_or(String::new(), |hotkey| hotkey.to_string()),
            scratchpad_assign: config
                .virtual_desktops
                .scratchpad_assign
                .map_or(String::new(), |hotkey| hotkey.to_string()),
            scratchpad_toggle: config
                .virtual_desktops
                .scratchpad_toggle
                .map_or(String::new(), |hotkey| hotkey.to_string()),
        },
        display_profiles: SafeDisplayProfiles {
            enabled: config.display_profiles.enabled,
            active_profile: config
                .display_profiles
                .active_profile
                .clone()
                .unwrap_or_default(),
            profiles: config
                .display_profiles
                .profiles
                .iter()
                .map(|profile| SafeDisplayProfile {
                    id: profile.id.clone(),
                    name: profile.name.clone(),
                    topology: profile.topology.label().into(),
                    route_count: profile.routes.len(),
                    confirmed: profile.confirmed,
                })
                .collect(),
        },
    };
    toml::to_string_pretty(&safe)
        .unwrap_or_else(|_| "# sanitized config formatting failed\n".into())
}

fn safe_selection(selection: &DeviceSelection, sanitizer: &mut Sanitizer) -> String {
    match selection {
        DeviceSelection::Default => "default".into(),
        DeviceSelection::Endpoint(value) => sanitizer.endpoint_token(value),
    }
}
fn safe_allowlist(allowlist: Option<&[String]>, sanitizer: &mut Sanitizer) -> Option<Vec<String>> {
    allowlist.map(|ids| {
        ids.iter()
            .map(|id| sanitizer.endpoint_token(id))
            .collect::<Vec<_>>()
    })
}

fn safe_monitor(monitor: &MonitorChoice) -> String {
    match monitor {
        MonitorChoice::Cursor => "cursor".into(),
        MonitorChoice::Primary => "primary".into(),
        MonitorChoice::Device(name) => format!("device:{name}"),
    }
}

#[derive(Debug, Serialize)]
struct SafeConfig {
    schema_version: u8,
    general: SafeGeneral,
    overlay: SafeOverlay,
    audio: SafeAudio,
    hotkeys: SafeHotkeys,
    virtual_desktops: SafeVirtualDesktops,
    display_profiles: SafeDisplayProfiles,
}

#[derive(Debug, Serialize)]
struct SafeGeneral {
    start_hotkeys_enabled: bool,
}

#[derive(Debug, Serialize)]
struct SafeOverlay {
    enabled: bool,
    duration_ms: u32,
    position: String,
    monitor: String,
    scale: f32,
    blur: String,
    appearance: String,
    show_microphone: bool,
    show_speaker: bool,
    show_current_app_audio: bool,
    show_workspace: bool,
    show_display_profile: bool,
}

#[derive(Debug, Serialize)]
struct SafeAudio {
    input_role: String,
    output_role: String,
    input_device: String,
    output_device: String,
    cycle_input_allowlist: Option<Vec<String>>,
    cycle_output_allowlist: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct SafeHotkeys {
    toggle_microphone: String,
    toggle_output: String,
    toggle_foreground_audio: String,
    cycle_input_device: String,
    cycle_output_device: String,
    foreground_volume_up: String,
    foreground_volume_down: String,
    display_profiles: Vec<SafeDisplayProfileHotkey>,
    disabled: Vec<SafeDisabledHotkey>,
}
#[derive(Debug, Serialize)]
struct SafeDisplayProfileHotkey {
    profile_id: String,
    hotkey: String,
}
#[derive(Debug, Serialize)]
struct SafeDisabledHotkey {
    action: String,
    hotkey: String,
}

#[derive(Debug, Serialize)]
struct SafeVirtualDesktops {
    enabled: bool,
    win_number_switching: bool,
    number_modifier: String,
    move_follow_modifier: String,
    move_silent_modifier: String,
    previous_desktop: String,
    scratchpad_assign: String,
    scratchpad_toggle: String,
}
#[derive(Debug, Serialize)]
struct SafeDisplayProfiles {
    enabled: bool,
    active_profile: String,
    profiles: Vec<SafeDisplayProfile>,
}

#[derive(Debug, Serialize)]
struct SafeDisplayProfile {
    id: String,
    name: String,
    topology: String,
    route_count: usize,
    confirmed: bool,
}

fn collect_logs(
    directory: Option<&Path>,
    sanitizer: &mut Sanitizer,
) -> (Vec<SupportLog>, Vec<String>) {
    let mut notes = Vec::new();
    let Some(directory) = directory else {
        return (Vec::new(), notes);
    };
    let mut candidates: Vec<(String, PathBuf)> = match fs::read_dir(directory) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                let name = path.file_name()?.to_string_lossy().into_owned();
                (entry.file_type().ok()?.is_file()
                    && name.starts_with("winshort-")
                    && name.ends_with(".log"))
                .then_some((name, path))
            })
            .collect(),
        Err(error) => {
            notes.push(format!("log directory could not be listed: {error}"));
            return (Vec::new(), notes);
        }
    };
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    let mut total = 0usize;
    let mut logs = Vec::new();
    for (name, path) in candidates.into_iter().take(MAX_LOG_FILES) {
        if total >= MAX_LOG_BYTES_TOTAL {
            notes.push("log input limit reached; older files skipped".into());
            break;
        }
        let remaining = (MAX_LOG_BYTES_TOTAL - total).min(MAX_LOG_BYTES_PER_FILE);
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(error) => {
                notes.push(format!("{name} skipped: {error}"));
                continue;
            }
        };
        let mut bytes = Vec::new();
        let mut limited = (&mut file).take((remaining + 1) as u64);
        if let Err(error) = limited.read_to_end(&mut bytes) {
            notes.push(format!("{name} skipped: {error}"));
            continue;
        }
        let truncated = bytes.len() > remaining;
        if truncated {
            bytes.truncate(remaining);
            notes.push(format!("{name} truncated at {remaining} bytes"));
        }
        total += bytes.len();
        let (contents, removed) =
            sanitize_log_contents(&String::from_utf8_lossy(&bytes), sanitizer);
        if removed > 0 {
            notes.push(format!(
                "{name}: removed {removed} privacy-sensitive line(s)"
            ));
        }
        logs.push(SupportLog { name, contents });
    }
    (logs, notes)
}

fn sanitize_log_contents(input: &str, sanitizer: &mut Sanitizer) -> (String, usize) {
    const FORBIDDEN_FIELDS: [&str; 8] = [
        "last_key",
        "recent_key",
        "raw_key",
        "key_event",
        "keystroke",
        "window_title",
        "command_line",
        "capture_history",
    ];
    let mut removed = 0usize;
    let mut output = String::new();
    for line in input.lines() {
        let lower = line.to_ascii_lowercase();
        if FORBIDDEN_FIELDS.iter().any(|field| lower.contains(field)) {
            removed += 1;
            continue;
        }
        if !output.is_empty() {
            output.push_str("\r\n");
        }
        output.push_str(&sanitizer.sanitize_text(line));
    }
    (output, removed)
}

fn format_manifest(
    snapshot: &DiagnosticsSnapshot,
    logs: &[SupportLog],
    notes: &[String],
    sanitizer: &mut Sanitizer,
) -> String {
    let mut out = String::new();
    line(
        &mut out,
        &format!("WinShort support bundle format: {SUPPORT_FORMAT_VERSION}"),
    );
    line(
        &mut out,
        &format!("WinShort version: {}", snapshot.application.version),
    );
    line(
        &mut out,
        &format!("Generated: {}", format_time(snapshot.generated_at)),
    );
    line(&mut out, "Files included:");
    line(&mut out, "- diagnostics.txt");
    line(&mut out, "- config.sanitized.toml");
    for log in logs {
        line(&mut out, &format!("- logs/{}", log.name));
    }
    line(&mut out, "- bundle-info.txt");
    line(
        &mut out,
        &format!(
            "Log policy: newest {MAX_LOG_FILES} files, {MAX_LOG_BYTES_PER_FILE} bytes/file, {MAX_LOG_BYTES_TOTAL} bytes total"
        ),
    );
    if notes.is_empty() {
        line(&mut out, "Truncation/skips: none");
    } else {
        line(&mut out, "Truncation/skips:");
        for note in notes {
            line(&mut out, &sanitizer.sanitize_text(note));
        }
    }
    line(
        &mut out,
        "Sanitizer: report-local path tokens and endpoint pseudonyms; raw key history excluded",
    );
    out
}

pub fn create_support_bundle(snapshot: &DiagnosticsSnapshot) -> Result<PathBuf> {
    crate::diagnostics::logging::flush();
    let mut sanitizer = Sanitizer::default();
    let report = build_report(snapshot, None, &mut sanitizer);
    let data_dir = snapshot
        .config
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(crate::config::data_dir);
    let support_dir = data_dir.join("Support");
    fs::create_dir_all(&support_dir)
        .map_err(|error| Error::config(format!("create support directory: {error}")))?;
    let path = support_dir.join(format!(
        "WinShort-support-{}.zip",
        file_stamp(snapshot.generated_at)
    ));
    let file = File::create(&path)
        .map_err(|error| Error::config(format!("create support bundle: {error}")))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    add_file(
        &mut archive,
        "diagnostics.txt",
        report.diagnostics.as_bytes(),
        options,
    )?;
    add_file(
        &mut archive,
        "config.sanitized.toml",
        report.config.as_bytes(),
        options,
    )?;
    for log in &report.logs {
        add_file(
            &mut archive,
            &format!("logs/{}", log.name),
            log.contents.as_bytes(),
            options,
        )?;
    }
    add_file(
        &mut archive,
        "bundle-info.txt",
        report.manifest.as_bytes(),
        options,
    )?;
    archive
        .finish()
        .map_err(|error| Error::config(format!("finish support bundle: {error}")))?;
    Ok(path)
}

fn add_file<W: Write + std::io::Seek>(
    archive: &mut ZipWriter<W>,
    name: &str,
    contents: &[u8],
    options: SimpleFileOptions,
) -> Result<()> {
    archive
        .start_file(name, options)
        .map_err(|error| Error::config(format!("add support file {name}: {error}")))?;
    archive
        .write_all(contents)
        .map_err(|error| Error::config(format!("write support file {name}: {error}")))?;
    Ok(())
}

pub(crate) fn log_directory(directory: Option<&Path>) -> PathBuf {
    directory
        .map(Path::to_path_buf)
        .unwrap_or_else(|| crate::config::data_dir().join("logs"))
}

pub fn copy_diagnostics(
    owner: windows::Win32::Foundation::HWND,
    snapshot: &DiagnosticsSnapshot,
) -> Result<()> {
    let text = diagnostics_text(snapshot);
    copy_unicode_text(owner, &text)
}

#[cfg(windows)]
fn validate_clipboard_owner(owner: windows::Win32::Foundation::HWND) -> Result<()> {
    if owner.0.is_null() {
        Err(Error::config("clipboard owner HWND is null"))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn copy_unicode_text(owner: windows::Win32::Foundation::HWND, text: &str) -> Result<()> {
    use std::ptr::NonNull;
    use windows::Win32::Foundation::{GlobalFree, HANDLE};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    validate_clipboard_owner(owner)?;

    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    utf16.push(0);
    let bytes = utf16.len() * std::mem::size_of::<u16>();
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }
        .map_err(|error| Error::config(format!("allocate clipboard text: {error}")))?;
    let Some(ptr) = NonNull::new(unsafe { GlobalLock(handle) }) else {
        unsafe {
            let _ = GlobalFree(Some(handle));
        };
        return Err(Error::config("lock clipboard text"));
    };
    unsafe {
        std::ptr::copy_nonoverlapping(
            utf16.as_ptr().cast::<u8>(),
            ptr.as_ptr().cast::<u8>(),
            bytes,
        );
        let _ = GlobalUnlock(handle);
    }
    if unsafe { OpenClipboard(Some(owner)) }.is_err() {
        unsafe {
            let _ = GlobalFree(Some(handle));
        };
        return Err(Error::config("open clipboard"));
    }
    let result = (|| {
        unsafe { EmptyClipboard() }
            .map_err(|error| Error::config(format!("empty clipboard: {error}")))?;
        unsafe {
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(handle.0)))
                .map_err(|error| Error::config(format!("set clipboard text: {error}")))?;
        }
        Ok::<(), crate::error::Error>(())
    })();
    unsafe {
        let _ = CloseClipboard();
    };
    if result.is_err() {
        unsafe {
            let _ = GlobalFree(Some(handle));
        };
    }
    result
}

#[cfg(not(windows))]
fn copy_unicode_text(_owner: windows::Win32::Foundation::HWND, _text: &str) -> Result<()> {
    Err(Error::config(
        "Unicode clipboard is only available on Windows",
    ))
}

fn line(output: &mut String, value: &str) {
    output.push_str(value);
    output.push_str("\r\n");
}

fn section(output: &mut String, value: &str) {
    output.push_str("\r\n[");
    output.push_str(value);
    output.push_str("]\r\n");
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn format_time(value: SystemTime) -> String {
    let seconds = value
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = seconds / 86_400;
    let day_seconds = seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        day_seconds / 3600,
        day_seconds / 60 % 60,
        day_seconds % 60
    )
}

fn file_stamp(value: SystemTime) -> String {
    format_time(value).replace(['-', ':', 'T', 'Z'], "")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::binding::Hotkey;

    #[test]
    fn path_redaction_removes_user_and_project_components() {
        let mut sanitizer = Sanitizer::default();
        let value = sanitizer.sanitize_text(
            r#"C:\Users\Alice\Projects\SecretClient\app.exe and C:\Users\Bob\Projects\SecretClient\app.exe"#,
        );
        assert!(!value.contains("Alice"));
        assert!(!value.contains("Bob"));
        assert!(!value.contains("SecretClient"));
        assert!(value.contains("app.exe [path#01]"));
        assert!(value.contains("app.exe [path#02]"));
    }

    #[test]
    fn path_recognizer_covers_unc_extended_quoted_and_forward_slash_forms() {
        let mut sanitizer = Sanitizer::default();
        let value = sanitizer.sanitize_text(
            r#""C:\Users\Alice\Projects\Secret Client\foo.exe" | \\corp-server\clients\SecretClient\tool.exe | \\?\C:\Users\Alice\foo.exe | \\?\UNC\server\share\Private\foo.exe | Path: C:\Users\Alice\Projects\Secret Client\foo.exe | C:/Users/Alice/Secret Client/foo.exe | \\.\DISPLAY1"#,
        );
        for private_component in [
            "Alice",
            "Secret Client",
            "corp-server",
            "clients",
            "SecretClient",
            "server",
            "share",
            "Private",
        ] {
            assert!(
                !value.contains(private_component),
                "sanitized output leaked {private_component}: {value}"
            );
        }
        assert!(value.contains("foo.exe [path#"));
        assert!(value.contains(r"\\.\DISPLAY1"));

        let mut repeated_sanitizer = Sanitizer::default();
        let repeated =
            repeated_sanitizer.sanitize_text(r#"C:\Private\foo.exe C:\Private\foo.exe."#);
        assert_eq!(repeated.matches("foo.exe [path#01]").count(), 2);
    }

    #[test]
    fn endpoint_pseudonyms_are_stable_within_report() {
        let mut sanitizer = Sanitizer::default();
        assert_eq!(sanitizer.endpoint_token("opaque-A"), "endpoint#01");
        assert_eq!(sanitizer.endpoint_token("opaque-A"), "endpoint#01");
        assert_eq!(sanitizer.endpoint_token("opaque-B"), "endpoint#02");
    }

    #[test]
    fn sanitized_logs_remove_sensitive_identity_fields() {
        let mut sanitizer = Sanitizer::default();
        let (value, removed) = sanitize_log_contents(
            r#"last_key=VK_A window_title="Secret" command_line="C:\Users\Alice\app.exe"
safe=1"#,
            &mut sanitizer,
        );
        assert_eq!(removed, 1);
        assert!(!value.contains("Alice"));
        assert!(!value.contains("C:\\Users"));
        assert!(!value.contains("last_key="));
        assert!(value.contains("safe=1"));
    }

    #[test]
    fn config_export_is_structured_and_does_not_include_raw_endpoint() {
        let mut sanitizer = Sanitizer::default();
        let mut config = Config::default();
        config.audio.input_device = DeviceSelection::Endpoint("opaque-endpoint".into());
        config.audio.cycle_input_allowlist = Some(vec!["opaque-allowlist".into()]);
        config.hotkeys.toggle_output = None;
        config.hotkeys.set_disabled_hotkey(
            "toggle_output".into(),
            Hotkey::parse("Ctrl+Alt+F20").unwrap(),
        );
        let output = format_config(&config, 1, &mut sanitizer);
        assert!(output.contains("schema_version = 1"));
        assert!(output.contains("endpoint#01"));
        assert!(!output.contains("opaque-endpoint"));
        assert!(output.contains("endpoint#02"));
        assert!(!output.contains("opaque-allowlist"));
        assert!(output.contains("disabled"));
        assert!(output.contains("toggle_output"));
        assert!(output.contains("Ctrl+Alt+F20"));
    }

    #[cfg(windows)]
    #[test]
    fn clipboard_owner_seam_rejects_null_hwnd() {
        use windows::Win32::Foundation::HWND;

        let null = HWND(std::ptr::null_mut());
        assert!(validate_clipboard_owner(null).is_err());
        let valid = HWND(std::ptr::dangling_mut());
        assert!(validate_clipboard_owner(valid).is_ok());
    }
    fn sample_snapshot(root: &Path) -> DiagnosticsSnapshot {
        use crate::diagnostics::snapshot::{
            ApplicationDiagnostics, AudioDiagnostics, AudioEndpointDiagnostics, ConfigDiagnostics,
            DegradedSubsystem, DesktopDiagnostics, DiagnosticsSnapshot, ForegroundAudioDiagnostics,
            Health, KeyboardDiagnostics, LoggingDiagnostics, OverlayDiagnostics,
            StartupDiagnostics, WindowsDiagnostics,
        };
        let mut config = Config::default();
        config.audio.input_device = DeviceSelection::Endpoint("opaque-endpoint-secret".into());
        let config_path = root.join("WinShort").join("config.toml");
        let log_dir = root.join("WinShort").join("logs");
        DiagnosticsSnapshot {
            generated_at: UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
            application: ApplicationDiagnostics {
                version: "0.1.0".into(),
                profile: "test".into(),
                architecture: "x86_64".into(),
            },
            windows: WindowsDiagnostics {
                architecture: "x86_64".into(),
                build: Some(26100),
                update_revision: Some(100),
                error: None,
            },
            keyboard: KeyboardDiagnostics {
                health: Health::Healthy,
                installed: true,
                hook_active: true,
                suspended: false,
                capture_active: false,
                bindings: vec![("Microphone".into(), "Ctrl+Alt+M".into())],
                conflicts: Vec::new(),
                reserved_win_numbers: true,
            },
            audio: AudioDiagnostics {
                input: AudioEndpointDiagnostics {
                    selector: "opaque-endpoint-secret".into(),
                    role: "Console (default)".into(),
                    health: Health::Healthy,
                    description: Some("Headset microphone".into()),
                },
                output: AudioEndpointDiagnostics {
                    selector: "default".into(),
                    role: "Console (default)".into(),
                    health: Health::Healthy,
                    description: Some("Speakers".into()),
                },
                microphone_state: "Active (80%)".into(),
                output_state: "Active (60%)".into(),
                foreground: ForegroundAudioDiagnostics {
                    health: Health::Warning,
                    aggregate: "No external app".into(),
                    app_name: None,
                    sessions: 0,
                    error: None,
                },
            },
            desktop: DesktopDiagnostics {
                health: Health::Healthy,
                native: "Available".into(),
                fallback: "Available".into(),
                active: "Native Shell".into(),
                last_served: "none yet".into(),
                desktop_count: Some(3),
                build: Some(26100),
                update_revision: Some(100),
                error: None,
            },
            config: ConfigDiagnostics {
                health: Health::Healthy,
                path: config_path,
                source_schema_version: Some(1),
                effective_schema_version: crate::config::model::CURRENT_SCHEMA_VERSION,
                read_only: false,
                warnings: Vec::new(),
                repaired_fields: Vec::new(),
                migrations: Vec::new(),
                validation: Vec::new(),
                hotkey_count: 3,
                raw: config,
            },
            overlay: OverlayDiagnostics {
                health: Health::Healthy,
                enabled: true,
                blur: "blur-medium".into(),
                appearance: "system".into(),
                resolved_appearance: Some("dark".into()),
                notifications: crate::config::model::OverlayNotifications::default(),
                animations_enabled: Some(true),
                high_contrast: Some(false),
                disable_overlapped_content: Some(false),
                position: "Bottom Center".into(),
                monitor_selector: "cursor".into(),
                target_monitor: Some(r"\\.\DISPLAY1".into()),
                render_dpi: Some(144),
                last_shown: None,
                window_available: true,
            },
            startup: StartupDiagnostics {
                health: Health::Healthy,
                state: "Disabled".into(),
                registered_command: None,
                current_command: Some(r#""\\?\C:\Users\Alice\Secret Client\WinShort.exe""#.into()),
                error: None,
            },
            logging: LoggingDiagnostics {
                health: Health::Healthy,
                directory: Some(log_dir.clone()),
                current_file: Some(log_dir.join("winshort-20260825.log")),
                level: "DEBUG".into(),
                default_level: "INFO".into(),
                temporary_debug: true,
                retention_days: 14,
                buffering: "BufWriter; Warn/Error + 5s dirty flush".into(),
            },
            degraded: vec![DegradedSubsystem {
                name: "test".into(),
                reason: r#"\\corp-server\clients\SecretClient\tool.exe"#.into(),
            }],
        }
    }

    #[test]
    fn support_bundle_flushes_buffered_records_before_collecting_logs() {
        let root = std::env::temp_dir().join(format!(
            "winshort-support-buffered-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let snapshot = sample_snapshot(&root);
        fs::create_dir_all(snapshot.logging.directory.as_ref().unwrap()).unwrap();
        crate::diagnostics::logging::init(
            snapshot.logging.directory.as_ref().unwrap(),
            crate::diagnostics::logging::Level::Info,
        );
        crate::info!("buffered support record");

        let bundle = create_support_bundle(&snapshot).unwrap();
        let file = File::open(&bundle).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut found = false;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            if entry.name().starts_with("logs/") {
                let mut contents = String::new();
                entry.read_to_string(&mut contents).unwrap();
                found |= contents.contains("buffered support record");
            }
        }
        assert!(found, "support bundle omitted buffered record");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn support_bundle_is_bounded_and_sanitized() {
        let root = std::env::temp_dir().join(format!(
            "winshort-support-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let log_dir = root.join("WinShort").join("logs");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(
            log_dir.join("winshort-20260825.log"),
            r#"safe=1
last_key=VK_A
window_title=Secret
path="C:\Users\Alice\Projects\Secret Client\foo.exe"
unc=\\corp-server\clients\SecretClient\tool.exe
extended=\\?\UNC\server\share\Private\foo.exe
panic=C:\Users\Alice\private-project\foo.rs
endpoint=opaque-endpoint-secret
"#,
        )
        .unwrap();
        let snapshot = sample_snapshot(&root);
        let bundle = create_support_bundle(&snapshot).unwrap();
        let file = File::open(&bundle).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"diagnostics.txt".into()));
        assert!(names.contains(&"config.sanitized.toml".into()));
        assert!(names.contains(&"bundle-info.txt".into()));
        assert!(names.iter().any(|name| name.starts_with("logs/")));

        let mut diagnostics = String::new();
        archive
            .by_name("diagnostics.txt")
            .unwrap()
            .read_to_string(&mut diagnostics)
            .unwrap();
        assert!(!diagnostics.contains("opaque-endpoint-secret"));
        assert!(!diagnostics.contains("Alice"));
        assert!(!diagnostics.contains("SecretClient"));
        assert!(!diagnostics.contains("corp-server"));
        assert!(!diagnostics.contains("Secret Client"));
        assert!(!diagnostics.contains("Private"));

        let mut config = String::new();
        archive
            .by_name("config.sanitized.toml")
            .unwrap()
            .read_to_string(&mut config)
            .unwrap();
        assert!(config.contains("endpoint#01"));
        assert!(!config.contains("opaque-endpoint-secret"));

        let log_name = names
            .into_iter()
            .find(|name| name.starts_with("logs/"))
            .unwrap();
        let mut log = String::new();
        archive
            .by_name(&log_name)
            .unwrap()
            .read_to_string(&mut log)
            .unwrap();
        assert!(!log.contains("last_key="));
        assert!(!log.contains("window_title="));
        assert!(!log.contains("Alice"));
        assert!(!log.contains("SecretClient"));
        assert!(!log.contains("corp-server"));
        assert!(!log.contains("Secret Client"));
        assert!(!log.contains("private-project"));
        assert!(!log.contains("opaque-endpoint-secret"));
        drop(archive);
        fs::remove_dir_all(root).unwrap();
    }
}
