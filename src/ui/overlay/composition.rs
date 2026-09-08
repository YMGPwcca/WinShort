//! Windows Composition backend for the overlay.
//!
//! Device creation, scene construction and per-frame drawing are separate so
//! the native lifetime graph stays explicit without one constructor owning the
//! details of every Direct3D, Direct2D and Composition resource.

use super::backend::{OverlayGraphics, OverlayRenderData, SurfaceSpec};
use super::drawing::draw_overlay;
use super::effect::GaussianBlurEffectGraph;
use super::layout::CARD_CORNER_RADIUS_DIP;
use super::palette::composition_tint_alpha;
use crate::config::model::OverlayBlur;
use crate::error::{Error, Result};
use crate::ui::theme::Color;

use windows::core::{Interface, HSTRING};
use windows::Foundation::Size;
use windows::Graphics::DirectX::{DirectXAlphaMode, DirectXPixelFormat};
use windows::Graphics::Effects::{IGraphicsEffect, IGraphicsEffectSource};
use windows::System::DispatcherQueueController;
use windows::Win32::Foundation::{HWND, POINT, SIZE};
use windows::Win32::Graphics::Direct2D::{
    ID2D1Device, ID2D1DeviceContext, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Composition::{
    ICompositionDrawingSurfaceInterop, ICompositorDesktopInterop, ICompositorInterop,
};
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DispatcherQueueOptions, DQTAT_COM_ASTA, DQTYPE_THREAD_CURRENT,
};
use windows::UI::Color as WinRtColor;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{
    CompositionColorBrush, CompositionDrawingSurface, CompositionEffectBrush,
    CompositionEffectSourceParameter, CompositionGeometricClip, CompositionGraphicsDevice,
    CompositionRoundedRectangleGeometry, CompositionSurfaceBrush, Compositor, ContainerVisual,
    SpriteVisual,
};
use windows_numerics::Vector2;

struct CompositionDevices {
    graphics_device: CompositionGraphicsDevice,
    _d2d_device: ID2D1Device,
    _d3d_device: ID3D11Device,
    _d3d_context: ID3D11DeviceContext,
}

struct CompositionScene {
    root: ContainerVisual,
    backdrop_visual: SpriteVisual,
    tint_visual: SpriteVisual,
    content_visual: SpriteVisual,
    _effect_brush: CompositionEffectBrush,
    blur_amount: f32,
    tint_brush: CompositionColorBrush,
    content_brush: CompositionSurfaceBrush,
    geometry: CompositionRoundedRectangleGeometry,
    _clip: CompositionGeometricClip,
}

pub(super) struct CompositionHost {
    _dispatcher: DispatcherQueueController,
    _compositor: Compositor,
    _target: DesktopWindowTarget,
    scene: CompositionScene,
    surface: CompositionDrawingSurface,
    devices: CompositionDevices,
    size: SIZE,
    dpi: u32,
}

fn create_dispatcher() -> Result<DispatcherQueueController> {
    let options = DispatcherQueueOptions {
        dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
        threadType: DQTYPE_THREAD_CURRENT,
        apartmentType: DQTAT_COM_ASTA,
    };
    unsafe { CreateDispatcherQueueController(options) }
        .map_err(|e| Error::win("CreateDispatcherQueueController(overlay)", &e))
}

fn create_target(hwnd: HWND) -> Result<(Compositor, DesktopWindowTarget)> {
    let compositor = Compositor::new().map_err(|e| Error::win("Compositor::new(overlay)", &e))?;
    let desktop: ICompositorDesktopInterop = compositor
        .cast()
        .map_err(|e| Error::win("ICompositorDesktopInterop(overlay)", &e))?;
    let target = unsafe { desktop.CreateDesktopWindowTarget(hwnd, false) }
        .map_err(|e| Error::win("CreateDesktopWindowTarget(overlay)", &e))?;
    Ok((compositor, target))
}

