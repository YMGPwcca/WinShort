use super::*;

#[test]
fn changing_pages_retains_dirty_display_draft_and_resume_action() {
    let mut ui = search_settings_ui();
    ui.draft.display_profiles.enabled = true;
    ui.draft.display_profiles.profiles = vec![sample_profile("draft", "Unsaved setup", false)];
    ui.draft.display_profiles.active_profile = Some("draft".into());
    ui.display.open(
        crate::ui::presentation::DisplayWizardStep::NameAndShortcut,
        true,
    );
    let draft = ui.draft.clone();
    let hwnd = HWND::default();
    ui.set_page(Page::Overlay);
    ui.rebuild_layout(hwnd);
    assert_eq!(ui.draft, draft);
    assert!(ui.display.is_dirty());
    assert!(ui.layout.element(ElementId::ResumeDisplayDraft).is_some());
    ui.activate(hwnd, ElementId::ResumeDisplayDraft);
    assert_eq!(ui.page, Page::Displays);
    assert_eq!(ui.draft, draft);
    assert_eq!(
        ui.display.step(),
        Some(crate::ui::presentation::DisplayWizardStep::NameAndShortcut)
    );
}

#[test]
fn search_click_and_enter_reveal_the_same_control_with_visible_focus() {
    for keyboard in [false, true] {
        let mut ui = search_settings_ui();
        ui.focus.set_target(Some(ElementId::Search));
        ui.search_query = "opacity hover".into();
        if keyboard {
            assert!(ui.handle_search_key(HWND::default(), 0x0D));
        } else {
            ui.activate_search_result(HWND::default(), 0);
        }
        assert_eq!(ui.page, Page::Overlay);
        assert_eq!(ui.focus.target(), Some(ElementId::OverlayHoverOpacity));
        assert!(ui.visual_focus(ElementId::OverlayHoverOpacity));
        let rect = ui
            .layout
            .element(ElementId::OverlayHoverOpacity)
            .unwrap()
            .rect;
        assert!(rect.y >= ui.layout.content_clip.y);
        assert!(rect.bottom() <= ui.layout.content_clip.bottom());
    }
}

#[test]
fn searching_disabled_overlay_reveals_its_master_switch_without_enabling_it() {
    let mut ui = search_settings_ui();
    ui.draft.overlay.enabled = false;
    ui.search_query = "opacity hover".into();
    ui.activate_search_result(HWND::default(), 0);
    assert!(!ui.draft.overlay.enabled);
    assert_eq!(ui.focus.target(), Some(ElementId::OverlayEnabled));
}

#[test]
fn wheel_scroll_policy_accumulates_native_sized_deltas() {
    assert_eq!(scroll_after_wheel(128.0, 120.0, 512.0), 48.0);
    assert_eq!(scroll_after_wheel(48.0, 120.0, 512.0), 0.0);
    assert_eq!(scroll_after_wheel(128.0, 240.0, 512.0), 0.0);
    assert_eq!(scroll_after_wheel(0.0, 120.0, 512.0), 0.0);
    assert_eq!(scroll_after_wheel(512.0, -120.0, 512.0), 512.0);
}

#[test]
fn wheel_and_page_scroll_are_immediate_and_accumulate_without_tween() {
    let mut scroll = 128.0;
    scroll = scroll_after_wheel(scroll, 120.0, 512.0);
    assert_eq!(scroll, 48.0);
    scroll = scroll_after_wheel(scroll, 240.0, 512.0);
    assert_eq!(scroll, 0.0);
    assert_eq!(page_scroll_target(128.0, 300.0, 512.0, true), 428.0);
    assert_eq!(page_scroll_target(128.0, 300.0, 512.0, false), 0.0);
    let mut motion = Motion::default();
    assert!(!motion.tick());
}
