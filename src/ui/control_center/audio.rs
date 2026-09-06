//! Audio edits commit complete candidates; presentation borrows a read-only AudioView.

use super::audio_view::{cycle_mode_choice, AudioView};
use super::state::SettingsUi;
use crate::audio::DeviceCycleFlow;
use crate::config::validate::Violation;
use crate::ui::layout::ElementId;
use crate::ui::presentation::{AllowlistMode, DeviceCycleSelection};
use windows::Win32::Foundation::HWND;

pub(super) fn allowlist_label(selection: &DeviceCycleSelection) -> String {
    match selection.mode() {
        AllowlistMode::All => "All available devices".into(),
        AllowlistMode::Disabled => "Don't cycle".into(),
        AllowlistMode::Selected => format!("{} selected", selection.endpoints().len()),
    }
}

impl SettingsUi {
    pub(super) fn audio_view(&self) -> AudioView<'_> {
        AudioView::new(
            &self.draft.audio,
            &self.devices,
            &self.runtime.microphone,
            self.runtime.microphone_name.as_deref(),
            &self.runtime.output,
        )
    }

    pub(super) fn choice_selected(&self, id: ElementId) -> bool {
        let (flow, index) = match id {
            ElementId::InputCycleMode(index) => (DeviceCycleFlow::Input, index),
            ElementId::OutputCycleMode(index) => (DeviceCycleFlow::Output, index),
            _ => return false,
        };
        cycle_mode_choice(index) == Some(self.audio_view().mode(flow))
    }

    pub(super) fn set_cycle_mode(
        &mut self,
        hwnd: HWND,
        flow: DeviceCycleFlow,
        mode: AllowlistMode,
    ) {
        let view = self.audio_view();
        let devices = view.devices(flow);
        if mode == AllowlistMode::Selected && devices.is_empty() {
            self.validation = vec![Violation {
                field: "Audio".into(),
                message: "No devices are available for a selected cycling list".into(),
            }];
            return;
        }

        let current = view.cycle_selection(flow);
        let selection = match mode {
            AllowlistMode::All => DeviceCycleSelection::All,
            AllowlistMode::Disabled => DeviceCycleSelection::Disabled,
            AllowlistMode::Selected => {
                let endpoints = if current.mode() == AllowlistMode::Selected {
                    current.endpoints().to_vec()
                } else {
                    devices
                        .iter()
                        .map(|device| device.endpoint.clone())
                        .collect()
                };
                match DeviceCycleSelection::selected(endpoints) {
                    Ok(selection) => selection,
                    Err(reason) => {
                        self.validation = vec![Violation {
                            field: "Audio".into(),
                            message: reason.into(),
                        }];
                        return;
                    }
                }
            }
        };

        let before = self.draft.clone();
        self.set_cycle_selection(flow, selection);
        self.commit_local_change(hwnd, before);
    }

    pub(super) fn toggle_cycle_device(&mut self, hwnd: HWND, flow: DeviceCycleFlow, index: usize) {
        let view = self.audio_view();
        let Some(endpoint) = view.devices(flow).get(index).map(|d| d.endpoint.clone()) else {
            return;
        };
        let mut endpoints = view.cycle_selection(flow).endpoints().to_vec();
        if let Some(position) = endpoints.iter().position(|value| value == &endpoint) {
            endpoints.remove(position);
        } else {
            endpoints.push(endpoint);
        }
        let selection = if endpoints.is_empty() {
            DeviceCycleSelection::Disabled
        } else {
            match DeviceCycleSelection::selected(endpoints) {
                Ok(selection) => selection,
                Err(_) => return,
            }
        };
        let before = self.draft.clone();
        self.set_cycle_selection(flow, selection);
        self.commit_local_change(hwnd, before);
    }

    fn set_cycle_selection(&mut self, flow: DeviceCycleFlow, selection: DeviceCycleSelection) {
        let configured = selection.into_config();
        match flow {
            DeviceCycleFlow::Input => self.draft.audio.cycle_input_allowlist = configured,
            DeviceCycleFlow::Output => self.draft.audio.cycle_output_allowlist = configured,
        }
    }
}
