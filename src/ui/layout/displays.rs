//! Displays for the layout.

use super::builder::{
    add_button_grid, add_card, add_element, add_heading, add_profile_card, add_region, add_row,
    add_section_content_gap,
};
use super::display_wizard::add_display_wizard;
use super::geometry::Rect;
use super::model::{ElementId, ElementKind, LayoutContext, RegionKind};
use super::shell::SettingsLayout;
use crate::ui::theme::UiTokens;

pub(super) fn add_displays(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    if context.display_profiles_enabled && !context.display_rollback_active {
        if let Some(step) = context.display_editor_step {
            add_display_wizard(layout, context, step);
            return;
        }
    }
    add_heading(
        layout,
        "Displays",
        "Save the way your screens work, then test before you keep it.",
        &mut y,
    );
    if !context.display_profiles_enabled {
        add_section_content_gap(&mut y);
        add_row(
            layout,
            &mut y,
            ElementId::DisplayProfilesEnabled,
            ElementKind::Toggle,
            "Display profiles",
            "Turn this on to save monitor arrangements",
        );
        add_region(layout, RegionKind::DisplaySafety, &mut y, 76.0);
        return;
    }
    if context.display_rollback_active {
        add_section_content_gap(&mut y);
        add_region(layout, RegionKind::DisplaySafety, &mut y, 94.0);
        let safety_action_y = y;
        if context.display_keep_available {
            add_element(
                layout,
                &mut y,
                ElementId::KeepDisplayChange,
                ElementKind::ButtonPrimary,
                "Keep this setup",
                "Confirm the tested display arrangement",
                Rect::new(layout.content_column.x, safety_action_y, 150.0, 36.0),
            );
        }
        add_element(
            layout,
            &mut y,
            ElementId::UndoDisplayChange,
            ElementKind::ButtonDanger,
            "Revert",
            "Restore the previous display arrangement",
            Rect::new(
                layout.content_column.x + 160.0,
                safety_action_y,
                120.0,
                36.0,
            ),
        );
        return;
    }
    add_heading(
        layout,
        "Display profiles",
        "Select a profile, then use Activate or Review.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    if context.profile_count == 0 {
        add_card(
            layout,
            &mut y,
            ElementId::NewDisplayProfile,
            "New from current",
            "Create a profile from the current Windows arrangement",
        );
        return;
    }
    add_profile_grid(layout, context, &mut y);
}

fn add_profile_grid(layout: &mut SettingsLayout, context: &LayoutContext, y: &mut f32) {
    let count = context.profile_count.min(32);
    let columns = if layout.content_column.w >= 660.0 {
        2
    } else {
        1
    };
    let new_y = *y;
    add_element(
        layout,
        y,
        ElementId::NewDisplayProfile,
        ElementKind::ButtonSecondary,
        "New from current",
        "Create a profile from the current Windows arrangement",
        Rect::new(layout.content_column.x, new_y, 180.0, 36.0),
    );
    let selected_row = context
        .selected_profile_index
        .filter(|index| *index < count)
        .map(|index| index / columns);
    for row in 0..count.div_ceil(columns) {
        for index in row * columns..((row + 1) * columns).min(count) {
            add_profile_card(
                layout,
                y,
                index as u8,
                index % columns,
                if count == 1 { 1 } else { columns },
            );
        }
        *y += UiTokens::PROFILE_ROW_STEP;
        if selected_row == Some(row) {
            add_region(layout, RegionKind::SelectedDisplayProfile, y, 32.0);
            add_profile_actions(layout, y);
            *y += UiTokens::SECTION_GAP;
        }
    }
}

fn add_profile_actions(layout: &mut SettingsLayout, y: &mut f32) {
    add_button_grid(
        layout,
        y,
        &[
            (
                ElementId::EditDisplayProfile,
                ElementKind::ButtonSecondary,
                "Edit profile",
                "Open the selected profile's editor",
            ),
            (
                ElementId::UpdateDisplayProfile,
                ElementKind::ButtonSecondary,
                "Replace from current",
                "Update the selected profile from Windows' arrangement",
            ),
            (
                ElementId::DuplicateDisplayProfile,
                ElementKind::ButtonSecondary,
                "Duplicate",
                "Create a separate editable copy",
            ),
            (
                ElementId::DeleteDisplayProfile,
                ElementKind::ButtonDanger,
                "Delete",
                "Remove the selected profile",
            ),
        ],
    );
}
