//! Typed picker models for the control center.

use crate::config::model::{
    Config, DeviceSelection, EndpointRole, MonitorChoice, OverlayAppearance, OverlayPosition,
};
use crate::keyboard::binding::ModifierMask;
use crate::ui::layout::ElementId;
use crate::ui::picker::{
    DeviceCycleSelection, PickerChoice, PickerChoiceValue, PickerCommit, PickerKind, PickerModel,
};
use crate::ui::presentation::{
    display_output_label, format_modifier as format_modifier_display, AllowlistMode,
    AudioDeviceKind,
};

type PickerParts = (Vec<PickerChoice>, Option<usize>, Vec<usize>);

pub(super) const fn picker_element(kind: PickerKind) -> ElementId {
    match kind {
        PickerKind::InputDevice => ElementId::InputDevice,
        PickerKind::OutputDevice => ElementId::OutputDevice,
        PickerKind::InputAllowlist => ElementId::InputAllowlist,
        PickerKind::OutputAllowlist => ElementId::OutputAllowlist,
        PickerKind::DisplayProfile => ElementId::DisplayProfile,
        PickerKind::DisplayOutputs => ElementId::DisplayOutputs,
        PickerKind::DisplayTopology => ElementId::DisplayTopology,
        PickerKind::DisplayRoute => ElementId::DisplayRoute,
        PickerKind::InputRole => ElementId::InputRole,
        PickerKind::OutputRole => ElementId::OutputRole,
        PickerKind::DesktopNumberModifier => ElementId::DesktopNumberModifier,
        PickerKind::MoveDesktopModifier => ElementId::MoveDesktopModifier,
        PickerKind::SilentMoveDesktopModifier => ElementId::SilentMoveDesktopModifier,
        PickerKind::OverlayAppearance => ElementId::OverlayAppearance,
        PickerKind::OverlayPosition => ElementId::OverlayPosition,
        PickerKind::OverlayMonitor => ElementId::OverlayMonitor,
    }
}

pub(super) fn picker_model(
    kind: PickerKind,
    draft: &Config,
    devices: &crate::audio::devices::DeviceLists,
    display_outputs: &[crate::display::DisplayOutput],
    selected_display_route: Option<usize>,
) -> Result<PickerModel, &'static str> {
    let (choices, current, selected_indices) = match kind {
        PickerKind::InputDevice => input_device_parts(draft, devices),
        PickerKind::OutputDevice => output_device_parts(draft, devices),
        PickerKind::InputAllowlist => allowlist_parts(
            &devices.inputs,
            DeviceCycleSelection::from_config(draft.audio.cycle_input_allowlist.as_deref()),
            AudioDeviceKind::Microphone,
        ),
        PickerKind::OutputAllowlist => allowlist_parts(
            &devices.outputs,
            DeviceCycleSelection::from_config(draft.audio.cycle_output_allowlist.as_deref()),
            AudioDeviceKind::Speaker,
        ),
        PickerKind::InputRole => role_parts(true, draft.audio.input_role),
        PickerKind::OutputRole => role_parts(false, draft.audio.output_role),
        PickerKind::DisplayProfile => display_profile_parts(draft),
        PickerKind::DisplayOutputs => display_outputs_parts(draft, display_outputs),
        PickerKind::DisplayTopology => display_topology_parts(draft),
        PickerKind::DisplayRoute => {
            display_route_parts(draft, display_outputs, selected_display_route)
        }
        PickerKind::DesktopNumberModifier => modifier_parts(
            false,
            draft.virtual_desktops.number_modifier,
            ModifierPicker::DesktopNumber,
        ),
        PickerKind::MoveDesktopModifier => modifier_parts(
            true,
            draft
                .virtual_desktops
                .move_follow_modifier
                .unwrap_or(ModifierMask::NONE),
            ModifierPicker::MoveDesktop,
        ),
        PickerKind::SilentMoveDesktopModifier => modifier_parts(
            true,
            draft
                .virtual_desktops
                .move_silent_modifier
                .unwrap_or(ModifierMask::NONE),
            ModifierPicker::SilentMoveDesktop,
        ),
        PickerKind::OverlayAppearance => overlay_appearance_parts(draft),
        PickerKind::OverlayPosition => overlay_position_parts(draft),
        PickerKind::OverlayMonitor => overlay_monitor_parts(draft),
    };
    PickerModel::new(kind, choices, current, selected_indices)
}

