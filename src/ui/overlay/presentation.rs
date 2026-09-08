//! Presentation for the overlay.

use super::model::{concise, OverlayIcon, OverlayLifetime, OverlayModel, OverlayRow, OverlayTone};

use crate::config::model::OverlayNotificationCategory;
use crate::ui::presentation::{friendly_device, AudioDeviceKind};

pub(crate) fn microphone_row(state: &crate::audio::AudioState) -> OverlayRow {
    use crate::audio::AudioState;
    match state {
        AudioState::Muted { volume_pct } => OverlayRow {
            category: Some(OverlayNotificationCategory::Microphone),
            icon: OverlayIcon::Microphone,
            tone: OverlayTone::Muted,
            title: "Microphone muted".into(),
            detail: format!("{volume_pct}% input volume"),
        },
        AudioState::Active { volume_pct } => OverlayRow {
            category: Some(OverlayNotificationCategory::Microphone),
            icon: OverlayIcon::Microphone,
            tone: OverlayTone::Active,
            title: "Microphone unmuted".into(),
            detail: format!("Ready · {volume_pct}% input volume"),
        },
        AudioState::Unavailable { .. } => OverlayRow {
            category: Some(OverlayNotificationCategory::Microphone),
            icon: OverlayIcon::Microphone,
            tone: OverlayTone::Unavailable,
            title: "Microphone unavailable".into(),
            detail: "Windows Audio is not available".into(),
        },
    }
}

pub(crate) fn microphone_overlay_model(state: &crate::audio::AudioState) -> OverlayModel {
    let lifetime = match state {
        crate::audio::AudioState::Muted { .. } => OverlayLifetime::Sticky(OverlayIcon::Microphone),
        _ => OverlayLifetime::Transient,
    };
    OverlayModel::single_with_lifetime(microphone_row(state), lifetime)
}

pub(crate) fn output_row(state: &crate::audio::OutputState) -> OverlayRow {
    use crate::audio::OutputState;
    match state {
        OutputState::Current {
            device,
            muted,
            volume_pct,
        } => OverlayRow {
            category: Some(OverlayNotificationCategory::Speaker),
            icon: OverlayIcon::Output,
            tone: if *muted {
                OverlayTone::Muted
            } else {
                OverlayTone::Active
            },
            title: if *muted {
                "Speaker muted".into()
            } else {
                concise(&friendly_device(device, AudioDeviceKind::Speaker).primary)
            },
            detail: if *muted {
                "Speaker output is muted".into()
            } else {
                format!("Default speaker · {volume_pct}% volume")
            },
        },
        OutputState::Unavailable { .. } => OverlayRow {
            category: Some(OverlayNotificationCategory::Speaker),
            icon: OverlayIcon::Output,
            tone: OverlayTone::Unavailable,
            title: "Speakers unavailable".into(),
            detail: "Windows Audio is not available".into(),
        },
    }
}

pub(crate) fn application_row(state: &crate::audio::AppAudioState) -> OverlayRow {
    use crate::audio::Aggregate;
    let (tone, detail) = match state.aggregate {
        Aggregate::AllMuted => (OverlayTone::Muted, "Muted".to_string()),
        Aggregate::AllActive => (OverlayTone::Active, "Active".to_string()),
        Aggregate::Mixed => (OverlayTone::Changed, "Mixed sessions".to_string()),
        Aggregate::NoSession => (OverlayTone::Unavailable, "No audio session".to_string()),
        Aggregate::Error => (
            OverlayTone::Unavailable,
            "Couldn't update current app audio".to_string(),
        ),
        Aggregate::NoExternalApp => (
            OverlayTone::Unavailable,
            "No current app with audio".to_string(),
        ),
    };
    OverlayRow {
        category: Some(OverlayNotificationCategory::CurrentAppAudio),
        icon: OverlayIcon::Application,
        tone,
        title: "Current app audio".into(),
        detail: if let Some(app_name) = &state.app_name {
            format!("{app_name} · {detail}")
        } else {
            detail
        },
    }
}

