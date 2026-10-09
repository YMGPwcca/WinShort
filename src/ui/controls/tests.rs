use super::{
    device_value_rect, interaction_state, row_control_width, scroll_from_scrollbar_pointer,
    scrollbar_hit_rect, scrollbar_thumb_rect, search_caret_rect, search_text_rect,
    section_accent_rect, section_divider_y, section_title_text_rect, shortcut_icon_geometry,
    slider_cluster_geometry, slider_track_rect, titlebar_glyph_bounds, value_control_rect,
    value_control_rect_for, value_text_rect, value_text_rect_for, ControlValue, Interaction,
    InteractionState, COMPACT_MONITOR_CONTROL_WIDTH, CONTROL_WIDTH, VALUE_TEXT_PADDING,
};
use crate::ui::layout::{Element, ElementId, ElementKind, Rect};
use std::borrow::Cow;

fn interaction() -> Interaction {
    Interaction {
        hovered: false,
        pressed: false,
        focused: false,
        disabled: false,
        hover_t: 0.0,
        state_t: 0.0,
    }
}

#[test]
fn interaction_state_has_explicit_precedence() {
    let mut value = interaction();
    assert_eq!(interaction_state(value), InteractionState::Idle);
    value.hovered = true;
    assert_eq!(interaction_state(value), InteractionState::Hovered);
    value.focused = true;
    assert_eq!(interaction_state(value), InteractionState::Focused);
    value.pressed = true;
    assert_eq!(interaction_state(value), InteractionState::Pressed);
    value.disabled = true;
    assert_eq!(interaction_state(value), InteractionState::Disabled);
}

#[test]
fn value_control_geometry_is_right_aligned_and_shared_across_kinds() {
    let row = Rect::new(24.0, 100.0, 560.0, 58.0);
    let value = value_control_rect(row, ElementKind::Value);
    let hotkey = value_control_rect(row, ElementKind::Hotkey);
    assert_eq!(value.w, CONTROL_WIDTH);
    assert_eq!(hotkey.w, CONTROL_WIDTH);
    assert_eq!(value.x, hotkey.x);
    assert_eq!(value.right(), row.right() - 18.0);
}

#[test]
fn device_value_geometry_uses_the_shared_control_column() {
    let wide = Rect::new(24.0, 100.0, 800.0, 58.0);
    let narrow = Rect::new(24.0, 100.0, 330.0, 58.0);
    assert_eq!(device_value_rect(wide).w, CONTROL_WIDTH);
    assert_eq!(device_value_rect(narrow).w, CONTROL_WIDTH);
    assert_eq!(device_value_rect(wide).right(), wide.right() - 18.0);
    assert_eq!(device_value_rect(narrow).right(), narrow.right() - 18.0);
}

#[test]
fn value_text_geometry_reserves_chevron_and_stays_positive() {
    let row = Rect::new(24.0, 100.0, 560.0, 58.0);
    let value = value_control_rect(row, ElementKind::Value);
    let text = value_text_rect(row, ElementKind::Value);
    assert!(text.w > 0.0);
    assert!(text.h > 0.0);
    assert!(text.x > value.x);
    assert!(text.right() < value.right() - 12.0);
    let hotkey = value_control_rect(row, ElementKind::Hotkey);
    let hotkey_text = value_text_rect(row, ElementKind::Hotkey);
    assert_eq!(hotkey_text.right(), hotkey.right() - VALUE_TEXT_PADDING);
}

#[test]
fn slider_geometry_expands_help_column_and_reduces_reference_track() {
    let row = Rect::new(248.0, 100.0, 680.0, 56.0);
    let geometry = slider_cluster_geometry(row);
    let reference_left = (row.inset(1.0).x + row.inset(1.0).w * 0.34)
        .clamp(row.inset(1.0).x + 150.0, row.inset(1.0).x + 230.0);
    let reference_width = geometry.value.x - 14.0 - reference_left;

    assert_eq!(geometry.label.w, 300.0);
    assert_eq!(geometry.description.w, geometry.label.w);
    assert_eq!(slider_track_rect(row), geometry.track);
    assert_eq!(geometry.track.w, 230.0);
    assert!((geometry.track.w / reference_width - 2.0 / 3.0).abs() < 0.01);
    assert_eq!(geometry.track.x - geometry.description.right(), 24.0);
    assert_eq!(geometry.value.x - geometry.track.right(), 18.0);
}