fn input_device_parts(draft: &Config, devices: &crate::audio::devices::DeviceLists) -> PickerParts {
    let default = devices.input_defaults.for_role(draft.audio.input_role);
    let choices = devices
        .inputs
        .iter()
        .enumerate()
        .map(|(index, device)| {
            PickerChoice::commit(
                crate::ui::presentation::device_choice_label_at(
                    &devices.inputs,
                    index,
                    default,
                    AudioDeviceKind::Microphone,
                )
                .unwrap_or_else(|| "Device unavailable".into()),
                PickerCommit::InputDevice(DeviceSelection::Endpoint(device.endpoint.clone())),
            )
        })
        .collect::<Vec<_>>();
    let current = current_device_index(&draft.audio.input_device, default, &choices);
    (choices, current, Vec::new())
}

fn output_device_parts(
    draft: &Config,
    devices: &crate::audio::devices::DeviceLists,
) -> PickerParts {
    let default = devices.output_defaults.for_role(draft.audio.output_role);
    let choices = devices
        .outputs
        .iter()
        .enumerate()
        .map(|(index, device)| {
            PickerChoice::commit(
                crate::ui::presentation::device_choice_label_at(
                    &devices.outputs,
                    index,
                    default,
                    AudioDeviceKind::Speaker,
                )
                .unwrap_or_else(|| "Device unavailable".into()),
                PickerCommit::OutputDevice(DeviceSelection::Endpoint(device.endpoint.clone())),
            )
        })
        .collect::<Vec<_>>();
    let current = current_device_index(&draft.audio.output_device, default, &choices);
    (choices, current, Vec::new())
}

fn current_device_index(
    selection: &DeviceSelection,
    default: Option<&crate::audio::DeviceId>,
    choices: &[PickerChoice],
) -> Option<usize> {
    let endpoint = match selection {
        DeviceSelection::Default => default.map(|device| device.endpoint.as_str()),
        DeviceSelection::Endpoint(endpoint) => Some(endpoint.as_str()),
    }?;
    choices.iter().position(|choice| {
        matches!(
            choice.commit_value(),
            Some(PickerCommit::InputDevice(DeviceSelection::Endpoint(id)))
                | Some(PickerCommit::OutputDevice(DeviceSelection::Endpoint(id)))
                if id == endpoint
        )
    })
}

fn allowlist_parts(
    devices: &[crate::audio::DeviceId],
    configured: DeviceCycleSelection,
    device_kind: AudioDeviceKind,
) -> PickerParts {
    let mut choices = vec![
        PickerChoice::allowlist_mode(
            crate::ui::presentation::allowlist_mode_label(AllowlistMode::All, device_kind),
            AllowlistMode::All,
        ),
        PickerChoice::allowlist_mode(
            crate::ui::presentation::allowlist_mode_label(AllowlistMode::Selected, device_kind),
            AllowlistMode::Selected,
        ),
        PickerChoice::allowlist_mode(
            crate::ui::presentation::allowlist_mode_label(AllowlistMode::Disabled, device_kind),
            AllowlistMode::Disabled,
        ),
    ];
    choices.extend(devices.iter().enumerate().map(|(index, device)| {
        PickerChoice::allowlist_endpoint(
            crate::ui::presentation::device_choice_label_at(devices, index, None, device_kind)
                .unwrap_or_else(|| "Device unavailable".into()),
            device.endpoint.clone(),
        )
    }));
    for endpoint in configured.endpoints() {
        if !devices.iter().any(|device| device.endpoint == *endpoint) {
            choices.push(PickerChoice::allowlist_endpoint(
                "Saved device unavailable — reconnect it to use it",
                endpoint.clone(),
            ));
        }
    }

    let mode_index = match configured.mode() {
        AllowlistMode::All => 0,
        AllowlistMode::Selected => 1,
        AllowlistMode::Disabled => 2,
    };
    let mut selected_indices = vec![mode_index];
    if configured.mode() == AllowlistMode::Selected {
        selected_indices.extend(choices.iter().enumerate().filter_map(|(index, choice)| {
            match choice.value() {
                PickerChoiceValue::AllowlistEndpoint(endpoint)
                    if configured.endpoints().iter().any(|saved| saved == endpoint) =>
                {
                    Some(index)
                }
                PickerChoiceValue::Commit(_)
                | PickerChoiceValue::AllowlistMode(_)
                | PickerChoiceValue::AllowlistEndpoint(_)
                | PickerChoiceValue::DisplayOutput(_) => None,
            }
        }));
    }
    (choices, Some(mode_index), selected_indices)
}

