use super::*;
use crate::ui::layout::LayoutContext;

#[test]
fn profile_selection_and_activation_have_distinct_hit_targets_and_keyboard_stops() {
    let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Displays, "", 3, None);
    let card = layout
        .element(ElementId::DisplayProfileCard(0))
        .unwrap()
        .rect;
    let action = layout
        .element(ElementId::DisplayProfileAction(0))
        .unwrap()
        .rect;
    assert_eq!(
        layout.hit_test(card.x + 20.0, card.y + 20.0),
        Some(ElementId::DisplayProfileCard(0))
    );
    assert_eq!(
        layout.hit_test(action.x + action.w / 2.0, action.y + action.h / 2.0),
        Some(ElementId::DisplayProfileAction(0))
    );
    assert!(layout
        .focus_order()
        .contains(&ElementId::DisplayProfileCard(0)));
    assert!(layout
        .focus_order()
        .contains(&ElementId::DisplayProfileAction(0)));
}

#[test]
fn management_actions_follow_the_selected_row_before_the_remaining_profiles() {
    let context = LayoutContext {
        profile_count: 32,
        selected_profile_index: Some(4),
        ..Default::default()
    };
    let layout = SettingsLayout::build_shell_with_context(
        960.0,
        660.0,
        0.0,
        Page::Displays,
        "",
        context,
        None,
    );
    let selected = layout
        .element(ElementId::DisplayProfileCard(4))
        .unwrap()
        .rect;
    let edit = layout.element(ElementId::EditDisplayProfile).unwrap().rect;
    let next_row = layout
        .element(ElementId::DisplayProfileCard(6))
        .unwrap()
        .rect;
    assert!(edit.y > selected.bottom());
    assert!(edit.bottom() < next_row.y);
}

#[test]
fn display_profiles_use_a_responsive_grid() {
    let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Displays, "", 3, None);
    let first = layout
        .element(ElementId::DisplayProfileCard(0))
        .expect("first profile card");
    let second = layout
        .element(ElementId::DisplayProfileCard(1))
        .expect("second profile card");
    let third = layout
        .element(ElementId::DisplayProfileCard(2))
        .expect("third profile card");
    assert!(second.rect.x > first.rect.x);
    assert!(third.rect.y > first.rect.y);
    assert!(first.rect.w > 200.0);

    let narrow = SettingsLayout::build_shell(760.0, 660.0, 0.0, Page::Displays, "", 2, None);
    let narrow_first = narrow
        .element(ElementId::DisplayProfileCard(0))
        .expect("narrow first profile card");
    let narrow_second = narrow
        .element(ElementId::DisplayProfileCard(1))
        .expect("narrow second profile card");
    assert_eq!(narrow_first.rect.x, narrow_second.rect.x);
    assert!(narrow_second.rect.y > narrow_first.rect.y);
}

#[test]
fn display_overview_keeps_expert_route_editing_out_of_normal_flow() {
    let layout = SettingsLayout::build_shell(1200.0, 900.0, 0.0, Page::Displays, "", 1, None);
    assert!(layout.element(ElementId::DisplayProfileCard(0)).is_some());
    assert!(layout.element(ElementId::EditDisplayRoute).is_none());
    assert!(layout.element(ElementId::DisplayRoute).is_none());
    assert!(layout.element(ElementId::EditDisplayProfile).is_some());
}

#[test]
fn display_recovery_layout_has_only_meaningful_keep_or_revert_actions() {
    let context = super::super::LayoutContext {
        profile_count: 1,
        display_rollback_active: true,
        display_keep_available: true,
        ..Default::default()
    };
    let layout = SettingsLayout::build_shell_with_context(
        1200.0,
        900.0,
        0.0,
        Page::Displays,
        "",
        context,
        None,
    );
    assert!(layout.element(ElementId::KeepDisplayChange).is_some());
    assert!(layout.element(ElementId::UndoDisplayChange).is_some());
    assert!(layout.element(ElementId::TestApplyDisplayProfile).is_none());
    assert!(layout.element(ElementId::DeleteDisplayProfile).is_none());
}

#[test]
fn one_profile_uses_the_available_card_width() {
    let layout = SettingsLayout::build_shell(1920.0, 1080.0, 0.0, Page::Displays, "", 1, None);
    let card = layout
        .element(ElementId::DisplayProfileCard(0))
        .expect("profile card");
    assert_eq!(card.rect.w, layout.content_column.w);
}

#[test]
fn display_editor_reflows_one_step_without_form_cemetery() {
    for (step, has_routes, has_topology, has_name) in [
        (DisplayWizardStep::Displays, true, false, false),
        (DisplayWizardStep::Arrangement, false, true, false),
        (DisplayWizardStep::NameAndShortcut, false, false, true),
    ] {
        let context = super::super::LayoutContext {
            display_editor_step: Some(step),
            display_output_count: 2,
            display_route_count: 2,
            ..Default::default()
        };
        let layout = SettingsLayout::build_shell_with_context(
            1200.0,
            900.0,
            0.0,
            Page::Displays,
            "",
            context,
            None,
        );
        assert_eq!(
            layout.element(ElementId::DisplayOutputCard(0)).is_some(),
            has_routes
        );
        assert_eq!(
            layout
                .element(ElementId::DisplayTopologyChoice(0))
                .is_some(),
            has_topology
        );
        assert_eq!(
            layout.element(ElementId::RenameDisplayProfile).is_some(),
            has_name
        );
    }
}

#[test]
fn display_editor_has_one_step_at_a_time() {
    let context = super::super::LayoutContext {
        profile_count: 1,
        display_editor_step: Some(DisplayWizardStep::Displays),
        display_output_count: 2,
        display_route_count: 2,
        ..Default::default()
    };
    let layout = SettingsLayout::build_shell_with_context(
        1200.0,
        900.0,
        0.0,
        Page::Displays,
        "",
        context,
        None,
    );
    assert!(layout.element(ElementId::DisplayOutputCard(0)).is_some());
    assert!(layout
        .element(ElementId::DisplayTopologyChoice(0))
        .is_none());
    assert!(layout.element(ElementId::DisplayWizardNext).is_some());
    assert_eq!(
        layout
            .sections
            .first()
            .map(|section| section.title.as_str()),
        Some("Display profile editor")
    );
    assert_eq!(
        layout
            .sections
            .iter()
            .filter(|section| section.title == "Displays")
            .count(),
        0
    );
}
