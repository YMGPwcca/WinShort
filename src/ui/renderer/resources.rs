//! Resources for the renderer.

use super::target::Renderer;
use crate::error::{Error, Result};
use crate::ui::theme::{Color, Theme};

use windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BrushRole {
    Background,
    BackgroundSubtle,
    Card,
    CardHover,
    ControlHover,
    CardPressed,
    Border,
    BorderStrong,
    Text,
    TextSecondary,
    TextDisabled,
    Accent,
    AccentHover,
    AccentPressed,
    AccentText,
    Danger,
    Warning,
    Success,
    Focus,
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TextStyle {
    Title,
    Subtitle,
    Section,
    SectionDescription,
    Body,
    BodyStrong,
    Caption,
    CaptionRight,
    Button,
    ButtonSmall,
    Value,
}

#[cfg(test)]
pub(super) fn brush_colors(theme: Theme) -> [(BrushRole, Color); 20] {
    [
        (BrushRole::Background, theme.bg),
        (BrushRole::BackgroundSubtle, theme.bg_subtle),
        (BrushRole::Card, theme.card),
        (BrushRole::CardHover, theme.card_hover),
        (BrushRole::ControlHover, theme.control_hover),
        (BrushRole::CardPressed, theme.card_pressed),
        (BrushRole::Border, theme.border),
        (BrushRole::BorderStrong, theme.border_strong),
        (BrushRole::Text, theme.text),
        (BrushRole::TextSecondary, theme.text_secondary),
        (BrushRole::TextDisabled, theme.text_disabled),
        (BrushRole::Accent, theme.accent),
        (BrushRole::AccentHover, theme.accent_hover),
        (BrushRole::AccentPressed, theme.accent_pressed),
        (BrushRole::AccentText, theme.accent_text),
        (BrushRole::Danger, theme.danger),
        (BrushRole::Warning, theme.warning),
        (BrushRole::Success, theme.success),
        (BrushRole::Focus, theme.focus),
        (BrushRole::Shadow, theme.shadow),
    ]
}

/// Every declared brush exists after construction, including after a theme change.
pub(super) struct BrushSet {
    background: ID2D1SolidColorBrush,
    background_subtle: ID2D1SolidColorBrush,
    card: ID2D1SolidColorBrush,
    card_hover: ID2D1SolidColorBrush,
    control_hover: ID2D1SolidColorBrush,
    card_pressed: ID2D1SolidColorBrush,
    border: ID2D1SolidColorBrush,
    border_strong: ID2D1SolidColorBrush,
    text: ID2D1SolidColorBrush,
    text_secondary: ID2D1SolidColorBrush,
    text_disabled: ID2D1SolidColorBrush,
    accent: ID2D1SolidColorBrush,
    accent_hover: ID2D1SolidColorBrush,
    accent_pressed: ID2D1SolidColorBrush,
    accent_text: ID2D1SolidColorBrush,
    danger: ID2D1SolidColorBrush,
    warning: ID2D1SolidColorBrush,
    success: ID2D1SolidColorBrush,
    focus: ID2D1SolidColorBrush,
    shadow: ID2D1SolidColorBrush,
}

impl BrushSet {
    pub(super) fn create(
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        theme: Theme,
    ) -> Result<Self> {
        let create = |color: Color| unsafe {
            target
                .CreateSolidColorBrush(&color.d2d(), None)
                .map_err(|error| Error::win("CreateSolidColorBrush", &error))
        };
        Ok(Self {
            background: create(theme.bg)?,
            background_subtle: create(theme.bg_subtle)?,
            card: create(theme.card)?,
            card_hover: create(theme.card_hover)?,
            control_hover: create(theme.control_hover)?,
            card_pressed: create(theme.card_pressed)?,
            border: create(theme.border)?,
            border_strong: create(theme.border_strong)?,
            text: create(theme.text)?,
            text_secondary: create(theme.text_secondary)?,
            text_disabled: create(theme.text_disabled)?,
            accent: create(theme.accent)?,
            accent_hover: create(theme.accent_hover)?,
            accent_pressed: create(theme.accent_pressed)?,
            accent_text: create(theme.accent_text)?,
            danger: create(theme.danger)?,
            warning: create(theme.warning)?,
            success: create(theme.success)?,
            focus: create(theme.focus)?,
            shadow: create(theme.shadow)?,
        })
    }

    fn get(&self, role: BrushRole) -> &ID2D1SolidColorBrush {
        match role {
            BrushRole::Background => &self.background,
            BrushRole::BackgroundSubtle => &self.background_subtle,
            BrushRole::Card => &self.card,
            BrushRole::CardHover => &self.card_hover,
            BrushRole::ControlHover => &self.control_hover,
            BrushRole::CardPressed => &self.card_pressed,
            BrushRole::Border => &self.border,
            BrushRole::BorderStrong => &self.border_strong,
            BrushRole::Text => &self.text,
            BrushRole::TextSecondary => &self.text_secondary,
            BrushRole::TextDisabled => &self.text_disabled,
            BrushRole::Accent => &self.accent,
            BrushRole::AccentHover => &self.accent_hover,
            BrushRole::AccentPressed => &self.accent_pressed,
            BrushRole::AccentText => &self.accent_text,
            BrushRole::Danger => &self.danger,
            BrushRole::Warning => &self.warning,
            BrushRole::Success => &self.success,
            BrushRole::Focus => &self.focus,
            BrushRole::Shadow => &self.shadow,
        }
    }
}

impl Renderer {
    pub(crate) fn brush(&self, role: BrushRole) -> &ID2D1SolidColorBrush {
        self.brushes.get(role)
    }
}
