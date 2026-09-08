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
