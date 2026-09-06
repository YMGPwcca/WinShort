//! Domain-specific UI labels; no window or filesystem dependencies.

mod audio;
mod displays;
mod hotkeys;

#[cfg(test)]
pub(crate) use audio::device_choice_label;
pub(crate) use audio::{
    allowlist_mode_label, device_choice_label_at, device_selection_presentation, friendly_device,
    friendly_device_name, AllowlistMode, AudioDeviceKind, DeviceCycleSelection,
    DeviceSelectionPresentation,
};
pub(crate) use displays::{display_output_label, monitor_choice_label, DisplayWizardStep};
pub(crate) use hotkeys::{
    format_desktop_modifier, format_hotkey, format_modifier, format_optional_hotkey,
};

#[cfg(test)]
use crate::audio::DeviceId;
#[cfg(test)]
use crate::config::model::DeviceSelection;
#[cfg(test)]
use crate::keyboard::binding::{Hotkey, ModifierMask};

#[cfg(test)]
mod tests;

pub(crate) use displays::{DisplayOutputCard, DisplayProfileCard, ProfileReadiness};
