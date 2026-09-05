//! Home for the layout.

use super::builder::{add_card_pair, add_element, add_heading, add_section_content_gap};
use super::geometry::Rect;
use super::model::{ElementId, ElementKind};
use super::shell::SettingsLayout;
use crate::ui::theme::UiTokens;

pub(super) fn add_home(layout: &mut SettingsLayout) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Welcome back",
        "What WinShort is doing right now.",
        &mut y,
    );
    add_heading(layout, "Audio", "Your current Windows devices.", &mut y);
    add_section_content_gap(&mut y);
    add_card_pair(
        layout,
        &mut y,
        (
            ElementId::HomeSpeaker,
            "Speakers",
            "Current playback device",
        ),
        (
            ElementId::HomeMicrophone,
            "Microphone",
            "Current recording device",
        ),
    );
    add_heading(
        layout,
        "Workspace",
        "Keep your windows within reach.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_card_pair(
        layout,
        &mut y,
        (
            ElementId::HomeCurrentDesktop,
            "Current desktop",
            "Your current normal workspace",
        ),
        (
            ElementId::HomeSpecial,
            "Special Desktop",
            "Keep windows you want to bring back quickly",
        ),
    );
    let action_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::HomePreviousDesktop,
        ElementKind::ButtonSecondary,
        "Previous desktop",
        "Return to the last normal desktop",
        Rect::new(
            layout.content_column.x,
            action_y,
            layout.content_column.w,
            UiTokens::ROW_HEIGHT,
        ),
    );
    add_heading(
        layout,
        "Display and shortcuts",
        "The two things you reach for most often.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_card_pair(
        layout,
        &mut y,
        (
            ElementId::HomeDisplayProfile,
            "Display",
            "Saved screen arrangements",
        ),
        (
            ElementId::HomeShortcutHealth,
            "Shortcuts",
            "Active actions and conflicts",
        ),
    );
    let status_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::HomeDiagnostics,
        ElementKind::ButtonSecondary,
        "System status",
        "Details and diagnostics when something needs attention",
        Rect::new(
            layout.content_column.x,
            status_y,
            layout.content_column.w,
            UiTokens::CARD_HEIGHT,
        ),
    );
}
