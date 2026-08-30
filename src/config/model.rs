//! Typed configuration model. Raw strings exist only at the TOML boundary
//! (see load/save); everything past parsing is structured (spec §8).

use serde::{Deserialize, Serialize};

#[allow(unused_imports)]
pub use crate::display::{DisplayProfile, DisplayProfilesCfg, DisplayRoute, DisplayTopology};
use crate::keyboard::binding::{Hotkey, ModifierMask};

pub const DEFAULT_TOGGLE_MICROPHONE: &str = "Ctrl+Alt+M";
pub const DEFAULT_TOGGLE_OUTPUT: &str = "Ctrl+Alt+O";
pub const DEFAULT_TOGGLE_FOREGROUND: &str = "Ctrl+Alt+P";
pub const CURRENT_SCHEMA_VERSION: u8 = 9;
pub const LEGACY_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub general: GeneralCfg,
    pub overlay: OverlayCfg,
    pub audio: AudioCfg,
    pub hotkeys: HotkeysCfg,
    pub virtual_desktops: VdCfg,
    pub display_profiles: DisplayProfilesCfg,
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
    pub appearance: OverlayAppearance,
    pub show_external_audio_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioCfg {
    pub input_role: EndpointRole,
    pub output_role: EndpointRole,
    pub input_device: DeviceSelection,
    pub output_device: DeviceSelection,
    /// `None` means all active capture endpoints; `Some(empty)` means none.
    pub cycle_input_allowlist: Option<Vec<String>>,
    /// `None` means all active render endpoints; `Some(empty)` means none.
    pub cycle_output_allowlist: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayProfileHotkey {
    pub profile_id: String,
    pub hotkey: Hotkey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeysCfg {
    pub toggle_microphone: Option<Hotkey>,
    pub toggle_output: Option<Hotkey>,
    pub toggle_foreground_audio: Option<Hotkey>,
    pub cycle_input_device: Option<Hotkey>,
    pub cycle_output_device: Option<Hotkey>,
    pub foreground_volume_up: Option<Hotkey>,
    pub foreground_volume_down: Option<Hotkey>,
    pub display_profiles: Vec<DisplayProfileHotkey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopRule {
    /// Executable basename (with optional `.exe`) or a full image path.
    pub executable: String,
    /// One-based virtual desktop number; missing desktops are created on demand.
    pub desktop: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdCfg {
    pub enabled: bool,
    pub win_number_switching: bool,
    pub number_modifier: ModifierMask,
    pub move_follow_modifier: Option<ModifierMask>,
    pub move_silent_modifier: Option<ModifierMask>,
    pub previous_desktop: Option<Hotkey>,
    pub scratchpad_assign: Option<Hotkey>,
    pub scratchpad_toggle: Option<Hotkey>,
    pub routing_rules: Vec<DesktopRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAppearance {
    System,
    Dark,
    Light,
}

impl OverlayAppearance {
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Follow System",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "system" => Self::System,
            "dark" => Self::Dark,
            "light" => Self::Light,
            _ => return None,
        })
    }
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
                appearance: OverlayAppearance::System,
                show_external_audio_changes: true,
            },
            audio: AudioCfg {
                input_role: EndpointRole::Console,
                output_role: EndpointRole::Console,
                input_device: DeviceSelection::Default,
                output_device: DeviceSelection::Default,
                cycle_input_allowlist: None,
                cycle_output_allowlist: None,
            },
            hotkeys: HotkeysCfg {
                toggle_microphone: Some(Hotkey::parse(DEFAULT_TOGGLE_MICROPHONE).unwrap()),
                toggle_output: Some(Hotkey::parse(DEFAULT_TOGGLE_OUTPUT).unwrap()),
                toggle_foreground_audio: Some(Hotkey::parse(DEFAULT_TOGGLE_FOREGROUND).unwrap()),
                cycle_input_device: None,
                cycle_output_device: None,
                foreground_volume_up: None,
                foreground_volume_down: None,
                display_profiles: Vec::new(),
            },
            virtual_desktops: VdCfg {
                enabled: true,
                win_number_switching: true,
                number_modifier: ModifierMask::WIN,
                move_follow_modifier: None,
                move_silent_modifier: None,
                previous_desktop: None,
                scratchpad_assign: None,
                scratchpad_toggle: None,
                routing_rules: Vec::new(),
            },
            display_profiles: DisplayProfilesCfg::default(),
        }
    }
}

