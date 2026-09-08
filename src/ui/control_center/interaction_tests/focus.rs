use super::*;

#[test]
fn pointer_focus_stays_semantic_without_painting_a_focus_ring() {
    let id = ElementId::OverlayEnabled;
    let mut ui = empty_settings_ui();
    {
        let value = Some(id);
        ui.focus.set_target(value);
    };
    {
        let value = false;
        ui.focus.set_indicator_visible(value);
    };
    assert!(!ui.interaction(id, false).focused);

    {
        let value = true;
        ui.focus.set_indicator_visible(value);
    };
    assert!(ui.interaction(id, false).focused);
}

#[test]
fn focus_policy_skips_disabled_and_wraps_both_directions() {
    let order = [
        ElementId::StartWithWindows,
        ElementId::InputRole,
        ElementId::OutputRole,
    ];
    let disabled = |id| id == ElementId::InputRole;
    assert_eq!(
        crate::ui::focus::next_focus_target(
            &order,
            Some(ElementId::StartWithWindows),
            false,
            disabled,
        ),
        Some(ElementId::OutputRole)
    );
    assert_eq!(
        crate::ui::focus::next_focus_target(&order, Some(ElementId::OutputRole), false, disabled,),
        Some(ElementId::StartWithWindows)
    );
    assert_eq!(
        crate::ui::focus::next_focus_target(&order, Some(ElementId::OutputRole), true, disabled,),
        Some(ElementId::StartWithWindows)
    );
}

#[test]
fn high_contrast_settings_theme_uses_system_pairs_for_hover_and_focus() {
    let visual = SystemVisualPreferences {
        high_contrast: true,
        high_contrast_background: crate::platform::visual::VisualRgb { r: 8, g: 16, b: 24 },
        high_contrast_foreground: crate::platform::visual::VisualRgb {
            r: 240,
            g: 232,
            b: 224,
        },
        high_contrast_highlight: crate::platform::visual::VisualRgb {
            r: 32,
            g: 96,
            b: 160,
        },
        high_contrast_highlight_foreground: crate::platform::visual::VisualRgb {
            r: 255,
            g: 255,
            b: 255,
        },
        ..SystemVisualPreferences::default()
    };
    let theme = settings_theme_for(Theme::dark(), visual);
    assert_eq!(theme.card, Color::rgb(8, 16, 24));
    assert_eq!(theme.card_hover, theme.card);
    assert_eq!(theme.control_hover, theme.card);
    assert_eq!(theme.picker_hover, theme.card);
    assert_eq!(theme.border_strong, Color::rgb(240, 232, 224));
    assert_eq!(theme.focus, theme.border_strong);
    assert_eq!(theme.accent, Color::rgb(32, 96, 160));
    assert_eq!(theme.accent_text, Color::rgb(255, 255, 255));
}

#[test]
fn search_pointer_focus_clears_on_blank_client_click() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = search_settings_ui();
    ui.set_pointer_focus(Some(ElementId::Search));
    ui.sync_search_caret(hwnd);
    assert!(ui.search_has_focus());

    let blank = ui
        .layout
        .hit_test(ui.layout.top_bar.x + 8.0, ui.layout.top_bar.y + 2.0);
    assert_eq!(blank, None);
    ui.set_pointer_focus(blank);
    ui.sync_search_caret(hwnd);

    assert_eq!(ui.focus.target(), None);
    assert!(!ui.search_has_focus());
    assert!(!ui.caret.visible());
}

#[test]
fn search_pointer_focus_transfers_to_another_control() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = search_settings_ui();
    ui.set_pointer_focus(Some(ElementId::Search));
    ui.sync_search_caret(hwnd);

    let audio = ui
        .layout
        .element(ElementId::Nav(Page::Audio))
        .expect("audio navigation")
        .rect;
    let target = ui
        .layout
        .hit_test(audio.x + audio.w * 0.5, audio.y + audio.h * 0.5);
    assert_eq!(target, Some(ElementId::Nav(Page::Audio)));
    ui.set_pointer_focus(target);
    ui.sync_search_caret(hwnd);

    assert_eq!(ui.focus.target(), target);
    assert!(!ui.search_has_focus());
    assert!(!ui.caret.visible());
}

#[test]
fn search_external_focus_loss_clears_editing_and_character_handling() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let outside = HWND(17usize as *mut _);
    let mut ui = search_settings_ui();
    ui.set_pointer_focus(Some(ElementId::Search));
    ui.sync_search_caret(hwnd);
    assert!(ui.handle_search_char(hwnd, 'a' as u16));
    assert_eq!(ui.search_query, "a");

    ui.on_window_focus(hwnd, false, outside);

    assert_eq!(ui.focus.target(), None);
    assert!(!ui.search_has_focus());
    assert!(!ui.caret.visible());
    assert!(!ui.handle_search_char(hwnd, 'b' as u16));
    assert!(!ui.handle_search_key(hwnd, 0x08));
    assert_eq!(ui.search_query, "a");
}