fn create_devices(
    compositor: &Compositor,
    graphics: &OverlayGraphics,
) -> Result<CompositionDevices> {
    let mut d3d_device: Option<ID3D11Device> = None;
    let mut d3d_context: Option<ID3D11DeviceContext> = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            Default::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some((&mut d3d_device) as *mut _),
            None,
            Some((&mut d3d_context) as *mut _),
        )
        .map_err(|e| Error::win("D3D11CreateDevice(overlay)", &e))?;
    }
    let d3d_device =
        d3d_device.ok_or_else(|| Error::internal("D3D11CreateDevice returned no device"))?;
    let d3d_context =
        d3d_context.ok_or_else(|| Error::internal("D3D11CreateDevice returned no context"))?;
    let dxgi_device: IDXGIDevice = d3d_device
        .cast()
        .map_err(|e| Error::win("IDXGIDevice(overlay)", &e))?;
    let d2d_device = unsafe {
        graphics
            .factory
            .CreateDevice(&dxgi_device)
            .map_err(|e| Error::win("Create D2D device(overlay)", &e))?
    };
    let compositor_interop: ICompositorInterop = compositor
        .cast()
        .map_err(|e| Error::win("ICompositorInterop(overlay)", &e))?;
    let graphics_device = unsafe {
        compositor_interop
            .CreateGraphicsDevice(&d2d_device)
            .map_err(|e| Error::win("CreateCompositionGraphicsDevice(overlay)", &e))?
    };

    Ok(CompositionDevices {
        graphics_device,
        _d2d_device: d2d_device,
        _d3d_device: d3d_device,
        _d3d_context: d3d_context,
    })
}

fn create_composition_surface(
    graphics_device: &CompositionGraphicsDevice,
    size: SIZE,
) -> Result<CompositionDrawingSurface> {
    graphics_device
        .CreateDrawingSurface(
            Size {
                Width: size.cx.max(1) as f32,
                Height: size.cy.max(1) as f32,
            },
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            DirectXAlphaMode::Premultiplied,
        )
        .map_err(|e| Error::win("CreateDrawingSurface(overlay)", &e))
}

fn clear_composition_surface(surface: &CompositionDrawingSurface, dpi: u32) -> Result<()> {
    let surface_interop: ICompositionDrawingSurfaceInterop = surface
        .cast()
        .map_err(|e| Error::win("CastCompositionSurface(overlay)", &e))?;
    let mut draw_offset = POINT::default();
    let drawing_context: ID2D1DeviceContext = unsafe {
        surface_interop
            .BeginDraw(None, &mut draw_offset)
            .map_err(|e| Error::win("BeginCompositionClear(overlay)", &e))?
    };
    let transparent = Color::rgba(0, 0, 0, 0).d2d();
    unsafe {
        drawing_context.SetDpi(dpi as f32, dpi as f32);
        drawing_context.Clear(Some(&transparent));
    }
    unsafe {
        surface_interop
            .EndDraw()
            .map_err(|e| Error::win("EndCompositionClear(overlay)", &e))
    }
}

fn create_backdrop_brush(
    compositor: &Compositor,
    blur_amount: f32,
) -> Result<CompositionEffectBrush> {
    let source_name = HSTRING::from("source");
    let source_parameter = CompositionEffectSourceParameter::Create(&source_name)
        .map_err(|e| Error::win("CreateEffectSourceParameter(overlay)", &e))?;
    let source: IGraphicsEffectSource = source_parameter
        .cast()
        .map_err(|e| Error::win("CastEffectSourceParameter(overlay)", &e))?;
    let effect: IGraphicsEffect = GaussianBlurEffectGraph {
        source,
        blur_amount,
    }
    .into();
    let effect_factory = compositor
        .CreateEffectFactory(&effect)
        .map_err(|e| Error::win("CreateEffectFactory(overlay)", &e))?;
    let effect_brush = effect_factory
        .CreateBrush()
        .map_err(|e| Error::win("CreateEffectBrush(overlay)", &e))?;
    let backdrop = compositor
        .CreateBackdropBrush()
        .map_err(|e| Error::win("CreateBackdropBrush(overlay)", &e))?;
    effect_brush
        .SetSourceParameter(&source_name, &backdrop)
        .map_err(|e| Error::win("SetBackdropSource(overlay)", &e))?;
    Ok(effect_brush)
}

