//! Audio for the layout.

use super::builder::{
    add_element, add_heading, add_hotkey_grid, add_region, add_row, add_section_content_gap,
};
use super::geometry::Rect;
use super::model::{ElementId, ElementKind, LayoutContext, RegionKind};
use super::shell::SettingsLayout;
use crate::ui::presentation::AllowlistMode;
use crate::ui::theme::UiTokens;

fn add_audio_mode_group(
    layout: &mut SettingsLayout,
    y: &mut f32,
    kind: crate::ui::presentation::AudioDeviceKind,
    mode: AllowlistMode,
    device_count: usize,
) {
    let row_gap = UiTokens::ROW_GAP;
    let mode_height = 36.0;
    let mode_step = mode_height + row_gap;
    let start = *y;
    for (index, candidate) in [
        AllowlistMode::All,
        AllowlistMode::Selected,
        AllowlistMode::Disabled,
    ]
    .into_iter()
    .enumerate()
    {
        let id = match kind {
            crate::ui::presentation::AudioDeviceKind::Microphone => {
                ElementId::InputCycleMode(index as u8)
            }
            crate::ui::presentation::AudioDeviceKind::Speaker => {
                ElementId::OutputCycleMode(index as u8)
            }
        };
        let selected = candidate == mode;
        add_element(
            layout,
            y,
            id,
            ElementKind::Choice,
            crate::ui::presentation::allowlist_mode_label(candidate, kind),
            if selected {
                "Selected"
            } else {
                "Choose this cycling mode"
            },
            Rect::new(
                layout.content_column.x,
                start + index as f32 * mode_step,
                layout.content_column.w,
                mode_height,
            ),
        );
    }
    *y = start + 3.0 * mode_step;
    if mode == AllowlistMode::Selected && device_count > 0 {
        let column_gap = UiTokens::CARD_COLUMN_GAP;
        let columns = if layout.content_column.w >= 660.0 {
            2
        } else {
            1
        };
        let card_w = if columns == 1 {
            layout.content_column.w
        } else {
            (layout.content_column.w - column_gap) * 0.5
        };
        let device_height = 42.0;
        let device_step = device_height + row_gap;
        let device_start = *y;
        for index in 0..device_count.min(32) {
            let row = index / columns;
            let column = index % columns;
            let id = match kind {
                crate::ui::presentation::AudioDeviceKind::Microphone => {
                    ElementId::InputCycleDevice(index as u8)
                }
                crate::ui::presentation::AudioDeviceKind::Speaker => {
                    ElementId::OutputCycleDevice(index as u8)
                }
            };
            add_element(
                layout,
                y,
                id,
                ElementKind::Checkbox,
                format!("{} option", kind.noun()),
                "Use this device when cycling",
                Rect::new(
                    layout.content_column.x + column as f32 * (card_w + column_gap),
                    device_start + row as f32 * device_step,
                    card_w,
                    device_height,
                ),
            );
        }
        let rows = device_count.min(32).div_ceil(columns);
        *y = device_start + rows as f32 * device_step;
    }
}

pub(super) fn add_audio(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Audio",
        "Choose Windows default devices and how Next speaker or microphone cycles.",
        &mut y,
    );
    add_heading(
        layout,
        "Speakers",
        "Choose the Windows playback device and cycling mode.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::OutputDevice,
        ElementKind::Value,
        "Windows playback device",
        "Choose the system default speaker",
    );
    add_audio_mode_group(
        layout,
        &mut y,
        crate::ui::presentation::AudioDeviceKind::Speaker,
        context.output_cycle_mode,
        context.output_device_count,
    );
    add_heading(
        layout,
        "Microphones",
        "Choose the Windows recording device and cycling mode.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::InputDevice,
        ElementKind::Value,
        "Windows recording device",
        "Choose the system default microphone",
    );
    add_audio_mode_group(
        layout,
        &mut y,
        crate::ui::presentation::AudioDeviceKind::Microphone,
        context.input_cycle_mode,
        context.input_device_count,
    );
    let current_app_description = if context.current_app_audio_available {
        "WinShort itself is not a target for these actions."
    } else {
        "Available when another app has audio."
    };
    add_heading(layout, "Current app audio", current_app_description, &mut y);
    add_section_content_gap(&mut y);
    if context.current_app_audio_available {
        add_region(layout, RegionKind::AudioCurrentApp, &mut y, 64.0);
    }
    add_hotkey_grid(
        layout,
        &mut y,
        &[
            (
                ElementId::ForegroundHotkey,
                "Mute current app",
                "Toggle all sessions owned by another app",
            ),
            (
                ElementId::ForegroundVolumeUpHotkey,
                "Current app volume up",
                "Raise the current app by five percent",
            ),
            (
                ElementId::ForegroundVolumeDownHotkey,
                "Current app volume down",
                "Lower the current app by five percent",
            ),
        ],
    );
}
