//! The WIC-to-HICON boundary owns every intermediate GDI object, even on failure.

use crate::error::{Error, Result};
use windows::Win32::Graphics::Gdi::{self as gdi, HBITMAP, HDC};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

/// A tray icon's bounded physical dimensions make row-size arithmetic safe.
#[derive(Clone, Copy)]
pub(super) struct IconSize(u32);

impl IconSize {
    pub(super) fn new(size: u32) -> Result<Self> {
        // The notification area uses small icons. Reject a bogus OS metric
        // before it becomes an unbounded allocation or negative GDI dimension.
        if !(1..=1024).contains(&size) {
            return Err(Error::internal("invalid tray icon dimensions"));
        }
        Ok(Self(size))
    }
    pub(super) fn pixels(self) -> u32 {
        self.0
    }
    pub(super) fn stride(self) -> u32 {
        self.0 * 4
    }
    pub(super) fn byte_count(self) -> usize {
        (self.stride() * self.0) as usize
    }
}

struct ScreenDc(HDC);
impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: acquired once with GetDC(None), never transferred.
        unsafe {
            gdi::ReleaseDC(None, self.0);
        }
    }
}
struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: created here and not selected into any DC.
        let _ = unsafe { gdi::DeleteObject(self.0.into()) };
    }
}

pub(super) fn icon_from_pixels(size: IconSize, pixels: &[u8]) -> Result<HICON> {
    if pixels.len() != size.byte_count() {
        return Err(Error::internal(
            "tray icon pixel buffer has the wrong length",
        ));
    }
    // SAFETY: dimensions are bounded, buffers have checked lengths, and guards
    // outlive CreateIconIndirect, which copies the two source bitmaps.
    unsafe {
        let dc = gdi::GetDC(None);
        if dc.is_invalid() {
            return Err(Error::internal("GetDC failed for tray icon"));
        }
        let dc = ScreenDc(dc);
        let bmi = gdi::BITMAPINFO {
            bmiHeader: gdi::BITMAPINFOHEADER {
                biSize: std::mem::size_of::<gdi::BITMAPINFOHEADER>() as u32,
                biWidth: size.pixels() as i32,
                biHeight: -(size.pixels() as i32),
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let color = Bitmap(
            gdi::CreateDIBSection(Some(dc.0), &bmi, gdi::DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| Error::win("CreateDIBSection", &e))?,
        );
        if bits.is_null() {
            return Err(Error::internal(
                "CreateDIBSection returned no pixel storage",
            ));
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast(), pixels.len());
        let mask_stride = size.pixels().div_ceil(16) * 2;
        let mask_bits = vec![0u8; (mask_stride * size.pixels()) as usize];
        let mask = gdi::CreateBitmap(
            size.pixels() as i32,
            size.pixels() as i32,
            1,
            1,
            Some(mask_bits.as_ptr().cast()),
        );
        if mask.is_invalid() {
            return Err(Error::internal("CreateBitmap failed for tray icon mask"));
        }
        let mask = Bitmap(mask);
        let info = ICONINFO {
            fIcon: true.into(),
            hbmMask: mask.0,
            hbmColor: color.0,
            ..Default::default()
        };
        CreateIconIndirect(&info).map_err(|e| Error::win("CreateIconIndirect", &e))
    }
}