// ---- TOML boundary types ------------------------------------------------

#[derive(Serialize, Deserialize)]
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
    #[serde(default)]
    pub display_profiles: DisplayProfilesCfg,
}

impl Default for ConfigToml {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            general: GeneralToml::default(),
            overlay: OverlayToml::default(),
            audio: AudioToml::default(),
            hotkeys: HotkeysToml::default(),
            virtual_desktops: VdToml::default(),
            display_profiles: DisplayProfilesCfg::default(),
        }
    }
}

fn default_schema_version() -> u8 {
    LEGACY_SCHEMA_VERSION
}

#[derive(Serialize, Deserialize)]
pub struct GeneralToml {
    /// Legacy key (#16): registry owns startup now; parsed but ignored+warned.
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default = "default_true")]
    pub start_hotkeys_enabled: bool,
}

impl Default for GeneralToml {
    fn default() -> Self {
        Self {
            start_with_windows: false,
            start_hotkeys_enabled: true,
        }
    }
}

#[derive(Serialize, Deserialize)]
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
    #[serde(default = "default_appearance")]
    pub appearance: String,
    #[serde(default = "default_show_external_audio_changes")]
    pub show_external_audio_changes: bool,
}

impl Default for OverlayToml {
    fn default() -> Self {
        Self {
            enabled: true,
            duration_ms: default_duration(),
            position: default_position(),
            monitor: default_monitor(),
            scale: default_scale(),
            opacity: default_opacity(),
            appearance: default_appearance(),
            show_external_audio_changes: default_show_external_audio_changes(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AudioToml {
    #[serde(default = "default_role")]
    pub input_role: String,
    #[serde(default = "default_role")]
    pub output_role: String,
    #[serde(default)]
    pub input_device: String,
    #[serde(default)]
    pub output_device: String,
    /// Absent means all active endpoints; an explicit empty list disables cycling.
    #[serde(default)]
    pub cycle_input_allowlist: Option<Vec<String>>,
    #[serde(default)]
    pub cycle_output_allowlist: Option<Vec<String>>,
}

impl Default for AudioToml {
    fn default() -> Self {
        Self {
            input_role: default_role(),
            output_role: default_role(),
            input_device: String::new(),
            output_device: String::new(),
            cycle_input_allowlist: None,
            cycle_output_allowlist: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct DisplayProfileHotkeyToml {
    pub profile_id: String,
    pub hotkey: String,
}

#[derive(Serialize, Deserialize)]
pub struct HotkeysToml {
    #[serde(default = "default_mic")]
    pub toggle_microphone: String,
    #[serde(default = "default_out")]
    pub toggle_output: String,
    #[serde(default = "default_fg")]
    pub toggle_foreground_audio: String,
    #[serde(default)]
    pub cycle_input_device: String,
    #[serde(default)]
    pub cycle_output_device: String,
    #[serde(default)]
    pub foreground_volume_up: String,
    #[serde(default)]
    pub foreground_volume_down: String,
    #[serde(default)]
    pub display_profiles: Vec<DisplayProfileHotkeyToml>,
}

impl Default for HotkeysToml {
    fn default() -> Self {
        Self {
            toggle_microphone: default_mic(),
            toggle_output: default_out(),
            toggle_foreground_audio: default_fg(),
            cycle_input_device: String::new(),
            cycle_output_device: String::new(),
            foreground_volume_up: String::new(),
            foreground_volume_down: String::new(),
            display_profiles: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct DesktopRuleToml {
    pub executable: String,
    pub desktop: u16,
}

#[derive(Serialize, Deserialize)]
pub struct VdToml {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub win_number_switching: bool,
    #[serde(default = "default_number_modifier")]
    pub number_modifier: String,
    #[serde(default)]
    pub move_follow_modifier: String,
    #[serde(default)]
    pub move_silent_modifier: String,
    #[serde(default)]
    pub previous_desktop: String,
    #[serde(default)]
    pub scratchpad_assign: String,
    #[serde(default)]
    pub scratchpad_toggle: String,
    #[serde(default)]
    pub routing_rules: Vec<DesktopRuleToml>,
}

impl Default for VdToml {
    fn default() -> Self {
        Self {
            enabled: true,
            win_number_switching: true,
            number_modifier: default_number_modifier(),
            move_follow_modifier: String::new(),
            move_silent_modifier: String::new(),
            previous_desktop: String::new(),
            scratchpad_assign: String::new(),
            scratchpad_toggle: String::new(),
            routing_rules: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_number_modifier() -> String {
    "Win".into()
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
fn default_appearance() -> String {
    "system".into()
}
fn default_show_external_audio_changes() -> bool {
    true
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

fn parse_modifier(
    raw: &str,
    default: ModifierMask,
    field: &str,
    warnings: &mut Vec<String>,
) -> ModifierMask {
    if raw.trim().is_empty() {
        return default;
    }
    match ModifierMask::parse(raw) {
        Ok(modifier) => modifier,
        Err(error) => {
            warnings.push(format!("virtual_desktops.{field}: {error}"));
            default
        }
    }
}

fn parse_optional_modifier(
    raw: &str,
    field: &str,
    warnings: &mut Vec<String>,
) -> Option<ModifierMask> {
    if raw.trim().is_empty() {
        return None;
    }
    match ModifierMask::parse(raw) {
        Ok(modifier) => Some(modifier),
        Err(error) => {
            warnings.push(format!("virtual_desktops.{field}: {error}"));
            None
        }
    }
}

impl Config {
    pub fn to_toml(&self) -> ConfigToml {
        ConfigToml {
            schema_version: CURRENT_SCHEMA_VERSION,
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
                appearance: self.overlay.appearance.as_str().into(),
                show_external_audio_changes: self.overlay.show_external_audio_changes,
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
                cycle_input_allowlist: self.audio.cycle_input_allowlist.clone(),
                cycle_output_allowlist: self.audio.cycle_output_allowlist.clone(),
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
                cycle_input_device: self
                    .hotkeys
                    .cycle_input_device
                    .map_or(String::new(), |h| h.to_string()),
                cycle_output_device: self
                    .hotkeys
                    .cycle_output_device
                    .map_or(String::new(), |h| h.to_string()),
                foreground_volume_up: self
                    .hotkeys
                    .foreground_volume_up
                    .map_or(String::new(), |h| h.to_string()),
                foreground_volume_down: self
                    .hotkeys
                    .foreground_volume_down
                    .map_or(String::new(), |h| h.to_string()),
                display_profiles: self
                    .hotkeys
                    .display_profiles
                    .iter()
                    .map(|binding| DisplayProfileHotkeyToml {
                        profile_id: binding.profile_id.clone(),
                        hotkey: binding.hotkey.to_string(),
                    })
                    .collect(),
            },
            virtual_desktops: VdToml {
                enabled: self.virtual_desktops.enabled,
                win_number_switching: self.virtual_desktops.win_number_switching,
                number_modifier: self.virtual_desktops.number_modifier.to_string(),
                move_follow_modifier: self
                    .virtual_desktops
                    .move_follow_modifier
                    .map_or(String::new(), |modifier| modifier.to_string()),
                move_silent_modifier: self
                    .virtual_desktops
                    .move_silent_modifier
                    .map_or(String::new(), |modifier| modifier.to_string()),
                previous_desktop: self
                    .virtual_desktops
                    .previous_desktop
                    .map_or(String::new(), |hotkey| hotkey.to_string()),
                scratchpad_assign: self
                    .virtual_desktops
                    .scratchpad_assign
                    .map_or(String::new(), |hotkey| hotkey.to_string()),
                scratchpad_toggle: self
                    .virtual_desktops
                    .scratchpad_toggle
                    .map_or(String::new(), |hotkey| hotkey.to_string()),
                routing_rules: self
                    .virtual_desktops
                    .routing_rules
                    .iter()
                    .map(|rule| DesktopRuleToml {
                        executable: rule.executable.clone(),
                        desktop: rule.desktop,
                    })
                    .collect(),
            },
            display_profiles: self.display_profiles.clone(),
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
        match OverlayAppearance::parse(&t.overlay.appearance) {
            Some(appearance) => c.overlay.appearance = appearance,
            None => warnings.push(format!(
                "overlay.appearance: unknown `{}`",
                t.overlay.appearance
            )),
        }
        c.overlay.show_external_audio_changes = t.overlay.show_external_audio_changes;

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
        c.audio.cycle_input_allowlist = t.audio.cycle_input_allowlist.clone();
        c.audio.cycle_output_allowlist = t.audio.cycle_output_allowlist.clone();

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
            (
                "cycle_input_device",
                &t.hotkeys.cycle_input_device,
                &mut c.hotkeys.cycle_input_device,
            ),
            (
                "cycle_output_device",
                &t.hotkeys.cycle_output_device,
                &mut c.hotkeys.cycle_output_device,
            ),
            (
                "foreground_volume_up",
                &t.hotkeys.foreground_volume_up,
                &mut c.hotkeys.foreground_volume_up,
            ),
            (
                "foreground_volume_down",
                &t.hotkeys.foreground_volume_down,
                &mut c.hotkeys.foreground_volume_down,
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
        for (index, binding) in t.hotkeys.display_profiles.iter().enumerate() {
            let profile_id = binding.profile_id.trim();
            if profile_id.is_empty() {
                warnings.push(format!(
                    "hotkeys.display_profiles[{index}].profile_id: must not be empty"
                ));
                continue;
            }
            match Hotkey::parse(&binding.hotkey) {
                Ok(hotkey) => c.hotkeys.display_profiles.push(DisplayProfileHotkey {
                    profile_id: profile_id.to_string(),
                    hotkey,
                }),
                Err(error) => {
                    warnings.push(format!("hotkeys.display_profiles[{index}].hotkey: {error}"))
                }
            }
        }

        c.virtual_desktops.enabled = t.virtual_desktops.enabled;
        c.virtual_desktops.win_number_switching = t.virtual_desktops.win_number_switching;
        c.virtual_desktops.number_modifier = parse_modifier(
            &t.virtual_desktops.number_modifier,
            ModifierMask::WIN,
            "number_modifier",
            &mut warnings,
        );
        c.virtual_desktops.move_follow_modifier = parse_optional_modifier(
            &t.virtual_desktops.move_follow_modifier,
            "move_follow_modifier",
            &mut warnings,
        );
        c.virtual_desktops.move_silent_modifier = parse_optional_modifier(
            &t.virtual_desktops.move_silent_modifier,
            "move_silent_modifier",
            &mut warnings,
        );
        if !t.virtual_desktops.previous_desktop.trim().is_empty() {
            match Hotkey::parse(&t.virtual_desktops.previous_desktop) {
                Ok(hotkey) => c.virtual_desktops.previous_desktop = Some(hotkey),
                Err(error) => warnings.push(format!("virtual_desktops.previous_desktop: {error}")),
            }
        }
        for (field, raw, slot) in [
            (
                "scratchpad_assign",
                &t.virtual_desktops.scratchpad_assign,
                &mut c.virtual_desktops.scratchpad_assign,
            ),
            (
                "scratchpad_toggle",
                &t.virtual_desktops.scratchpad_toggle,
                &mut c.virtual_desktops.scratchpad_toggle,
            ),
        ] {
            if raw.trim().is_empty() {
                *slot = None;
            } else {
                match Hotkey::parse(raw) {
                    Ok(hotkey) => *slot = Some(hotkey),
                    Err(error) => warnings.push(format!("virtual_desktops.{field}: {error}")),
                }
            }
        }
        c.virtual_desktops.routing_rules = t
            .virtual_desktops
            .routing_rules
            .iter()
            .map(|rule| DesktopRule {
                executable: rule.executable.clone(),
                desktop: rule.desktop,
            })
            .collect();
        c.display_profiles = t.display_profiles.clone();

        (c, warnings)
    }
}

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
            "appearance",
            "show_external_audio_changes",
        ]),
        "audio" => Some(&[
            "input_device",
            "output_device",
            "input_role",
            "output_role",
            "cycle_input_allowlist",
            "cycle_output_allowlist",
        ]),
        "hotkeys" => Some(&[
            "toggle_microphone",
            "toggle_output",
            "toggle_foreground_audio",
            "cycle_input_device",
            "cycle_output_device",
            "foreground_volume_up",
            "foreground_volume_down",
            "display_profiles",
        ]),
        "virtual_desktops" => Some(&[
            "enabled",
            "win_number_switching",
            "number_modifier",
            "move_follow_modifier",
            "move_silent_modifier",
            "previous_desktop",
            "scratchpad_assign",
            "scratchpad_toggle",
            "routing_rules",
        ]),
        "display_profiles" => Some(&["enabled", "active_profile", "profiles"]),
        _ => None,
    }
}

impl Config {
    /// Field-level repair for validation violations (#15a): clamp numeric
    /// ranges, drop conflicting hotkeys. Only violated fields are touched.
    pub fn repair(&mut self, violations: &[crate::config::validate::Violation]) {
        let mut drop_hotkeys: Vec<String> = Vec::new();
        let routing_rules_invalid = violations.iter().any(|violation| {
            violation
                .field
                .starts_with("virtual_desktops.routing_rules[")
        });
        let input_allowlist_invalid = violations
            .iter()
            .any(|violation| violation.field.starts_with("audio.cycle_input_allowlist["));
        let output_allowlist_invalid = violations
            .iter()
            .any(|violation| violation.field.starts_with("audio.cycle_output_allowlist["));
        let display_profiles_invalid = violations
            .iter()
            .any(|violation| violation.field.starts_with("display_profiles."));
        let profile_hotkeys_invalid = violations
            .iter()
            .any(|violation| violation.field.starts_with("hotkeys.display_profiles"));
        for v in violations {
            match v.field.as_str() {
                "overlay.duration_ms" => self.overlay.duration_ms = 2000,
                "overlay.scale" => self.overlay.scale = 1.0,

                "overlay.opacity" => self.overlay.opacity = 0.85,
                "virtual_desktops.number_modifier" => {
                    self.virtual_desktops.number_modifier = ModifierMask::WIN
                }
                "virtual_desktops.move_follow_modifier" if v.message.contains("conflicts") => {
                    self.virtual_desktops.move_follow_modifier = None;
                }
                "virtual_desktops.move_silent_modifier" if v.message.contains("conflicts") => {
                    self.virtual_desktops.move_silent_modifier = None;
                }
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
                "cycle_input_device" => self.hotkeys.cycle_input_device = None,
                "cycle_output_device" => self.hotkeys.cycle_output_device = None,
                "foreground_volume_up" => self.hotkeys.foreground_volume_up = None,
                "foreground_volume_down" => self.hotkeys.foreground_volume_down = None,
                "previous_desktop" => self.virtual_desktops.previous_desktop = None,
                "scratchpad_assign" => self.virtual_desktops.scratchpad_assign = None,
                "scratchpad_toggle" => self.virtual_desktops.scratchpad_toggle = None,
                _ => {}
            }
        }
        if routing_rules_invalid {
            let mut seen = std::collections::HashSet::new();
            self.virtual_desktops.routing_rules.retain(|rule| {
                let executable = rule.executable.trim();
                (1..=256).contains(&rule.desktop)
                    && !executable.is_empty()
                    && seen.insert(executable.to_ascii_lowercase())
            });
        }
        for (invalid, allowlist) in [
            (
                input_allowlist_invalid,
                &mut self.audio.cycle_input_allowlist,
            ),
            (
                output_allowlist_invalid,
                &mut self.audio.cycle_output_allowlist,
            ),
        ] {
            if invalid {
                if let Some(ids) = allowlist {
                    let mut seen = std::collections::HashSet::new();
                    ids.retain(|id| !id.trim().is_empty() && seen.insert(id.clone()));
                }
            }
        }
        if display_profiles_invalid {
            let mut seen_profiles = std::collections::HashSet::new();
            let mut seen_names = std::collections::HashSet::new();
            self.display_profiles.profiles.retain(|profile| {
                let id = profile.id.trim();
                let mut seen_routes = std::collections::HashSet::new();
                !id.is_empty()
                    && !profile.name.trim().is_empty()
                    && !profile.routes.is_empty()
                    && profile.routes.len() <= 32
                    && profile.routes.iter().all(|route| {
                        !route.target_path.trim().is_empty()
                            && route.source_width > 0
                            && route.source_height > 0
                            && route.active_width > 0
                            && route.active_height > 0
                            && route.refresh_numerator > 0
                            && route.refresh_denominator > 0
                            && matches!(route.rotation, 1..=4)
                            && seen_routes.insert(format!(
                                "{}|{}|{}|{}|{}",
                                route.target_path.trim().to_ascii_lowercase(),
                                route.source_adapter,
                                route.source_id,
                                route.target_adapter,
                                route.target_id
                            ))
                    })
                    && crate::display::validate_profile(profile).is_ok()
                    && seen_profiles.insert(id.to_ascii_lowercase())
                    && seen_names.insert(profile.name.trim().to_ascii_lowercase())
            });
            self.display_profiles
                .profiles
                .truncate(crate::display::MAX_PROFILES);
            if self
                .display_profiles
                .active_profile
                .as_deref()
                .is_some_and(|active| {
                    !self
                        .display_profiles
                        .profiles
                        .iter()
                        .any(|profile| profile.id.eq_ignore_ascii_case(active))
                })
            {
                self.display_profiles.active_profile = None;
            }
        }
        if display_profiles_invalid || profile_hotkeys_invalid {
            let valid_profiles = self
                .display_profiles
                .profiles
                .iter()
                .map(|profile| profile.id.trim().to_ascii_lowercase())
                .collect::<std::collections::HashSet<_>>();
            let mut used_profile_keys = std::collections::HashSet::new();
            let mut used = std::collections::HashSet::new();
            for hotkey in [
                self.hotkeys.toggle_microphone,
                self.hotkeys.toggle_output,
                self.hotkeys.toggle_foreground_audio,
                self.hotkeys.cycle_input_device,
                self.hotkeys.cycle_output_device,
                self.hotkeys.foreground_volume_up,
                self.hotkeys.foreground_volume_down,
                self.virtual_desktops.previous_desktop,
                self.virtual_desktops.scratchpad_assign,
                self.virtual_desktops.scratchpad_toggle,
            ]
            .into_iter()
            .flatten()
            {
                used.insert(hotkey);
            }
            if self.virtual_desktops.enabled {
                for modifier in [
                    self.virtual_desktops
                        .win_number_switching
                        .then_some(self.virtual_desktops.number_modifier),
                    self.virtual_desktops.move_follow_modifier,
                    self.virtual_desktops.move_silent_modifier,
                ]
                .into_iter()
                .flatten()
                .filter(|modifier| !modifier.is_empty())
                {
                    for number in 1u16..=9 {
                        used.insert(Hotkey {
                            modifiers: modifier,
                            key: crate::keyboard::binding::VirtualKey(0x30 + number),
                        });
                    }
                }
            }
            self.hotkeys.display_profiles.retain(|binding| {
                valid_profiles.contains(&binding.profile_id.trim().to_ascii_lowercase())
                    && used_profile_keys.insert(crate::display::profile_id_key(&binding.profile_id))
                    && used.insert(binding.hotkey)
            });
            self.hotkeys.display_profiles.truncate(u8::MAX as usize + 1);
        }
    }
}

#[cfg(test)]
mod hotkey_schema_tests {
    use super::*;

    #[test]
    fn defaults_leave_phase_one_hotkeys_unassigned() {
        let config = Config::default();
        assert!(config.hotkeys.cycle_input_device.is_none());
        assert!(config.hotkeys.cycle_output_device.is_none());
        assert!(config.hotkeys.foreground_volume_up.is_none());
        assert!(config.hotkeys.foreground_volume_down.is_none());
    }

    #[test]
    fn schema_v2_preserves_existing_values_and_defaults_new_hotkeys() {
        let raw = r#"
schema_version = 2

[general]
start_hotkeys_enabled = false

[overlay]
appearance = "dark"
show_external_audio_changes = false

[audio]
output_device = "opaque-output-id"

[hotkeys]
toggle_microphone = "Ctrl+Alt+F1"
toggle_output = "Ctrl+Alt+F2"
toggle_foreground_audio = "Ctrl+Alt+F3"
"#;
        let boundary: ConfigToml = toml::from_str(raw).unwrap();
        let (config, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!config.general.start_hotkeys_enabled);
        assert_eq!(config.overlay.appearance, OverlayAppearance::Dark);
        assert!(!config.overlay.show_external_audio_changes);
        assert_eq!(
            config.audio.output_device,
            DeviceSelection::Endpoint("opaque-output-id".into())
        );
        assert_eq!(
            config.hotkeys.toggle_microphone,
            Some(Hotkey::parse("Ctrl+Alt+F1").unwrap())
        );
        assert_eq!(
            config.hotkeys.toggle_output,
            Some(Hotkey::parse("Ctrl+Alt+F2").unwrap())
        );
        assert_eq!(
            config.hotkeys.toggle_foreground_audio,
            Some(Hotkey::parse("Ctrl+Alt+F3").unwrap())
        );
        assert!(config.hotkeys.cycle_input_device.is_none());
        assert!(config.hotkeys.cycle_output_device.is_none());
        assert!(config.hotkeys.foreground_volume_up.is_none());
        assert!(config.hotkeys.foreground_volume_down.is_none());
    }

    #[test]
    fn assigned_phase_one_hotkeys_round_trip_through_toml() {
        let mut config = Config::default();
        config.hotkeys.cycle_input_device = Some(Hotkey::parse("Ctrl+Alt+F4").unwrap());
        config.hotkeys.cycle_output_device = Some(Hotkey::parse("Ctrl+Alt+F5").unwrap());
        config.hotkeys.foreground_volume_up = Some(Hotkey::parse("Ctrl+Alt+F6").unwrap());
        config.hotkeys.foreground_volume_down = Some(Hotkey::parse("Ctrl+Alt+F7").unwrap());

        let text = toml::to_string_pretty(&config.to_toml()).unwrap();
        let boundary: ConfigToml = toml::from_str(&text).unwrap();
        assert_eq!(boundary.schema_version, CURRENT_SCHEMA_VERSION);
        let (round_tripped, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(round_tripped, config);
    }

    #[test]
    fn desktop_chord_fields_round_trip_through_schema_v4() {
        let mut config = Config::default();
        config.virtual_desktops.number_modifier = ModifierMask::CTRL.union(ModifierMask::ALT);
        config.virtual_desktops.move_follow_modifier = Some(ModifierMask::WIN);
        config.virtual_desktops.move_silent_modifier =
            Some(ModifierMask::CTRL.union(ModifierMask::WIN));
        config.virtual_desktops.previous_desktop = Some(Hotkey::parse("Ctrl+Alt+F8").unwrap());

        let text = toml::to_string_pretty(&config.to_toml()).unwrap();
        let boundary: ConfigToml = toml::from_str(&text).unwrap();
        assert_eq!(boundary.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(boundary.virtual_desktops.number_modifier, "Ctrl+Alt");
        let (round_tripped, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(round_tripped, config);
    }

    #[test]
    fn scratchpad_hotkeys_round_trip_through_schema_v5() {
        let mut config = Config::default();
        config.virtual_desktops.scratchpad_assign = Some(Hotkey::parse("Ctrl+Alt+F9").unwrap());
        config.virtual_desktops.scratchpad_toggle = Some(Hotkey::parse("Ctrl+Alt+F10").unwrap());

        let text = toml::to_string_pretty(&config.to_toml()).unwrap();
        let boundary: ConfigToml = toml::from_str(&text).unwrap();
        assert_eq!(boundary.schema_version, CURRENT_SCHEMA_VERSION);
        let (round_tripped, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(round_tripped, config);
    }

    #[test]
    fn audio_allowlists_round_trip_through_schema_v7() {
        let mut config = Config::default();
        config.audio.cycle_input_allowlist = Some(vec!["capture-a".into(), "capture-b".into()]);
        config.audio.cycle_output_allowlist = Some(Vec::new());

        let text = toml::to_string_pretty(&config.to_toml()).unwrap();
        let boundary: ConfigToml = toml::from_str(&text).unwrap();
        assert_eq!(boundary.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(
            boundary.audio.cycle_input_allowlist,
            Some(vec!["capture-a".to_string(), "capture-b".to_string()])
        );
        assert_eq!(boundary.audio.cycle_output_allowlist, Some(Vec::new()));
        let (round_tripped, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(round_tripped, config);
    }

    #[test]
    fn schema_v6_defaults_new_audio_allowlists() {
        let raw = r#"
schema_version = 6
[audio]
input_device = "default"
output_device = "default"
"#;
        let boundary: ConfigToml = toml::from_str(raw).unwrap();
        let (config, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(config.audio.cycle_input_allowlist.is_none());
        assert!(config.audio.cycle_output_allowlist.is_none());
    }

    #[test]
    fn display_profile_hotkeys_round_trip_by_stable_id() {
        let mut config = Config::default();
        config.hotkeys.display_profiles = vec![DisplayProfileHotkey {
            profile_id: "gaming-id".into(),
            hotkey: Hotkey::parse("Ctrl+Alt+F11").unwrap(),
        }];
        let text = toml::to_string_pretty(&config.to_toml()).unwrap();
        let boundary: ConfigToml = toml::from_str(&text).unwrap();
        assert_eq!(boundary.schema_version, CURRENT_SCHEMA_VERSION);
        let (round_tripped, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            round_tripped.hotkeys.display_profiles,
            config.hotkeys.display_profiles
        );
    }

    #[test]
    fn schema_v8_defaults_display_profile_hotkeys_and_confirmation() {
        let raw = r#"
schema_version = 8
[hotkeys]
[display_profiles]
[[display_profiles.profiles]]
id = "legacy"
name = "Legacy"
topology = "extend"
"#;
        let boundary: ConfigToml = toml::from_str(raw).unwrap();
        let (config, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(config.hotkeys.display_profiles.is_empty());
        assert_eq!(config.display_profiles.profiles.len(), 1);
        assert!(!config.display_profiles.profiles[0].confirmed);
    }

    #[test]
    fn display_profiles_round_trip_through_schema_v8() {
        let mut config = Config::default();
        config.display_profiles.active_profile = Some("work".into());
        config.display_profiles.profiles = vec![DisplayProfile {
            id: "work".into(),
            name: "Work".into(),
            topology: DisplayTopology::Extend,
            confirmed: false,
            routes: vec![DisplayRoute {
                target_path: r"\\?\DISPLAY#MONITOR-A".into(),
                source_id: 1,
                target_id: 2,
                source_width: 1920,
                source_height: 1080,
                ..Default::default()
            }],
        }];

        let text = toml::to_string_pretty(&config.to_toml()).unwrap();
        let boundary: ConfigToml = toml::from_str(&text).unwrap();
        assert_eq!(boundary.schema_version, CURRENT_SCHEMA_VERSION);
        let (round_tripped, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(round_tripped, config);
    }

    #[test]
    fn schema_v7_defaults_new_display_profiles() {
        let raw = "schema_version = 7\n[display_profiles]\nenabled = true\n";
        let boundary: ConfigToml = toml::from_str(raw).unwrap();
        let (config, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(config.display_profiles.enabled);
        assert!(config.display_profiles.profiles.is_empty());
        assert!(config.display_profiles.active_profile.is_none());
    }

    #[test]
    fn executable_routing_rules_round_trip_through_schema_v6() {
        let mut config = Config::default();
        config.virtual_desktops.routing_rules = vec![
            DesktopRule {
                executable: "notepad.exe".into(),
                desktop: 2,
            },
            DesktopRule {
                executable: r"C:\Apps\Player.exe".into(),
                desktop: 8,
            },
        ];

        let text = toml::to_string_pretty(&config.to_toml()).unwrap();
        let boundary: ConfigToml = toml::from_str(&text).unwrap();
        assert_eq!(boundary.schema_version, CURRENT_SCHEMA_VERSION);
        let (round_tripped, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(round_tripped, config);
    }

    #[test]
    fn schema_v5_defaults_new_executable_routing_rules() {
        let raw = r#"
schema_version = 5
[virtual_desktops]
enabled = true
win_number_switching = true
number_modifier = "Win"
"#;
        let boundary: ConfigToml = toml::from_str(raw).unwrap();
        let (config, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(config.virtual_desktops.routing_rules.is_empty());
    }

    #[test]
    fn schema_v4_defaults_new_scratchpad_hotkeys() {
        let raw = r#"
schema_version = 4
[virtual_desktops]
enabled = true
win_number_switching = true
number_modifier = "Win"
"#;
        let boundary: ConfigToml = toml::from_str(raw).unwrap();
        let (config, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(config.virtual_desktops.scratchpad_assign.is_none());
        assert!(config.virtual_desktops.scratchpad_toggle.is_none());
    }

    #[test]
    fn schema_v3_defaults_new_desktop_controls() {
        let raw = r#"
schema_version = 3
[virtual_desktops]
enabled = true
win_number_switching = true
"#;
        let boundary: ConfigToml = toml::from_str(raw).unwrap();
        let (config, warnings) = Config::from_toml(&boundary);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.virtual_desktops.number_modifier, ModifierMask::WIN);
        assert!(config.virtual_desktops.move_follow_modifier.is_none());
        assert!(config.virtual_desktops.move_silent_modifier.is_none());
        assert!(config.virtual_desktops.previous_desktop.is_none());
    }
}
