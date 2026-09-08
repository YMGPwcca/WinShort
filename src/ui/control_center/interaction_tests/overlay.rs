use super::*;

#[test]
fn compact_overlay_preview_keeps_sample_inside_canvas() {
    let canvas = UiRect::new(0.0, 0.0, 444.0, 108.0);
    for position in OverlayPosition::ALL {
        for scale in [0.7, 1.0, 1.6] {
            let sample = overlay_preview_card_rect(canvas, position, scale);
            assert!(sample.x >= canvas.x);
            assert!(sample.y >= canvas.y);
            assert!(sample.right() <= canvas.right());
            assert!(sample.bottom() <= canvas.bottom());
        }
    }
}
#[test]
fn overlay_size_label_reports_the_runtime_scale() {
    for (scale, label) in [
        (0.7, "0.7×"),
        (0.8, "0.8×"),
        (0.9, "0.9×"),
        (1.0, "1×"),
        (1.1, "1.1×"),
        (1.6, "1.6×"),
    ] {
        assert_eq!(overlay_scale_label(scale), label);
    }
}

#[test]
fn overlay_preview_treatments_are_distinct_and_transparent_has_no_surface() {
    use crate::config::model::{OverlayAppearance, OverlayBlur};
    use crate::ui::renderer::BrushRole;

    assert_eq!(
        preview_treatment(OverlayAppearance::System, OverlayBlur::Transparent),
        None
    );
    assert_eq!(
        preview_treatment(OverlayAppearance::System, OverlayBlur::BlurLight),
        Some((BrushRole::Background, 0.25))
    );
    assert_eq!(
        preview_treatment(OverlayAppearance::System, OverlayBlur::BlurMedium),
        Some((BrushRole::Background, 0.50))
    );
    assert_eq!(
        preview_treatment(OverlayAppearance::System, OverlayBlur::BlurHeavy),
        Some((BrushRole::Background, 0.75))
    );
    assert_eq!(
        preview_treatment(OverlayAppearance::System, OverlayBlur::Solid),
        Some((BrushRole::CardPressed, 1.0))
    );
    assert_eq!(
        preview_treatment(OverlayAppearance::Dark, OverlayBlur::BlurMedium),
        Some((BrushRole::CardPressed, 0.50))
    );
    assert_eq!(
        preview_treatment(OverlayAppearance::Light, OverlayBlur::BlurMedium),
        Some((BrushRole::BackgroundSubtle, 0.50))
    );
}
