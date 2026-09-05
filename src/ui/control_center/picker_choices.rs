//! Picker choices for the control center.

use crate::config::model::{
    Config, DeviceSelection, EndpointRole, MonitorChoice, OverlayAppearance, OverlayPosition,
};
use crate::keyboard::binding::ModifierMask;
use crate::ui::layout::ElementId;
use crate::ui::picker::{PickerChoice, PickerKind, PickerValue};
use crate::ui::presentation::{
    allowlist_mode, display_output_label, format_modifier as format_modifier_display,
    AllowlistMode, AudioDeviceKind,
};

pub(super) fn picker_element(kind: PickerKind) -> Option<ElementId> {
    Some(match kind {
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
    })
}

pub(super) fn picker_choices(
    kind: PickerKind,
    draft: &Config,
    devices: &crate::audio::devices::DeviceLists,
    _monitors: &[crate::platform::monitor::MonitorGeometry],
    display_outputs: &[crate::display::DisplayOutput],
    selected_display_route: usize,
) -> (Vec<PickerChoice>, usize) {
    let mut choices = Vec::new();
    match kind {
        PickerKind::InputDevice => {
            choices.extend(device_choices(
                &devices.inputs,
                devices.input_defaults.for_role(draft.audio.input_role),
                AudioDeviceKind::Microphone,
            ));
        }
        PickerKind::OutputDevice => {
            choices.extend(device_choices(
                &devices.outputs,
                devices.output_defaults.for_role(draft.audio.output_role),
                AudioDeviceKind::Speaker,
            ));
        }
        PickerKind::InputAllowlist => {
            choices.extend(allowlist_choices(
                &devices.inputs,
                draft.audio.cycle_input_allowlist.as_deref(),
                AudioDeviceKind::Microphone,
            ));
        }
        PickerKind::OutputAllowlist => {
            choices.extend(allowlist_choices(
                &devices.outputs,
                draft.audio.cycle_output_allowlist.as_deref(),
                AudioDeviceKind::Speaker,
            ));
        }
        PickerKind::DisplayProfile => {
            choices.extend(display_profile_choices(&draft.display_profiles));
        }
        PickerKind::DisplayOutputs => {
            choices.extend(display_outputs.iter().map(|output| {
                let label = display_output_label(
                    &output.monitor_name,
                    &output.adapter_name,
                    &output.connector_name,
                    output.active,
                );
                PickerChoice {
                    label: label.compact(),
                    value: PickerValue::DisplayOutput(output.route.clone()),
                }
            }));
        }
        PickerKind::DisplayTopology => {
            for topology in [
                crate::display::DisplayTopology::Extend,
                crate::display::DisplayTopology::Clone,
            ] {
                choices.push(PickerChoice {
                    label: topology.label().into(),
                    value: PickerValue::DisplayTopology(topology),
                });
            }
            if let Some(topology) = draft
                .display_profiles
                .active()
                .map(|profile| profile.topology)
                .filter(|topology| {
                    !matches!(
                        topology,
                        crate::display::DisplayTopology::Extend
                            | crate::display::DisplayTopology::Clone
                    )
                })
            {
                choices.push(PickerChoice {
                    label: "Current arrangement (advanced)".into(),
                    value: PickerValue::DisplayTopology(topology),
                });
            }
        }
        PickerKind::DisplayRoute => {
            if let Some(profile) = draft.display_profiles.active() {
                choices.extend(profile.routes.iter().enumerate().map(|(index, route)| {
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
                    PickerChoice {
                        label,
                        value: PickerValue::DisplayRoute(index),
                    }
                }));
            }
        }
        PickerKind::InputRole => {
            for role in [
                EndpointRole::Console,
                EndpointRole::Multimedia,
                EndpointRole::Communications,
            ] {
                choices.push(PickerChoice {
                    label: role.label().into(),
                    value: PickerValue::Role(role),
                });
            }
        }
        PickerKind::OutputRole => {
            for role in [
                EndpointRole::Console,
                EndpointRole::Multimedia,
                EndpointRole::Communications,
            ] {
                choices.push(PickerChoice {
                    label: role.label().into(),
                    value: PickerValue::Role(role),
                });
            }
        }
        PickerKind::DesktopNumberModifier => {
            choices.extend(modifier_choices(false));
        }
        PickerKind::MoveDesktopModifier => {
            choices.extend(modifier_choices(true));
        }
        PickerKind::SilentMoveDesktopModifier => {
            choices.extend(modifier_choices(true));
        }
        PickerKind::OverlayAppearance => {
            choices.extend(
                OverlayAppearance::ALL
                    .into_iter()
                    .map(|appearance| PickerChoice {
                        label: appearance.label().into(),
                        value: PickerValue::Appearance(appearance),
                    }),
            );
        }
        PickerKind::OverlayPosition => {
            choices.extend(
                OverlayPosition::ALL
                    .into_iter()
                    .map(|position| PickerChoice {
                        label: position.label().into(),
                        value: PickerValue::Position(position),
                    }),
            );
        }
        PickerKind::OverlayMonitor => {
            choices.push(PickerChoice {
                label: "Primary".into(),
                value: PickerValue::Monitor(MonitorChoice::Primary),
            });
            choices.push(PickerChoice {
                label: "Cursor position".into(),
                value: PickerValue::Monitor(MonitorChoice::Cursor),
            });
        }
    }
    let current_index = match kind {
        PickerKind::InputDevice => current_device_index(
            &draft.audio.input_device,
            devices.input_defaults.for_role(draft.audio.input_role),
            &choices,
        ),
        PickerKind::OutputDevice => current_device_index(
            &draft.audio.output_device,
            devices.output_defaults.for_role(draft.audio.output_role),
            &choices,
        ),
        PickerKind::InputAllowlist | PickerKind::OutputAllowlist => {
            let configured = if kind == PickerKind::InputAllowlist {
                draft.audio.cycle_input_allowlist.as_deref()
            } else {
                draft.audio.cycle_output_allowlist.as_deref()
            };
            match allowlist_mode(configured) {
                AllowlistMode::All => 0,
                AllowlistMode::Selected => 1,
                AllowlistMode::Disabled => 2,
            }
        }
        PickerKind::DisplayOutputs => 0,
        _ => {
            let current = current_picker_value(kind, draft, selected_display_route);
            choices
                .iter()
                .position(|choice| choice.value == current)
                .unwrap_or(0)
        }
    };
    (choices, current_index)
}