pub(crate) fn device_cycle_row(
    flow: crate::audio::DeviceCycleFlow,
    device: &crate::audio::DeviceId,
) -> OverlayRow {
    let input = matches!(flow, crate::audio::DeviceCycleFlow::Input);
    OverlayRow {
        category: Some(if input {
            OverlayNotificationCategory::Microphone
        } else {
            OverlayNotificationCategory::Speaker
        }),
        icon: if input {
            OverlayIcon::Microphone
        } else {
            OverlayIcon::Output
        },
        tone: OverlayTone::Changed,
        title: if input {
            "Next microphone".into()
        } else {
            "Next speaker".into()
        },
        detail: concise(
            &friendly_device(
                device,
                if input {
                    AudioDeviceKind::Microphone
                } else {
                    AudioDeviceKind::Speaker
                },
            )
            .primary,
        ),
    }
}

pub(crate) fn device_cycle_no_devices_row(flow: crate::audio::DeviceCycleFlow) -> OverlayRow {
    let input = matches!(flow, crate::audio::DeviceCycleFlow::Input);
    OverlayRow {
        category: Some(if input {
            OverlayNotificationCategory::Microphone
        } else {
            OverlayNotificationCategory::Speaker
        }),
        icon: if input {
            OverlayIcon::Microphone
        } else {
            OverlayIcon::Output
        },
        tone: OverlayTone::Unavailable,
        title: if input {
            "Next microphone unavailable".into()
        } else {
            "Next speaker unavailable".into()
        },
        detail: if input {
            "No active microphones are available".into()
        } else {
            "No active speakers are available".into()
        },
    }
}

pub(crate) fn device_cycle_error_row(
    flow: crate::audio::DeviceCycleFlow,
    _error: &str,
) -> OverlayRow {
    let input = matches!(flow, crate::audio::DeviceCycleFlow::Input);
    OverlayRow {
        category: Some(if input {
            OverlayNotificationCategory::Microphone
        } else {
            OverlayNotificationCategory::Speaker
        }),
        icon: if input {
            OverlayIcon::Microphone
        } else {
            OverlayIcon::Output
        },
        tone: OverlayTone::Unavailable,
        title: if input {
            "Next microphone unavailable".into()
        } else {
            "Next speaker unavailable".into()
        },
        detail: "Couldn't change the device. Open Diagnostics for help".into(),
    }
}

pub(crate) fn application_volume_row(state: &crate::audio::AppVolumeState) -> OverlayRow {
    let (tone, detail) = if state.app_name.is_none() {
        (OverlayTone::Unavailable, "No current app with audio".into())
    } else {
        match (
            state.min_volume_pct,
            state.max_volume_pct,
            state.sessions,
            state.error.is_some(),
        ) {
            (Some(min), Some(max), _, has_error) => {
                let value = if min == max {
                    format!("Volume {min}%")
                } else {
                    format!("Volume {min}–{max}%")
                };
                (
                    if has_error {
                        OverlayTone::Unavailable
                    } else {
                        OverlayTone::Changed
                    },
                    if has_error {
                        format!("{value} · Some sessions couldn't be updated")
                    } else {
                        value
                    },
                )
            }
            (_, _, 0, true) => (
                OverlayTone::Unavailable,
                "Couldn't change current app audio".into(),
            ),
            (_, _, 0, false) => (OverlayTone::Unavailable, "No active audio session".into()),
            _ => (
                OverlayTone::Unavailable,
                "Current app volume unavailable".into(),
            ),
        }
    };
    OverlayRow {
        category: Some(OverlayNotificationCategory::CurrentAppAudio),
        icon: OverlayIcon::Application,
        tone,
        title: "Current app audio".into(),
        detail: if let Some(app_name) = &state.app_name {
            format!("{app_name} · {detail}")
        } else {
            detail
        },
    }
}
