//! Shortcuts for the layout.

use super::builder::{add_heading, add_hotkey_grid, add_region, add_row, add_section_content_gap};
use super::model::{ElementId, ElementKind, LayoutContext, RegionKind};
use super::shell::SettingsLayout;

pub(super) fn add_shortcuts(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Shortcuts",
        "Record the actions you use most. Each keycap is ready to change.",
        &mut y,
    );
    if context.paused {
        add_section_content_gap(&mut y);
        add_region(layout, RegionKind::PauseNotice, &mut y, 68.0);
    }
    add_heading(
        layout,
        "Audio",
        "Control devices and the app in front of you.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_hotkey_grid(
        layout,
        &mut y,
        &[
            (
                ElementId::MicHotkey,
                "Mute microphone",
                "Toggle your microphone from any app",
            ),
            (
                ElementId::OutputHotkey,
                "Mute speakers",
                "Toggle speaker mute from any app",
            ),
            (
                ElementId::CycleInputHotkey,
                "Next microphone",
                "Switch to the next selected microphone",
            ),
            (
                ElementId::CycleOutputHotkey,
                "Next speaker",
                "Switch to the next selected speaker",
            ),
            (
                ElementId::ForegroundHotkey,
                "Mute current app",
                "Toggle audio for the app in front",
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
    add_heading(
        layout,
        "Workspaces",
        "Desktop actions are available when Workspace shortcuts are on.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::DesktopsEnabled,
        ElementKind::Toggle,
        "Workspace shortcuts",
        if context.workspace_enabled {
            "Desktop and Special Desktop actions are enabled"
        } else {
            "Turn this on to enable desktop and Special Desktop actions"
        },
    );
    if context.workspace_enabled {
        add_hotkey_grid(
            layout,
            &mut y,
            &[
                (
                    ElementId::PreviousDesktopHotkey,
                    "Previous desktop",
                    "Return to the last normal desktop",
                ),
                (
                    ElementId::AssignScratchpadHotkey,
                    "Move window to Special Desktop",
                    "Keep the current window out of the way",
                ),
                (
                    ElementId::ToggleScratchpadHotkey,
                    "Open / close Special Desktop",
                    "Open Special Desktop or return",
                ),
            ],
        );
    } else {
        add_region(layout, RegionKind::WorkspaceNotice, &mut y, 70.0);
    }
    add_heading(
        layout,
        "Numbered desktops",
        "Use one modifier with the familiar 1–9 family.",
        &mut y,
    );
    if context.workspace_enabled {
        add_section_content_gap(&mut y);
        add_row(
            layout,
            &mut y,
            ElementId::WinNumberEnabled,
            ElementKind::Toggle,
            "Desktop number shortcuts",
            "Switch to numbered desktops",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DesktopNumberModifier,
            ElementKind::Value,
            "Desktop shortcut modifier",
            "Shown as Win + 1–9 or another modifier family",
        );
    }
    add_heading(
        layout,
        "Display profiles",
        "Give the selected arrangement a shortcut.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_hotkey_grid(
        layout,
        &mut y,
        &[(
            (ElementId::DisplayProfileHotkey),
            "Selected display profile",
            "Activate the selected profile from any app",
        )],
    );
}
