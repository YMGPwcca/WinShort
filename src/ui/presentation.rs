//! User-facing presentation models for the Control Center.
//!
//! Opaque Windows identities stay in the configuration/runtime layers. This
//! module turns those identities into truthful labels, action copy, and small
//! state policies that can be tested without a window or COM.

use crate::audio::DeviceId;
use crate::config::model::{DeviceSelection, MonitorChoice};
use crate::keyboard::binding::{Hotkey, ModifierMask};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDeviceKind {
    Speaker,
    Microphone,
}

impl AudioDeviceKind {
    pub const fn noun(self) -> &'static str {
        match self {
            Self::Speaker => "speaker",
            Self::Microphone => "microphone",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendlyLabel {
    pub primary: String,
    pub detail: Option<String>,
}

impl FriendlyLabel {
    pub fn compact(&self) -> String {
        match &self.detail {
            Some(detail) if !detail.is_empty() => format!("{} · {detail}", self.primary),
            _ => self.primary.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSelectionPresentation {
    pub primary: String,
    pub secondary: Option<String>,
    pub status: Option<String>,
}

impl DeviceSelectionPresentation {
    pub fn accessible_value(&self) -> String {
        [
            Some(self.primary.as_str()),
            self.secondary.as_deref(),
            self.status.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }
}

/// The three meanings represented by the persisted `Option<Vec<String>>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowlistMode {
    All,
    Selected,
    Disabled,
}

pub fn allowlist_mode(value: Option<&[String]>) -> AllowlistMode {
    match value {
        None => AllowlistMode::All,
        Some([]) => AllowlistMode::Disabled,
        Some(_) => AllowlistMode::Selected,
    }
}

pub fn allowlist_mode_label(mode: AllowlistMode, kind: AudioDeviceKind) -> &'static str {
    match mode {
        AllowlistMode::All => match kind {
            AudioDeviceKind::Speaker => "All available speakers",
            AudioDeviceKind::Microphone => "All available microphones",
        },
        AllowlistMode::Selected => match kind {
            AudioDeviceKind::Speaker => "Selected speakers",
            AudioDeviceKind::Microphone => "Selected microphones",
        },
        AllowlistMode::Disabled => match kind {
            AudioDeviceKind::Speaker => "Don't cycle speakers",
            AudioDeviceKind::Microphone => "Don't cycle microphones",
        },
    }
}

/// Normalize only the index wrappers emitted by WinShort's old enumeration
/// presentation. Arbitrary user-provided friendly names remain untouched.
pub fn friendly_device_name(name: &str, kind: AudioDeviceKind) -> FriendlyLabel {
    let original = name.trim();
    let (without_prefix, _) = strip_index_wrapper(original);
    let (without_suffix, mut detail) = strip_index_suffix(without_prefix);
    let mut primary = without_suffix.trim().to_string();
    let fallback = primary.clone();
    let categories = match kind {
        AudioDeviceKind::Speaker => ["speakers", "speaker"],
        AudioDeviceKind::Microphone => ["microphones", "microphone"],
    };
    for category in categories {
        if primary.eq_ignore_ascii_case(category) {
            primary.clear();
            break;
        }
        let Some(prefix) = primary.get(..category.len()) else {
            continue;
        };
        let rest = &primary[category.len()..];
        if !prefix.eq_ignore_ascii_case(category)
            || !rest.chars().next().is_some_and(char::is_whitespace)
        {
            continue;
        }
        primary = rest.trim().to_string();
        if primary.starts_with('(') && primary.ends_with(')') && primary.len() > 2 {
            primary = primary[1..primary.len() - 1].trim().to_string();
        }
        break;
    }
    if primary.is_empty() {
        primary = detail
            .take()
            .filter(|value| !value.is_empty())
            .or_else(|| (!fallback.is_empty()).then_some(fallback))
            .unwrap_or_else(|| "Unknown device".into());
    }
    FriendlyLabel {
        primary,
        detail: detail.filter(|value| !value.is_empty()),
    }
}

pub fn friendly_device(device: &DeviceId, kind: AudioDeviceKind) -> FriendlyLabel {
    friendly_device_name(&device.name, kind)
}

pub fn device_selection_presentation(
    _selection: &DeviceSelection,
    _devices: &[DeviceId],
    default: Option<&DeviceId>,
    kind: AudioDeviceKind,
) -> DeviceSelectionPresentation {
    if let Some(device) = default {
        let label = friendly_device(device, kind);
        DeviceSelectionPresentation {
            primary: label.primary,
            secondary: label.detail,
            status: None,
        }
    } else {
        DeviceSelectionPresentation {
            primary: "Windows default unavailable".into(),
            secondary: None,
            status: None,
        }
    }
}

pub fn device_choice_label(
    device: &DeviceId,
    _default: Option<&DeviceId>,
    kind: AudioDeviceKind,
) -> String {
    friendly_device(device, kind).compact()
}
pub fn device_choice_label_at(
    devices: &[DeviceId],
    index: usize,
    default: Option<&DeviceId>,
    kind: AudioDeviceKind,
) -> Option<String> {
    let device = devices.get(index)?;
    let base = device_choice_label(device, default, kind);
    let primary = friendly_device(device, kind).primary;
    let duplicate_index = devices
        .iter()
        .take(index)
        .filter(|candidate| friendly_device(candidate, kind).primary == primary)
        .count();
    let duplicate_count = devices
        .iter()
        .filter(|candidate| friendly_device(candidate, kind).primary == primary)
        .count();
    if duplicate_count > 1 {
        Some(format!("{base} · Option {}", duplicate_index + 1))
    } else {
        Some(base)
    }
}

pub fn display_output_label(
    monitor_name: &str,
    adapter_name: &str,
    connector_name: &str,
    active: bool,
) -> FriendlyLabel {
    let (monitor_name, _) = strip_index_wrapper(monitor_name.trim());
    let primary = if monitor_name.is_empty() {
        "Display".to_string()
    } else {
        monitor_name.to_string()
    };
    let mut details = Vec::new();
    for value in [adapter_name, connector_name] {
        let (value, _) = strip_index_wrapper(value.trim());
        if !value.is_empty()
            && !details
                .iter()
                .any(|item: &&str| item.eq_ignore_ascii_case(value))
        {
            details.push(value);
        }
    }
    if active {
        details.push("Active now");
    }
    FriendlyLabel {
        primary,
        detail: (!details.is_empty()).then(|| details.join(" · ")),
    }
}

pub fn monitor_choice_label(choice: &MonitorChoice) -> String {
    match choice {
        MonitorChoice::Foreground => "App's monitor".into(),
        MonitorChoice::Primary => "Primary monitor".into(),
        MonitorChoice::Device(_) => "Saved monitor".into(),
    }
}

pub fn format_modifier(modifier: ModifierMask) -> String {
    modifier
        .parts()
        .into_iter()
        .filter(|(_, part)| modifier.contains(*part))
        .map(|(name, _)| name)
        .collect::<Vec<_>>()
        .join(" + ")
}

pub fn format_desktop_modifier(modifier: ModifierMask) -> String {
    let formatted = format_modifier(modifier);
    if formatted.is_empty() {
        "Choose a modifier".into()
    } else {
        format!("{formatted} + 1–9")
    }
}

pub fn format_hotkey(hotkey: Hotkey) -> String {
    let key = friendly_key_name(&hotkey.key.name());
    let modifier = format_modifier(hotkey.modifiers);
    if modifier.is_empty() {
        key
    } else {
        format!("{modifier} + {key}")
    }
}

pub fn format_optional_hotkey(hotkey: Option<Hotkey>) -> String {
    hotkey.map_or_else(|| "Not assigned".into(), format_hotkey)
}

fn friendly_key_name(name: &str) -> String {
    match name {
        "Left" => "←".into(),
        "Up" => "↑".into(),
        "Right" => "→".into(),
        "Down" => "↓".into(),
        "PageUp" => "Page Up".into(),
        "PageDown" => "Page Down".into(),
        "CapsLock" => "Caps Lock".into(),
        "NumLock" => "Num Lock".into(),
        "ScrollLock" => "Scroll Lock".into(),
        value if value.starts_with("Numpad") => format!("Num {}", &value[6..]),
        value if value.starts_with("Oem") => value
            .strip_prefix("Oem")
            .unwrap_or(value)
            .replace("OpenBrackets", "[")
            .replace("CloseBrackets", "]")
            .replace("Semicolon", ";")
            .replace("Comma", ",")
            .replace("Period", ".")
            .replace("Slash", "/")
            .replace("Tilde", "`")
            .replace("Pipe", "\\")
            .replace("Quotes", "'"),
        value => value.to_string(),
    }
}

fn strip_index_wrapper(value: &str) -> (&str, bool) {
    let Some((digits_end, separator)) = indexed_separator(value) else {
        return (value, false);
    };
    let start = digits_end + separator.len_utf8();
    (value[start..].trim_start(), true)
}

fn strip_index_suffix(value: &str) -> (&str, Option<String>) {
    let Some(open) = value.rfind(" (") else {
        return (value, None);
    };
    if !value.ends_with(')') || open + 2 >= value.len() - 1 {
        return (value, None);
    }
    let inside = &value[open + 2..value.len() - 1];
    let Some((digits_end, separator)) = indexed_separator(inside) else {
        return (value, None);
    };
    let detail = inside[digits_end + separator.len_utf8()..].trim_start();
    if detail.is_empty() {
        (value, None)
    } else {
        (value[..open].trim_end(), Some(detail.to_string()))
    }
}

fn indexed_separator(value: &str) -> Option<(usize, char)> {
    let digits_end = value
        .char_indices()
        .take_while(|(_, character)| character.is_ascii_digit())
        .last()
        .map_or(0, |(index, character)| index + character.len_utf8());
    if digits_end == 0 {
        return None;
    }
    let remainder = &value[digits_end..];
    let hyphen_index = remainder
        .char_indices()
        .find(|(_, character)| *character == '-')
        .map(|(index, _)| index)?;
    if remainder[..hyphen_index].chars().all(char::is_whitespace) {
        Some((digits_end + hyphen_index, '-'))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayWizardStep {
    Displays,
    Arrangement,
    NameAndShortcut,
    Review,
}

impl DisplayWizardStep {
    pub const ALL: [Self; 4] = [
        Self::Displays,
        Self::Arrangement,
        Self::NameAndShortcut,
        Self::Review,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Displays => "Displays",
            Self::Arrangement => "Arrangement",
            Self::NameAndShortcut => "Name & shortcut",
            Self::Review => "Review",
        }
    }

    pub const fn number(self) -> u8 {
        match self {
            Self::Displays => 1,
            Self::Arrangement => 2,
            Self::NameAndShortcut => 3,
            Self::Review => 4,
        }
    }

    pub const fn previous(self) -> Option<Self> {
        match self {
            Self::Displays => None,
            Self::Arrangement => Some(Self::Displays),
            Self::NameAndShortcut => Some(Self::Arrangement),
            Self::Review => Some(Self::NameAndShortcut),
        }
    }

    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Displays => Some(Self::Arrangement),
            Self::Arrangement => Some(Self::NameAndShortcut),
            Self::NameAndShortcut => Some(Self::Review),
            Self::Review => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> DeviceId {
        DeviceId {
            endpoint: name.into(),
            name: name.into(),
        }
    }

    #[test]
    fn normalizes_only_known_enumeration_wrappers() {
        let label = friendly_device_name(
            "3 - SAMSUNG (2- AMD High Definition Audio Device)",
            AudioDeviceKind::Speaker,
        );
        assert_eq!(label.primary, "SAMSUNG");
        assert_eq!(
            label.detail.as_deref(),
            Some("AMD High Definition Audio Device")
        );

        let unchanged = friendly_device_name("Studio (USB Audio)", AudioDeviceKind::Speaker);
        assert_eq!(unchanged.primary, "Studio (USB Audio)");
        assert!(unchanged.detail.is_none());
    }

    #[test]
    fn removes_redundant_category_prefix_without_touching_identity() {
        let label =
            friendly_device_name("Microphone (SIMGOT EW300 DSP)", AudioDeviceKind::Microphone);
        assert_eq!(label.primary, "SIMGOT EW300 DSP");
    }

    #[test]
    fn default_and_legacy_explicit_render_the_same_system_endpoint() {
        let current = device("current");
        let following = device_selection_presentation(
            &DeviceSelection::Default,
            std::slice::from_ref(&current),
            Some(&current),
            AudioDeviceKind::Speaker,
        );
        let legacy_explicit = device_selection_presentation(
            &DeviceSelection::Endpoint("current".into()),
            std::slice::from_ref(&current),
            Some(&current),
            AudioDeviceKind::Speaker,
        );
        assert_eq!(following.primary, "current");
        assert_eq!(following.secondary, None);
        assert_eq!(following.status, None);
        assert_eq!(legacy_explicit.primary, following.primary);
        assert_eq!(legacy_explicit.secondary, following.secondary);
        assert_eq!(legacy_explicit.status, None);
    }

    #[test]
    fn audio_selection_keeps_device_identity_without_redundant_status_badges() {
        let current = device("speaker");
        let mut named = current.clone();
        named.name = "3 - SAMSUNG (2- AMD High Definition Audio Device)".into();
        let devices = [named.clone()];
        for selection in [
            DeviceSelection::Default,
            DeviceSelection::Endpoint(named.endpoint.clone()),
        ] {
            let presentation = device_selection_presentation(
                &selection,
                &devices,
                Some(&named),
                AudioDeviceKind::Speaker,
            );
            assert_eq!(presentation.primary, "SAMSUNG");
            assert_eq!(
                presentation.secondary.as_deref(),
                Some("AMD High Definition Audio Device")
            );
            assert_eq!(presentation.status, None);
            assert!(!presentation.accessible_value().contains("default"));
            assert!(!presentation.accessible_value().contains("Explicit"));
        }
        assert_eq!(
            device_choice_label(&named, Some(&named), AudioDeviceKind::Speaker),
            "SAMSUNG · AMD High Definition Audio Device"
        );
    }

    #[test]
    fn device_names_normalize_singular_and_plural_category_wrappers() {
        let microphone = friendly_device_name(
            "2 - Microphone (3- AMD High Definition Audio Device)",
            AudioDeviceKind::Microphone,
        );
        assert_eq!(microphone.primary, "AMD High Definition Audio Device");
        assert!(microphone.detail.is_none());

        let speakers =
            friendly_device_name("Speakers (Realtek USB Audio)", AudioDeviceKind::Speaker);
        assert_eq!(speakers.primary, "Realtek USB Audio");
        assert!(speakers.detail.is_none());
    }

    #[test]
    fn duplicate_friendly_devices_get_neutral_option_numbers() {
        let mut first = device("one");
        let mut second = device("two");
        first.name = "USB microphone".into();
        second.name = "USB microphone".into();
        let devices = vec![first, second];
        assert_eq!(
            device_choice_label_at(&devices, 0, None, AudioDeviceKind::Microphone)
                .expect("first label"),
            "USB microphone · Option 1"
        );
        assert_eq!(
            device_choice_label_at(&devices, 1, None, AudioDeviceKind::Microphone)
                .expect("second label"),
            "USB microphone · Option 2"
        );
    }

    #[test]
    fn hotkeys_use_human_spacing_and_direction_glyphs() {
        let hotkey = Hotkey::parse("Shift+Win+Up").expect("hotkey");
        assert_eq!(format_hotkey(hotkey), "Shift + Win + ↑");
        assert_eq!(format_desktop_modifier(ModifierMask::WIN), "Win + 1–9");
    }

    #[test]
    fn invalid_desktop_modifier_gets_actionable_copy() {
        assert_eq!(
            format_desktop_modifier(ModifierMask::from_bits(0)),
            "Choose a modifier"
        );
    }

    #[test]
    fn allowlist_modes_preserve_persisted_semantics() {
        assert_eq!(allowlist_mode(None), AllowlistMode::All);
        assert_eq!(allowlist_mode(Some(&[])), AllowlistMode::Disabled);
        assert_eq!(
            allowlist_mode(Some(&["endpoint".to_string()])),
            AllowlistMode::Selected
        );
    }

    #[test]
    fn wizard_steps_are_ordered_and_numbered() {
        assert_eq!(
            DisplayWizardStep::Displays.next(),
            Some(DisplayWizardStep::Arrangement)
        );
        assert_eq!(
            DisplayWizardStep::Review.previous(),
            Some(DisplayWizardStep::NameAndShortcut)
        );
        assert_eq!(DisplayWizardStep::NameAndShortcut.number(), 3);
    }
}
