//! Tray icon rendering: Direct2D vector art into an HICON at the current
//! small-icon size. No asset files; crisp on every scale factor.

use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct2D::Common::D2D1_PIXEL_FORMAT;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN, D2D1_FIGURE_BEGIN_FILLED,
    D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_FILL_MODE_WINDING,
    D2D_RECT_F, D2D_SIZE_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory1, ID2D1Geometry, ID2D1PathGeometry1, ID2D1RenderTarget,
    ID2D1SolidColorBrush, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SIZE_SMALL,
    D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_ROUNDED_RECT, D2D1_SWEEP_DIRECTION_CLOCKWISE,
};
use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, IWICBitmap, IWICImagingFactory};
use windows_numerics::Vector2;

use crate::error::{Error, Result};
use crate::tray::TrayState;

/// GUID_WICPixelFormat32bppPBGRA.
const WIC_FMT_PBGRA: GUID = windows::Win32::Graphics::Imaging::GUID_WICPixelFormat32bppPBGRA;

/// Render both tray icons at the given pixel size. Ownership transfers to
/// [`crate::tray::OwnedIcon`] guards so no GDI icon leaks on later failures (#23).
pub fn create_icons(size: u32) -> Result<(crate::tray::OwnedIcon, crate::tray::OwnedIcon)> {
    let normal = crate::tray::OwnedIcon(render_one(size, TrayState::Normal)?);
    let suspended = crate::tray::OwnedIcon(render_one(size, TrayState::HotkeysSuspended)?);
    Ok((normal, suspended))
}

fn d2d_factory() -> Result<ID2D1Factory1> {
    unsafe {
        // SAFETY: single-threaded factory created before any window renders.
        D2D1CreateFactory::<ID2D1Factory1>(
            D2D1_FACTORY_TYPE_SINGLE_THREADED,
            Some(&D2D1_FACTORY_OPTIONS::default()),
        )
        .map_err(|e| Error::win("D2D1CreateFactory", &e))
    }
}

fn wic_factory() -> Result<IWICImagingFactory> {
    unsafe {
        windows::Win32::System::Com::CoCreateInstance(
            &CLSID_WICImagingFactory,
            None,
            windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
        )
        .map_err(|e| Error::win("CoCreateInstance(WIC)", &e))
    }
}

fn render_one(
    size: u32,
    state: TrayState,
) -> Result<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::Win32::Graphics::Gdi as gdi;
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};

    let factory = d2d_factory()?;
    let wic = wic_factory()?;

    // SAFETY: COM/GDI calls with valid handles; all GDI objects freed on exit.
    unsafe {
        let bitmap: IWICBitmap = wic
            .CreateBitmap(
                size,
                size,
                &WIC_FMT_PBGRA,
                windows::Win32::Graphics::Imaging::WICBitmapCreateCacheOption(1),
            )
            .map_err(|e| Error::win("WIC CreateBitmap", &e))?;

        let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            ..Default::default()
        };
        let rt: ID2D1RenderTarget = factory
            .CreateWicBitmapRenderTarget(&bitmap, &rt_props)
            .map_err(|e| Error::win("CreateWicBitmapRenderTarget", &e))?;

        rt.BeginDraw();
        draw_glyphs(&factory, &rt, size as f32, state)?;
        rt.EndDraw(None, None)
            .map_err(|e| Error::win("icon EndDraw", &e))?;

        let stride = size * 4;
        let mut pixels = vec![0u8; (stride * size) as usize];
        bitmap
            .CopyPixels(std::ptr::null(), stride, &mut pixels)
            .map_err(|e| Error::win("WIC CopyPixels", &e))?;

        let hdc_screen = gdi::GetDC(None);
        let bmi = gdi::BITMAPINFO {
            bmiHeader: gdi::BITMAPINFOHEADER {
                biSize: std::mem::size_of::<gdi::BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let hbm_color = gdi::CreateDIBSection(
            Some(hdc_screen),
            &bmi,
            gdi::DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )
        .map_err(|e| Error::win("CreateDIBSection", &e))?;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());

        let mask_row = (size.div_ceil(16) * 2) as usize;
        let mask_bits = vec![0u8; mask_row * size as usize];
        let hbm_mask = gdi::CreateBitmap(
            size as i32,
            size as i32,
            1,
            1,
            Some(mask_bits.as_ptr().cast()),
        );

        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm_mask,
            hbmColor: hbm_color,
        };
        let hicon = CreateIconIndirect(&info).map_err(|e| Error::win("CreateIconIndirect", &e))?;

        let _ = gdi::DeleteObject(gdi::HGDIOBJ(hbm_mask.0));
        let _ = gdi::DeleteObject(gdi::HGDIOBJ(hbm_color.0));
        let _ = gdi::ReleaseDC(None, hdc_screen);
        Ok(hicon)
    }
}

