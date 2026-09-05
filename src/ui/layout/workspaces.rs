//! Workspaces for the layout.

use super::builder::{add_heading, add_hotkey_grid, add_region, add_row, add_section_content_gap};
use super::model::{ElementId, ElementKind, LayoutContext, RegionKind};
use super::shell::SettingsLayout;

pub(super) fn add_workspaces(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Workspaces",
        "Configure desktop and Special Desktop shortcuts.",
        &mut y,
    );
    add_heading(
        layout,
        "Workspace shortcuts",
        "One master switch controls desktop and Special Desktop actions.",
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
            "Turn this on to use the controls below"
        },
    );
    if !context.workspace_enabled {
        add_region(layout, RegionKind::WorkspaceNotice, &mut y, 76.0);
        add_heading(
            layout,
            "Special Desktop",
            "A dedicated place for windows you want nearby but out of the way.",
            &mut y,
        );
        add_hotkey_grid(
            layout,
            &mut y,
            &[
                (
                    ElementId::AssignScratchpadHotkey,
                    "Move window to Special Desktop",
                    "Send the current window to Special Desktop",
                ),
                (
                    ElementId::ToggleScratchpadHotkey,
                    "Open / close Special Desktop",
                    "Open Special Desktop or return",
                ),
            ],
        );
        return;
    }
    add_heading(
        layout,
        "Numbered desktops",
        "The modifier applies to the nine normal desktop numbers.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::WinNumberEnabled,
        ElementKind::Toggle,
        "Desktop number shortcuts",
        "Switch to desktops 1–9",
    );
    add_row(
        layout,
        &mut y,
        ElementId::DesktopNumberModifier,
        ElementKind::Value,
        "Desktop shortcut modifier",
        "Use this modifier with 1–9",
    );
    add_row(
        layout,
        &mut y,
        ElementId::MoveDesktopModifier,
        ElementKind::Value,
        "Move window and follow",
        "Move the current window, then follow it",
    );
    add_row(
        layout,
        &mut y,
        ElementId::SilentMoveDesktopModifier,
        ElementKind::Value,
        "Move window quietly",
        "Move without leaving the current desktop",
    );
    add_hotkey_grid(
        layout,
        &mut y,
        &[(
            ElementId::PreviousDesktopHotkey,
            "Previous desktop",
            "Return to the last normal desktop",
        )],
    );
    add_heading(
        layout,
        "Special Desktop",
        "A dedicated place for windows you want nearby but out of the way.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_hotkey_grid(
        layout,
        &mut y,
        &[
            (
                ElementId::AssignScratchpadHotkey,
                "Move window to Special Desktop",
                "Send the current window to Special Desktop",
            ),
            (
                ElementId::ToggleScratchpadHotkey,
                "Open / close Special Desktop",
                "Open Special Desktop or return",
            ),
        ],
    );
}
