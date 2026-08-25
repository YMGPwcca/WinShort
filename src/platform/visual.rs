//! Cached Windows visual/accessibility preferences for presentation surfaces.
//!
//! Raw SystemParametersInfo/GetSysColor calls stay here; overlay rendering
//! consumes the copied value and never queries Win32 preference APIs itself.

use windows::core::BOOL;
use windows::Win32::Graphics::Gdi::{GetSysColor, COLOR_HIGHLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SPI_GETDISABLEOVERLAPPEDCONTENT,
    SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemVisualPreferences {
    pub animations_enabled: bool,
    pub high_contrast: bool,
    pub disable_overlapped_content: bool,
    pub system_theme: crate::ui::theme::ThemeMode,
    pub high_contrast_background: VisualRgb,
    pub high_contrast_foreground: VisualRgb,
    pub high_contrast_accent: VisualRgb,
}

impl Default for SystemVisualPreferences {
    fn default() -> Self {
        Self {
            animations_enabled: true,
            high_contrast: false,
            disable_overlapped_content: false,
            system_theme: crate::ui::theme::ThemeMode::Dark,
            high_contrast_background: VisualRgb { r: 0, g: 0, b: 0 },
            high_contrast_foreground: VisualRgb {
                r: 255,
                g: 255,
                b: 255,
            },
            high_contrast_accent: VisualRgb {
                r: 255,
                g: 255,
                b: 0,
            },
        }
    }
}

impl SystemVisualPreferences {
    pub fn query() -> Self {
        let mut preferences = Self {
            system_theme: crate::ui::theme::system_theme_mode(),
            ..Self::default()
        };
        let mut animations = BOOL(1);
        if unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                Some((&mut animations as *mut BOOL).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .is_ok()
        {
            preferences.animations_enabled = animations.as_bool();
        }

        let mut overlapped = BOOL(0);
        if unsafe {
            SystemParametersInfoW(
                SPI_GETDISABLEOVERLAPPEDCONTENT,
                0,
                Some((&mut overlapped as *mut BOOL).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .is_ok()
        {
            preferences.disable_overlapped_content = overlapped.as_bool();
        }

        let mut high_contrast = HIGHCONTRASTW {
            cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
            ..Default::default()
        };
        if unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                high_contrast.cbSize,
                Some((&mut high_contrast as *mut HIGHCONTRASTW).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .is_ok()
        {
            preferences.high_contrast = high_contrast.dwFlags.contains(HCF_HIGHCONTRASTON);
        }

        preferences.high_contrast_background = rgb(COLOR_WINDOW);
        preferences.high_contrast_foreground = rgb(COLOR_WINDOWTEXT);
        preferences.high_contrast_accent = rgb(COLOR_HIGHLIGHT);
        preferences
    }
}

fn rgb(index: windows::Win32::Graphics::Gdi::SYS_COLOR_INDEX) -> VisualRgb {
    let value = unsafe { GetSysColor(index) };
    VisualRgb {
        r: (value & 0xFF) as u8,
        g: ((value >> 8) & 0xFF) as u8,
        b: ((value >> 16) & 0xFF) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preferences_are_conservative_without_querying() {
        let preferences = SystemVisualPreferences::default();
        assert!(preferences.animations_enabled);
        assert!(!preferences.high_contrast);
        assert!(!preferences.disable_overlapped_content);
    }
}
