//! Appearance for the control center.

use crate::platform::visual::SystemVisualPreferences;
use crate::ui::theme::{Color, Theme};

pub(super) fn settings_theme() -> Theme {
    settings_theme_for(Theme::current(), SystemVisualPreferences::query())
}

pub(super) fn settings_theme_for(theme: Theme, visual: SystemVisualPreferences) -> Theme {
    if !visual.high_contrast {
        return theme;
    }
    let background = Color::rgb(
        visual.high_contrast_background.r,
        visual.high_contrast_background.g,
        visual.high_contrast_background.b,
    );
    let foreground = Color::rgb(
        visual.high_contrast_foreground.r,
        visual.high_contrast_foreground.g,
        visual.high_contrast_foreground.b,
    );
    let highlight = Color::rgb(
        visual.high_contrast_highlight.r,
        visual.high_contrast_highlight.g,
        visual.high_contrast_highlight.b,
    );
    let highlight_text = Color::rgb(
        visual.high_contrast_highlight_foreground.r,
        visual.high_contrast_highlight_foreground.g,
        visual.high_contrast_highlight_foreground.b,
    );
    Theme {
        mode: theme.mode,
        bg: background,
        bg_subtle: background,
        card: background,
        card_hover: background,
        control_hover: background,
        picker_hover: background,
        card_pressed: background,
        border: foreground,
        border_strong: foreground,
        text: foreground,
        text_secondary: foreground,
        text_disabled: foreground,
        accent: highlight,
        accent_hover: highlight,
        accent_pressed: highlight,
        accent_text: highlight_text,
        danger: foreground,
        warning: foreground,
        success: foreground,
        focus: foreground,
        shadow: Color::rgba(0, 0, 0, 0),
    }
}