#[test]
fn stale_search_element_does_not_consume_input_without_settings_focus() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = search_settings_ui();
    {
        let value = Some(ElementId::Search);
        ui.focus.set_target(value);
    };
    {
        let value = AutomationFocusOwner::Outside;
        ui.focus.set_owner(value);
    };
    ui.search_query = "keep".into();
    ui.caret = super::super::search::SearchCaret::active(std::time::Instant::now());

    assert!(!ui.handle_search_char(hwnd, 'x' as u16));
    assert!(!ui.handle_search_key(hwnd, 0x08));
    ui.sync_search_caret(hwnd);
    assert_eq!(ui.search_query, "keep");
    assert!(!ui.caret.visible());
    assert!(!ui.search_has_focus());
}

#[test]
fn search_caret_visibility_uses_the_editing_focus_predicate() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = search_settings_ui();
    {
        let value = Some(ElementId::Search);
        ui.focus.set_target(value);
    };
    {
        let value = AutomationFocusOwner::Outside;
        ui.focus.set_owner(value);
    };
    ui.caret = super::super::search::SearchCaret::active(std::time::Instant::now());
    ui.sync_search_caret(hwnd);
    assert!(!ui.caret.visible());

    {
        let value = AutomationFocusOwner::Settings;
        ui.focus.set_owner(value);
    };
    ui.sync_search_caret(hwnd);
    assert!(ui.search_has_focus());
    assert!(ui.caret.visible());
    assert!(ui.caret.deadline().is_some());

    {
        let value = Some(ElementId::Nav(Page::Audio));
        ui.focus.set_target(value);
    };
    ui.sync_search_caret(hwnd);
    assert!(!ui.caret.visible());
    assert!(ui.caret.deadline().is_none());
}

#[test]
fn disabled_keep_focus_is_repaired_before_snapshot_publication() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = empty_settings_ui();
    ui.install_automation(hwnd);
    {
        let value = AutomationFocusOwner::Settings;
        ui.focus.set_owner(value);
    };
    {
        let value = Some(ElementId::KeepDisplayChange);
        ui.focus.set_target(value);
    };
    assert!(ui.is_disabled(ElementId::KeepDisplayChange));

    ui.publish_automation_snapshot(hwnd);
    let snapshot = ui.automation.as_ref().expect("automation").snapshot();
    assert_ne!(snapshot.focused, Some(ElementId::KeepDisplayChange));
    assert!(snapshot
        .nodes
        .iter()
        .all(|node| !node.focused || node.enabled));
    assert!(snapshot.focused.is_none_or(|id| snapshot
        .nodes
        .iter()
        .any(|node| node.id == id && node.enabled)));
}

#[test]
fn dependent_disable_repairs_focus_and_preserves_tab_navigation() {
    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = empty_settings_ui();
    ui.install_automation(hwnd);
    {
        let value = AutomationFocusOwner::Settings;
        ui.focus.set_owner(value);
    };
    {
        let value = Some(ElementId::WinNumberEnabled);
        ui.focus.set_target(value);
    };
    ui.draft.virtual_desktops.enabled = false;

    ui.publish_automation_snapshot(hwnd);
    let repaired = ui.focus.target().expect("repaired focus");
    assert_ne!(repaired, ElementId::WinNumberEnabled);
    assert!(!ui.is_disabled(repaired));

    let next = crate::ui::focus::next_focus_target(
        &ui.layout.focus_order(),
        Some(repaired),
        false,
        |id| ui.is_disabled(id),
    )
    .expect("enabled next target");
    assert!(!ui.is_disabled(next));
    let previous =
        crate::ui::focus::next_focus_target(&ui.layout.focus_order(), Some(next), true, |id| {
            ui.is_disabled(id)
        });
    assert_eq!(previous, Some(repaired));
}

#[test]
fn focus_repair_does_not_duplicate_notifications() {
    use std::cell::Cell;

    let hwnd = HWND(std::ptr::dangling_mut());
    let mut ui = empty_settings_ui();
    ui.install_automation(hwnd);
    {
        let value = AutomationFocusOwner::Settings;
        ui.focus.set_owner(value);
    };
    {
        let value = Some(ElementId::KeepDisplayChange);
        ui.focus.set_target(value);
    };
    ui.publish_automation_snapshot(hwnd);
    let automation = ui.automation.clone().expect("automation");

    let first = Cell::new(0);
    automation.flush_pending_events_for_test(|_| first.set(first.get() + 1));
    assert_eq!(first.get(), 2);

    ui.publish_automation_snapshot(hwnd);
    let second = Cell::new(0);
    automation.flush_pending_events_for_test(|_| second.set(second.get() + 1));
    assert_eq!(second.get(), 0);
}

#[test]
fn phase_one_hotkey_rows_are_in_focus_order_and_layout() {
    for id in [
        ElementId::CycleInputHotkey,
        ElementId::CycleOutputHotkey,
        ElementId::ForegroundVolumeUpHotkey,
        ElementId::ForegroundVolumeDownHotkey,
    ] {
        let layout = SettingsLayout::build(610.0, 720.0, 0.0);
        assert!(layout.focus_order().contains(&id));
        assert!(layout.element(id).is_some());
    }
}
