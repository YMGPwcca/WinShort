//! Extract executable icons once and cache target-owned Direct2D bitmaps.
use super::group::ClusterIcon;
use super::model::{OverlayIcon, OverlayModel};
use crate::error::{Error, Result};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    ID2D1Bitmap, ID2D1RenderTarget, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ExecutableIcon {
    id: u64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
struct OwnedIcon(HICON);
impl Drop for OwnedIcon {
    fn drop(&mut self) {
        let _ = unsafe { DestroyIcon(self.0) };
    }
}

pub(crate) fn executable_icon(path: &str) -> Option<OverlayIcon> {
    load(path)
        .ok()
        .map(|icon| OverlayIcon::Executable(Arc::new(icon)))
}

fn load(path: &str) -> Result<ExecutableIcon> {
    let path = HSTRING::from(path);
    let mut info = SHFILEINFOW::default();
    // SHGetFileInfo transfers this HICON to us. The guard covers every later failure.
    let found = unsafe {
        SHGetFileInfoW(
            PCWSTR(path.as_ptr()),
            Default::default(),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    // SHFILEINFOW is packed on i686. Copy the handle by value before any
    // method borrows it, so no unaligned reference to the packed field forms.
    let handle = info.hIcon;
    if found == 0 || handle.is_invalid() {
        return Err(Error::internal("No executable icon"));
    }
    let icon = OwnedIcon(handle);
    let factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }?;
    let bitmap = unsafe { factory.CreateBitmapFromHICON(icon.0) }?;
    let converter = unsafe { factory.CreateFormatConverter() }?;
    unsafe {
        converter.Initialize(
            &bitmap,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
    }?;
    let (mut width, mut height) = (0, 0);
    unsafe { converter.GetSize(&mut width, &mut height) }?;
    if width == 0 || height == 0 || width > 256 || height > 256 {
        return Err(Error::internal("Invalid executable icon size"));
    }
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    unsafe { converter.CopyPixels(std::ptr::null(), width * 4, &mut pixels) }?;
    static NEXT_ICON: AtomicU64 = AtomicU64::new(1);
    Ok(ExecutableIcon {
        id: NEXT_ICON.fetch_add(1, Ordering::Relaxed),
        width,
        height,
        pixels,
    })
}

#[derive(Default)]
pub(super) struct BitmapCache(RefCell<HashMap<u64, ID2D1Bitmap>>);
impl BitmapCache {
    pub(super) fn retain(&self, model: &OverlayModel, peers: &[ClusterIcon]) {
        let ids: HashSet<_> = model
            .rows
            .iter()
            .chain(peers.iter().map(|peer| &peer.row))
            .filter_map(|row| {
                if let OverlayIcon::Executable(icon) = &row.icon {
                    Some(icon.id)
                } else {
                    None
                }
            })
            .collect();
        self.0.borrow_mut().retain(|id, _| ids.contains(id));
    }

    pub(super) fn draw(
        &self,
        target: &ID2D1RenderTarget,
        icon: &ExecutableIcon,
        rect: D2D_RECT_F,
        alpha: f32,
    ) -> Result<()> {
        let mut bitmaps = self.0.borrow_mut();
        let bitmap = match bitmaps.entry(icon.id) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let props = D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                };
                let bitmap = unsafe {
                    target.CreateBitmap(
                        D2D_SIZE_U {
                            width: icon.width,
                            height: icon.height,
                        },
                        Some(icon.pixels.as_ptr().cast()),
                        icon.width * 4,
                        &props,
                    )
                }?;
                entry.insert(bitmap)
            }
        };
        unsafe {
            target.DrawBitmap(
                &*bitmap,
                Some(&rect),
                alpha,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            )
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_real_executable_icon_and_releases_the_native_handle() {
        let _com = crate::platform::com::ComApartment::init_sta();
        let path = std::env::var("WINDIR").unwrap() + "\\System32\\notepad.exe";
        let icon = load(&path).unwrap();
        assert!(icon.pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
        assert_eq!(icon.pixels.len(), (icon.width * icon.height * 4) as usize);
    }
}
