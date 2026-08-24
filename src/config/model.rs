//! Typed configuration model. Raw strings exist only at the TOML boundary
//! (see load/save); everything past parsing is structured (spec §8).

use serde::{Deserialize, Serialize};

use crate::keyboard::binding::Hotkey;

pub const DEFAULT_TOGGLE_MICROPHONE: &str = "Ctrl+Alt+M";
pub const DEFAULT_TOGGLE_OUTPUT: &str = "Ctrl+Alt+O";
pub const DEFAULT_TOGGLE_FOREGROUND: &str = "Ctrl+Alt+P";

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub general: GeneralCfg,
    pub overlay: OverlayCfg,
    pub audio: AudioCfg,
    pub hotkeys: HotkeysCfg,
    pub virtual_desktops: VdCfg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneralCfg {
    pub start_hotkeys_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayCfg {
    pub enabled: bool,
    pub duration_ms: u32,
    pub position: OverlayPosition,
    pub monitor: MonitorChoice,
    pub scale: f32,
    pub opacity: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioCfg {
    pub input_role: EndpointRole,
    pub output_role: EndpointRole,
    pub input_device: DeviceSelection,
    pub output_device: DeviceSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeysCfg {
    pub toggle_microphone: Option<Hotkey>,
    pub toggle_output: Option<Hotkey>,
    pub toggle_foreground_audio: Option<Hotkey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VdCfg {
    pub enabled: bool,
    pub win_number_switching: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPosition {
    TopLeft,
    TopCenter,
    TopRight,
    Center,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl OverlayPosition {
    pub fn label(self) -> &'static str {
        match self {
            OverlayPosition::TopLeft => "Top Left",
            OverlayPosition::TopCenter => "Top Center",
            OverlayPosition::TopRight => "Top Right",
            OverlayPosition::Center => "Center",
            OverlayPosition::BottomLeft => "Bottom Left",
            OverlayPosition::BottomCenter => "Bottom Center",
            OverlayPosition::BottomRight => "Bottom Right",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "top-left" => OverlayPosition::TopLeft,
            "top-center" => OverlayPosition::TopCenter,
            "top-right" => OverlayPosition::TopRight,
            "center" => OverlayPosition::Center,
            "bottom-left" => OverlayPosition::BottomLeft,
            "bottom-center" => OverlayPosition::BottomCenter,
            "bottom-right" => OverlayPosition::BottomRight,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            OverlayPosition::TopLeft => "top-left",
            OverlayPosition::TopCenter => "top-center",
            OverlayPosition::TopRight => "top-right",
            OverlayPosition::Center => "center",
            OverlayPosition::BottomLeft => "bottom-left",
            OverlayPosition::BottomCenter => "bottom-center",
            OverlayPosition::BottomRight => "bottom-right",
        }
    }

    pub const ALL: [OverlayPosition; 7] = [
        OverlayPosition::TopLeft,
        OverlayPosition::TopCenter,
        OverlayPosition::TopRight,
        OverlayPosition::Center,
        OverlayPosition::BottomLeft,
        OverlayPosition::BottomCenter,
        OverlayPosition::BottomRight,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorChoice {
    /// Monitor containing the foreground window.
    Foreground,
    Primary,
    /// Stable device identity, e.g. "\\\\.\\DISPLAY1" (#26). Enumeration
    /// indices change with topology; device names survive reboots.
    Device(String),
}

impl MonitorChoice {
    pub fn label(&self) -> String {
        match self {
            MonitorChoice::Foreground => "Foreground window's monitor".into(),
            MonitorChoice::Primary => "Primary monitor".into(),
            MonitorChoice::Device(name) => format!("Monitor {name}"),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "foreground" => MonitorChoice::Foreground,
            "primary" => MonitorChoice::Primary,
            other => {
                // Legacy `index:N` migrates to Primary (best effort, #26):
                // enumeration indices are not stable across topology changes.
                if other.starts_with("index:") {
                    return Some(MonitorChoice::Primary);
                }
                let name = other.strip_prefix("device:")?;
                if name.is_empty() {
                    return None;
                }
                MonitorChoice::Device(name.to_string())
            }
        })
    }

    pub fn as_str(&self) -> String {
        match self {
            MonitorChoice::Foreground => "foreground".into(),
            MonitorChoice::Primary => "primary".into(),
            // Legacy `index:N` configs migrate to Primary on load (#26).
            MonitorChoice::Device(name) => format!("device:{name}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointRole {
    Console,
    Multimedia,
    Communications,
}

impl EndpointRole {
    pub fn label(self) -> &'static str {
        match self {
            EndpointRole::Console => "Console (default)",
            EndpointRole::Multimedia => "Multimedia",
            EndpointRole::Communications => "Communications",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "console" => EndpointRole::Console,
            "multimedia" => EndpointRole::Multimedia,
            "communications" => EndpointRole::Communications,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EndpointRole::Console => "console",
            EndpointRole::Multimedia => "multimedia",
            EndpointRole::Communications => "communications",
        }
    }
}

/// "default" (follow system default) or a specific endpoint GUID string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelection {
    Default,
    Endpoint(String),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralCfg {
                start_hotkeys_enabled: true,
            },
            overlay: OverlayCfg {
                enabled: true,
                duration_ms: 1300,
                position: OverlayPosition::BottomCenter,
                monitor: MonitorChoice::Foreground,
                scale: 1.0,
                opacity: 1.0,
            },
            audio: AudioCfg {
                input_role: EndpointRole::Console,
                output_role: EndpointRole::Console,
                input_device: DeviceSelection::Default,
                output_device: DeviceSelection::Default,
            },
            hotkeys: HotkeysCfg {
                toggle_microphone: Some(Hotkey::parse(DEFAULT_TOGGLE_MICROPHONE).unwrap()),
                toggle_output: Some(Hotkey::parse(DEFAULT_TOGGLE_OUTPUT).unwrap()),
                toggle_foreground_audio: Some(Hotkey::parse(DEFAULT_TOGGLE_FOREGROUND).unwrap()),
            },
            virtual_desktops: VdCfg {
                enabled: true,
                win_number_switching: true,
            },
        }
    }
}

// ---- TOML boundary types ------------------------------------------------

#[derive(Serialize, Deserialize, Default)]
pub struct ConfigToml {
    #[serde(default = "default_schema_version")]
    pub schema_version: u8,
    #[serde(default)]
    pub general: GeneralToml,
    #[serde(default)]
    pub overlay: OverlayToml,
    #[serde(default)]
    pub audio: AudioToml,
    #[serde(default)]
    pub hotkeys: HotkeysToml,
    #[serde(default)]
    pub virtual_desktops: VdToml,
}

fn default_schema_version() -> u8 {
    1
}

#[derive(Serialize, Deserialize, Default)]
pub struct GeneralToml {
    /// Legacy key (#16): registry owns startup now; parsed but ignored+warned.
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default = "default_true")]
    pub start_hotkeys_enabled: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct OverlayToml {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_duration")]
    pub duration_ms: u32,
    #[serde(default = "default_position")]
    pub position: String,
    #[serde(default = "default_monitor")]
    pub monitor: String,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

#[derive(Serialize, Deserialize, Default)]
pub struct AudioToml {
    #[serde(default = "default_role")]
    pub input_role: String,
    #[serde(default = "default_role")]
    pub output_role: String,
    #[serde(default)]
    pub input_device: String,
    #[serde(default)]
    pub output_device: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct HotkeysToml {
    #[serde(default = "default_mic")]
    pub toggle_microphone: String,
    #[serde(default = "default_out")]
    pub toggle_output: String,
    #[serde(default = "default_fg")]
    pub toggle_foreground_audio: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct VdToml {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub win_number_switching: bool,
}

fn default_true() -> bool {
    true
}
fn default_duration() -> u32 {
    1300
}
fn default_position() -> String {
    "bottom-center".into()
}
fn default_monitor() -> String {
    "foreground".into()
}
fn default_scale() -> f32 {
    1.0
}
fn default_opacity() -> f32 {
    1.0
}
fn default_role() -> String {
    "console".into()
}
fn default_mic() -> String {
    DEFAULT_TOGGLE_MICROPHONE.into()
}
fn default_out() -> String {
    DEFAULT_TOGGLE_OUTPUT.into()
}
fn default_fg() -> String {
    DEFAULT_TOGGLE_FOREGROUND.into()
}

impl Config {
    pub fn to_toml(&self) -> ConfigToml {
        ConfigToml {
            schema_version: 1,
            general: GeneralToml {
                // Legacy key never written anymore (#16).
                start_with_windows: false,
                start_hotkeys_enabled: self.general.start_hotkeys_enabled,
            },
            overlay: OverlayToml {
                enabled: self.overlay.enabled,
                duration_ms: self.overlay.duration_ms,
                position: self.overlay.position.as_str().into(),
                monitor: self.overlay.monitor.as_str(),
                scale: self.overlay.scale,
                opacity: self.overlay.opacity,
            },
            audio: AudioToml {
                input_role: self.audio.input_role.as_str().into(),
                output_role: self.audio.output_role.as_str().into(),
                input_device: match &self.audio.input_device {
                    DeviceSelection::Default => "default".into(),
                    DeviceSelection::Endpoint(s) => s.clone(),
                },
                output_device: match &self.audio.output_device {
                    DeviceSelection::Default => "default".into(),
                    DeviceSelection::Endpoint(s) => s.clone(),
                },
            },
            hotkeys: HotkeysToml {
                toggle_microphone: self
                    .hotkeys
                    .toggle_microphone
                    .map_or(String::new(), |h| h.to_string()),
                toggle_output: self
                    .hotkeys
                    .toggle_output
                    .map_or(String::new(), |h| h.to_string()),
                toggle_foreground_audio: self
                    .hotkeys
                    .toggle_foreground_audio
                    .map_or(String::new(), |h| h.to_string()),
            },
            virtual_desktops: VdToml {
                enabled: self.virtual_desktops.enabled,
                win_number_switching: self.virtual_desktops.win_number_switching,
            },
        }
    }

    /// Parse typed config from the TOML boundary. Unknown fields tolerated;
    /// invalid values fall back to defaults per-field (validation reports them).
    pub fn from_toml(t: &ConfigToml) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut c = Config::default();

        if t.general.start_with_windows {
            warnings.push(
                "ignored general.start_with_windows; startup is managed in Settings/tray (registry)".into(),
            );
        }
        c.general.start_hotkeys_enabled = t.general.start_hotkeys_enabled;

        c.overlay.enabled = t.overlay.enabled;
        c.overlay.duration_ms = t.overlay.duration_ms;
        match OverlayPosition::parse(&t.overlay.position) {
            Some(p) => c.overlay.position = p,
            None if t.overlay.position.is_empty() => {}
            None => warnings.push(format!(
                "overlay.position: unknown `{}`",
                t.overlay.position
            )),
        }
        match MonitorChoice::parse(&t.overlay.monitor) {
            Some(m) => c.overlay.monitor = m,
            None if t.overlay.monitor.is_empty() => {}
            None => warnings.push(format!("overlay.monitor: unknown `{}`", t.overlay.monitor)),
        }
        c.overlay.scale = t.overlay.scale;
        c.overlay.opacity = t.overlay.opacity;

        match EndpointRole::parse(&t.audio.input_role) {
            Some(r) => c.audio.input_role = r,
            None => warnings.push(format!(
                "audio.input_role: unknown `{}`",
                t.audio.input_role
            )),
        }
        match EndpointRole::parse(&t.audio.output_role) {
            Some(r) => c.audio.output_role = r,
            None => warnings.push(format!(
                "audio.output_role: unknown `{}`",
                t.audio.output_role
            )),
        }
        for (_field, raw, slot) in [
            (
                "input_device",
                &t.audio.input_device,
                &mut c.audio.input_device,
            ),
            (
                "output_device",
                &t.audio.output_device,
                &mut c.audio.output_device,
            ),
        ] {
            // Windows endpoint IDs are opaque strings (typically
            // "{0.0.0.00000000}.{guid}") — accept any non-empty id (#7).
            if raw.is_empty() || raw == "default" {
                *slot = DeviceSelection::Default;
            } else {
                *slot = DeviceSelection::Endpoint(raw.clone());
            }
        }

        for (field, raw, slot) in [
            (
                "toggle_microphone",
                &t.hotkeys.toggle_microphone,
                &mut c.hotkeys.toggle_microphone,
            ),
            (
                "toggle_output",
                &t.hotkeys.toggle_output,
                &mut c.hotkeys.toggle_output,
            ),
            (
                "toggle_foreground_audio",
                &t.hotkeys.toggle_foreground_audio,
                &mut c.hotkeys.toggle_foreground_audio,
            ),
        ] {
            if raw.is_empty() {
                *slot = None;
            } else {
                match Hotkey::parse(raw) {
                    Ok(h) => *slot = Some(h),
                    Err(e) => warnings.push(format!("hotkeys.{field}: {e}")),
                }
            }
        }

        c.virtual_desktops.enabled = t.virtual_desktops.enabled;
        c.virtual_desktops.win_number_switching = t.virtual_desktops.win_number_switching;

        (c, warnings)
    }
}

/// Known TOML sections/keys for unknown-field warnings (#15c).
/// Known TOML sections/keys for unknown-field warnings (#15c).
pub fn known_keys(section: &str) -> Option<&'static [&'static str]> {
    match section {
        "general" => Some(&["start_hotkeys_enabled", "start_with_windows"]),
        "overlay" => Some(&[
            "enabled",
            "position",
            "monitor",
            "duration_ms",
            "opacity",
            "scale",
        ]),
        "audio" => Some(&["input_device", "output_device", "input_role", "output_role"]),
        "hotkeys" => Some(&[
            "toggle_microphone",
            "toggle_output",
            "toggle_foreground_audio",
        ]),
        "virtual_desktops" => Some(&["enabled", "win_number_switching"]),
        _ => None,
    }
}

impl Config {
    /// Field-level repair for validation violations (#15a): clamp numeric
    /// ranges, drop conflicting hotkeys. Only violated fields are touched.
    pub fn repair(&mut self, violations: &[crate::config::validate::Violation]) {
        let mut drop_hotkeys: Vec<String> = Vec::new();
        for v in violations {
            match v.field.as_str() {
                "overlay.duration_ms" => self.overlay.duration_ms = 2000,
                "overlay.scale" => self.overlay.scale = 1.0,
                "overlay.opacity" => self.overlay.opacity = 0.85,
                f if f.starts_with("hotkeys.") && v.message.contains("conflicts") => {
                    // Conflict-class violations: drop the offending binding.
                    drop_hotkeys.push(f.trim_start_matches("hotkeys.").to_string());
                }
                _ => {}
            }
        }
        for field in drop_hotkeys {
            match field.as_str() {
                "toggle_microphone" => self.hotkeys.toggle_microphone = None,
                "toggle_output" => self.hotkeys.toggle_output = None,
                "toggle_foreground_audio" => self.hotkeys.toggle_foreground_audio = None,
                _ => {}
            }
        }
    }
}