fn create_clip(
    compositor: &Compositor,
    size: Vector2,
    dpi: u32,
) -> Result<(
    CompositionRoundedRectangleGeometry,
    CompositionGeometricClip,
)> {
    let geometry = compositor
        .CreateRoundedRectangleGeometry()
        .map_err(|e| Error::win("CreateRoundedRectangleGeometry(overlay)", &e))?;
    geometry
        .SetSize(size)
        .map_err(|e| Error::win("SetShapeSize(overlay)", &e))?;
    geometry
        .SetCornerRadius(composition_radius(dpi))
        .map_err(|e| Error::win("SetShapeRadius(overlay)", &e))?;
    let clip = compositor
        .CreateGeometricClipWithGeometry(&geometry)
        .map_err(|e| Error::win("CreateGeometricClip(overlay)", &e))?;
    Ok((geometry, clip))
}

fn create_backdrop_visual(
    compositor: &Compositor,
    size: Vector2,
    clip: &CompositionGeometricClip,
    brush: &CompositionEffectBrush,
) -> Result<SpriteVisual> {
    let visual = compositor
        .CreateSpriteVisual()
        .map_err(|e| Error::win("CreateBackdropVisual(overlay)", &e))?;
    visual
        .SetSize(size)
        .map_err(|e| Error::win("SetBackdropSize(overlay)", &e))?;
    visual
        .SetBrush(brush)
        .map_err(|e| Error::win("SetBackdropBrush(overlay)", &e))?;
    visual
        .SetClip(clip)
        .map_err(|e| Error::win("SetBackdropClip(overlay)", &e))?;
    visual
        .SetIsVisible(false)
        .map_err(|e| Error::win("HideBackdropVisual(overlay)", &e))?;
    Ok(visual)
}

fn create_tint_layer(
    compositor: &Compositor,
    size: Vector2,
    clip: &CompositionGeometricClip,
) -> Result<(CompositionColorBrush, SpriteVisual)> {
    let brush = compositor
        .CreateColorBrushWithColor(WinRtColor {
            A: 0,
            R: 0,
            G: 0,
            B: 0,
        })
        .map_err(|e| Error::win("CreateTintBrush(overlay)", &e))?;
    let visual = compositor
        .CreateSpriteVisual()
        .map_err(|e| Error::win("CreateTintVisual(overlay)", &e))?;
    visual
        .SetSize(size)
        .map_err(|e| Error::win("SetTintSize(overlay)", &e))?;
    visual
        .SetBrush(&brush)
        .map_err(|e| Error::win("SetTintBrush(overlay)", &e))?;
    visual
        .SetClip(clip)
        .map_err(|e| Error::win("SetTintClip(overlay)", &e))?;
    visual
        .SetIsVisible(false)
        .map_err(|e| Error::win("HideTintVisual(overlay)", &e))?;
    Ok((brush, visual))
}

fn create_content_layer(
    compositor: &Compositor,
    surface: &CompositionDrawingSurface,
    size: Vector2,
    clip: &CompositionGeometricClip,
) -> Result<(CompositionSurfaceBrush, SpriteVisual)> {
    let brush = compositor
        .CreateSurfaceBrushWithSurface(surface)
        .map_err(|e| Error::win("CreateSurfaceBrush(overlay)", &e))?;
    let visual = compositor
        .CreateSpriteVisual()
        .map_err(|e| Error::win("CreateContentVisual(overlay)", &e))?;
    visual
        .SetSize(size)
        .map_err(|e| Error::win("SetContentSize(overlay)", &e))?;
    visual
        .SetBrush(&brush)
        .map_err(|e| Error::win("SetContentBrush(overlay)", &e))?;
    visual
        .SetClip(clip)
        .map_err(|e| Error::win("SetContentClip(overlay)", &e))?;
    Ok((brush, visual))
}

