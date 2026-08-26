//! Event-driven Direct2D / DirectWrite HWND renderer.
//!
//! Coordinates are 96-DPI logical pixels. The render target owns the current
//! window DPI, so moving between monitors rerenders vector/text content instead
//! of raster scaling. This renderer is used by the settings and diagnostics
//! windows; the overlay has its own WIC/DIB path.

use std::collections::HashMap;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory1, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_OPTIONS,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING_NO_WRAP,
};

use crate::error::{Error, Result};
use crate::ui::theme::{Color, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrushRole {
    Background,
    BackgroundSubtle,
    Card,
    CardHover,
    ControlHover,
    PickerHover,
    CardPressed,
    Border,
    BorderStrong,
    Text,
    TextSecondary,
    TextDisabled,
    Accent,
    AccentHover,
    AccentPressed,
    AccentText,
    Danger,
    Warning,
    Success,
    Focus,
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextStyle {
    Title,
    Subtitle,
    Section,
    Body,
    BodyStrong,
    Caption,
    CaptionRight,
    Button,
    ButtonSmall,
    Value,
}

pub struct Renderer {
    hwnd: HWND,
    dpi: u32,
    /// Kept alive deliberately: D2D/DWrite objects created from it retain
    /// references, but rustc cannot see the transitive use.
    #[allow(dead_code)]
    factory: ID2D1Factory1,
    target: ID2D1HwndRenderTarget,
    dwrite: IDWriteFactory,
    brushes: HashMap<BrushRole, ID2D1SolidColorBrush>,
    formats: HashMap<TextStyle, IDWriteTextFormat>,
    theme: Theme,
}

impl Renderer {
    pub fn new(hwnd: HWND, dpi: u32, theme: Theme) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory1 = D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                Some(&D2D1_FACTORY_OPTIONS::default()),
            )
            .map_err(|e| Error::win("D2D1CreateFactory(settings)", &e))?;

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

            let mut renderer = Self {
                hwnd,
                dpi,
                factory,
                target,
                dwrite,
                brushes: HashMap::new(),
                formats: HashMap::new(),
                theme,
            };
            renderer.rebuild_brushes()?;
            renderer.rebuild_formats()?;
            Ok(renderer)
        }
    }

    pub fn set_theme(&mut self, theme: Theme) -> Result<()> {
        self.theme = theme;
        self.rebuild_brushes()
    }

    pub fn set_dpi(&mut self, dpi: u32) -> Result<()> {
        self.dpi = dpi.max(96);
        unsafe { self.target.SetDpi(self.dpi as f32, self.dpi as f32) };
        self.rebuild_formats()
    }

    pub fn client_size_dip(&self) -> (f32, f32) {
        let (w, h) = client_size(self.hwnd);
        let scale = 96.0 / self.dpi as f32;
        (w as f32 * scale, h as f32 * scale)
    }

    pub fn push_clip(&self, rect: D2D_RECT_F) {
        unsafe {
            self.target.PushAxisAlignedClip(
                &rect,
                windows::Win32::Graphics::Direct2D::D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            )
        }
    }

    pub fn pop_clip(&self) {
        unsafe { self.target.PopAxisAlignedClip() }
    }

    pub fn resize(&mut self) -> Result<()> {
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

    pub fn begin(&self) {
        unsafe {
            self.target.BeginDraw();
            let bg = self.theme.bg.d2d();
            self.target.Clear(Some(std::ptr::from_ref(&bg)));
        }
    }

    pub fn end(&self) -> Result<()> {
        unsafe {
            self.target
                .EndDraw(None, None)
                .map_err(|e| Error::win("ID2D1HwndRenderTarget::EndDraw", &e))
        }
    }

    pub fn fill_rect(&self, rect: D2D_RECT_F, role: BrushRole) {
        unsafe { self.target.FillRectangle(&rect, self.brush(role)) }
    }

    pub fn fill_rounded(&self, rect: D2D_RECT_F, radius: f32, role: BrushRole) {
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe { self.target.FillRoundedRectangle(&rr, self.brush(role)) }
    }

    pub fn stroke_rounded(&self, rect: D2D_RECT_F, radius: f32, role: BrushRole, width: f32) {
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            self.target
                .DrawRoundedRectangle(&rr, self.brush(role), width, None)
        }
    }

    pub fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32, role: BrushRole, width: f32) {
        unsafe {
            self.target.DrawLine(
                windows_numerics::Vector2 { X: x1, Y: y1 },
                windows_numerics::Vector2 { X: x2, Y: y2 },
                self.brush(role),
                width,
                None,
            )
        }
    }

    #[allow(clippy::too_many_arguments)] // mirrors D2D1_ELLIPSE shape
    pub fn ellipse(
        &self,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        role: BrushRole,
        fill: bool,
        width: f32,
    ) {
        let ellipse = windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
            point: windows_numerics::Vector2 { X: cx, Y: cy },
            radiusX: rx,
            radiusY: ry,
        };
        unsafe {
            if fill {
                self.target.FillEllipse(&ellipse, self.brush(role));
            } else {
                self.target
                    .DrawEllipse(&ellipse, self.brush(role), width, None);
            }
        }
    }

    pub fn text(&self, text: &str, rect: D2D_RECT_F, style: TextStyle, role: BrushRole) {
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            self.target.DrawText(
                &wide,
                self.format(style),
                std::ptr::from_ref(&rect),
                self.brush(role),
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        }
    }

    pub fn brush(&self, role: BrushRole) -> &ID2D1SolidColorBrush {
        self.brushes.get(&role).expect("theme brush")
    }

    pub fn format(&self, style: TextStyle) -> &IDWriteTextFormat {
        self.formats.get(&style).expect("text format")
    }

    fn rebuild_brushes(&mut self) -> Result<()> {
        self.brushes.clear();
        for (role, color) in brush_colors(self.theme) {
            let raw = color.d2d();
            let brush = unsafe {
                self.target
                    .CreateSolidColorBrush(std::ptr::from_ref(&raw), None)
                    .map_err(|e| Error::win("CreateSolidColorBrush", &e))?
            };
            self.brushes.insert(role, brush);
        }
        Ok(())
    }

    fn rebuild_formats(&mut self) -> Result<()> {
        self.formats.clear();
        let entries = [
            (
                TextStyle::Title,
                24.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            ),
            (
                TextStyle::Subtitle,
                13.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            ),
            (
                TextStyle::Section,
                14.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            ),
            (
                TextStyle::Body,
                13.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            ),
            (
                TextStyle::BodyStrong,
                13.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            ),
            (
                TextStyle::Caption,
                11.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            ),
            (
                TextStyle::CaptionRight,
                11.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
            ),
            (
                TextStyle::Button,
                13.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_CENTER,
            ),
            (
                TextStyle::ButtonSmall,
                11.5,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_CENTER,
            ),
            (
                TextStyle::Value,
                12.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            ),
        ];
        for (style, size, weight, alignment) in entries {
            let format = self.create_format(size, weight, alignment)?;
            self.formats.insert(style, format);
        }
        Ok(())
    }

    fn create_format(
        &self,
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
        alignment: windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT,
    ) -> Result<IDWriteTextFormat> {
        let family = HSTRING::from("Segoe UI Variable Text");
        let locale = HSTRING::from("en-US");
        let format = unsafe {
            self.dwrite
                .CreateTextFormat(
                    PCWSTR(family.as_ptr()),
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    PCWSTR(locale.as_ptr()),
                )
                .or_else(|_| {
                    let fallback = HSTRING::from("Segoe UI");
                    self.dwrite.CreateTextFormat(
                        PCWSTR(fallback.as_ptr()),
                        None,
                        weight,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        size,
                        PCWSTR(locale.as_ptr()),
                    )
                })
                .map_err(|e| Error::win("CreateTextFormat", &e))?
        };
        unsafe {
            format
                .SetTextAlignment(alignment)
                .map_err(|e| Error::win("SetTextAlignment", &e))?;
            format
                .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
                .map_err(|e| Error::win("SetParagraphAlignment", &e))?;
            format
                .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                .map_err(|e| Error::win("SetWordWrapping", &e))?;
        }
        Ok(format)
    }
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