fn role_parts(input: bool, current_role: EndpointRole) -> PickerParts {
    let choices = [
        EndpointRole::Console,
        EndpointRole::Multimedia,
        EndpointRole::Communications,
    ]
    .into_iter()
    .map(|role| {
        let commit = if input {
            PickerCommit::InputRole(role)
        } else {
            PickerCommit::OutputRole(role)
        };
        PickerChoice::commit(role.label(), commit)
    })
    .collect::<Vec<_>>();
    let current_commit = if input {
        PickerCommit::InputRole(current_role)
    } else {
        PickerCommit::OutputRole(current_role)
    };
    let current = current_commit_index(&choices, &current_commit);
    (choices, current, Vec::new())
}

fn display_profile_parts(draft: &Config) -> PickerParts {
    let mut choices = vec![PickerChoice::commit(
        "No profile selected",
        PickerCommit::DisplayProfile(None),
    )];
    choices.extend(draft.display_profiles.profiles.iter().map(|profile| {
        let status = if profile.confirmed {
            "Ready"
        } else {
            "Needs a test"
        };
        PickerChoice::commit(
            format!("{} — {} ({status})", profile.name, profile.topology.label()),
            PickerCommit::DisplayProfile(Some(profile.id.clone())),
        )
    }));
    let current = current_commit_index(
        &choices,
        &PickerCommit::DisplayProfile(draft.display_profiles.active_profile.clone()),
    );
    (choices, current, Vec::new())
}

fn display_outputs_parts(
    draft: &Config,
    display_outputs: &[crate::display::DisplayOutput],
) -> PickerParts {
    let choices = display_outputs
        .iter()
        .map(|output| {
            let label = display_output_label(
                &output.monitor_name,
                &output.adapter_name,
                &output.connector_name,
                output.active,
            );
            PickerChoice::display_output(label.compact(), output.route.clone())
        })
        .collect::<Vec<_>>();
    let selected_indices = draft
        .display_profiles
        .active()
        .map(|profile| {
            choices
                .iter()
                .enumerate()
                .filter_map(|(index, choice)| match choice.value() {
                    PickerChoiceValue::DisplayOutput(route)
                        if profile
                            .routes
                            .iter()
                            .any(|configured| crate::display::same_output(configured, route)) =>
                    {
                        Some(index)
                    }
                    PickerChoiceValue::Commit(_)
                    | PickerChoiceValue::AllowlistMode(_)
                    | PickerChoiceValue::AllowlistEndpoint(_)
                    | PickerChoiceValue::DisplayOutput(_) => None,
                })
                .collect()
        })
        .unwrap_or_default();
    (choices, None, selected_indices)
}

fn display_topology_parts(draft: &Config) -> PickerParts {
    let mut choices = [
        crate::display::DisplayTopology::Extend,
        crate::display::DisplayTopology::Clone,
    ]
    .into_iter()
    .map(|topology| PickerChoice::commit(topology.label(), PickerCommit::DisplayTopology(topology)))
    .collect::<Vec<_>>();
    if let Some(topology) = draft
        .display_profiles
        .active()
        .map(|profile| profile.topology)
        .filter(|topology| {
            !matches!(
                topology,
                crate::display::DisplayTopology::Extend | crate::display::DisplayTopology::Clone
            )
        })
    {
        choices.push(PickerChoice::commit(
            "Current arrangement (advanced)",
            PickerCommit::DisplayTopology(topology),
        ));
    }
    let current = draft.display_profiles.active().and_then(|profile| {
        current_commit_index(&choices, &PickerCommit::DisplayTopology(profile.topology))
    });
    (choices, current, Vec::new())
}

