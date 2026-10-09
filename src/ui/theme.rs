//! WinShort visual system: restrained Fluent 2 surfaces for an operator UI.
//! Colors are authored in sRGB and converted to Direct2D floats at render time.

use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn d2d(self) -> D2D1_COLOR_F {
        D2D1_COLOR_F {
            r: self.r as f32 / 255.0,
            g: self.g as f32 / 255.0,
            b: self.b as f32 / 255.0,
            a: self.a as f32 / 255.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}
/// Shared logical design tokens. All values are DIPs at the 96-DPI design
/// scale; Win32 rendering scales them through the current target DPI.
pub struct UiTokens;

impl UiTokens {
    pub const NAV_WIDTH: f32 = 216.0;
    pub const TOP_BAR_HEIGHT: f32 = 40.0;
    pub const FOOTER_HEIGHT: f32 = 34.0;
    pub const PAGE_MARGIN: f32 = 32.0;
    pub const VIEWPORT_TOP_INSET: f32 = 8.0;
    pub const TITLEBAR_BUTTON_WIDTH: f32 = 44.0;
    pub const TITLEBAR_BUTTON_HEIGHT: f32 = 32.0;
    pub const TITLEBAR_BUTTON_TOP: f32 = 4.0;
    pub const TITLEBAR_BUTTON_RIGHT: f32 = 8.0;
    pub const BRAND_ROW_HEIGHT: f32 = 80.0;
    pub const BRAND_ROW_LEFT: f32 = 24.0;
    pub const BRAND_ICON_SIZE: f32 = 34.0;
    pub const BRAND_TEXT_GAP: f32 = 12.0;
    pub const BRAND_TEXT_HEIGHT: f32 = 45.0;
    pub const BRAND_ROW_RIGHT: f32 = 16.0;
    pub const NAV_FIRST_ITEM_TOP: f32 = Self::BRAND_ROW_HEIGHT + Self::ROW_GAP;
    pub const TOP_CHROME_SEARCH_GAP: f32 = 24.0;
    pub const ROW_HEIGHT: f32 = 58.0;
    pub const ROW_GAP: f32 = 8.0;
    pub const SECTION_CONTENT_GAP: f32 = 12.0;
    pub const SECTION_GAP: f32 = 20.0;
    pub const PAGE_HEADER_HEIGHT: f32 = 80.0;
    pub const SECTION_HEADER_HEIGHT: f32 = 68.0;
    pub const CARD_RADIUS: f32 = 12.0;
    pub const CONTROL_RADIUS: f32 = 7.0;
    pub const NAV_RADIUS: f32 = 8.0;
    pub const PICKER_INSET: f32 = 4.0;
    pub const PICKER_RADIUS: f32 = Self::CONTROL_RADIUS;
    pub const CONTROL_WIDTH: f32 = 206.0;
    pub const CARD_HEIGHT: f32 = 104.0;
    pub const CARD_COLUMN_GAP: f32 = 16.0;
    pub const PROFILE_CARD_HEIGHT: f32 = 120.0;
    pub const PROFILE_ROW_STEP: f32 = Self::PROFILE_CARD_HEIGHT + Self::ROW_GAP;

    pub const fn content_max_width(page: crate::ui::navigation::Page) -> f32 {
        match page {
            crate::ui::navigation::Page::Home => 1_260.0,
            crate::ui::navigation::Page::Displays => 1_220.0,
            crate::ui::navigation::Page::Shortcuts => 1_080.0,
            crate::ui::navigation::Page::Audio => 1_000.0,
            crate::ui::navigation::Page::Workspaces => 1_000.0,
            crate::ui::navigation::Page::Overlay => 980.0,
            crate::ui::navigation::Page::System | crate::ui::navigation::Page::Advanced => 860.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,
    pub bg: Color,
    pub bg_subtle: Color,
    pub card: Color,
    pub card_hover: Color,
    pub control_hover: Color,
    pub picker_hover: Color,
    pub card_pressed: Color,
    pub border: Color,
    pub border_strong: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub text_disabled: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_pressed: Color,
    pub accent_text: Color,
    pub danger: Color,
    pub warning: Color,
    pub success: Color,
    pub focus: Color,
    pub shadow: Color,
}

impl Theme {
    pub fn current() -> Self {
        match acceptance_theme_mode().unwrap_or_else(system_theme_mode) {
            ThemeMode::Light => Self::light(),
            ThemeMode::Dark => Self::dark(),
        }
    }

    pub const fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            bg: Color::rgb(31, 31, 31),
            bg_subtle: Color::rgb(36, 36, 36),
            card: Color::rgb(43, 43, 43),
            card_hover: Color::rgb(49, 49, 49),
            control_hover: Color::rgb(59, 59, 59),
            picker_hover: Color::rgb(64, 64, 64),
            card_pressed: Color::rgb(38, 38, 38),
            border: Color::rgb(57, 57, 57),
            border_strong: Color::rgb(72, 72, 72),
            text: Color::rgb(255, 255, 255),
            text_secondary: Color::rgb(191, 191, 191),
            text_disabled: Color::rgb(166, 166, 166),
            accent: Color::rgb(96, 205, 255),
            accent_hover: Color::rgb(153, 224, 255),
            accent_pressed: Color::rgb(0, 120, 212),
            accent_text: Color::rgb(0, 29, 43),
            danger: Color::rgb(229, 121, 94),
            warning: Color::rgb(246, 190, 70),
            success: Color::rgb(108, 204, 181),
            focus: Color::rgb(96, 205, 255),
            shadow: Color::rgba(0, 0, 0, 78),
        }
    }

    pub const fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            bg: Color::rgb(243, 243, 243),
            bg_subtle: Color::rgb(238, 238, 238),
            card: Color::rgb(255, 255, 255),
            card_hover: Color::rgb(249, 249, 249),
            control_hover: Color::rgb(244, 244, 244),
            picker_hover: Color::rgb(238, 238, 238),
            card_pressed: Color::rgb(238, 238, 238),
            border: Color::rgb(229, 229, 229),
            border_strong: Color::rgb(204, 204, 204),
            text: Color::rgb(27, 27, 27),
            text_secondary: Color::rgb(92, 92, 92),
            text_disabled: Color::rgb(108, 108, 108),
            accent: Color::rgb(0, 103, 192),
            accent_hover: Color::rgb(0, 87, 163),
            accent_pressed: Color::rgb(0, 75, 141),
            accent_text: Color::rgb(255, 255, 255),
            danger: Color::rgb(196, 43, 28),
            warning: Color::rgb(143, 89, 2),
            success: Color::rgb(15, 110, 80),
            focus: Color::rgb(0, 103, 192),
            shadow: Color::rgba(0, 0, 0, 36),
        }
    }
}

