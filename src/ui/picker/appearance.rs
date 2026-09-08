//! Appearance for the picker.

use crate::platform::visual::SystemVisualPreferences;
use crate::ui::theme::{Color, Theme};

use windows::Win32::Foundation::COLORREF;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PickerColors {
    pub(super) background: Color,
    pub(super) foreground: Color,
    pub(super) border: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PickerItemState {
    Idle,
    Hovered,
    Selected,
    Disabled,
}

pub(super) fn picker_item_state(selected: bool, hovered: bool, disabled: bool) -> PickerItemState {
    if disabled {
        PickerItemState::Disabled
    } else if selected {
        PickerItemState::Selected
    } else if hovered {
        PickerItemState::Hovered
    } else {
        PickerItemState::Idle
    }
}

pub(super) fn picker_colors(selected: bool) -> PickerColors {
    picker_colors_for(selected, SystemVisualPreferences::query(), Theme::current())
}

pub(super) fn picker_colors_for(
    selected: bool,
    visual: SystemVisualPreferences,
    theme: Theme,
) -> PickerColors {
    picker_colors_for_state(picker_item_state(selected, false, false), visual, theme)
}

pub(super) fn picker_colors_for_state(
    state: PickerItemState,
    visual: SystemVisualPreferences,
    theme: Theme,
) -> PickerColors {
    if visual.high_contrast {
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
        let highlight_foreground = Color::rgb(
            visual.high_contrast_highlight_foreground.r,
            visual.high_contrast_highlight_foreground.g,
            visual.high_contrast_highlight_foreground.b,
        );
        return match state {
            PickerItemState::Selected => PickerColors {
                background: highlight,
                foreground: highlight_foreground,
                border: foreground,
            },
            PickerItemState::Idle | PickerItemState::Hovered | PickerItemState::Disabled => {
                PickerColors {
                    background,
                    foreground,
                    border: foreground,
                }
            }
        };
    }
    match state {
        PickerItemState::Idle => PickerColors {
            background: theme.card,
            foreground: theme.text,
            border: theme.border_strong,
        },
        PickerItemState::Hovered => PickerColors {
            background: theme.picker_hover,
            foreground: theme.text,
            border: theme.border_strong,
        },
        PickerItemState::Selected => PickerColors {
            background: theme.bg_subtle,
            foreground: theme.text,
            border: theme.accent,
        },
        PickerItemState::Disabled => PickerColors {
            background: theme.card_pressed,
            foreground: theme.text_disabled,
            border: theme.border,
        },
    }
}

pub(super) fn to_colorref(color: Color) -> COLORREF {
    COLORREF(color.r as u32 | ((color.g as u32) << 8) | ((color.b as u32) << 16))
}