fn create_scene(
    compositor: &Compositor,
    surface: &CompositionDrawingSurface,
    size: SIZE,
    dpi: u32,
) -> Result<CompositionScene> {
    let vector_size = composition_size(size);
    let root = compositor
        .CreateContainerVisual()
        .map_err(|e| Error::win("CreateContainerVisual(overlay)", &e))?;
    root.SetSize(vector_size)
        .map_err(|e| Error::win("SetRootSize(overlay)", &e))?;

    let (geometry, clip) = create_clip(compositor, vector_size, dpi)?;
    let effect_brush =
        create_backdrop_brush(compositor, OverlayBlur::BlurMedium.blur_amount().unwrap())?;
    let backdrop_visual = create_backdrop_visual(compositor, vector_size, &clip, &effect_brush)?;
    let (tint_brush, tint_visual) = create_tint_layer(compositor, vector_size, &clip)?;
    let (content_brush, content_visual) =
        create_content_layer(compositor, surface, vector_size, &clip)?;

    let children = root
        .Children()
        .map_err(|e| Error::win("GetRootChildren(overlay)", &e))?;
    children
        .InsertAtBottom(&backdrop_visual)
        .map_err(|e| Error::win("InsertBackdropVisual(overlay)", &e))?;
    children
        .InsertAtTop(&tint_visual)
        .map_err(|e| Error::win("InsertTintVisual(overlay)", &e))?;
    children
        .InsertAtTop(&content_visual)
        .map_err(|e| Error::win("InsertContentVisual(overlay)", &e))?;

    Ok(CompositionScene {
        root,
        backdrop_visual,
        tint_visual,
        content_visual,
        _effect_brush: effect_brush,
        blur_amount: OverlayBlur::BlurMedium.blur_amount().unwrap(),
        tint_brush,
        content_brush,
        geometry,
        _clip: clip,
    })
}

fn composition_size(size: SIZE) -> Vector2 {
    Vector2 {
        X: size.cx.max(1) as f32,
        Y: size.cy.max(1) as f32,
    }
}

fn composition_radius(dpi: u32) -> Vector2 {
    let radius = CARD_CORNER_RADIUS_DIP * dpi.max(96) as f32 / 96.0;
    Vector2 {
        X: radius,
        Y: radius,
    }
}

impl CompositionHost {
    pub(super) fn create(
        hwnd: HWND,
        size: SIZE,
        dpi: u32,
        graphics: &OverlayGraphics,
    ) -> Result<Self> {
        let dispatcher = create_dispatcher()?;
        let (compositor, target) = create_target(hwnd)?;
        let devices = create_devices(&compositor, graphics)?;
        let surface = create_composition_surface(&devices.graphics_device, size)?;
        clear_composition_surface(&surface, dpi)?;
        let scene = create_scene(&compositor, &surface, size, dpi)?;
        target
            .SetRoot(&scene.root)
            .map_err(|e| Error::win("SetCompositionRoot(overlay)", &e))?;

        Ok(Self {
            _dispatcher: dispatcher,
            _compositor: compositor,
            _target: target,
            scene,
            surface,
            devices,
            size,
            dpi,
        })
    }

    pub(super) fn sync_geometry(&mut self, spec: SurfaceSpec) -> Result<()> {
        if self.size == spec.size && self.dpi == spec.dpi {
            return Ok(());
        }
        let surface = create_composition_surface(&self.devices.graphics_device, spec.size)?;
        clear_composition_surface(&surface, spec.dpi)?;
        self.scene
            .content_brush
            .SetSurface(&surface)
            .map_err(|e| Error::win("SetCompositionSurface(overlay)", &e))?;
        let vector_size = composition_size(spec.size);
        self.scene
            .root
            .SetSize(vector_size)
            .map_err(|e| Error::win("SetRootSize(overlay)", &e))?;
        for visual in [
            &self.scene.backdrop_visual,
            &self.scene.tint_visual,
            &self.scene.content_visual,
        ] {
            visual
                .SetSize(vector_size)
                .map_err(|e| Error::win("SetVisualSize(overlay)", &e))?;
        }
        self.scene
            .geometry
            .SetSize(vector_size)
            .map_err(|e| Error::win("SetShapeSize(overlay)", &e))?;
        self.scene
            .geometry
            .SetCornerRadius(composition_radius(spec.dpi))
            .map_err(|e| Error::win("SetShapeRadius(overlay)", &e))?;
        self.surface = surface;
        self.size = spec.size;
        self.dpi = spec.dpi;
        Ok(())
    }

