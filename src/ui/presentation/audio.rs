//! Audio for the presentation.

use crate::audio::DeviceId;
use crate::config::model::DeviceSelection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioDeviceKind {
    Speaker,
    Microphone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FriendlyLabel {
    pub primary: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceSelectionPresentation {
    pub primary: String,
    pub secondary: Option<String>,
    pub status: Option<String>,
}

/// The three meanings represented by the persisted `Option<Vec<String>>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllowlistMode {
    All,
    Selected,
    Disabled,
}

pub(crate) fn allowlist_mode(value: Option<&[String]>) -> AllowlistMode {
    match value {
        None => AllowlistMode::All,
        Some([]) => AllowlistMode::Disabled,
        Some(_) => AllowlistMode::Selected,
    }
}

pub(crate) fn allowlist_mode_label(mode: AllowlistMode, kind: AudioDeviceKind) -> &'static str {
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
pub(crate) fn friendly_device_name(name: &str, kind: AudioDeviceKind) -> FriendlyLabel {
    let original = name.trim();
    let (without_prefix, _) = strip_index_wrapper(original);
    let (without_suffix, mut detail) = strip_index_suffix(without_prefix);
    let (without_adapter, adapter_detail) = strip_known_adapter_suffix(without_suffix);
    if detail.is_none() {
        detail = adapter_detail;
    }
    let mut primary = without_adapter.trim().to_string();
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
        primary = format!("Unknown {}", kind.noun());
    }
    FriendlyLabel {
        primary,
        detail: detail.filter(|value| !value.is_empty()),
    }
}

pub(crate) fn friendly_device(device: &DeviceId, kind: AudioDeviceKind) -> FriendlyLabel {
    friendly_device_name(&device.name, kind)
}

pub(crate) fn device_selection_presentation(
    _selection: &DeviceSelection,
    _devices: &[DeviceId],
    default: Option<&DeviceId>,
    kind: AudioDeviceKind,
) -> DeviceSelectionPresentation {
    if let Some(device) = default {
        let label = friendly_device(device, kind);
        DeviceSelectionPresentation {
            primary: label.primary,
            // Keep adapter/driver metadata available to accessibility while
            // compact controls render only the canonical primary name.
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

pub(crate) fn device_choice_label(
    device: &DeviceId,
    _default: Option<&DeviceId>,
    kind: AudioDeviceKind,
) -> String {
    friendly_device(device, kind).primary
}

pub(crate) fn device_choice_label_at(
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

pub(super) fn strip_index_wrapper(value: &str) -> (&str, bool) {
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

fn strip_known_adapter_suffix(value: &str) -> (&str, Option<String>) {
    let is_adapter_metadata = |detail: &str| {
        let detail = detail.to_ascii_lowercase();
        detail.contains("audio device") || detail.contains("adapter") || detail.contains("driver")
    };
    if let Some(open) = value.rfind(" (") {
        if value.ends_with(')') && open + 2 < value.len() - 1 {
            let detail = &value[open + 2..value.len() - 1];
            if is_adapter_metadata(detail) {
                return (value[..open].trim_end(), Some(detail.trim().to_string()));
            }
        }
    }
    if let Some((primary, detail)) = value.rsplit_once(" · ") {
        if !primary.trim().is_empty() && is_adapter_metadata(detail) {
            return (primary.trim_end(), Some(detail.trim().to_string()));
        }
    }
    (value, None)
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

impl AudioDeviceKind {
    pub(crate) const fn noun(self) -> &'static str {
        match self {
            Self::Speaker => "speaker",
            Self::Microphone => "microphone",
        }
    }
}

impl FriendlyLabel {
    pub(crate) fn compact(&self) -> String {
        match &self.detail {
            Some(detail) if !detail.is_empty() => format!("{} · {detail}", self.primary),
            _ => self.primary.clone(),
        }
    }
}

impl DeviceSelectionPresentation {
    pub(crate) fn accessible_value(&self) -> String {
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
