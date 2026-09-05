//! Borrowed audio presentation. No window, config I/O or mutation is available here.

use crate::audio::devices::DeviceLists;
use crate::audio::{AudioState, DeviceCycleFlow, DeviceId, OutputState};
use crate::config::model::{AudioCfg, DeviceSelection};
use crate::ui::presentation::{
    allowlist_mode, device_choice_label_at, device_selection_presentation, friendly_device,
    friendly_device_name, AllowlistMode, AudioDeviceKind, DeviceSelectionPresentation,
};

pub(super) struct AudioView<'a> {
    config: &'a AudioCfg,
    devices: &'a DeviceLists,
    microphone: &'a AudioState,
    microphone_name: Option<&'a str>,
    output: &'a OutputState,
}

impl<'a> AudioView<'a> {
    pub(super) fn new(
        config: &'a AudioCfg,
        devices: &'a DeviceLists,
        microphone: &'a AudioState,
        microphone_name: Option<&'a str>,
        output: &'a OutputState,
    ) -> Self {
        Self {
            config,
            devices,
            microphone,
            microphone_name,
            output,
        }
    }

    pub(super) fn current_input_name(&self) -> String {
        if matches!(self.microphone, AudioState::Unavailable { .. }) {
            return "Microphone unavailable".into();
        }
        let raw = self
            .microphone_name
            .or_else(|| {
                self.devices
                    .input_defaults
                    .for_role(self.config.input_role)
                    .map(|d| d.name.as_str())
            })
            .or_else(|| match &self.config.input_device {
                DeviceSelection::Endpoint(endpoint) => self
                    .devices
                    .inputs
                    .iter()
                    .find(|device| device.endpoint == *endpoint)
                    .map(|d| d.name.as_str()),
                DeviceSelection::Default => None,
            })
            .unwrap_or("Windows default microphone");
        friendly_device_name(raw, AudioDeviceKind::Microphone).primary
    }

    pub(super) fn current_output_name(&self) -> String {
        match self.output {
            OutputState::Current { device, .. } => {
                friendly_device(device, AudioDeviceKind::Speaker).primary
            }
            OutputState::Unavailable { .. } => "Speakers unavailable".into(),
        }
    }

    pub(super) fn selection(&self, flow: DeviceCycleFlow) -> DeviceSelectionPresentation {
        let (selection, default, kind) = match flow {
            DeviceCycleFlow::Input => (
                &self.config.input_device,
                self.devices.input_defaults.for_role(self.config.input_role),
                AudioDeviceKind::Microphone,
            ),
            DeviceCycleFlow::Output => (
                &self.config.output_device,
                self.devices
                    .output_defaults
                    .for_role(self.config.output_role),
                AudioDeviceKind::Speaker,
            ),
        };
        device_selection_presentation(selection, self.devices(flow), default, kind)
    }

    pub(super) fn devices(&self, flow: DeviceCycleFlow) -> &'a [DeviceId] {
        match flow {
            DeviceCycleFlow::Input => &self.devices.inputs,
            DeviceCycleFlow::Output => &self.devices.outputs,
        }
    }

    pub(super) fn allowlist(&self, flow: DeviceCycleFlow) -> Option<&'a [String]> {
        match flow {
            DeviceCycleFlow::Input => self.config.cycle_input_allowlist.as_deref(),
            DeviceCycleFlow::Output => self.config.cycle_output_allowlist.as_deref(),
        }
    }

    pub(super) fn mode(&self, flow: DeviceCycleFlow) -> AllowlistMode {
        allowlist_mode(self.allowlist(flow))
    }

    pub(super) fn cycle_device_selected(&self, flow: DeviceCycleFlow, index: usize) -> bool {
        let Some(device) = self.devices(flow).get(index) else {
            return false;
        };
        self.allowlist(flow)
            .is_some_and(|ids| ids.contains(&device.endpoint))
    }

    pub(super) fn cycle_device_label(&self, flow: DeviceCycleFlow, index: usize) -> String {
        let (default, kind) = match flow {
            DeviceCycleFlow::Input => (
                self.devices.input_defaults.for_role(self.config.input_role),
                AudioDeviceKind::Microphone,
            ),
            DeviceCycleFlow::Output => (
                self.devices
                    .output_defaults
                    .for_role(self.config.output_role),
                AudioDeviceKind::Speaker,
            ),
        };
        device_choice_label_at(self.devices(flow), index, default, kind)
            .unwrap_or_else(|| "Device unavailable".into())
    }
}

pub(super) fn cycle_mode_choice(index: u8) -> Option<AllowlistMode> {
    match index {
        0 => Some(AllowlistMode::All),
        1 => Some(AllowlistMode::Selected),
        2 => Some(AllowlistMode::Disabled),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_choice_is_not_silently_treated_as_disabled() {
        assert_eq!(cycle_mode_choice(0), Some(AllowlistMode::All));
        assert_eq!(cycle_mode_choice(1), Some(AllowlistMode::Selected));
        assert_eq!(cycle_mode_choice(2), Some(AllowlistMode::Disabled));
        assert_eq!(cycle_mode_choice(3), None);
        assert_eq!(cycle_mode_choice(u8::MAX), None);
    }
}
