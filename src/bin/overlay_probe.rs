#[cfg(not(windows))]
fn main() {
    eprintln!("overlay-probe is Windows-only");
}

#[cfg(windows)]
mod probe {
    #![allow(non_camel_case_types, non_snake_case)]
    use std::ffi::c_void;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::ptr::null_mut;

    use windows::core::{implement, w, Interface, GUID, HSTRING, PCWSTR};
    use windows::Foundation::{IPropertyValue, PropertyValue, Size};
    use windows::Graphics::DirectX::{DirectXAlphaMode, DirectXPixelFormat};
    use windows::Graphics::Effects::{
        IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectSource_Impl, IGraphicsEffect_Impl,
    };
    use windows::System::DispatcherQueueController;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Direct2D::Common::{
        D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
    };
    use windows::Win32::Graphics::Direct2D::{
        D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, ID2D1HwndRenderTarget,
        ID2D1RenderTarget, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_CLIP,
        D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED,
        D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
        D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_ROUNDED_RECT,
    };
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_SDK_VERSION,
    };
    use windows::Win32::Graphics::DirectWrite::{
        DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED,
        DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
        DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
        DWRITE_WORD_WRAPPING_NO_WRAP,
    };
    use windows::Win32::Graphics::Dwm::{
        DwmExtendFrameIntoClientArea, DwmFlush, DwmSetWindowAttribute, DWMSBT_TRANSIENTWINDOW,
        DWMWA_SYSTEMBACKDROP_TYPE,
    };
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateRoundRectRgn, DeleteDC,
        DeleteObject, EndPaint, GetDC, ReleaseDC, SelectObject, SetWindowRgn, UpdateWindow,
        BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HGDIOBJ, PAINTSTRUCT, ROP_CODE, SRCCOPY,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::WinRT::Composition::{
        ICompositionDrawingSurfaceInterop, ICompositorDesktopInterop, ICompositorInterop,
    };
    use windows::Win32::System::WinRT::{
        CreateDispatcherQueueController, DispatcherQueueOptions, DQTAT_COM_ASTA,
        DQTYPE_THREAD_CURRENT,
    };
    use windows::Win32::UI::Controls::MARGINS;
    use windows::Win32::UI::HiDpi::{
        GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetMessageW, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, PostMessageW,
        RegisterClassExW, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
        CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HWND_TOPMOST, MSG, SM_CXSCREEN, SM_CYSCREEN,
        SWP_NOACTIVATE, SW_SHOWNOACTIVATE, WM_APP, WM_ERASEBKGND, WM_NCCREATE, WM_NCDESTROY,
        WM_PAINT, WM_SIZE, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };
    use windows::UI::Color;
    use windows::UI::Composition::{
        CompositionDrawingSurface, CompositionEffectSourceParameter, CompositionGraphicsDevice,
        CompositionSurfaceBrush, Compositor, ContainerVisual, Desktop::DesktopWindowTarget,
    };
    use windows_numerics::Vector2;

    const CLASS_NAME: PCWSTR = w!("WinShort.OverlayProbe");
    const WIDTH_DIP: f32 = 372.0;
    const HEIGHT_DIP: f32 = 94.0;
    const RADIUS_DIP: f32 = 14.0;
    const CAPTURE_MARGIN_PX: i32 = 56;
    const WM_PROBE_READY: u32 = WM_APP + 1;

    type ProbeResult<T> = Result<T, Box<dyn std::error::Error>>;

    windows::core::imp::define_interface!(
        IGraphicsEffectD2D1Interop,
        IGraphicsEffectD2D1Interop_Vtbl,
        0x2fc57384_a068_44d7_a331_30982fcf7177
    );
    windows::core::imp::interface_hierarchy!(IGraphicsEffectD2D1Interop, windows::core::IUnknown);
    impl windows::core::RuntimeName for IGraphicsEffectD2D1Interop {}

    #[repr(C)]
    #[doc(hidden)]
    pub struct IGraphicsEffectD2D1Interop_Vtbl {
        pub base__: windows::core::IUnknown_Vtbl,
        pub GetEffectId:
            unsafe extern "system" fn(*mut c_void, *mut GUID) -> windows::core::HRESULT,
        pub GetNamedPropertyMapping: unsafe extern "system" fn(
            *mut c_void,
            PCWSTR,
            *mut u32,
            *mut i32,
        ) -> windows::core::HRESULT,
        pub GetPropertyCount:
            unsafe extern "system" fn(*mut c_void, *mut u32) -> windows::core::HRESULT,
        pub GetProperty:
            unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> windows::core::HRESULT,
        pub GetSource:
            unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> windows::core::HRESULT,
        pub GetSourceCount:
            unsafe extern "system" fn(*mut c_void, *mut u32) -> windows::core::HRESULT,
    }

    pub trait IGraphicsEffectD2D1Interop_Impl: windows::core::IUnknownImpl {
        fn GetEffectId(&self, id: *mut GUID) -> windows::core::Result<()>;
        fn GetNamedPropertyMapping(
            &self,
            name: PCWSTR,
            index: *mut u32,
            mapping: *mut i32,
        ) -> windows::core::Result<()>;
        fn GetPropertyCount(&self, count: *mut u32) -> windows::core::Result<()>;
        fn GetProperty(&self, index: u32, value: *mut *mut c_void) -> windows::core::Result<()>;
        fn GetSource(&self, index: u32, source: *mut *mut c_void) -> windows::core::Result<()>;
        fn GetSourceCount(&self, count: *mut u32) -> windows::core::Result<()>;
    }

    impl IGraphicsEffectD2D1Interop_Vtbl {
        pub const fn new<Identity: IGraphicsEffectD2D1Interop_Impl, const OFFSET: isize>() -> Self {
            unsafe extern "system" fn get_effect_id<
                Identity: IGraphicsEffectD2D1Interop_Impl,
                const OFFSET: isize,
            >(
                this: *mut c_void,
                id: *mut GUID,
            ) -> windows::core::HRESULT {
                unsafe {
                    let this: &Identity =
                        &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                    IGraphicsEffectD2D1Interop_Impl::GetEffectId(this, id).into()
                }
            }
            unsafe extern "system" fn get_named_property_mapping<
                Identity: IGraphicsEffectD2D1Interop_Impl,
                const OFFSET: isize,
            >(
                this: *mut c_void,
                name: PCWSTR,
                index: *mut u32,
                mapping: *mut i32,
            ) -> windows::core::HRESULT {
                unsafe {
                    let this: &Identity =
                        &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                    IGraphicsEffectD2D1Interop_Impl::GetNamedPropertyMapping(
                        this, name, index, mapping,
                    )
                    .into()
                }
            }
            unsafe extern "system" fn get_property_count<
                Identity: IGraphicsEffectD2D1Interop_Impl,
                const OFFSET: isize,
            >(
                this: *mut c_void,
                count: *mut u32,
            ) -> windows::core::HRESULT {
                unsafe {
                    let this: &Identity =
                        &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                    IGraphicsEffectD2D1Interop_Impl::GetPropertyCount(this, count).into()
                }
            }
            unsafe extern "system" fn get_property<
                Identity: IGraphicsEffectD2D1Interop_Impl,
                const OFFSET: isize,
            >(
                this: *mut c_void,
                index: u32,
                value: *mut *mut c_void,
            ) -> windows::core::HRESULT {
                unsafe {
                    let this: &Identity =
                        &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                    IGraphicsEffectD2D1Interop_Impl::GetProperty(this, index, value).into()
                }
            }
            unsafe extern "system" fn get_source<
                Identity: IGraphicsEffectD2D1Interop_Impl,
                const OFFSET: isize,
            >(
                this: *mut c_void,
                index: u32,
                source: *mut *mut c_void,
            ) -> windows::core::HRESULT {
                unsafe {
                    let this: &Identity =
                        &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                    IGraphicsEffectD2D1Interop_Impl::GetSource(this, index, source).into()
                }
            }
            unsafe extern "system" fn get_source_count<
                Identity: IGraphicsEffectD2D1Interop_Impl,
                const OFFSET: isize,
            >(
                this: *mut c_void,
                count: *mut u32,
            ) -> windows::core::HRESULT {
                unsafe {
                    let this: &Identity =
                        &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                    IGraphicsEffectD2D1Interop_Impl::GetSourceCount(this, count).into()
                }
            }
            Self {
                base__: windows::core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
                GetEffectId: get_effect_id::<Identity, OFFSET>,
                GetNamedPropertyMapping: get_named_property_mapping::<Identity, OFFSET>,
                GetPropertyCount: get_property_count::<Identity, OFFSET>,
                GetProperty: get_property::<Identity, OFFSET>,
                GetSource: get_source::<Identity, OFFSET>,
                GetSourceCount: get_source_count::<Identity, OFFSET>,
            }
        }

        pub fn matches(iid: &GUID) -> bool {
            iid == &<IGraphicsEffectD2D1Interop as windows::core::Interface>::IID
        }
    }

    const CLSID_D2D1_GAUSSIAN_BLUR: GUID = GUID::from_u128(0x1feb6d69_2fe6_4ac9_8c58_1d7f93e7a6a5);
    const E_INVALIDARG_HRESULT: windows::core::HRESULT =
        windows::core::HRESULT(0x80070057u32 as i32);
    const E_POINTER_HRESULT: windows::core::HRESULT = windows::core::HRESULT(0x80004003u32 as i32);

    fn invalid_argument<T>() -> windows::core::Result<T> {
        Err(windows::core::Error::from_hresult(E_INVALIDARG_HRESULT))
    }

    fn null_pointer<T>() -> windows::core::Result<T> {
        Err(windows::core::Error::from_hresult(E_POINTER_HRESULT))
    }

    #[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
    struct GaussianBlurEffectGraph {
        source: IGraphicsEffectSource,
    }

    impl IGraphicsEffectSource_Impl for GaussianBlurEffectGraph_Impl {}

    impl IGraphicsEffect_Impl for GaussianBlurEffectGraph_Impl {
        fn Name(&self) -> windows::core::Result<HSTRING> {
            Ok(HSTRING::from("GaussianBlur"))
        }

        fn SetName(&self, _name: &HSTRING) -> windows::core::Result<()> {
            Ok(())
        }
    }

    impl IGraphicsEffectD2D1Interop_Impl for GaussianBlurEffectGraph_Impl {
        fn GetEffectId(&self, id: *mut GUID) -> windows::core::Result<()> {
            if id.is_null() {
                return null_pointer();
            }
            unsafe { *id = CLSID_D2D1_GAUSSIAN_BLUR };
            Ok(())
        }

        fn GetNamedPropertyMapping(
            &self,
            name: PCWSTR,
            index: *mut u32,
            mapping: *mut i32,
        ) -> windows::core::Result<()> {
            if name.0.is_null() || index.is_null() || mapping.is_null() {
                return null_pointer();
            }
            let name = unsafe { name.to_string()? };
            let property_index = match name.as_str() {
                "BlurAmount" => 0,
                "Optimization" => 1,
                "BorderMode" => 2,
                _ => return invalid_argument(),
            };
            unsafe {
                *index = property_index;
                *mapping = 1;
            }
            Ok(())
        }

        fn GetPropertyCount(&self, count: *mut u32) -> windows::core::Result<()> {
            if count.is_null() {
                return null_pointer();
            }
            unsafe { *count = 3 };
            Ok(())
        }

        fn GetProperty(&self, index: u32, value: *mut *mut c_void) -> windows::core::Result<()> {
            if value.is_null() {
                return null_pointer();
            }
            let property: IPropertyValue = match index {
                0 => PropertyValue::CreateSingle(18.0)?.cast()?,
                1 | 2 => PropertyValue::CreateUInt32(1)?.cast()?,
                _ => return invalid_argument(),
            };
            unsafe { *value = property.into_raw() };
            Ok(())
        }

        fn GetSource(&self, index: u32, source: *mut *mut c_void) -> windows::core::Result<()> {
            if source.is_null() {
                return null_pointer();
            }
            if index != 0 {
                return invalid_argument();
            }
            unsafe { *source = self.source.clone().into_raw() };
            Ok(())
        }

        fn GetSourceCount(&self, count: *mut u32) -> windows::core::Result<()> {
            if count.is_null() {
                return null_pointer();
            }
            unsafe { *count = 1 };
            Ok(())
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Mode {
        D2d,
        Dwm,
        Composition,
    }

    impl Mode {
        fn parse(value: Option<&str>) -> ProbeResult<Self> {
            match value {
                Some("d2d") => Ok(Self::D2d),
                Some("dwm") => Ok(Self::Dwm),
                Some("composition") => Ok(Self::Composition),
                Some(other) => {
                    Err(format!("unknown mode {other:?}; expected d2d, dwm, or composition").into())
                }
                None => Err("missing mode; expected d2d, dwm, or composition".into()),
            }
        }

        fn name(self) -> &'static str {
            match self {
                Self::D2d => "d2d",
                Self::Dwm => "dwm",
                Self::Composition => "composition",
            }
        }
    }

    struct ComApartment;

    impl ComApartment {
        fn initialize() -> ProbeResult<Self> {
            let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if result.0 < 0 {
                return Err(
                    format!("CoInitializeEx failed: HRESULT 0x{:08x}", result.0 as u32).into(),
                );
            }
            Ok(Self)
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    #[derive(Clone, Copy)]
    struct ApiResult {
        called: bool,
        ok: bool,
        hresult: Option<i32>,
        value: Option<i32>,
    }

    impl ApiResult {
        fn not_called() -> Self {
            Self {
                called: false,
                ok: false,
                hresult: None,
                value: None,
            }
        }

        fn hresult(result: windows::core::Result<()>) -> Self {
            match result {
                Ok(()) => Self {
                    called: true,
                    ok: true,
                    hresult: Some(0),
                    value: None,
                },
                Err(error) => Self {
                    called: true,
                    ok: false,
                    hresult: Some(error.code().0),
                    value: None,
                },
            }
        }

        fn value(value: i32) -> Self {
            Self {
                called: true,
                ok: value != 0,
                hresult: None,
                value: Some(value),
            }
        }

        fn json(self) -> String {
            format!(
                "{{\"called\":{},\"ok\":{},\"hresult\":{},\"value\":{}}}",
                self.called,
                self.ok,
                optional_i32(self.hresult),
                optional_i32(self.value),
            )
        }
    }

    struct Diagnostics {
        mode: Mode,
        hwnd_rect: RECT,
        dpi: u32,
        backdrop_enabled: bool,
        composition_enabled: bool,
        backdrop_attribute: ApiResult,
        extend_frame: ApiResult,
        window_region: ApiResult,
        capture_rect: RECT,
    }

    impl Diagnostics {
        fn json(&self) -> String {
            format!(
                concat!(
                    "{{\n",
                    "  \"mode\":\"{}\",\n",
                    "  \"hwnd_rect\":{},\n",
                    "  \"dpi\":{},\n",
                    "  \"dwm_backdrop_enabled\":\"{}\",\n",
                    "  \"composition_enabled\":\"{}\",\n",
                    "  \"dwm_set_window_attribute_result\":{},\n",
                    "  \"dwm_extend_frame_into_client_area_result\":{},\n",
                    "  \"set_window_rgn_result\":{},\n",
                    "  \"capture_rect\":{}\n",
                    "}}\n"
                ),
                self.mode.name(),
                rect_json(self.hwnd_rect),
                self.dpi,
                if self.backdrop_enabled { "yes" } else { "no" },
                if self.composition_enabled {
                    "yes"
                } else {
                    "no"
                },
                self.backdrop_attribute.json(),
                self.extend_frame.json(),
                self.window_region.json(),
                rect_json(self.capture_rect),
            )
        }
    }

    struct Graphics {
        factory: ID2D1Factory1,
        dwrite: IDWriteFactory,
    }

    impl Graphics {
        fn create() -> ProbeResult<Self> {
            unsafe {
                let factory = D2D1CreateFactory::<ID2D1Factory1>(
                    D2D1_FACTORY_TYPE_SINGLE_THREADED,
                    Some(&D2D1_FACTORY_OPTIONS::default()),
                )
                .map_err(|error| format!("D2D1CreateFactory: {error}"))?;
                let dwrite = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                    .map_err(|error| format!("DWriteCreateFactory: {error}"))?;
                Ok(Self { factory, dwrite })
            }
        }

        fn create_target(
            &self,
            hwnd: HWND,
            dpi: u32,
            width: i32,
            height: i32,
        ) -> ProbeResult<ID2D1HwndRenderTarget> {
            let properties = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi as f32,
                dpiY: dpi as f32,
                ..Default::default()
            };
            let hwnd_properties = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U {
                    width: width.max(1) as u32,
                    height: height.max(1) as u32,
                },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            unsafe {
                let target = self
                    .factory
                    .CreateHwndRenderTarget(&properties, &hwnd_properties)
                    .map_err(|error| format!("CreateHwndRenderTarget: {error}"))?;
                target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                target.SetDpi(dpi as f32, dpi as f32);
                Ok(target)
            }
        }
    }
    struct CompositionLayer {
        _dispatcher: DispatcherQueueController,
        _compositor: Compositor,
        _target: DesktopWindowTarget,
        _root: ContainerVisual,
        _graphics_device: CompositionGraphicsDevice,
        _surface: CompositionDrawingSurface,
        _content_brush: CompositionSurfaceBrush,
        _d3d_device: ID3D11Device,
        _d3d_context: ID3D11DeviceContext,
        _d2d_device: ID2D1Device,
    }

    impl CompositionLayer {
        fn create(hwnd: HWND, width: i32, height: i32, graphics: &Graphics) -> ProbeResult<Self> {
            let dispatcher_options = DispatcherQueueOptions {
                dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_ASTA,
            };
            let dispatcher = unsafe { CreateDispatcherQueueController(dispatcher_options) }
                .map_err(|error| format!("CreateDispatcherQueueController: {error}"))?;
            let compositor =
                Compositor::new().map_err(|error| format!("Compositor::new: {error}"))?;
            let desktop: ICompositorDesktopInterop = compositor
                .cast()
                .map_err(|error| format!("Compositor desktop interop: {error}"))?;
            let target = unsafe { desktop.CreateDesktopWindowTarget(hwnd, false) }
                .map_err(|error| format!("CreateDesktopWindowTarget: {error}"))?;
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
                .map_err(|error| format!("D3D11CreateDevice: {error}"))?;
            }
            let d3d_device = d3d_device.ok_or("D3D11CreateDevice returned no device")?;
            let d3d_context = d3d_context.ok_or("D3D11CreateDevice returned no context")?;
            let dxgi_device: IDXGIDevice = d3d_device
                .cast()
                .map_err(|error| format!("Cast IDXGIDevice: {error}"))?;
            let d2d_device = unsafe {
                graphics
                    .factory
                    .CreateDevice(&dxgi_device)
                    .map_err(|error| format!("Create D2D device: {error}"))?
            };
            let compositor_interop: ICompositorInterop = compositor
                .cast()
                .map_err(|error| format!("Cast compositor interop: {error}"))?;
            let composition_graphics_device = unsafe {
                compositor_interop
                    .CreateGraphicsDevice(&d2d_device)
                    .map_err(|error| format!("Create composition graphics device: {error}"))?
            };
            let surface = composition_graphics_device
                .CreateDrawingSurface(
                    Size {
                        Width: width as f32,
                        Height: height as f32,
                    },
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    DirectXAlphaMode::Premultiplied,
                )
                .map_err(|error| format!("Create composition drawing surface: {error}"))?;
            let surface_interop: ICompositionDrawingSurfaceInterop = surface
                .cast()
                .map_err(|error| format!("Cast drawing surface interop: {error}"))?;
            let mut draw_offset = POINT::default();
            let drawing_context: ID2D1DeviceContext = unsafe {
                surface_interop
                    .BeginDraw(None, &mut draw_offset)
                    .map_err(|error| format!("Begin composition surface draw: {error}"))?
            };
            let transform = windows_numerics::Matrix3x2 {
                M11: 1.0,
                M12: 0.0,
                M21: 0.0,
                M22: 1.0,
                M31: draw_offset.x as f32,
                M32: draw_offset.y as f32,
            };
            unsafe { drawing_context.SetTransform(&transform) };
            let draw_result = draw_content_body(
                &drawing_context,
                &graphics.dwrite,
                width as f32,
                height as f32,
                false,
            );
            let end_result = unsafe {
                surface_interop
                    .EndDraw()
                    .map_err(|error| format!("End composition surface draw: {error}"))
            };
            draw_result?;
            end_result?;
            let content_brush = compositor
                .CreateSurfaceBrushWithSurface(&surface)
                .map_err(|error| format!("Create content surface brush: {error}"))?;

            let size = Vector2 {
                X: width as f32,
                Y: height as f32,
            };
            let root = compositor
                .CreateContainerVisual()
                .map_err(|error| format!("CreateContainerVisual: {error}"))?;
            root.SetSize(size)
                .map_err(|error| format!("SetRootSize: {error}"))?;

            let source_name = HSTRING::from("source");
            let source_parameter = CompositionEffectSourceParameter::Create(&source_name)
                .map_err(|error| format!("CreateEffectSourceParameter: {error}"))?;
            let source: IGraphicsEffectSource = source_parameter
                .cast()
                .map_err(|error| format!("CastEffectSourceParameter: {error}"))?;
            let effect: IGraphicsEffect = GaussianBlurEffectGraph { source }.into();
            let effect_factory = compositor
                .CreateEffectFactory(&effect)
                .map_err(|error| format!("CreateEffectFactory: {error}"))?;
            let effect_brush = effect_factory
                .CreateBrush()
                .map_err(|error| format!("CreateEffectBrush: {error}"))?;
            let backdrop = compositor
                .CreateBackdropBrush()
                .map_err(|error| format!("CreateBackdropBrush: {error}"))?;
            effect_brush
                .SetSourceParameter(&source_name, &backdrop)
                .map_err(|error| format!("SetBackdropSource: {error}"))?;

            let geometry = compositor
                .CreateRoundedRectangleGeometry()
                .map_err(|error| format!("CreateRoundedRectangleGeometry: {error}"))?;
            geometry
                .SetSize(size)
                .map_err(|error| format!("SetShapeSize: {error}"))?;
            geometry
                .SetCornerRadius(Vector2 {
                    X: RADIUS_DIP,
                    Y: RADIUS_DIP,
                })
                .map_err(|error| format!("SetShapeRadius: {error}"))?;
            let clip = compositor
                .CreateGeometricClipWithGeometry(&geometry)
                .map_err(|error| format!("CreateGeometricClip: {error}"))?;

            let backdrop_visual = compositor
                .CreateSpriteVisual()
                .map_err(|error| format!("CreateBackdropVisual: {error}"))?;
            backdrop_visual
                .SetSize(size)
                .map_err(|error| format!("SetBackdropSize: {error}"))?;
            backdrop_visual
                .SetBrush(&effect_brush)
                .map_err(|error| format!("SetBackdropBrush: {error}"))?;
            backdrop_visual
                .SetClip(&clip)
                .map_err(|error| format!("SetBackdropClip: {error}"))?;

            let tint_brush = compositor
                .CreateColorBrushWithColor(Color {
                    A: 42,
                    R: 20,
                    G: 30,
                    B: 44,
                })
                .map_err(|error| format!("CreateTintBrush: {error}"))?;
            let tint_visual = compositor
                .CreateSpriteVisual()
                .map_err(|error| format!("CreateTintVisual: {error}"))?;
            tint_visual
                .SetSize(size)
                .map_err(|error| format!("SetTintSize: {error}"))?;
            tint_visual
                .SetBrush(&tint_brush)
                .map_err(|error| format!("SetTintBrush: {error}"))?;
            tint_visual
                .SetClip(&clip)
                .map_err(|error| format!("SetTintClip: {error}"))?;

            let content_visual = compositor
                .CreateSpriteVisual()
                .map_err(|error| format!("CreateContentVisual: {error}"))?;
            content_visual
                .SetSize(size)
                .map_err(|error| format!("SetContentSize: {error}"))?;
            content_visual
                .SetBrush(&content_brush)
                .map_err(|error| format!("SetContentBrush: {error}"))?;
            content_visual
                .SetClip(&clip)
                .map_err(|error| format!("SetContentClip: {error}"))?;

            let children = root
                .Children()
                .map_err(|error| format!("GetRootChildren: {error}"))?;
            children
                .InsertAtBottom(&backdrop_visual)
                .map_err(|error| format!("InsertBackdropVisual: {error}"))?;
            children
                .InsertAtTop(&tint_visual)
                .map_err(|error| format!("InsertTintVisual: {error}"))?;
            children
                .InsertAtTop(&content_visual)
                .map_err(|error| format!("InsertContentVisual: {error}"))?;
            target
                .SetRoot(&root)
                .map_err(|error| format!("SetCompositionRoot: {error}"))?;

            Ok(Self {
                _dispatcher: dispatcher,
                _compositor: compositor,
                _target: target,
                _root: root,
                _graphics_device: composition_graphics_device,
                _surface: surface,
                _content_brush: content_brush,
                _d3d_device: d3d_device,
                _d3d_context: d3d_context,
                _d2d_device: d2d_device,
            })
        }
    }

    struct WindowState {
        mode: Mode,
        dwrite: IDWriteFactory,
        target: Option<ID2D1HwndRenderTarget>,
        _composition: Option<CompositionLayer>,
        width: f32,
        height: f32,
    }
    fn draw_content(
        target: &ID2D1RenderTarget,
        dwrite: &IDWriteFactory,
        width: f32,
        height: f32,
        fill_card: bool,
    ) -> ProbeResult<()> {
        unsafe { target.BeginDraw() };
        let draw_result = draw_content_body(target, dwrite, width, height, fill_card);
        let end_result = unsafe {
            target
                .EndDraw(None, None)
                .map_err(|error| format!("EndDraw: {error}"))
        };
        draw_result?;
        end_result?;
        Ok(())
    }

    fn draw_content_body(
        target: &ID2D1RenderTarget,
        dwrite: &IDWriteFactory,
        width: f32,
        height: f32,
        fill_card: bool,
    ) -> ProbeResult<()> {
        unsafe {
            let transparent = D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            target.Clear(Some(&transparent));

            if fill_card {
                let card_color = solid(0.055, 0.075, 0.105, 0.97);
                let card = target.CreateSolidColorBrush(&card_color, None)?;
                target.FillRoundedRectangle(&rounded(0.0, 0.0, width, height), &card);
            }

            let border_color = solid(0.52, 0.60, 0.70, 0.62);
            let border = target.CreateSolidColorBrush(&border_color, None)?;
            target.DrawRoundedRectangle(
                &rounded(0.5, 0.5, width - 0.5, height - 0.5),
                &border,
                1.0,
                None,
            );

            let accent_color = solid(0.18, 0.52, 0.94, 0.98);
            let accent = target.CreateSolidColorBrush(&accent_color, None)?;
            target.FillEllipse(
                &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                    point: Vector2 { X: 42.0, Y: 47.0 },
                    radiusX: 22.0,
                    radiusY: 22.0,
                },
                &accent,
            );
            let icon_color = solid(1.0, 1.0, 1.0, 0.96);
            let icon = target.CreateSolidColorBrush(&icon_color, None)?;
            target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 36.0,
                        top: 35.0,
                        right: 48.0,
                        bottom: 53.0,
                    },
                    radiusX: 6.0,
                    radiusY: 6.0,
                },
                &icon,
                2.0,
                None,
            );
            target.DrawLine(
                Vector2 { X: 33.0, Y: 49.0 },
                Vector2 { X: 33.0, Y: 53.0 },
                &icon,
                2.0,
                None,
            );
            target.DrawLine(
                Vector2 { X: 33.0, Y: 53.0 },
                Vector2 { X: 51.0, Y: 53.0 },
                &icon,
                2.0,
                None,
            );
            target.DrawLine(
                Vector2 { X: 42.0, Y: 53.0 },
                Vector2 { X: 42.0, Y: 58.0 },
                &icon,
                2.0,
                None,
            );

            let title_format = make_format(dwrite, 15.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let detail_format = make_format(dwrite, 12.5, DWRITE_FONT_WEIGHT_NORMAL)?;
            let title_color = solid(0.96, 0.98, 1.0, 0.98);
            let title = target.CreateSolidColorBrush(&title_color, None)?;
            let detail_color = solid(0.73, 0.80, 0.88, 0.96);
            let detail = target.CreateSolidColorBrush(&detail_color, None)?;
            draw_text(
                target,
                "Microphone",
                &title_format,
                D2D_RECT_F {
                    left: 78.0,
                    top: 22.0,
                    right: width - 20.0,
                    bottom: 50.0,
                },
                &title,
            );
            draw_text(
                target,
                "Ready · 50% input volume",
                &detail_format,
                D2D_RECT_F {
                    left: 78.0,
                    top: 50.0,
                    right: width - 20.0,
                    bottom: 76.0,
                },
                &detail,
            );
        }
        Ok(())
    }

    impl WindowState {
        fn render(&self) -> ProbeResult<()> {
            let Some(target) = self.target.as_ref() else {
                return Ok(());
            };
            draw_content(
                target,
                &self.dwrite,
                self.width,
                self.height,
                self.mode == Mode::D2d,
            )
        }

        fn resize(&mut self, hwnd: HWND) -> ProbeResult<()> {
            let Some(target) = self.target.as_ref() else {
                return Ok(());
            };
            let mut rect = RECT::default();
            unsafe {
                GetClientRect(hwnd, &mut rect)
                    .map_err(|error| format!("GetClientRect: {error}"))?;
            }
            let width = (rect.right - rect.left).max(1) as u32;
            let height = (rect.bottom - rect.top).max(1) as u32;
            unsafe {
                target
                    .Resize(&D2D_SIZE_U { width, height })
                    .map_err(|error| format!("Resize: {error}"))?;
            }
            Ok(())
        }
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            if message == WM_NCCREATE {
                let create =
                    &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
                #[cfg(target_pointer_width = "64")]
                let state_value = create.lpCreateParams as isize;
                #[cfg(target_pointer_width = "32")]
                let state_value = create.lpCreateParams as i32;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_value);
                return DefWindowProcW(hwnd, message, wparam, lparam);
            }
            if message == WM_NCDESTROY {
                let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                if !state.is_null() {
                    drop(Box::from_raw(state));
                }
                return DefWindowProcW(hwnd, message, wparam, lparam);
            }

            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            match message {
                WM_PAINT => {
                    let mut paint = PAINTSTRUCT::default();
                    BeginPaint(hwnd, &mut paint);
                    if !state.is_null() {
                        if let Err(error) = (*state).render() {
                            eprintln!("overlay-probe paint: {error}");
                        }
                    }
                    let _ = EndPaint(hwnd, &paint);
                    LRESULT(0)
                }
                WM_SIZE => {
                    if !state.is_null() {
                        if let Err(error) = (*state).resize(hwnd) {
                            eprintln!("overlay-probe resize: {error}");
                        }
                    }
                    LRESULT(0)
                }
                WM_ERASEBKGND => LRESULT(1),
                _ => DefWindowProcW(hwnd, message, wparam, lparam),
            }
        }
    }

    fn register_class() -> ProbeResult<()> {
        let instance = unsafe { GetModuleHandleW(None) }
            .map_err(|error| format!("GetModuleHandleW: {error}"))?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        let atom = unsafe { RegisterClassExW(&class) };
        if atom == 0 {
            return Err(format!("RegisterClassExW failed: {}", unsafe {
                windows::Win32::Foundation::GetLastError().0
            })
            .into());
        }
        Ok(())
    }

    fn create_window(mode: Mode, graphics: &Graphics) -> ProbeResult<(HWND, Diagnostics)> {
        register_class()?;
        let state = Box::new(WindowState {
            mode,
            dwrite: graphics.dwrite.clone(),
            target: None,
            _composition: None,
            width: WIDTH_DIP,
            height: HEIGHT_DIP,
        });
        let state_ptr = Box::into_raw(state);
        let extended_style = if mode == Mode::Composition {
            WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE
                | WS_EX_TRANSPARENT
                | WS_EX_NOREDIRECTIONBITMAP
        } else {
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT
        };
        let hwnd = match unsafe {
            CreateWindowExW(
                extended_style,
                CLASS_NAME,
                w!("Overlay probe"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(
                    GetModuleHandleW(None)
                        .map_err(|error| format!("GetModuleHandleW: {error}"))?
                        .into(),
                ),
                Some(state_ptr.cast::<c_void>()),
            )
        } {
            Ok(hwnd) => hwnd,
            Err(error) => {
                unsafe { drop(Box::from_raw(state_ptr)) };
                return Err(format!("CreateWindowExW: {error}").into());
            }
        };

        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let width = dip_to_px(WIDTH_DIP, dpi);
        let height = dip_to_px(HEIGHT_DIP, dpi);
        let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let x = (screen_width - width) / 2;
        let y = (screen_height - height) / 2;
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE,
            )
            .map_err(|error| format!("SetWindowPos: {error}"))?;
        }

        let target = if mode == Mode::Composition {
            None
        } else {
            Some(match graphics.create_target(hwnd, dpi, width, height) {
                Ok(target) => target,
                Err(error) => {
                    unsafe { DestroyWindow(hwnd).ok() };
                    return Err(error);
                }
            })
        };
        unsafe { (*state_ptr).target = target };
        let composition = if mode == Mode::Composition {
            match CompositionLayer::create(hwnd, width, height, graphics) {
                Ok(layer) => Some(layer),
                Err(error) => {
                    unsafe { DestroyWindow(hwnd).ok() };
                    return Err(error);
                }
            }
        } else {
            None
        };
        unsafe { (*state_ptr)._composition = composition };

        let backdrop_attribute = if mode == Mode::Dwm {
            let backdrop = DWMSBT_TRANSIENTWINDOW;
            ApiResult::hresult(unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_SYSTEMBACKDROP_TYPE,
                    std::ptr::from_ref(&backdrop).cast(),
                    std::mem::size_of_val(&backdrop) as u32,
                )
            })
        } else {
            ApiResult::not_called()
        };
        let extend_frame = if mode == Mode::Dwm {
            let margins = MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            };
            ApiResult::hresult(unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) })
        } else {
            ApiResult::not_called()
        };

        let region = unsafe {
            CreateRoundRectRgn(
                0,
                0,
                width,
                height,
                dip_to_px(RADIUS_DIP * 2.0, dpi),
                dip_to_px(RADIUS_DIP * 2.0, dpi),
            )
        };
        if region.is_invalid() {
            unsafe { DestroyWindow(hwnd).ok() };
            return Err("CreateRoundRectRgn failed".into());
        }
        let region_result = unsafe { SetWindowRgn(hwnd, Some(region), true) };
        let window_region = ApiResult::value(region_result);
        if !window_region.ok {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(region.0)).ok();
            }
        }

        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = UpdateWindow(hwnd).ok();
            let _ = DwmFlush().ok();
            if mode == Mode::Composition {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }

        let mut hwnd_rect = RECT::default();
        unsafe {
            GetWindowRect(hwnd, &mut hwnd_rect)
                .map_err(|error| format!("GetWindowRect: {error}"))?;
        }
        let capture_rect = RECT {
            left: hwnd_rect.left - CAPTURE_MARGIN_PX,
            top: hwnd_rect.top - CAPTURE_MARGIN_PX,
            right: hwnd_rect.right + CAPTURE_MARGIN_PX,
            bottom: hwnd_rect.bottom + CAPTURE_MARGIN_PX,
        };
        let diagnostics = Diagnostics {
            mode,
            hwnd_rect,
            dpi,
            backdrop_enabled: mode == Mode::Dwm && backdrop_attribute.ok && extend_frame.ok,
            composition_enabled: mode == Mode::Composition,
            backdrop_attribute,
            extend_frame,
            window_region,
            capture_rect,
        };
        Ok((hwnd, diagnostics))
    }
    fn pump_composition(hwnd: HWND) -> ProbeResult<()> {
        let hwnd_value = hwnd.0 as isize;
        let notifier = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(350));
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd_value as *mut c_void)),
                    WM_PROBE_READY,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        });
        loop {
            let mut message = MSG::default();
            let status = unsafe { GetMessageW(&mut message, None, 0, 0) };
            if status.0 < 0 {
                let _ = notifier.join();
                return Err("GetMessageW failed".into());
            }
            if status.0 == 0 || message.message == WM_PROBE_READY {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        notifier
            .join()
            .map_err(|_| "composition wait thread panicked".into())
    }

    struct ScreenCapture {
        width: u32,
        height: u32,
        bgra: Vec<u8>,
    }

    fn copy_from_screen(rect: RECT) -> ProbeResult<ScreenCapture> {
        let width = (rect.right - rect.left).max(1) as u32;
        let height = (rect.bottom - rect.top).max(1) as u32;
        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return Err("GetDC(screen) failed".into());
            }
            let memory = CreateCompatibleDC(Some(screen));
            if memory.is_invalid() {
                ReleaseDC(None, screen);
                return Err("CreateCompatibleDC failed".into());
            }
            let bitmap_info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = null_mut();
            let bitmap = match CreateDIBSection(
                Some(screen),
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            ) {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    let _ = DeleteDC(memory);
                    ReleaseDC(None, screen);
                    return Err(format!("CreateDIBSection: {error}").into());
                }
            };
            let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
            let rop = ROP_CODE(SRCCOPY.0);
            let copied = BitBlt(
                memory,
                0,
                0,
                width as i32,
                height as i32,
                Some(screen),
                rect.left,
                rect.top,
                rop,
            );
            let mut bgra = vec![0u8; width as usize * height as usize * 4];
            if copied.is_ok() && !bits.is_null() {
                std::ptr::copy_nonoverlapping(bits.cast::<u8>(), bgra.as_mut_ptr(), bgra.len());
            }
            SelectObject(memory, previous);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            if copied.is_err() {
                return Err("BitBlt screen capture failed".into());
            }
            Ok(ScreenCapture {
                width,
                height,
                bgra,
            })
        }
    }

    fn write_png(path: &Path, capture: &ScreenCapture) -> ProbeResult<()> {
        let row_bytes = capture.width as usize * 4;
        let mut raw = Vec::with_capacity((row_bytes + 1) * capture.height as usize);
        for row in capture.bgra.chunks_exact(row_bytes) {
            raw.push(0);
            for pixel in row.chunks_exact(4) {
                raw.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
        }
        let compressed = zlib_store(&raw);
        let mut png = Vec::with_capacity(compressed.len() + 128);
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        let mut header = Vec::with_capacity(13);
        header.extend_from_slice(&capture.width.to_be_bytes());
        header.extend_from_slice(&capture.height.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        png_chunk(&mut png, b"IHDR", &header);
        png_chunk(&mut png, b"IDAT", &compressed);
        png_chunk(&mut png, b"IEND", &[]);
        fs::write(path, png)?;
        Ok(())
    }

    fn zlib_store(raw: &[u8]) -> Vec<u8> {
        let mut output = vec![0x78, 0x01];
        if raw.is_empty() {
            output.extend_from_slice(&[1, 0, 0, 255, 255]);
        } else {
            for (index, chunk) in raw.chunks(65_535).enumerate() {
                let final_block = index == raw.chunks(65_535).count() - 1;
                output.push(if final_block { 1 } else { 0 });
                let length = chunk.len() as u16;
                output.extend_from_slice(&length.to_le_bytes());
                output.extend_from_slice(&(!length).to_le_bytes());
                output.extend_from_slice(chunk);
            }
        }
        output.extend_from_slice(&adler32(raw).to_be_bytes());
        output
    }

    fn adler32(bytes: &[u8]) -> u32 {
        let mut a = 1u32;
        let mut b = 0u32;
        for &byte in bytes {
            a = (a + byte as u32) % 65_521;
            b = (b + a) % 65_521;
        }
        (b << 16) | a
    }

    fn png_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let mut crc_input = Vec::with_capacity(kind.len() + data.len());
        crc_input.extend_from_slice(kind);
        crc_input.extend_from_slice(data);
        png.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &byte in bytes {
            let mut value = crc ^ byte as u32;
            for _ in 0..8 {
                value = if value & 1 != 0 {
                    0xedb8_8320 ^ (value >> 1)
                } else {
                    value >> 1
                };
            }
            crc = value;
        }
        !crc
    }

    unsafe fn make_format(
        dwrite: &IDWriteFactory,
        size: f32,
        weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT,
    ) -> ProbeResult<windows::Win32::Graphics::DirectWrite::IDWriteTextFormat> {
        let family = HSTRING::from("Segoe UI Variable Text");
        let locale = HSTRING::from("en-US");
        let format = dwrite
            .CreateTextFormat(
                PCWSTR(family.as_ptr()),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                PCWSTR(locale.as_ptr()),
            )
            .map_err(|error| format!("CreateTextFormat: {error}"))?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        Ok(format)
    }

    unsafe fn draw_text(
        target: &ID2D1RenderTarget,
        value: &str,
        format: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
        rect: D2D_RECT_F,
        brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    ) {
        let wide: Vec<u16> = value.encode_utf16().collect();
        target.DrawText(
            &wide,
            format,
            &rect,
            brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    fn solid(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
        D2D1_COLOR_F { r, g, b, a }
    }

    unsafe fn rounded(left: f32, top: f32, right: f32, bottom: f32) -> D2D1_ROUNDED_RECT {
        D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left,
                top,
                right,
                bottom,
            },
            radiusX: RADIUS_DIP,
            radiusY: RADIUS_DIP,
        }
    }

    fn dip_to_px(value: f32, dpi: u32) -> i32 {
        (value * dpi as f32 / 96.0).round().max(1.0) as i32
    }

    fn rect_json(rect: RECT) -> String {
        format!(
            "{{\"left\":{},\"top\":{},\"right\":{},\"bottom\":{}}}",
            rect.left, rect.top, rect.right, rect.bottom
        )
    }

    fn optional_i32(value: Option<i32>) -> String {
        value.map_or_else(|| "null".to_owned(), |value| value.to_string())
    }

    pub fn run() -> ProbeResult<()> {
        let mode = Mode::parse(std::env::args().nth(1).as_deref())?;
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let _com = ComApartment::initialize()?;
        let graphics = Graphics::create()?;
        let (hwnd, diagnostics) = create_window(mode, &graphics)?;
        if mode == Mode::Composition {
            pump_composition(hwnd)?;
        }
        let output_dir = PathBuf::from("target/overlay-probe");
        fs::create_dir_all(&output_dir)?;
        let capture = copy_from_screen(diagnostics.capture_rect)?;
        let image_path = output_dir.join(format!("{}.png", mode.name()));
        let result_path = output_dir.join(format!("{}.json", mode.name()));
        write_png(&image_path, &capture)?;
        fs::write(&result_path, diagnostics.json())?;
        unsafe {
            DestroyWindow(hwnd).ok();
        }
        println!("{}", image_path.display());
        println!("{}", result_path.display());
        Ok(())
    }
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    probe::run()
}