pub(super) fn picker_selection_indices(
    kind: PickerKind,
    draft: &Config,
    choices: &[PickerChoice],
) -> Vec<usize> {
    if kind == PickerKind::DisplayOutputs {
        let Some(profile) = draft.display_profiles.active() else {
            return Vec::new();
        };
        return choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| match &choice.value {
                PickerValue::DisplayOutput(route)
                    if profile
                        .routes
                        .iter()
                        .any(|configured| crate::display::same_output(configured, route)) =>
                {
                    Some(index)
                }
                _ => None,
            })
            .collect();
    }
    let configured = match kind {
        PickerKind::InputAllowlist => draft.audio.cycle_input_allowlist.as_deref(),
        PickerKind::OutputAllowlist => draft.audio.cycle_output_allowlist.as_deref(),
        _ => return Vec::new(),
    };
    let mode = allowlist_mode(configured);
    let mode_index = match mode {
        AllowlistMode::All => 0,
        AllowlistMode::Selected => 1,
        AllowlistMode::Disabled => 2,
    };
    let mut selected = vec![mode_index];
    if mode == AllowlistMode::Selected {
        selected.extend(choices.iter().enumerate().filter_map(
            |(index, choice)| match &choice.value {
                PickerValue::Allowlist(Some(values))
                    if values.len() == 1
                        && configured.is_some_and(|ids| ids.iter().any(|id| id == &values[0])) =>
                {
                    Some(index)
                }
                _ => None,
            },
        ));
    }
    selected
}

fn allowlist_choices(
    devices: &[crate::audio::DeviceId],
    configured: Option<&[String]>,
    kind: AudioDeviceKind,
) -> Vec<PickerChoice> {
    let mut choices = vec![
        PickerChoice {
            label: crate::ui::presentation::allowlist_mode_label(AllowlistMode::All, kind).into(),
            value: PickerValue::AllowlistMode(AllowlistMode::All),
        },
        PickerChoice {
            label: crate::ui::presentation::allowlist_mode_label(AllowlistMode::Selected, kind)
                .into(),
            value: PickerValue::AllowlistMode(AllowlistMode::Selected),
        },
        PickerChoice {
            label: crate::ui::presentation::allowlist_mode_label(AllowlistMode::Disabled, kind)
                .into(),
            value: PickerValue::AllowlistMode(AllowlistMode::Disabled),
        },
    ];
    choices.extend(devices.iter().enumerate().map(|(index, device)| {
        PickerChoice {
            label: crate::ui::presentation::device_choice_label_at(devices, index, None, kind)
                .unwrap_or_else(|| "Device unavailable".into()),
            value: PickerValue::Allowlist(Some(vec![device.endpoint.clone()])),
        }
    }));
    if let Some(configured) = configured {
        for endpoint in configured {
            if !devices.iter().any(|device| device.endpoint == *endpoint) {
                choices.push(PickerChoice {
                    label: "Saved device unavailable — reconnect it to use it".into(),
                    value: PickerValue::Allowlist(Some(vec![endpoint.clone()])),
                });
            }
        }
    }
    choices
}