fn pt(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

fn draw_glyphs(
    factory: &ID2D1Factory1,
    rt: &ID2D1RenderTarget,
    size: f32,
    state: TrayState,
) -> Result<()> {
    unsafe {
        let s = size / 24.0; // design space 24x24

        let bg_color = match state {
            TrayState::Normal => rgba(0x0f, 0x6c, 0xbd, 255),
            TrayState::HotkeysSuspended => rgba(0x60, 0x60, 0x60, 255),
        };
        let fg_alpha = match state {
            TrayState::Normal => 240u8,
            TrayState::HotkeysSuspended => 150u8,
        };

        rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);

        let bg_brush = create_solid(rt, bg_color);
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: s,
                top: s,
                right: 23.0 * s,
                bottom: 23.0 * s,
            },
            radiusX: 5.5 * s,
            radiusY: 5.5 * s,
        };
        rt.FillRoundedRectangle(&rr, &bg_brush);

        // Speaker glyph.
        let speaker_pts = [
            pt(6.0 * s, 9.6 * s),
            pt(9.4 * s, 9.6 * s),
            pt(13.2 * s, 6.4 * s),
            pt(13.2 * s, 17.6 * s),
            pt(9.4 * s, 14.4 * s),
            pt(6.0 * s, 14.4 * s),
        ];
        let speaker = line_figure(factory, &speaker_pts, D2D1_FIGURE_BEGIN_FILLED)?;
        let fg_brush = create_solid(rt, rgba(255, 255, 255, fg_alpha));
        let speaker_geom: ID2D1Geometry = speaker
            .cast()
            .map_err(|e| Error::win("cast geometry", &e))?;
        rt.FillGeometry(&speaker_geom, &fg_brush, None);

        match state {
            TrayState::Normal => {
                let wave = create_solid(rt, rgba(255, 255, 255, 225));
                for radius in [3.4 * s, 6.2 * s] {
                    let arc = arc_wave(factory, 13.8 * s, 12.0 * s, radius)?;
                    rt.DrawGeometry(&arc, &wave, 1.9 * s, None);
                }
            }
            TrayState::HotkeysSuspended => {
                let slash = create_solid(rt, rgba(0xff, 0x8a, 0x6b, 235));
                let line = open_figure(factory, &[pt(5.5 * s, 18.5 * s), pt(18.5 * s, 5.5 * s)])?;
                rt.DrawGeometry(&line, &slash, 2.2 * s, None);
            }
        }
        Ok(())
    }
}

unsafe fn create_solid(rt: &ID2D1RenderTarget, c: D2D1_COLOR_F) -> ID2D1SolidColorBrush {
    // SAFETY: COM call on a live render target created above.
    unsafe {
        rt.CreateSolidColorBrush(std::ptr::from_ref(&c), None)
            .expect("solid brush")
    }
}

/// Closed polyline figure.
unsafe fn line_figure(
    factory: &ID2D1Factory1,
    pts: &[Vector2],
    begin: D2D1_FIGURE_BEGIN,
) -> Result<ID2D1PathGeometry1> {
    // SAFETY: all calls operate on COM objects created within this function.
    unsafe {
        let geom = factory
            .CreatePathGeometry()
            .map_err(|e| Error::win("CreatePathGeometry", &e))?;
        let sink = geom
            .Open()
            .map_err(|e| Error::win("GeometrySink Open", &e))?;
        sink.SetFillMode(D2D1_FILL_MODE_WINDING);
        sink.BeginFigure(pts[0], begin);
        for p in &pts[1..] {
            sink.AddLine(*p);
        }
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close().map_err(|e| Error::win("Sink Close", &e))?;
        Ok(geom)
    }
}

/// Open two-point figure.
unsafe fn open_figure(factory: &ID2D1Factory1, pts: &[Vector2]) -> Result<ID2D1PathGeometry1> {
    // SAFETY: all calls operate on COM objects created within this function.
    unsafe {
        let geom = factory
            .CreatePathGeometry()
            .map_err(|e| Error::win("CreatePathGeometry", &e))?;
        let sink = geom
            .Open()
            .map_err(|e| Error::win("GeometrySink Open", &e))?;
        sink.BeginFigure(pts[0], D2D1_FIGURE_BEGIN_HOLLOW);
        for p in &pts[1..] {
            sink.AddLine(*p);
        }
        sink.EndFigure(D2D1_FIGURE_END_OPEN);
        sink.Close().map_err(|e| Error::win("Sink Close", &e))?;
        Ok(geom)
    }
}

/// Right-facing sound-wave arc (-45° to +45°) around (cx,cy).
unsafe fn arc_wave(
    factory: &ID2D1Factory1,
    cx: f32,
    cy: f32,
    radius: f32,
) -> Result<ID2D1PathGeometry1> {
    // SAFETY: all calls operate on COM objects created within this function.
    unsafe {
        let k = std::f32::consts::FRAC_1_SQRT_2;
        let start = pt(cx + radius * k, cy - radius * k);
        let end = pt(cx + radius * k, cy + radius * k);
        let geom = factory
            .CreatePathGeometry()
            .map_err(|e| Error::win("CreatePathGeometry", &e))?;
        let sink = geom
            .Open()
            .map_err(|e| Error::win("GeometrySink Open", &e))?;
        sink.BeginFigure(start, D2D1_FIGURE_BEGIN_HOLLOW);
        sink.AddArc(&windows::Win32::Graphics::Direct2D::D2D1_ARC_SEGMENT {
            point: end,
            size: D2D_SIZE_F {
                width: radius,
                height: radius,
            },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        });
        sink.EndFigure(D2D1_FIGURE_END_OPEN);
        sink.Close().map_err(|e| Error::win("Sink Close", &e))?;
        Ok(geom)
    }
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    }
}
