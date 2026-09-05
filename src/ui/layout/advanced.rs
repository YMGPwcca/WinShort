//! Advanced for the layout.

use super::builder::{add_element, add_heading, add_row, add_section_content_gap};
use super::geometry::Rect;
use super::model::{ElementId, ElementKind};
use super::shell::SettingsLayout;

pub(super) fn add_advanced(layout: &mut SettingsLayout) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Advanced",
        "Technical controls for troubleshooting and fine tuning.",
        &mut y,
    );
    add_heading(layout, "Audio", "Windows default-device behavior.", &mut y);
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::InputRole,
        ElementKind::Value,
        "Microphone device role",
        "Role used for mute and status tracking",
    );
    add_row(
        layout,
        &mut y,
        ElementId::OutputRole,
        ElementKind::Value,
        "Speaker device role",
        "Role used for mute and status tracking",
    );
    add_heading(
        layout,
        "Display output editing",
        "Exact values are useful when a profile needs repair.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::DisplayRoute,
        ElementKind::Value,
        "Display output",
        "Select an output from the active display profile",
    );
    let edit_output_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::EditDisplayRoute,
        ElementKind::ButtonSecondary,
        "Edit display output",
        "Position, resolution, refresh, and rotation",
        Rect::new(
            layout.content_column.x,
            edit_output_y,
            layout.content_column.w,
            52.0,
        ),
    );
    add_heading(
        layout,
        "Troubleshooting",
        "Temporary diagnostics only; no setting is persisted here.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::DebugLogging,
        ElementKind::Toggle,
        "Temporary debug logging",
        "Add detail until WinShort restarts",
    );
    let diagnostics_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::DiagnosticsStatus,
        ElementKind::ButtonSecondary,
        "Diagnostics and support",
        "Inspect runtime and implementation detail",
        Rect::new(
            layout.content_column.x,
            diagnostics_y,
            layout.content_column.w,
            52.0,
        ),
    );
}