fn display_profile_choices(profiles: &crate::display::DisplayProfilesCfg) -> Vec<PickerChoice> {
    let mut choices = vec![PickerChoice {
        label: "No profile selected".into(),
        value: PickerValue::DisplayProfile(None),
    }];
    choices.extend(profiles.profiles.iter().map(|profile| {
        let status = if profile.confirmed {
            "Ready"
        } else {
            "Needs a test"
        };
        PickerChoice {
            label: format!("{} — {} ({status})", profile.name, profile.topology.label()),
            value: PickerValue::DisplayProfile(Some(profile.id.clone())),
        }
    }));
    choices
}

fn modifier_choices(allow_unassigned: bool) -> Vec<PickerChoice> {
    let mut choices = Vec::new();
    if allow_unassigned {
        choices.push(PickerChoice {
            label: "Unassigned".into(),
            value: PickerValue::Modifier(ModifierMask::NONE),
        });
    }
    for bits in 1u8..=0b1111 {
        let modifier = ModifierMask::from_bits(bits);
        choices.push(PickerChoice {
            label: format_modifier_display(modifier),
            value: PickerValue::Modifier(modifier),
        });
    }
    choices
}

fn device_choices(
    devices: &[crate::audio::DeviceId],
    default: Option<&crate::audio::DeviceId>,
    kind: AudioDeviceKind,
) -> Vec<PickerChoice> {
    devices
        .iter()
        .enumerate()
        .map(|(index, device)| PickerChoice {
            label: crate::ui::presentation::device_choice_label_at(devices, index, default, kind)
                .unwrap_or_else(|| "Device unavailable".into()),
            value: PickerValue::Device(DeviceSelection::Endpoint(device.endpoint.clone())),
        })
        .collect()
}

fn current_device_index(
    selection: &DeviceSelection,
    default: Option<&crate::audio::DeviceId>,
    choices: &[PickerChoice],
) -> usize {
    let endpoint = match selection {
        DeviceSelection::Default => default.map(|device| device.endpoint.as_str()),
        DeviceSelection::Endpoint(endpoint) => Some(endpoint.as_str()),
    };
    endpoint
        .and_then(|endpoint| {
            choices.iter().position(|choice| {
                matches!(
                    &choice.value,
                    PickerValue::Device(DeviceSelection::Endpoint(id)) if id == endpoint
                )
            })
        })
        .unwrap_or(0)
}

fn current_picker_value(
    kind: PickerKind,
    draft: &Config,
    selected_display_route: usize,
) -> PickerValue {
    match kind {
        PickerKind::InputDevice => PickerValue::Device(draft.audio.input_device.clone()),
        PickerKind::OutputDevice => PickerValue::Device(draft.audio.output_device.clone()),
        PickerKind::InputAllowlist => {
            PickerValue::AllowlistMode(allowlist_mode(draft.audio.cycle_input_allowlist.as_deref()))
        }
        PickerKind::OutputAllowlist => PickerValue::AllowlistMode(allowlist_mode(
            draft.audio.cycle_output_allowlist.as_deref(),
        )),
        PickerKind::DisplayProfile => {
            PickerValue::DisplayProfile(draft.display_profiles.active_profile.clone())
        }
        PickerKind::DisplayOutputs => PickerValue::DisplayOutputs(Vec::new()),
        PickerKind::DisplayTopology => PickerValue::DisplayTopology(
            draft
                .display_profiles
                .active()
                .map(|profile| profile.topology)
                .unwrap_or_default(),
        ),
        PickerKind::DisplayRoute => PickerValue::DisplayRoute(selected_display_route),
        PickerKind::InputRole => PickerValue::Role(draft.audio.input_role),
        PickerKind::OutputRole => PickerValue::Role(draft.audio.output_role),
        PickerKind::DesktopNumberModifier => {
            PickerValue::Modifier(draft.virtual_desktops.number_modifier)
        }
        PickerKind::MoveDesktopModifier => PickerValue::Modifier(
            draft
                .virtual_desktops
                .move_follow_modifier
                .unwrap_or(ModifierMask::NONE),
        ),
        PickerKind::SilentMoveDesktopModifier => PickerValue::Modifier(
            draft
                .virtual_desktops
                .move_silent_modifier
                .unwrap_or(ModifierMask::NONE),
        ),
        PickerKind::OverlayAppearance => PickerValue::Appearance(draft.overlay.appearance),
        PickerKind::OverlayPosition => PickerValue::Position(draft.overlay.position),
        PickerKind::OverlayMonitor => PickerValue::Monitor(draft.overlay.monitor.clone()),
    }
}
