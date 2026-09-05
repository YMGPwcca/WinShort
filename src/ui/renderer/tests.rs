use super::{
    brush_colors, trimming_for, BrushRole, TextStyle, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
};

#[test]
fn value_text_uses_directwrite_trailing_character_trimming() {
    let trimming = trimming_for(TextStyle::Value).expect("value trimming");
    assert_eq!(trimming.granularity, DWRITE_TRIMMING_GRANULARITY_CHARACTER);
    assert_eq!(trimming.delimiter, 0);
    assert_eq!(trimming.delimiterCount, 0);
    assert!(trimming_for(TextStyle::Body).is_some());
}
#[test]
fn dark_and_light_themes_supply_a_complete_brush_palette() {
    for theme in [
        crate::ui::theme::Theme::dark(),
        crate::ui::theme::Theme::light(),
    ] {
        let colors = brush_colors(theme);
        assert_eq!(colors.len(), 20);
        assert!(colors.iter().all(|(_, color)| color.a > 0));
        assert_eq!(
            colors
                .iter()
                .filter(|(role, _)| *role == BrushRole::Background)
                .count(),
            1
        );
        assert_eq!(
            colors
                .iter()
                .filter(|(role, _)| *role == BrushRole::Shadow)
                .count(),
            1
        );
    }
}
