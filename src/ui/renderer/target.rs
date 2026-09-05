//! Direct2D HWND target ownership and device-dependent lifecycle.

use super::resources::BrushSet;
use super::text::TextFormats;
use crate::error::{Error, Result};
use crate::ui::theme::Theme;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory1, ID2D1HwndRenderTarget, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED,
};

pub(crate) struct Renderer {
    pub(super) hwnd: HWND,
    pub(super) dpi: u32,
    /// Kept alive deliberately: D2D/DWrite objects created from it retain
    /// references, but rustc cannot see the transitive use.
    #[allow(dead_code)]
    pub(super) factory: ID2D1Factory1,
    pub(super) target: ID2D1HwndRenderTarget,
    pub(super) dwrite: IDWriteFactory,
    pub(super) brushes: BrushSet,
    pub(super) formats: TextFormats,
    pub(super) theme: Theme,
}

fn client_size(hwnd: HWND) -> (u32, u32) {
    let mut rect = RECT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
    }
    (
        (rect.right - rect.left).max(0) as u32,
        (rect.bottom - rect.top).max(0) as u32,
    )
}

impl Renderer {
    pub(crate) fn new(hwnd: HWND, dpi: u32, theme: Theme) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory1 = D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                Some(&D2D1_FACTORY_OPTIONS::default()),
            )
            .map_err(|e| Error::win("D2D1CreateFactory(control center)", &e))?;

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                .map_err(|e| Error::win("DWriteCreateFactory", &e))?;

            let (width, height) = client_size(hwnd);
            let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: dpi as f32,
                dpiY: dpi as f32,
                ..Default::default()
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U { width, height },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let target = factory
                .CreateHwndRenderTarget(&rt_props, &hwnd_props)
                .map_err(|e| Error::win("CreateHwndRenderTarget", &e))?;
            target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            target.SetDpi(dpi as f32, dpi as f32);

            let brushes = BrushSet::create(&target, theme)?;
            let formats = TextFormats::create(&dwrite)?;
            let renderer = Self {
                hwnd,
                dpi,
                factory,
                target,
                dwrite,
                brushes,
                formats,
                theme,
            };
            Ok(renderer)
        }
    }

    pub(crate) fn set_theme(&mut self, theme: Theme) -> Result<()> {
        let brushes = BrushSet::create(&self.target, theme)?;
        self.theme = theme;
        self.brushes = brushes;
        Ok(())
    }

    pub(crate) fn set_dpi(&mut self, dpi: u32) -> Result<()> {
        self.rebuild_formats()?;
        self.dpi = dpi.max(96);
        unsafe { self.target.SetDpi(self.dpi as f32, self.dpi as f32) };
        Ok(())
    }

    pub(crate) fn client_size_dip(&self) -> (f32, f32) {
        let (w, h) = client_size(self.hwnd);
        let scale = 96.0 / self.dpi as f32;
        (w as f32 * scale, h as f32 * scale)
    }

    pub(crate) fn push_clip(&self, rect: D2D_RECT_F) {
        unsafe {
            self.target.PushAxisAlignedClip(
                &rect,
                windows::Win32::Graphics::Direct2D::D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            )
        }
    }

    pub(crate) fn pop_clip(&self) {
        unsafe { self.target.PopAxisAlignedClip() }
    }

    pub(crate) fn resize(&mut self) -> Result<()> {
        let (width, height) = client_size(self.hwnd);
        if width == 0 || height == 0 {
            return Ok(());
        }
        unsafe {
            self.target
                .Resize(&D2D_SIZE_U { width, height })
                .map_err(|e| Error::win("ID2D1HwndRenderTarget::Resize", &e))
        }
    }

    pub(crate) fn begin(&self) {
        unsafe {
            self.target.BeginDraw();
            let bg = self.theme.bg.d2d();
            self.target.Clear(Some(std::ptr::from_ref(&bg)));
        }
    }

    pub(crate) fn end(&self) -> Result<()> {
        unsafe {
            self.target
                .EndDraw(None, None)
                .map_err(|e| Error::win("ID2D1HwndRenderTarget::EndDraw", &e))
        }
    }
}
