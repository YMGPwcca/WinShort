//! Palette for the overlay.

use crate::config::model::OverlayAppearance;
use crate::platform::visual::{SystemVisualPreferences, VisualRgb};
use crate::ui::theme::{Color, Theme, ThemeMode};

const DARK_COMPOSITION_TINT_ALPHA: f32 = 148.0;

const LIGHT_COMPOSITION_TINT_ALPHA: f32 = 200.0;

#[derive(Debug, Clone, Copy)]
pub(super) struct OverlayPalette {
    pub(super) surface: Color,
    pub(super) border: Color,
    pub(super) text: Color,
    pub(super) secondary: Color,
    pub(super) opaque: bool,
    pub(super) icon: Color,
    pub(super) changed_icon: Color,
    pub(super) tone_muted: Color,
    pub(super) tone_active: Color,
    pub(super) tone_changed: Color,
    pub(super) tone_unavailable: Color,
    pub(super) unavailable_text: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BackdropMode {
    Acrylic,
    Opaque,
}

pub(super) fn backdrop_mode(
    preferences: SystemVisualPreferences,
    api_available: bool,
) -> BackdropMode {
    if preferences.high_contrast || preferences.disable_overlapped_content || !api_available {
        BackdropMode::Opaque
    } else {
        BackdropMode::Acrylic
    }
}

fn acceptance_forces_opaque() -> bool {
    std::env::var_os("WINSHORT_UI_ACCEPTANCE").is_some()
        && std::env::var_os("WINSHORT_UI_ACCEPTANCE_FORCE_OPAQUE").is_some()
}

pub(super) fn acceptance_forces_composition_failure() -> bool {
    std::env::var_os("WINSHORT_UI_ACCEPTANCE").is_some()
        && std::env::var_os("WINSHORT_UI_ACCEPTANCE_FORCE_COMPOSITION_FAILURE").is_some()
}

pub(super) fn composition_blur_enabled(
    preferences: SystemVisualPreferences,
    composition_available: bool,
) -> bool {
    !acceptance_forces_opaque()
        && backdrop_mode(preferences, composition_available) == BackdropMode::Acrylic
}

pub(super) fn resolved_theme_mode(
    appearance: OverlayAppearance,
    preferences: SystemVisualPreferences,
) -> ThemeMode {
    match appearance {
        OverlayAppearance::System => preferences.system_theme,
        OverlayAppearance::Dark => ThemeMode::Dark,
        OverlayAppearance::Light => ThemeMode::Light,
    }
}

pub(super) fn composition_tint_alpha(theme_mode: ThemeMode, opacity: f32) -> u8 {
    let base = match theme_mode {
        ThemeMode::Dark => DARK_COMPOSITION_TINT_ALPHA,
        ThemeMode::Light => LIGHT_COMPOSITION_TINT_ALPHA,
    };
    (base * opacity.clamp(0.3, 1.0)).round() as u8
}

pub(super) fn palette_for(
    appearance: OverlayAppearance,
    preferences: SystemVisualPreferences,
) -> OverlayPalette {
    let simple = preferences.high_contrast || preferences.disable_overlapped_content;
    if preferences.high_contrast {
        let background = color_from_visual(preferences.high_contrast_background);
        let foreground = color_from_visual(preferences.high_contrast_foreground);
        let highlight = color_from_visual(preferences.high_contrast_highlight);
        let highlight_foreground =
            color_from_visual(preferences.high_contrast_highlight_foreground);
        return OverlayPalette {
            surface: background,
            border: foreground,
            text: foreground,
            secondary: foreground,
            opaque: true,
            icon: foreground,
            changed_icon: highlight_foreground,
            tone_muted: background,
            tone_active: background,
            tone_changed: highlight,
            tone_unavailable: background,
            unavailable_text: foreground,
        };
    }
    let theme = match resolved_theme_mode(appearance, preferences) {
        ThemeMode::Dark => Theme::dark(),
        ThemeMode::Light => Theme::light(),
    };
    let surface = Color::rgba(
        theme.card.r,
        theme.card.g,
        theme.card.b,
        if simple { 255 } else { 248 },
    );
    OverlayPalette {
        surface,
        border: theme.border_strong,
        text: theme.text,
        secondary: if theme.mode == ThemeMode::Dark {
            Color::rgb(230, 236, 240)
        } else {
            theme.text_secondary
        },
        opaque: simple,
        icon: surface,
        changed_icon: surface,
        tone_muted: theme.danger,
        tone_active: theme.success,
        tone_changed: theme.accent,
        tone_unavailable: theme.text_disabled,
        unavailable_text: theme.text_disabled,
    }
}

pub(super) fn opaque_palette(mut palette: OverlayPalette) -> OverlayPalette {
    palette.surface = Color::rgb(palette.surface.r, palette.surface.g, palette.surface.b);
    palette.opaque = true;
    palette
}

fn color_from_visual(value: VisualRgb) -> Color {
    Color::rgb(value.r, value.g, value.b)
}