    pub(super) fn render(&mut self, data: &OverlayRenderData, spec: SurfaceSpec) -> Result<()> {
        if let Some(blur_amount) = data.blur.blur_amount() {
            if (self.scene.blur_amount - blur_amount).abs() > f32::EPSILON {
                let effect_brush = create_backdrop_brush(&self._compositor, blur_amount)?;
                self.scene
                    .backdrop_visual
                    .SetBrush(&effect_brush)
                    .map_err(|e| Error::win("SetBackdropBrush(overlay)", &e))?;
                self.scene._effect_brush = effect_brush;
                self.scene.blur_amount = blur_amount;
            }
        }
        let blurred =
            spec.blur_enabled && data.blur.blur_amount().is_some() && !data.palette.opaque;
        self.scene
            .backdrop_visual
            .SetIsVisible(blurred)
            .map_err(|e| Error::win("SetBackdropVisibility(overlay)", &e))?;
        self.scene
            .tint_visual
            .SetIsVisible(blurred)
            .map_err(|e| Error::win("SetTintVisibility(overlay)", &e))?;
        self.scene
            .backdrop_visual
            .SetOpacity(data.alpha.clamp(0.0, 1.0))
            .map_err(|e| Error::win("SetBackdropOpacity(overlay)", &e))?;
        self.scene
            .tint_visual
            .SetOpacity(data.alpha.clamp(0.0, 1.0))
            .map_err(|e| Error::win("SetTintOpacity(overlay)", &e))?;
        let tint_alpha = composition_tint_alpha(data.theme_mode, data.blur);
        self.scene
            .tint_brush
            .SetColor(WinRtColor {
                A: tint_alpha,
                R: data.palette.surface.r,
                G: data.palette.surface.g,
                B: data.palette.surface.b,
            })
            .map_err(|e| Error::win("SetTintColor(overlay)", &e))?;

        let surface_interop: ICompositionDrawingSurfaceInterop = self
            .surface
            .cast()
            .map_err(|e| Error::win("CastCompositionSurface(overlay)", &e))?;
        let mut draw_offset = POINT::default();
        let drawing_context: ID2D1DeviceContext = unsafe {
            surface_interop
                .BeginDraw(None, &mut draw_offset)
                .map_err(|e| Error::win("BeginCompositionDraw(overlay)", &e))?
        };
        let transform = windows_numerics::Matrix3x2 {
            M11: 1.0,
            M12: 0.0,
            M21: 0.0,
            M22: 1.0,
            M31: draw_offset.x as f32,
            M32: draw_offset.y as f32,
        };
        unsafe {
            drawing_context.SetDpi(spec.dpi as f32, spec.dpi as f32);
            drawing_context.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            drawing_context.SetTransform(&transform);
        }
        unsafe {
            let transparent = Color::rgba(0, 0, 0, 0).d2d();
            drawing_context.Clear(Some(&transparent));
        }
        let draw_result = draw_overlay(
            &drawing_context,
            &data.dwrite,
            &data.model,
            data.scale,
            data.palette,
            data.alpha,
            data.palette.opaque,
        );
        let end_result = unsafe {
            surface_interop
                .EndDraw()
                .map_err(|e| Error::win("EndCompositionDraw(overlay)", &e))
        };
        draw_result?;
        end_result
    }
}
