//! The picker owns a font, while paint calls borrow only its native handle.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
    DEFAULT_PITCH, FF_DONTCARE, FW_NORMAL, HFONT, OUT_DEFAULT_PRECIS,
};

pub(super) struct PickerFont(HFONT);

impl PickerFont {
    // Only the allocation path can manufacture an owner; stock fonts cannot enter.
    fn from_created(font: HFONT) -> Option<Self> {
        (!font.is_invalid()).then_some(Self(font))
    }

    pub(super) fn handle(&self) -> HFONT {
        self.0
    }
}

impl Drop for PickerFont {
    fn drop(&mut self) {
        // SAFETY: this module owns the allocated font and never exposes ownership.
        let _ = unsafe { DeleteObject(self.0.into()) };
    }
}

const PICKER_FONT_SIZE_DIP: f32 = 14.0;

pub(super) const PICKER_FONT_FAMILY: &str = "Segoe UI Variable Text";

pub(super) const PICKER_FONT_FALLBACK: &str = "Segoe UI";

pub(super) fn picker_font_height(dpi: u32) -> i32 {
    -((PICKER_FONT_SIZE_DIP * dpi.max(96) as f32 / 96.0).round() as i32).max(1)
}

pub(super) fn create_picker_font(dpi: u32) -> Option<PickerFont> {
    let height = picker_font_height(dpi);
    let create = |family: &str| {
        let family = HSTRING::from(family);
        unsafe {
            CreateFontW(
                height,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
                PCWSTR(family.as_ptr()),
            )
        }
    };
    let font = create(PICKER_FONT_FAMILY);
    if font.is_invalid() {
        PickerFont::from_created(create(PICKER_FONT_FALLBACK))
    } else {
        Some(PickerFont(font))
    }
}
