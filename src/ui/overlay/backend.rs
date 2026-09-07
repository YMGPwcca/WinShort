//! Rendering backend selection, resource creation and Composition failure recovery.

use super::composition::CompositionHost;
use super::drawing::draw_overlay;
use super::model::OverlayModel;
use super::palette::{opaque_palette, OverlayPalette};
use super::state::OverlayState;
use super::timeline::{Phase, ShowPlan};
use super::window::{client_size, remove_no_redirection_bitmap};
use crate::error::{Error, Result};

use crate::ui::theme::{Color, ThemeMode};
use windows::Win32::Foundation::{HWND, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory1, ID2D1HwndRenderTarget, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

pub(super) fn render_prepared_frame(
    cell: &std::cell::RefCell<OverlayState>,
    hwnd: HWND,
    plan: ShowPlan,
) -> Result<()> {
    let (spec, data) = {
        let state = cell.borrow();
        if state.phase == Phase::Hidden {
            return Ok(());
        }
        (state.surface_spec(plan.size), state.render_data(plan.alpha))
    };
    run_surface_operation(cell, hwnd, spec, Some(data))
}

pub(super) const RECT_FALLBACK: windows::Win32::Foundation::RECT =
    windows::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

#[derive(Clone, Copy)]
pub(super) struct SurfaceSpec {
    pub(super) size: SIZE,
    pub(super) dpi: u32,
    pub(super) blur_enabled: bool,
}

#[derive(Clone)]
pub(super) struct OverlayRenderData {
    pub(super) dwrite: IDWriteFactory,
    pub(super) model: OverlayModel,
    pub(super) scale: f32,
    pub(super) palette: OverlayPalette,
    pub(super) theme_mode: ThemeMode,
    pub(super) alpha: f32,
    pub(super) opacity: f32,
}

pub(super) enum OverlaySurface {
    Hwnd(HwndOverlaySurface),
    Composition(CompositionHost),
}

pub(super) struct HwndOverlaySurface {
    target: ID2D1HwndRenderTarget,
    size: SIZE,
    dpi: u32,
}

#[derive(Clone)]
pub(super) struct OverlayGraphics {
    pub(super) factory: ID2D1Factory1,
    pub(super) dwrite: IDWriteFactory,
}

fn run_surface_operation(
    cell: &std::cell::RefCell<OverlayState>,
    hwnd: HWND,
    spec: SurfaceSpec,
    data: Option<OverlayRenderData>,
) -> Result<()> {
    let (mut surface, graphics) = {
        let mut state = cell.borrow_mut();
        let Some(surface) = state.surface.take() else {
            return Err(Error::internal("overlay surface missing"));
        };
        (surface, state.graphics.clone())
    };

    let mut result = surface.sync_geometry(spec);
    if result.is_ok() {
        if let Some(data) = data.as_ref() {
            result = surface.render(data, spec);
        }
    }

    let mut switched_to_fallback = false;
    if result.is_err() && surface.is_composition() {
        remove_no_redirection_bitmap(hwnd);
        match graphics.create_surface(hwnd, spec.dpi, spec.size) {
            Ok(fallback_surface) => {
                let mut fallback = OverlaySurface::Hwnd(fallback_surface);
                let fallback_spec = SurfaceSpec {
                    blur_enabled: false,
                    ..spec
                };
                let mut fallback_result = fallback.sync_geometry(fallback_spec);
                if fallback_result.is_ok() {
                    if let Some(data) = data.as_ref() {
                        let mut fallback_data = data.clone();
                        fallback_data.palette = opaque_palette(fallback_data.palette);
                        fallback_result = fallback.render(&fallback_data, fallback_spec);
                    }
                }
                if fallback_result.is_ok() {
                    surface = fallback;
                    switched_to_fallback = true;
                    result = Ok(());
                } else {
                    crate::warn_!("opaque D2D overlay fallback failed: {fallback_result:?}");
                }
            }
            Err(fallback_error) => {
                crate::warn_!("could not create opaque D2D overlay fallback: {fallback_error}");
            }
        }
    }

    let mut restored = false;
    {
        let mut state = cell.borrow_mut();
        if state.surface.is_none() {
            state.surface = Some(surface);
            if switched_to_fallback {
                state.backdrop_enabled = false;
                state.palette = opaque_palette(state.palette);
            }
            restored = true;
        }
    }
    if !restored {
        crate::warn_!("overlay surface changed during rendering; keeping newer surface");
    }
    result
}

pub(super) fn resize_surface(cell: &std::cell::RefCell<OverlayState>, hwnd: HWND) -> Result<()> {
    let Some(size) = client_size(hwnd)? else {
        return Ok(());
    };
    let spec = {
        let mut state = cell.borrow_mut();
        state.surface_size = size;
        state.surface_spec(size)
    };
    run_surface_operation(cell, hwnd, spec, None)
}

pub(super) fn render_current_frame(
    cell: &std::cell::RefCell<OverlayState>,
    hwnd: HWND,
) -> Result<()> {
    let (spec, data) = {
        let state = cell.borrow();
        if state.phase == Phase::Hidden {
            return Ok(());
        }
        let (alpha, _) = state.frame_values();
        (
            state.surface_spec(state.surface_size),
            state.render_data(alpha),
        )
    };
    run_surface_operation(cell, hwnd, spec, Some(data))
}

impl OverlaySurface {
    pub(super) fn is_composition(&self) -> bool {
        matches!(self, Self::Composition(_))
    }

    pub(super) fn sync_geometry(&mut self, spec: SurfaceSpec) -> Result<()> {
        match self {
            Self::Hwnd(surface) => surface.sync_geometry(spec),
            Self::Composition(host) => host.sync_geometry(spec),
        }
    }

    pub(super) fn render(&self, data: &OverlayRenderData, spec: SurfaceSpec) -> Result<()> {
        match self {
            Self::Hwnd(surface) => surface.render(data),
            Self::Composition(host) => host.render(data, spec),
        }
    }
}

impl HwndOverlaySurface {
    pub(super) fn render(&self, data: &OverlayRenderData) -> Result<()> {
        unsafe {
            self.target.BeginDraw();
            let clear = if data.palette.opaque {
                data.palette.surface.d2d()
            } else {
                Color::rgba(0, 0, 0, 0).d2d()
            };
            self.target.Clear(Some(&clear));
            let draw_result = draw_overlay(
                &self.target,
                &data.dwrite,
                &data.model,
                data.scale,
                data.palette,
                data.alpha
                    * if data.palette.opaque {
                        1.0
                    } else {
                        data.opacity.clamp(0.3, 1.0)
                    },
                data.palette.opaque,
            );
            let end_result = self
                .target
                .EndDraw(None, None)
                .map_err(|e| Error::win("overlay EndDraw", &e));
            draw_result?;
            end_result
        }
    }

    pub(super) fn sync_geometry(&mut self, spec: SurfaceSpec) -> Result<()> {
        if self.dpi != spec.dpi {
            unsafe {
                self.target.SetDpi(spec.dpi as f32, spec.dpi as f32);
            }
            self.dpi = spec.dpi;
        }
        if self.size == spec.size {
            return Ok(());
        }
        unsafe {
            self.target
                .Resize(&D2D_SIZE_U {
                    width: spec.size.cx.max(1) as u32,
                    height: spec.size.cy.max(1) as u32,
                })
                .map_err(|e| Error::win("ID2D1HwndRenderTarget::Resize(overlay)", &e))?;
        }
        self.size = spec.size;
        Ok(())
    }
}

impl OverlayGraphics {
    pub(super) fn create() -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory1 = D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                Some(&D2D1_FACTORY_OPTIONS::default()),
            )
            .map_err(|e| Error::win("D2D1CreateFactory(overlay)", &e))?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                .map_err(|e| Error::win("DWriteCreateFactory(overlay)", &e))?;
            Ok(Self { factory, dwrite })
        }
    }

    pub(super) fn create_surface(
        &self,
        hwnd: HWND,
        dpi: u32,
        size: SIZE,
    ) -> Result<HwndOverlaySurface> {
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi as f32,
            dpiY: dpi as f32,
            ..Default::default()
        };
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U {
                width: size.cx.max(1) as u32,
                height: size.cy.max(1) as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        unsafe {
            let target = self
                .factory
                .CreateHwndRenderTarget(&props, &hwnd_props)
                .map_err(|e| Error::win("CreateHwndRenderTarget(overlay)", &e))?;
            target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            target.SetDpi(dpi as f32, dpi as f32);
            Ok(HwndOverlaySurface { target, size, dpi })
        }
    }
}
