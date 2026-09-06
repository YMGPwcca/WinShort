use super::*;
use crate::platform::visual::VisualRgb;

#[test]
fn popup_prefers_below_then_flips_above() {
    let work = PopupRect::new(0, 0, 1000, 800);
    let below = place_popup(PopupRect::new(100, 100, 300, 140), work, 240, 200);
    assert_eq!(below.top, 140);
    let above = place_popup(PopupRect::new(100, 700, 300, 740), work, 240, 200);
    assert_eq!(above.bottom, 700);
}

#[test]
fn popup_clamps_negative_and_right_edges() {
    let work = PopupRect::new(-500, -200, 500, 600);
    let rect = place_popup(PopupRect::new(450, 100, 480, 140), work, 300, 200);
    assert_eq!(rect.right, 480);
    assert!(rect.left >= work.left);
    assert!(rect.top >= work.top);
}

#[test]
fn wider_popup_preserves_control_attachment_at_right_edge() {
    let anchor = PopupRect::new(700, 100, 800, 140);
    let popup = place_popup(anchor, PopupRect::new(0, 0, 900, 800), 300, 180);
    assert_eq!(popup.right, anchor.right);
    assert_eq!(popup.top, anchor.bottom);
}

#[test]
fn picker_palette_uses_shared_dark_light_theme_colors() {
    let visual = SystemVisualPreferences::default();
    for theme in [Theme::dark(), Theme::light()] {
        let normal = picker_colors_for(false, visual, theme);
        let selected = picker_colors_for(true, visual, theme);
        assert_eq!(normal.background, theme.card);
        assert_eq!(normal.foreground, theme.text);
        assert_eq!(normal.border, theme.border_strong);
        assert_eq!(selected.background, theme.bg_subtle);
        assert_eq!(selected.foreground, theme.text);
        assert_eq!(selected.border, theme.accent);
    }
}

#[test]
fn picker_palette_pairs_high_contrast_colors() {
    let visual = SystemVisualPreferences {
        high_contrast: true,
        high_contrast_background: VisualRgb {
            r: 10,
            g: 20,
            b: 30,
        },
        high_contrast_foreground: VisualRgb {
            r: 240,
            g: 200,
            b: 160,
        },
        high_contrast_highlight: VisualRgb {
            r: 50,
            g: 100,
            b: 150,
        },
        high_contrast_highlight_foreground: VisualRgb { r: 1, g: 2, b: 3 },
        ..SystemVisualPreferences::default()
    };
    let normal = picker_colors_for(false, visual, Theme::dark());
    let selected = picker_colors_for(true, visual, Theme::dark());
    assert_eq!(normal.background, Color::rgb(10, 20, 30));
    assert_eq!(normal.foreground, Color::rgb(240, 200, 160));
    assert_eq!(selected.background, Color::rgb(50, 100, 150));
    assert_eq!(selected.foreground, Color::rgb(1, 2, 3));
}

#[test]
fn picker_item_state_prioritizes_disabled_then_selected_then_hover() {
    assert_eq!(
        picker_item_state(false, false, false),
        PickerItemState::Idle
    );
    assert_eq!(
        picker_item_state(false, true, false),
        PickerItemState::Hovered
    );
    assert_eq!(
        picker_item_state(true, true, false),
        PickerItemState::Selected
    );
    assert_eq!(
        picker_item_state(true, true, true),
        PickerItemState::Disabled
    );
}

#[test]
fn picker_hover_uses_shared_light_and_dark_hover_surfaces() {
    let visual = SystemVisualPreferences::default();
    for theme in [Theme::dark(), Theme::light()] {
        let colors = picker_colors_for_state(PickerItemState::Hovered, visual, theme);
        assert_eq!(colors.background, theme.picker_hover);
        assert_eq!(colors.foreground, theme.text);
    }
}

#[test]
fn selected_picker_item_stays_selected_when_pointer_leaves() {
    let visual = SystemVisualPreferences::default();
    let colors = picker_colors_for_state(
        picker_item_state(true, false, false),
        visual,
        Theme::light(),
    );
    assert_eq!(colors.background, Theme::light().bg_subtle);
    assert_eq!(colors.foreground, Theme::light().text);
}