fn brush_colors(theme: Theme) -> [(BrushRole, Color); 21] {
    [
        (BrushRole::Background, theme.bg),
        (BrushRole::BackgroundSubtle, theme.bg_subtle),
        (BrushRole::Card, theme.card),
        (BrushRole::CardHover, theme.card_hover),
        (BrushRole::ControlHover, theme.control_hover),
        (BrushRole::PickerHover, theme.picker_hover),
        (BrushRole::CardPressed, theme.card_pressed),
        (BrushRole::Border, theme.border),
        (BrushRole::BorderStrong, theme.border_strong),
        (BrushRole::Text, theme.text),
        (BrushRole::TextSecondary, theme.text_secondary),
        (BrushRole::TextDisabled, theme.text_disabled),
        (BrushRole::Accent, theme.accent),
        (BrushRole::AccentHover, theme.accent_hover),
        (BrushRole::AccentPressed, theme.accent_pressed),
        (BrushRole::AccentText, theme.accent_text),
        (BrushRole::Danger, theme.danger),
        (BrushRole::Warning, theme.warning),
        (BrushRole::Success, theme.success),
        (BrushRole::Focus, theme.focus),
        (BrushRole::Shadow, theme.shadow),
    ]
}

pub fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left,
        top,
        right,
        bottom,
    }
}
