use super::*;

#[test]
fn overlay_layout_has_no_duplicate_windows_audio_toggle() {
    let layout = SettingsLayout::build_shell(960.0, 900.0, 0.0, Page::Overlay, "", 0, None);
    assert!(layout.element(ElementId::OverlayPosition).is_none());
    assert!(layout.element(ElementId::OverlayAppearance).is_some());
    assert!(layout.element(ElementId::OverlayMicrophone).is_some());
    assert!(layout.element(ElementId::OverlaySpeaker).is_some());
    assert!(layout.element(ElementId::OverlayCurrentAppAudio).is_some());
    assert!(layout.element(ElementId::OverlayWorkspace).is_some());
    assert!(layout.element(ElementId::OverlayDisplayProfile).is_some());
    assert!(layout.element(ElementId::OverlayBlur).is_some());

    let system = SettingsLayout::build_shell(960.0, 900.0, 0.0, Page::System, "", 0, None);
    assert!(system.element(ElementId::OverlayAppearance).is_none());
}

#[test]
fn overlay_placement_columns_are_equal_and_side_by_side() {
    for width in [960.0, 1200.0, 1920.0] {
        let layout = SettingsLayout::build_shell_with_context(
            width,
            900.0,
            0.0,
            Page::Overlay,
            "",
            super::super::LayoutContext {
                overlay_preview_aspect: (16, 9),
                ..Default::default()
            },
            None,
        );
        let region = layout
            .regions
            .iter()
            .find(|region| region.kind == RegionKind::OverlayPreview)
            .expect("overlay placement");
        let geometry = overlay_placement_geometry(
            super::super::Rect::new(
                layout.content_column.x,
                region.rect.y,
                layout.content_column.w,
                0.0,
            ),
            (16, 9),
        );
        assert_eq!(region.rect, geometry.region);
        assert!(geometry.preview.w > 0.0);
        assert_eq!(geometry.preview.w, geometry.controls.w);
        assert_eq!(geometry.preview.h, geometry.controls.h);
        assert_eq!(geometry.preview.y, geometry.controls.y);
        assert_eq!(geometry.preview.bottom(), geometry.controls.bottom());
        assert_eq!(
            geometry.preview.right() + super::super::OVERLAY_PLACEMENT_GAP,
            geometry.controls.x
        );
        assert_eq!(geometry.controls.right(), layout.content_column.right());
        let first_cell = overlay_position_grid_rect(geometry.controls, 0);
        let last_cell = overlay_position_grid_rect(geometry.controls, 8);
        assert!(first_cell.w > 0.0);
        assert!(first_cell.h > 0.0);
        assert!(last_cell.right() <= geometry.controls.right());
        assert!(last_cell.bottom() < geometry.controls.bottom());
        assert_eq!(
            layout
                .element(ElementId::OverlayPositionCell(0))
                .expect("position cell")
                .rect,
            overlay_position_grid_rect(geometry.controls, 0)
        );
        let enabled = layout
            .element(ElementId::OverlayEnabled)
            .expect("status toggle")
            .rect;
        let monitor = layout
            .element(ElementId::OverlayMonitor)
            .expect("monitor selector")
            .rect;
        assert_eq!(
            layout
                .element(ElementId::OverlayMonitor)
                .expect("monitor selector")
                .description,
            "Overlay location"
        );
        let (expected_enabled, expected_monitor) =
            overlay_status_row_rects(super::super::Rect::new(
                layout.content_column.x,
                enabled.y,
                layout.content_column.w,
                enabled.h,
            ));
        assert_eq!(enabled, expected_enabled);
        assert_eq!(monitor, expected_monitor);
        assert_eq!(enabled.w, monitor.w);
        assert_eq!(
            monitor.x,
            enabled.right() + super::super::UiTokens::CARD_COLUMN_GAP
        );
        assert_eq!(
            layout
                .sections
                .iter()
                .filter(|section| section.title == "Position")
                .count(),
            0
        );
        let canvas = geometry.preview_canvas;
        assert!(canvas.x >= geometry.preview.x);
        assert!(canvas.y >= geometry.preview.y);
        assert!(canvas.right() <= geometry.preview.right());
        assert!(canvas.bottom() <= geometry.preview.bottom());
        assert_ne!(canvas, geometry.region);
        assert!((last_cell.bottom() - canvas.bottom()).abs() < 0.01);
    }
}
#[test]
fn overlay_preview_and_position_canvas_edges_align_for_all_aspects() {
    for aspect in [(16, 9), (16, 10), (21, 9), (9, 16)] {
        let geometry =
            overlay_placement_geometry(super::super::Rect::new(0.0, 0.0, 640.0, 0.0), aspect);
        let first_cell = overlay_position_grid_rect(geometry.controls, 0);
        let last_cell = overlay_position_grid_rect(geometry.controls, 8);
        assert!((geometry.position_grid.y - geometry.preview_canvas.y).abs() < 0.01);
        assert!((geometry.position_grid.bottom() - geometry.preview_canvas.bottom()).abs() < 0.01);
        assert!((first_cell.y - geometry.preview_canvas.y).abs() < 0.01);
        assert!((last_cell.bottom() - geometry.preview_canvas.bottom()).abs() < 0.01);
        assert!(first_cell.h > 30.0);
    }
}

#[test]
fn overlay_preview_canvas_preserves_monitor_aspect_ratios() {
    let available = 884.0;
    for aspect in [(16, 9), (16, 10), (21, 9), (9, 16)] {
        let (width, height) = overlay_preview_canvas_size(available, aspect);
        let expected = aspect.0 as f32 / aspect.1 as f32;
        assert!(((width / height) - expected).abs() < 0.001);
    }
    let (wide_width, wide_height) = overlay_preview_canvas_size(available, (21, 9));
    let (portrait_width, portrait_height) = overlay_preview_canvas_size(available, (9, 16));
    assert!(wide_width > wide_height);
    assert!(portrait_height > portrait_width);
}

#[test]
fn overlay_position_grid_has_nine_accessible_cells() {
    let layout = SettingsLayout::build_shell(1200.0, 900.0, 0.0, Page::Overlay, "", 0, None);
    assert_eq!(
        layout
            .elements
            .iter()
            .filter(|element| matches!(element.id, ElementId::OverlayPositionCell(_)))
            .count(),
        9
    );
}
