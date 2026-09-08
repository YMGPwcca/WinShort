//! Display profile wizard layout.

use super::builder::{add_element, add_heading, add_region, add_row, add_section_content_gap};
use super::geometry::Rect;
use super::model::{ElementId, ElementKind, LayoutContext, RegionKind};
use super::shell::SettingsLayout;
use crate::ui::presentation::DisplayWizardStep;
use crate::ui::theme::UiTokens;

pub(super) fn add_display_wizard(
    layout: &mut SettingsLayout,
    context: &LayoutContext,
    step: DisplayWizardStep,
) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Display profile editor",
        "Build an arrangement, then test it before keeping it.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_region(layout, RegionKind::DisplayWizardSteps, &mut y, 58.0);

    match step {
        DisplayWizardStep::Displays => add_screen_step(layout, context, &mut y),
        DisplayWizardStep::Arrangement => add_arrangement_step(layout, context, &mut y),
        DisplayWizardStep::NameAndShortcut => add_name_step(layout, &mut y),
        DisplayWizardStep::Review => add_review_step(layout, context, &mut y),
    }
    add_wizard_navigation(layout, step, &mut y);
}

fn add_screen_step(layout: &mut SettingsLayout, context: &LayoutContext, y: &mut f32) {
    add_heading(
        layout,
        "Which screens",
        if context.display_inventory_unknown {
            "Windows display information is unavailable. These are saved screens; connection status is unknown."
        } else {
            "Choose which screens this profile should use."
        },
        y,
    );
    add_section_content_gap(y);
    if context.display_output_count == 0 {
        add_region(layout, RegionKind::DisplayWizardSummary, y, 104.0);
        return;
    }

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
    let start = *y;
    let count = context.display_output_count.min(32);
    for index in 0..count {
        let row = index / columns;
        let column = index % columns;
        add_element(
            layout,
            y,
            ElementId::DisplayOutputCard(index as u8),
            ElementKind::Checkbox,
            "Screen",
            "Select this screen",
            Rect::new(
                layout.content_column.x + column as f32 * (card_w + column_gap),
                start + row as f32 * (66.0 + UiTokens::ROW_GAP),
                card_w,
                66.0,
            ),
        );
    }
    let rows = count.div_ceil(columns);
    *y = start + rows as f32 * (66.0 + UiTokens::ROW_GAP);
}

fn add_arrangement_step(layout: &mut SettingsLayout, context: &LayoutContext, y: &mut f32) {
    add_heading(
        layout,
        "How they work",
        "Choose a simple arrangement for the selected screens.",
        y,
    );
    add_section_content_gap(y);
    if context.display_route_count <= 1 {
        add_region(layout, RegionKind::DisplayWizardSummary, y, 122.0);
        return;
    }

    for (index, (label, description)) in [
        ("Extend", "Show selected screens as separate desktops"),
        ("Duplicate", "Show the same picture on each selected screen"),
    ]
    .into_iter()
    .enumerate()
    {
        let choice_y = *y;
        add_element(
            layout,
            y,
            ElementId::DisplayTopologyChoice(index as u8),
            ElementKind::Choice,
            label,
            description,
            Rect::new(
                layout.content_column.x,
                choice_y,
                layout.content_column.w,
                170.0,
            ),
        );
    }
}

fn add_name_step(layout: &mut SettingsLayout, y: &mut f32) {
    add_heading(
        layout,
        "Name & shortcut",
        "Give this arrangement a name people can recognize.",
        y,
    );
    add_section_content_gap(y);
    add_row(
        layout,
        y,
        ElementId::RenameDisplayProfile,
        ElementKind::Action,
        "Profile name",
        "Choose a short name for this arrangement",
    );
    add_row(
        layout,
        y,
        ElementId::DisplayProfileHotkey,
        ElementKind::Hotkey,
        "Shortcut (optional)",
        "Activate this profile from any app",
    );
}

fn add_review_step(layout: &mut SettingsLayout, context: &LayoutContext, y: &mut f32) {
    add_heading(
        layout,
        "Review",
        "Check the summary, then test the display setup safely.",
        y,
    );
    add_section_content_gap(y);
    let summary_rect = add_region(layout, RegionKind::DisplayWizardSummary, y, 178.0);
    add_element(
        layout,
        y,
        ElementId::DisplayWizardSummary,
        ElementKind::Info,
        "Display profile review",
        "Profile, screens, arrangement, shortcut, and readiness",
        summary_rect,
    );
    if context.display_draft_dirty {
        let discard_y = *y;
        add_element(
            layout,
            y,
            ElementId::DiscardDisplayEdits,
            ElementKind::ButtonSecondary,
            "Discard changes",
            "Return to the last saved profile",
            Rect::new(layout.content_column.x, discard_y, 150.0, 36.0),
        );
    }
    let test_y = *y;
    add_element(
        layout,
        y,
        ElementId::TestApplyDisplayProfile,
        ElementKind::ButtonPrimary,
        "Test profile",
        if context.display_inventory_unknown {
            "Status not previewed here; Test revalidates before applying"
        } else {
            "Try it for 15 seconds before keeping it"
        },
        Rect::new(
            layout.content_column.x,
            test_y,
            layout.content_column.w,
            UiTokens::ROW_HEIGHT,
        ),
    );
}

fn add_wizard_navigation(layout: &mut SettingsLayout, step: DisplayWizardStep, y: &mut f32) {
    let nav_y = *y;
    if step.previous().is_some() {
        add_element(
            layout,
            y,
            ElementId::DisplayWizardBack,
            ElementKind::ButtonSecondary,
            "Back",
            "Return to the previous step",
            Rect::new(layout.content_column.x, nav_y, 106.0, 36.0),
        );
    }

    let (id, kind, label, description) = if step.next().is_some() {
        (
            ElementId::DisplayWizardNext,
            ElementKind::ButtonPrimary,
            "Next",
            "Continue to the next step",
        )
    } else {
        (
            ElementId::DisplayWizardCancel,
            ElementKind::ButtonSecondary,
            "Cancel",
            "Discard this display draft and return to Display Profiles",
        )
    };
    add_element(
        layout,
        y,
        id,
        kind,
        label,
        description,
        Rect::new(layout.content_column.right() - 106.0, nav_y, 106.0, 36.0),
    );
}