/// Process-local theme override used only by the isolated Windows acceptance
/// harness; ordinary launches continue to read the Windows Personalize value.
fn acceptance_theme_mode() -> Option<ThemeMode> {
    let value = std::env::var_os("WINSHORT_UI_ACCEPTANCE_THEME")?;
    match value.to_string_lossy().as_ref() {
        "light" => Some(ThemeMode::Light),
        "dark" => Some(ThemeMode::Dark),
        _ => None,
    }
}
/// System app theme from the documented Personalize registry value. Missing
/// value defaults to dark because WinShort's launch scene is the tray at night.
pub fn system_theme_mode() -> ThemeMode {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, REG_VALUE_TYPE, RRF_RT_REG_DWORD,
    };

    unsafe {
        let key =
            HSTRING::from("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let name = HSTRING::from("AppsUseLightTheme");
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let mut kind = REG_VALUE_TYPE(0);
        let result = RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            Some(&mut kind),
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        );
        if result == ERROR_SUCCESS && value != 0 {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        }
    }
}
#[cfg(test)]
mod tests {
    use super::Theme;

    #[test]
    fn control_and_picker_hover_surfaces_are_distinct_in_both_themes() {
        for theme in [Theme::dark(), Theme::light()] {
            assert_ne!(theme.control_hover, theme.card);
            assert_ne!(theme.picker_hover, theme.card);
        }
    }
}
