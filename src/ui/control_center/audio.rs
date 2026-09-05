//! Audio edits commit complete candidates; presentation borrows a read-only AudioView.

use super::audio_view::{cycle_mode_choice, AudioView};
use super::state::SettingsUi;
use crate::audio::DeviceCycleFlow;
use crate::config::validate::Violation;
use crate::ui::layout::ElementId;
use crate::ui::presentation::AllowlistMode;
use windows::Win32::Foundation::HWND;

pub(super) fn allowlist_label(allowlist: Option<&[String]>) -> String {
    match allowlist {
        None => "All available devices".into(),
        Some([]) => "Don't cycle".into(),
        Some(ids) => format!("{} selected", ids.len()),
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
        let allowlist = match mode {
            AllowlistMode::All => None,
            AllowlistMode::Disabled => Some(Vec::new()),
            AllowlistMode::Selected => Some(view.allowlist(flow).map_or_else(
                || {
                    devices
                        .iter()
                        .map(|device| device.endpoint.clone())
                        .collect()
                },
                <[String]>::to_vec,
            )),
        };
        let before = self.draft.clone();
        *self.cycle_allowlist_mut(flow) = allowlist;
        self.commit_local_change(hwnd, before);
    }

    pub(super) fn toggle_cycle_device(&mut self, hwnd: HWND, flow: DeviceCycleFlow, index: usize) {
        let Some(endpoint) = self
            .audio_view()
            .devices(flow)
            .get(index)
            .map(|d| d.endpoint.clone())
        else {
            return;
        };
        let before = self.draft.clone();
        let values = self.cycle_allowlist_mut(flow).get_or_insert_with(Vec::new);
        if let Some(position) = values.iter().position(|value| value == &endpoint) {
            values.remove(position);
        } else {
            values.push(endpoint);
        }
        self.commit_local_change(hwnd, before);
    }

    fn cycle_allowlist_mut(&mut self, flow: DeviceCycleFlow) -> &mut Option<Vec<String>> {
        match flow {
            DeviceCycleFlow::Input => &mut self.draft.audio.cycle_input_allowlist,
            DeviceCycleFlow::Output => &mut self.draft.audio.cycle_output_allowlist,
        }
    }
}
