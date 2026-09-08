use super::*;

#[test]
fn brand_row_centers_icon_and_text_from_shared_row_geometry() {
    let brand = brand_row_geometry(216.0);
    let row_center = brand.row.y + brand.row.h * 0.5;
    assert_eq!(brand.icon.y + brand.icon.h * 0.5, row_center);
    assert_eq!(brand.text.y + brand.text.h * 0.5, row_center);
    assert_eq!(
        brand.icon.y + brand.icon.h * 0.5,
        brand.text.y + brand.text.h * 0.5
    );
    assert_eq!(
        brand.text.x,
        brand.icon.right() + super::super::UiTokens::BRAND_TEXT_GAP
    );
    assert!(brand.row.h > super::super::UiTokens::TOP_BAR_HEIGHT);
    let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
    assert_eq!(
        layout
            .element(ElementId::Nav(Page::Home))
            .expect("home navigation")
            .rect
            .y,
        super::super::UiTokens::NAV_FIRST_ITEM_TOP
    );
    let home_nav = layout
        .element(ElementId::Nav(Page::Home))
        .expect("home navigation");
    assert_eq!(
        home_nav.rect.y - layout.brand.row.bottom(),
        super::super::UiTokens::ROW_GAP
    );
}

#[test]
fn top_chrome_separator_starts_at_content_boundary() {
    let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
    let separator = top_chrome_separator_rect(layout.top_bar);

    assert_eq!(separator.x, layout.nav_width);
    assert_eq!(separator.x, layout.brand.row.right());
    assert_eq!(separator.y, layout.top_bar.bottom());
    assert!(!separator.intersects(layout.brand.row));
    assert!(!separator.intersects(layout.brand.icon));
    assert!(!separator.intersects(layout.brand.text));
}

#[test]
fn home_cards_use_dashboard_grid_then_stack_when_narrow() {
    let wide = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
    let wide_speaker = wide
        .element(ElementId::HomeSpeaker)
        .expect("wide speaker card");
    let wide_microphone = wide
        .element(ElementId::HomeMicrophone)
        .expect("wide microphone card");
    assert_eq!(wide_speaker.rect.y, wide_microphone.rect.y);
    assert!(wide_microphone.rect.x > wide_speaker.rect.x);

    let narrow = SettingsLayout::build_shell(760.0, 660.0, 0.0, Page::Home, "", 0, None);
    let narrow_speaker = narrow
        .element(ElementId::HomeSpeaker)
        .expect("narrow speaker card");
    let narrow_microphone = narrow
        .element(ElementId::HomeMicrophone)
        .expect("narrow microphone card");
    assert!(narrow_microphone.rect.y > narrow_speaker.rect.y);

    let previous = wide
        .element(ElementId::HomePreviousDesktop)
        .expect("previous desktop action");
    assert_eq!(previous.rect.w, wide.content_column.w);
    assert_eq!(previous.rect.h, super::super::UiTokens::ROW_HEIGHT);
    let status = wide
        .element(ElementId::HomeDiagnostics)
        .expect("home status surface");
    assert_eq!(status.rect.w, wide.content_column.w);
    assert_eq!(status.rect.h, super::super::UiTokens::CARD_HEIGHT);
}

#[test]
fn ordinary_content_has_a_page_aware_maximum_width() {
    let layout = SettingsLayout::build_shell(1920.0, 1080.0, 0.0, Page::System, "", 0, None);
    assert!(layout.content_column.w <= 860.0);
    assert!(layout.content_column.right() < layout.content_clip.right());
    let displays = SettingsLayout::build_shell(1920.0, 1080.0, 0.0, Page::Displays, "", 3, None);
    assert!(displays.content_column.w > layout.content_column.w);
}

#[test]
fn partially_visible_content_intersects_the_viewport() {
    let viewport = super::super::Rect::new(0.0, 100.0, 400.0, 300.0);

    assert!(viewport.intersects(super::super::Rect::new(20.0, 80.0, 120.0, 40.0)));
    assert!(viewport.intersects(super::super::Rect::new(20.0, 390.0, 120.0, 40.0)));
    assert!(!viewport.intersects(super::super::Rect::new(20.0, 20.0, 120.0, 40.0)));
    assert!(!viewport.intersects(super::super::Rect::new(20.0, 400.0, 120.0, 40.0)));
}

#[test]
fn every_page_has_a_heading_and_reachable_navigation() {
    let pages = Page::PRIMARY.into_iter().chain(Page::SECONDARY);
    for page in pages {
        let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, page, "", 0, None);
        let expected = if page == Page::Home {
            "Welcome back"
        } else {
            page.label()
        };
        assert_eq!(
            layout
                .sections
                .first()
                .map(|section| section.title.as_str()),
            Some(expected)
        );
        assert!(layout.element(ElementId::Nav(page)).is_some());
        assert!(!layout.focus_order().is_empty());
    }
}

