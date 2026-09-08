use super::*;

#[test]
fn shell_has_primary_navigation_and_search() {
    let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
    assert!(layout.element(ElementId::Search).is_some());
    assert!(layout.element(ElementId::Nav(Page::Audio)).is_some());
    assert!(layout.element(ElementId::HomeSpeaker).is_some());
}

#[test]
fn compact_shell_keeps_search_clear_of_window_edge() {
    let layout = SettingsLayout::build_shell(760.0, 660.0, 0.0, Page::System, "", 0, None);
    let right_gap = layout.top_bar.right() - layout.search_rect.right();
    assert!(right_gap >= 64.0);
}

#[test]
fn content_hit_testing_excludes_scrolled_elements_outside_viewport() {
    let layout = SettingsLayout::build_shell(960.0, 660.0, 500.0, Page::Shortcuts, "", 0, None);
    assert!(layout
        .elements
        .iter()
        .filter(|element| element.scrolls)
        .all(|element| { !element.rect.contains(0.0, 0.0) }));
}

#[test]
fn scrolled_content_hit_testing_stops_at_top_bar_boundary() {
    let layout = SettingsLayout::build_shell(960.0, 660.0, 500.0, Page::Audio, "", 0, None);
    assert_eq!(
        layout.content_clip.y,
        layout.top_bar.bottom() + super::super::UiTokens::VIEWPORT_TOP_INSET
    );
    assert_eq!(
        layout.hit_test(layout.content_column.x + 8.0, layout.content_clip.y - 1.0),
        None
    );
}

#[test]
fn fixed_top_chrome_has_one_close_and_aligned_search() {
    let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
    let chrome = top_chrome_geometry(layout.width, layout.nav_width);
    let close = layout
        .element(ElementId::WindowClose)
        .expect("close button");
    assert_eq!(layout.top_bar, chrome.row);
    assert_eq!(layout.search_rect, chrome.search);
    assert_eq!(close.rect, chrome.close);
    assert_eq!(layout.search_rect.y, close.rect.y);
    assert_eq!(layout.search_rect.h, close.rect.h);
    assert!(chrome.caption.w > 0.0);
    assert!(layout.search_rect.right() < close.rect.x);
    assert_eq!(
        layout
            .elements
            .iter()
            .filter(|element| { !element.scrolls && element.kind == ElementKind::ButtonSecondary })
            .map(|element| element.id)
            .collect::<Vec<_>>(),
        vec![ElementId::WindowClose]
    );
    assert_eq!(
        layout.content_clip.y,
        chrome.row.bottom() + super::super::UiTokens::VIEWPORT_TOP_INSET
    );
}