#[test]
fn high_contrast_hover_uses_system_pair_and_outline() {
    let visual = SystemVisualPreferences {
        high_contrast: true,
        high_contrast_background: VisualRgb {
            r: 10,
            g: 20,
            b: 30,
        },
        high_contrast_foreground: VisualRgb {
            r: 240,
            g: 200,
            b: 160,
        },
        ..SystemVisualPreferences::default()
    };
    let colors = picker_colors_for_state(PickerItemState::Hovered, visual, Theme::dark());
    assert_eq!(colors.background, Color::rgb(10, 20, 30));
    assert_eq!(colors.foreground, Color::rgb(240, 200, 160));
    assert_eq!(colors.border, colors.foreground);
}

#[test]
fn picker_item_height_scales_with_dpi() {
    assert_eq!(picker_item_height_px(96), 32);
    assert_eq!(picker_item_height_px(144), 48);
    assert_eq!(picker_item_height_px(192), 64);
}

#[test]
fn picker_font_policy_is_dpi_scaled_and_explicit() {
    assert_eq!(picker_font_height(96), -14);
    assert_eq!(picker_font_height(144), -21);
    assert_eq!(PICKER_FONT_FAMILY, "Segoe UI Variable Text");
    assert_eq!(PICKER_FONT_FALLBACK, "Segoe UI");
}

#[test]
fn picker_host_is_a_control_center_child_with_shared_clipping() {
    assert_ne!(PICKER_HOST_STYLE.0 & WS_CHILD.0, 0);
    assert_ne!(PICKER_HOST_STYLE.0 & WS_CLIPCHILDREN.0, 0);
    let geometry = PopupRect::new(0, 0, 320, 200);
    assert_eq!(
        picker_list_rect(geometry, 96),
        PopupRect::new(4, 4, 316, 196)
    );
    assert_eq!(picker_corner_diameter_px(96), 14);
    assert_eq!(picker_corner_diameter_px(144), 21);
}

#[test]
fn device_cycle_selection_translates_config_sentinels_only_at_the_boundary() {
    assert_eq!(
        DeviceCycleSelection::from_config(None),
        DeviceCycleSelection::All
    );
    assert_eq!(
        DeviceCycleSelection::from_config(Some(&[])),
        DeviceCycleSelection::Disabled
    );

    let configured = vec!["one".to_string(), "two".to_string()];
    let selected = DeviceCycleSelection::from_config(Some(&configured));
    assert_eq!(selected.endpoints(), configured.as_slice());
    assert_eq!(selected.clone().into_config(), Some(configured));
    assert_eq!(DeviceCycleSelection::All.into_config(), None);
    assert_eq!(
        DeviceCycleSelection::Disabled.into_config(),
        Some(Vec::new())
    );
}

#[test]
fn selected_device_cycle_state_cannot_be_empty() {
    assert!(DeviceCycleSelection::selected(Vec::new()).is_err());
    assert_eq!(
        DeviceCycleSelection::selected(vec!["endpoint".into()])
            .expect("non-empty selection")
            .endpoints(),
        &["endpoint".to_string()]
    );
}

#[test]
fn picker_model_rejects_commit_from_another_domain() {
    let choices = vec![PickerChoice::commit(
        "Speaker",
        PickerCommit::OutputDevice(crate::config::model::DeviceSelection::Endpoint(
            "speaker".into(),
        )),
    )];
    assert!(PickerModel::new(PickerKind::InputDevice, choices, Some(0), Vec::new()).is_err());
}

#[test]
fn picker_model_preserves_no_current_selection() {
    let choices = vec![PickerChoice::commit(
        "Microphone",
        PickerCommit::InputDevice(crate::config::model::DeviceSelection::Endpoint(
            "microphone".into(),
        )),
    )];
    let model = PickerModel::new(PickerKind::InputDevice, choices, None, Vec::new())
        .expect("valid picker model without a current row");
    assert_eq!(model.current(), None);
}