#[test]
fn workspaces_settings_has_no_desktop_switcher_surface() {
    let layout = SettingsLayout::build_shell_with_context(
        1200.0,
        900.0,
        0.0,
        Page::Workspaces,
        "",
        super::super::LayoutContext::default(),
        None,
    );
    assert!(layout.regions.is_empty());
    assert!(layout.element(ElementId::WinNumberEnabled).is_some());
    assert!(layout.element(ElementId::DesktopNumberModifier).is_some());
    assert!(layout.element(ElementId::PreviousDesktopHotkey).is_some());
    assert!(layout
        .sections
        .iter()
        .all(|section| section.title != "Normal desktops"));
}

#[test]
fn sibling_cards_and_rows_use_named_row_gap() {
    let audio = SettingsLayout::build_shell_with_context(
        960.0,
        900.0,
        0.0,
        Page::Audio,
        "",
        super::super::LayoutContext {
            current_app_audio_available: true,
            ..Default::default()
        },
        None,
    );
    let speakers = audio
        .sections
        .iter()
        .find(|section| section.title == "Speakers")
        .expect("speakers heading");
    let output = audio.element(ElementId::OutputDevice).expect("output row");
    assert_eq!(
        output.rect.y - (speakers.y + speakers.height),
        super::super::UiTokens::SECTION_CONTENT_GAP
    );
    let status = audio
        .regions
        .iter()
        .find(|region| region.kind == RegionKind::AudioCurrentApp)
        .expect("current app status");
    let mute = audio
        .element(ElementId::HotkeyCard(HotkeySlot::Foreground))
        .expect("mute current app card");
    let volume_up = audio
        .element(ElementId::HotkeyCard(HotkeySlot::ForegroundVolumeUp))
        .expect("current app volume card");
    assert_eq!(
        mute.rect.y - status.rect.bottom(),
        super::super::UiTokens::ROW_GAP
    );
    assert_eq!(
        volume_up.rect.y - mute.rect.bottom(),
        super::super::UiTokens::ROW_GAP
    );

    let home = SettingsLayout::build_shell(960.0, 900.0, 0.0, Page::Home, "", 0, None);
    let desktop = home
        .element(ElementId::HomeCurrentDesktop)
        .expect("current desktop card");
    let special = home.element(ElementId::HomeSpecial).expect("special card");
    let previous = home
        .element(ElementId::HomePreviousDesktop)
        .expect("previous desktop row");
    assert_eq!(desktop.rect.y, special.rect.y);
    assert_eq!(
        previous.rect.y - desktop.rect.bottom(),
        super::super::UiTokens::ROW_GAP
    );
}

#[test]
fn editor_header_is_the_only_page_header() {
    for step in DisplayWizardStep::ALL {
        let layout = SettingsLayout::build_shell_with_context(
            1200.0,
            900.0,
            0.0,
            Page::Displays,
            "",
            super::super::LayoutContext {
                display_editor_step: Some(step),
                display_output_count: 2,
                display_route_count: 2,
                ..Default::default()
            },
            None,
        );
        assert_eq!(
            layout
                .sections
                .first()
                .map(|section| section.title.as_str()),
            Some("Display profile editor")
        );
        assert!(layout
            .sections
            .iter()
            .skip(1)
            .all(|section| !section.page_header));
    }
}

#[test]
fn review_summary_is_accessible_without_joining_keyboard_order() {
    let layout = SettingsLayout::build_shell_with_context(
        1200.0,
        900.0,
        0.0,
        Page::Displays,
        "",
        super::super::LayoutContext {
            display_editor_step: Some(DisplayWizardStep::Review),
            display_route_count: 2,
            display_draft_dirty: true,
            ..Default::default()
        },
        None,
    );
    assert_eq!(
        layout
            .element(ElementId::DisplayWizardSummary)
            .map(|element| element.kind),
        Some(ElementKind::Info)
    );
    assert!(!layout
        .focus_order()
        .contains(&ElementId::DisplayWizardSummary));
}

#[test]
fn headers_reserve_separate_title_and_description_geometry_at_each_scale() {
    for width in [960.0, 1200.0, 1920.0] {
        for height in [660.0, 900.0, 1200.0] {
            let layout = SettingsLayout::build_shell(width, height, 0.0, Page::Audio, "", 0, None);
            for section in &layout.sections {
                assert!(section.height >= if section.page_header { 84.0 } else { 64.0 });
            }
        }
    }
}