fn display_route_parts(
    draft: &Config,
    display_outputs: &[crate::display::DisplayOutput],
    selected_display_route: Option<usize>,
) -> PickerParts {
    let choices = draft
        .display_profiles
        .active()
        .map(|profile| {
            profile
                .routes
                .iter()
                .enumerate()
                .map(|(index, route)| {
                    let label = display_outputs
                        .iter()
                        .find(|output| crate::display::same_output(&output.route, route))
                        .map(|output| {
                            display_output_label(
                                &output.monitor_name,
                                &output.adapter_name,
                                &output.connector_name,
                                output.active,
                            )
                            .compact()
                        })
                        .unwrap_or_else(|| {
                            format!("Configured display {} — unavailable", index + 1)
                        });
                    PickerChoice::commit(label, PickerCommit::DisplayRoute(index))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let current = selected_display_route
        .and_then(|selected| current_commit_index(&choices, &PickerCommit::DisplayRoute(selected)));
    (choices, current, Vec::new())
}

#[derive(Debug, Clone, Copy)]
enum ModifierPicker {
    DesktopNumber,
    MoveDesktop,
    SilentMoveDesktop,
}

impl ModifierPicker {
    fn commit(self, modifier: ModifierMask) -> PickerCommit {
        match self {
            Self::DesktopNumber => PickerCommit::DesktopNumberModifier(modifier),
            Self::MoveDesktop => PickerCommit::MoveDesktopModifier(modifier),
            Self::SilentMoveDesktop => PickerCommit::SilentMoveDesktopModifier(modifier),
        }
    }
}

fn modifier_parts(
    allow_unassigned: bool,
    current: ModifierMask,
    picker: ModifierPicker,
) -> PickerParts {
    let mut choices = Vec::new();
    if allow_unassigned {
        choices.push(PickerChoice::commit(
            "Unassigned",
            picker.commit(ModifierMask::NONE),
        ));
    }
    for bits in 1u8..=0b1111 {
        let modifier = ModifierMask::from_bits(bits);
        choices.push(PickerChoice::commit(
            format_modifier_display(modifier),
            picker.commit(modifier),
        ));
    }
    let current = current_commit_index(&choices, &picker.commit(current));
    (choices, current, Vec::new())
}

fn overlay_appearance_parts(draft: &Config) -> PickerParts {
    let choices = OverlayAppearance::ALL
        .into_iter()
        .map(|appearance| {
            PickerChoice::commit(
                appearance.label(),
                PickerCommit::OverlayAppearance(appearance),
            )
        })
        .collect::<Vec<_>>();
    let current = current_commit_index(
        &choices,
        &PickerCommit::OverlayAppearance(draft.overlay.appearance),
    );
    (choices, current, Vec::new())
}

fn overlay_position_parts(draft: &Config) -> PickerParts {
    let choices = OverlayPosition::ALL
        .into_iter()
        .filter(|position| *position != OverlayPosition::Center)
        .map(|position| {
            PickerChoice::commit(position.label(), PickerCommit::OverlayPosition(position))
        })
        .collect::<Vec<_>>();
    let current = current_commit_index(
        &choices,
        &PickerCommit::OverlayPosition(draft.overlay.position),
    );
    (choices, current, Vec::new())
}

fn overlay_monitor_parts(draft: &Config) -> PickerParts {
    let choices = vec![
        PickerChoice::commit(
            "Primary",
            PickerCommit::OverlayMonitor(MonitorChoice::Primary),
        ),
        PickerChoice::commit(
            "Cursor position",
            PickerCommit::OverlayMonitor(MonitorChoice::Cursor),
        ),
    ];
    let current = current_commit_index(
        &choices,
        &PickerCommit::OverlayMonitor(draft.overlay.monitor.clone()),
    );
    (choices, current, Vec::new())
}

fn current_commit_index(choices: &[PickerChoice], current: &PickerCommit) -> Option<usize> {
    choices
        .iter()
        .position(|choice| choice.commit_value() == Some(current))
}
