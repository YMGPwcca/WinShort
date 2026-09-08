//! System for the layout.

use super::builder::{add_element, add_heading, add_row, add_section_content_gap};
use super::geometry::Rect;
use super::model::{ElementId, ElementKind};
use super::shell::SettingsLayout;

pub(super) fn add_system(layout: &mut SettingsLayout) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "System",
        "Small choices that shape how WinShort lives on your PC.",
        &mut y,
    );
    add_heading(
        layout,
        "Startup",
        "Decide whether WinShort is ready after sign-in.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::StartWithWindows,
        ElementKind::Toggle,
        "Start WinShort with Windows",
        "Launch quietly after you sign in",
    );
    add_heading(
        layout,
        "Behavior",
        "Pause everything without changing your shortcuts.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::StartHotkeysEnabled,
        ElementKind::Toggle,
        "Pause all shortcuts",
        "Temporarily stop global shortcut actions",
    );
    add_heading(
        layout,
        "Support",
        "Keep technical details available when you need them.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    let diagnostics_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::DiagnosticsStatus,
        ElementKind::ButtonSecondary,
        "Diagnostics and support",
        "Inspect status, copy details, or create a sanitized bundle",
        Rect::new(
            layout.content_column.x,
            diagnostics_y,
            layout.content_column.w,
            52.0,
        ),
    );
    let folder_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::OpenConfigFolder,
        ElementKind::ButtonSecondary,
        "Open configuration folder",
        "Open WinShort's local files",
        Rect::new(
            layout.content_column.x,
            folder_y,
            layout.content_column.w,
            52.0,
        ),
    );
    add_heading(
        layout,
        "Reset",
        "Reset is destructive and always asks twice.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    let reset_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::ResetSettings,
        ElementKind::ButtonDanger,
        "Reset WinShort",
        "Restore defaults after a second confirmation",
        Rect::new(
            layout.content_column.x,
            reset_y,
            layout.content_column.w,
            52.0,
        ),
    );
    add_heading(
        layout,
        "About",
        concat!("WinShort · Version ", env!("CARGO_PKG_VERSION")),
        &mut y,
    );
}