#[test]
fn compact_monitor_value_leaves_room_for_complete_helper_text() {
    let element = Element {
        id: ElementId::OverlayMonitor,
        kind: ElementKind::Value,
        rect: Rect::new(0.0, 0.0, 332.0, 58.0),
        label: "Monitor".into(),
        description: "Overlay location".into(),
        scrolls: false,
    };
    let value = ControlValue::Text(Cow::Borrowed("Cursor position"));
    let control_width = row_control_width(&element, &value);
    let helper_width = element.rect.w - 18.0 - control_width - 14.0 - super::BODY_LEFT - 14.0;
    let control = value_control_rect_for(element.rect, element.id, element.kind);
    let value_text = value_text_rect_for(element.rect, element.kind, control_width);

    assert_eq!(control_width, COMPACT_MONITOR_CONTROL_WIDTH);
    assert!(helper_width >= 100.0);
    assert_eq!(control.w, control_width);
    assert!(value_text.w >= 90.0);
    assert_eq!(element.description, "Overlay location");
}
#[test]
fn scrollbar_drag_maps_thumb_centers_to_scroll_extremes() {
    let viewport = Rect::new(200.0, 100.0, 400.0, 300.0);
    let max_scroll = 600.0;
    let start = scrollbar_thumb_rect(viewport, 0.0, max_scroll).expect("start thumb");
    let end = scrollbar_thumb_rect(viewport, max_scroll, max_scroll).expect("end thumb");
    assert!(end.y > start.y);

    let offset = start.h * 0.5;
    let start_scroll =
        scroll_from_scrollbar_pointer(viewport, start.y + offset, offset, max_scroll);
    let end_scroll =
        scroll_from_scrollbar_pointer(viewport, end.y + end.h * 0.5, offset, max_scroll);
    assert!(start_scroll.abs() < f32::EPSILON);
    assert!((end_scroll - max_scroll).abs() < f32::EPSILON);

    let hit = scrollbar_hit_rect(viewport);
    assert!(hit.contains(viewport.right() - 8.0, viewport.y + 20.0));
    assert!(!hit.contains(viewport.right() - 20.0, viewport.y + 20.0));
}
#[test]
fn search_caret_geometry_is_visible_at_text_end_and_empty_start() {
    let field = Rect::new(100.0, 4.0, 400.0, 32.0);
    let text = search_text_rect(field);
    let empty = search_caret_rect(field, 0.0);
    let typed = search_caret_rect(field, 96.0);
    assert_eq!(empty.x, text.x);
    assert!(typed.x > empty.x);
    assert!(typed.x < text.right());
    assert!(typed.y > field.y);
    assert!(typed.bottom() < field.bottom());
    assert!(typed.w > 0.0);
    assert!(typed.h > 0.0);
}
#[test]
fn shortcut_icon_geometry_has_keyboard_rows_and_spacebar() {
    let geometry = shortcut_icon_geometry(Rect::new(0.0, 0.0, 20.0, 20.0));
    assert!(geometry.outer.w > 14.0);
    assert!(geometry.outer.h > 8.0);
    assert!(geometry
        .upper_keys
        .iter()
        .all(|key| key.w > key.h && key.w >= 1.5));
    assert!(geometry.spacebar.w > geometry.upper_keys[0].w * 2.0);
    assert!(geometry.spacebar.y > geometry.upper_keys[0].y);
}
#[test]
fn titlebar_glyph_bounds_are_compact_inside_full_hit_target() {
    let button = Rect::new(900.0, 4.0, 44.0, 32.0);
    let glyph = titlebar_glyph_bounds(button);
    assert_eq!(glyph.w, 12.0);
    assert_eq!(glyph.h, 12.0);
    assert!(glyph.x > button.x);
    assert!(glyph.right() < button.right());
    assert!(glyph.y > button.y);
    assert!(glyph.bottom() < button.bottom());
}
#[test]
fn section_accent_is_centered_on_title_text_geometry() {
    let heading = Rect::new(40.0, 120.0, 620.0, 88.0);
    let title = section_title_text_rect(heading);
    let accent = section_accent_rect(heading);
    assert_eq!(accent.x, heading.x);
    assert_eq!(accent.y + accent.h * 0.5, title.y + title.h * 0.5);
    assert!(title.y >= heading.y);
    assert!(title.bottom() <= heading.bottom());
}
#[test]
fn section_divider_stays_inside_named_section_gap() {
    let heading = Rect::new(40.0, 120.0, 620.0, 88.0);
    let divider = section_divider_y(heading);
    assert!(divider > heading.y - super::UiTokens::SECTION_GAP);
    assert!(divider < section_title_text_rect(heading).y);
}
