//! Per-pixel-alpha status HUD. One persistent HWND updates in place for every
//! event; it is topmost, no-activate, tool-window, click-through, and hidden
//! when idle (spec §30–§33).

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory1, ID2D1RenderTarget, D2D1_FACTORY_OPTIONS,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
    HGDIOBJ,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmap, IWICImagingFactory,
    WICBitmapCreateCacheOption,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, KillTimer, SetTimer, SetWindowPos, ShowWindow,
    UpdateLayeredWindow, CREATESTRUCTW, HTTRANSPARENT, HWND_TOPMOST, MA_NOACTIVATE, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_DPICHANGED, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY,
    WM_NCHITTEST, WM_SETTINGCHANGE, WM_SYSCOLORCHANGE, WM_THEMECHANGED, WM_TIMER, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::config::model::{MonitorChoice, OverlayAppearance, OverlayCfg, OverlayPosition};
use crate::error::{Error, Result};
use crate::platform::visual::{SystemVisualPreferences, VisualRgb};
use crate::platform::window as win;
use crate::ui::theme::{Color, Theme, ThemeMode};

pub const CLASS_NAME: &str = "WinShort.Overlay";

const TIMER_ID: usize = 2;
const TIMER_MS: u32 = 16;
const COALESCE_WINDOW_MS: u64 = 180;
const BASE_WIDTH: f32 = 372.0;
const ROW_HEIGHT: f32 = 62.0;
const PAD: f32 = 16.0;
const SHADOW_PAD: f32 = 14.0;

static REGISTERED: OnceLock<u16> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayIcon {
    Microphone,
    Output,
    Application,
    /// Reserved for informational rows (tone ladder completeness).
    #[allow(dead_code)]
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayTone {
    Muted,
    Active,
    Changed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MotionPolicy {
    Animated,
    Reduced,
}

#[derive(Debug, Clone, Copy)]
struct OverlayPalette {
    surface: Color,
    border: Color,
    text: Color,
    secondary: Color,
    shadow: Color,
    shadow_enabled: bool,
    opaque: bool,
    tone_muted: Color,
    tone_active: Color,
    tone_changed: Color,
    tone_unavailable: Color,
}
fn motion_policy(preferences: SystemVisualPreferences) -> MotionPolicy {
    if preferences.animations_enabled {
        MotionPolicy::Animated
    } else {
        MotionPolicy::Reduced
    }
}

fn palette_for(
    appearance: OverlayAppearance,
    preferences: SystemVisualPreferences,
) -> OverlayPalette {
    let simple = preferences.high_contrast || preferences.disable_overlapped_content;
    if preferences.high_contrast {
        let background = color_from_visual(preferences.high_contrast_background);
        let foreground = color_from_visual(preferences.high_contrast_foreground);
        let accent = color_from_visual(preferences.high_contrast_accent);
        return OverlayPalette {
            surface: background,
            border: foreground,
            text: foreground,
            secondary: foreground,
            shadow: Color::rgba(0, 0, 0, 0),
            shadow_enabled: false,
            opaque: true,
            tone_muted: foreground,
            tone_active: foreground,
            tone_changed: accent,
            tone_unavailable: foreground,
        };
    }
    let theme = match appearance {
        OverlayAppearance::System => match preferences.system_theme {
            ThemeMode::Dark => Theme::dark(),
            ThemeMode::Light => Theme::light(),
        },
        OverlayAppearance::Dark => Theme::dark(),
        OverlayAppearance::Light => Theme::light(),
    };
    OverlayPalette {
        surface: Color::rgba(
            theme.card.r,
            theme.card.g,
            theme.card.b,
            if simple { 255 } else { 248 },
        ),
        border: theme.border_strong,
        text: theme.text,
        secondary: theme.text_secondary,
        shadow: theme.shadow,
        shadow_enabled: !simple,
        opaque: simple,
        tone_muted: theme.danger,
        tone_active: theme.success,
        tone_changed: theme.accent,
        tone_unavailable: theme.text_disabled,
    }
}

fn color_from_visual(value: VisualRgb) -> Color {
    Color::rgb(value.r, value.g, value.b)
}
fn row_rank(icon: OverlayIcon) -> usize {
    match icon {
        OverlayIcon::Microphone => 0,
        OverlayIcon::Output => 1,
        OverlayIcon::Application => 2,
        OverlayIcon::Info => 3,
    }
}

fn merge_overlay_models(current: &OverlayModel, incoming: &OverlayModel) -> OverlayModel {
    if incoming.rows.len() != 1 {
        return OverlayModel {
            rows: incoming.rows.iter().take(3).cloned().collect(),
        };
    }
    let incoming_row = &incoming.rows[0];
    let mut rows = current.rows.clone();
    if let Some(existing) = rows.iter_mut().find(|row| row.icon == incoming_row.icon) {
        *existing = incoming_row.clone();
    } else {
        rows.push(incoming_row.clone());
    }
    rows.sort_by_key(|row| row_rank(row.icon));
    rows.truncate(3);
    OverlayModel { rows }
}

#[derive(Debug, Clone)]
pub struct OverlayRow {
    pub icon: OverlayIcon,
    pub tone: OverlayTone,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct OverlayModel {
    pub rows: Vec<OverlayRow>,
}

impl OverlayModel {
    pub fn single(row: OverlayRow) -> Self {
        Self { rows: vec![row] }
    }
}

pub struct OverlayWindow {
    pub hwnd: HWND,
}

#[derive(Debug, Clone, Default)]
pub struct OverlayRuntimeStatus {
    pub window_available: bool,
    pub resolved_appearance: Option<String>,
    pub animations_enabled: Option<bool>,
    pub high_contrast: Option<bool>,
    pub disable_overlapped_content: Option<bool>,
    pub target_monitor: Option<String>,
    pub render_dpi: Option<u32>,
    pub last_shown: Option<SystemTime>,
}

pub fn microphone_row(state: &crate::audio::AudioState) -> OverlayRow {
    use crate::audio::AudioState;
    match state {
        AudioState::Muted { volume_pct } => OverlayRow {
            icon: OverlayIcon::Microphone,
            tone: OverlayTone::Muted,
            title: "Microphone".into(),
            detail: format!("Muted • {volume_pct}% input volume"),
        },
        AudioState::Active { volume_pct } => OverlayRow {
            icon: OverlayIcon::Microphone,
            tone: OverlayTone::Active,
            title: "Microphone".into(),
            detail: format!("Active • {volume_pct}% input volume"),
        },
        AudioState::Unavailable { reason } => OverlayRow {
            icon: OverlayIcon::Microphone,
            tone: OverlayTone::Unavailable,
            title: "Microphone unavailable".into(),
            detail: concise(reason),
        },
    }
}

pub fn output_row(state: &crate::audio::OutputState) -> OverlayRow {
    use crate::audio::OutputState;
    match state {
        OutputState::Current {
            device,
            muted,
            volume_pct,
        } => OverlayRow {
            icon: OverlayIcon::Output,
            tone: if *muted {
                OverlayTone::Muted
            } else {
                OverlayTone::Active
            },
            title: device.name.clone(),
            detail: if *muted {
                "Output muted".into()
            } else {
                format!("{volume_pct}% volume")
            },
        },
        OutputState::Unavailable { reason } => OverlayRow {
            icon: OverlayIcon::Output,
            tone: OverlayTone::Unavailable,
            title: "Output".into(),
            detail: concise(reason),
        },
    }
}

pub fn application_row(state: &crate::audio::AppAudioState) -> OverlayRow {
    use crate::audio::Aggregate;
    let (tone, detail) = match state.aggregate {
        Aggregate::AllMuted => (OverlayTone::Muted, "Muted".to_string()),
        Aggregate::AllActive => (OverlayTone::Active, "Active".to_string()),
        Aggregate::Mixed => (OverlayTone::Changed, "Mixed sessions".to_string()),
        Aggregate::NoSession => (OverlayTone::Unavailable, "No audio session".to_string()),
        Aggregate::Error => (
            OverlayTone::Unavailable,
            state
                .error
                .clone()
                .map(|reason| format!("Audio error: {reason}"))
                .unwrap_or_else(|| "Audio error".into()),
        ),
        Aggregate::NoExternalApp => (
            OverlayTone::Unavailable,
            "No external application selected".to_string(),
        ),
    };
    OverlayRow {
        icon: OverlayIcon::Application,
        tone,
        title: state
            .app_name
            .clone()
            .unwrap_or_else(|| "Current app".into()),
        detail,
    }
}

/// Transient "output changed" card (#17b).
pub fn output_changed_row(device: &crate::audio::state::DeviceId) -> OverlayRow {
    OverlayRow {
        icon: OverlayIcon::Output,
        tone: OverlayTone::Changed,
        title: "Output changed".into(),
        detail: concise(&device.name),
    }
}

fn concise(value: &str) -> String {
    const MAX: usize = 58;
    if value.chars().count() <= MAX {
        value.to_owned()
    } else {
        format!("{}…", value.chars().take(MAX - 1).collect::<String>())
    }
}

impl OverlayWindow {
    pub fn create() -> Result<Self> {
        let _atom = *REGISTERED.get_or_init(|| {
            win::register_class(CLASS_NAME, Some(overlay_wndproc)).expect("register overlay class")
        });
        let graphics = OverlayGraphics::create()?;
        let state = Box::new(OverlayState::new(graphics));
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(
                    WS_EX_LAYERED.0
                        | WS_EX_TOPMOST.0
                        | WS_EX_TOOLWINDOW.0
                        | WS_EX_NOACTIVATE.0
                        | WS_EX_TRANSPARENT.0,
                ),
                PCWSTR(HSTRING::from(CLASS_NAME).as_ptr()),
                PCWSTR(HSTRING::from("WinShort status").as_ptr()),
                WINDOW_STYLE(WS_POPUP.0),
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                Some(Box::into_raw(state).cast()),
            )
        }
        .map_err(|e| Error::win("CreateWindowExW(overlay)", &e))?;
        Ok(Self { hwnd })
    }

    pub fn show(&self, model: OverlayModel, config: OverlayCfg) -> Result<()> {
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(self.hwnd) }) else {
            return Err(Error::internal("overlay state missing"));
        };
        cell.borrow_mut().show(self.hwnd, model, config)
    }

    pub fn hide(&self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_ID);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

impl OverlayWindow {
    pub fn status(&self) -> OverlayRuntimeStatus {
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(self.hwnd) }) else {
            return OverlayRuntimeStatus::default();
        };
        let state = cell.borrow();
        let resolved = match state.config.appearance {
            OverlayAppearance::System => state.preferences.system_theme,
            OverlayAppearance::Dark => ThemeMode::Dark,
            OverlayAppearance::Light => ThemeMode::Light,
        };
        OverlayRuntimeStatus {
            window_available: true,
            resolved_appearance: Some(
                match resolved {
                    ThemeMode::Dark => "dark",
                    ThemeMode::Light => "light",
                }
                .into(),
            ),
            animations_enabled: Some(state.preferences.animations_enabled),
            high_contrast: Some(state.preferences.high_contrast),
            disable_overlapped_content: Some(state.preferences.disable_overlapped_content),
            target_monitor: state.last_target_monitor.clone(),
            render_dpi: state.last_render_dpi,
            last_shown: state.last_shown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Hidden,
    Appearing,
    Holding,
    Leaving,
}

struct OverlayState {
    graphics: OverlayGraphics,
    surface: Option<LayeredSurface>,
    model: OverlayModel,
    config: OverlayCfg,
    preferences: SystemVisualPreferences,
    palette: OverlayPalette,
    motion: MotionPolicy,
    base_position: POINT,
    phase: Phase,
    phase_started: Instant,
    hold_until: Instant,
    last_presented: Instant,
    dpi: u32,
    last_target_monitor: Option<String>,
    last_render_dpi: Option<u32>,
    last_shown: Option<SystemTime>,
}

impl OverlayState {
    fn new(graphics: OverlayGraphics) -> Self {
        let now = Instant::now();
        let preferences = SystemVisualPreferences::query();
        let config = crate::config::Config::default().overlay;
        Self {
            graphics,
            surface: None,
            model: OverlayModel::default(),
            palette: palette_for(config.appearance, preferences),
            config,
            preferences,
            motion: motion_policy(preferences),
            base_position: POINT::default(),
            phase: Phase::Hidden,
            phase_started: now,
            hold_until: now,
            last_presented: now,
            dpi: 96,
            last_target_monitor: None,
            last_render_dpi: None,
            last_shown: None,
        }
    }

    fn show(&mut self, hwnd: HWND, model: OverlayModel, config: OverlayCfg) -> Result<()> {
        if model.rows.is_empty() || !config.enabled {
            return Ok(());
        }
        self.preferences = SystemVisualPreferences::query();
        self.motion = motion_policy(self.preferences);
        self.config = config;
        let now = Instant::now();
        let coalesce = self.phase != Phase::Hidden
            && now.duration_since(self.last_presented) <= Duration::from_millis(COALESCE_WINDOW_MS);
        self.model = if coalesce {
            merge_overlay_models(&self.model, &model)
        } else {
            model
        };
        self.last_presented = now;
        let monitor = select_monitor(self.config.monitor.clone());
        self.rebuild_surface(monitor.as_ref())?;
        self.hold_until = now + Duration::from_millis(self.config.duration_ms as u64);
        if coalesce {
            if self.motion == MotionPolicy::Reduced || self.phase == Phase::Leaving {
                self.phase = Phase::Holding;
                self.phase_started = now;
            }
        } else if self.motion == MotionPolicy::Reduced {
            self.phase = Phase::Holding;
            self.phase_started = now;
        } else {
            self.phase = Phase::Appearing;
            self.phase_started = now;
            self.hold_until += Duration::from_millis(140);
        }
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                self.base_position.x,
                self.base_position.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        self.arm_timer(hwnd);
        self.render_frame(hwnd)
    }

    fn rebuild_surface(
        &mut self,
        monitor: Option<&crate::platform::monitor::MonitorGeometry>,
    ) -> Result<()> {
        self.dpi = crate::platform::dpi::effective_render_dpi(monitor.map(|value| value.dpi));
        self.last_target_monitor = monitor.map(|value| value.device_name.clone());
        self.last_render_dpi = Some(self.dpi);
        self.last_shown = Some(SystemTime::now());
        self.palette = palette_for(self.config.appearance, self.preferences);
        self.surface =
            Some(
                self.graphics
                    .render(&self.model, self.dpi, self.config.scale, self.palette)?,
            );
        let size = self.surface.as_ref().expect("surface").size;
        self.base_position = position_for(
            monitor.map(|value| value.work).unwrap_or(RECT_FALLBACK),
            size,
            self.config.position,
            self.dpi,
        );
        Ok(())
    }

    fn arm_timer(&self, hwnd: HWND) {
        let interval = if self.motion == MotionPolicy::Reduced {
            self.hold_until
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, u32::MAX as u128) as u32
        } else {
            TIMER_MS
        };
        unsafe {
            let _ = SetTimer(Some(hwnd), TIMER_ID, interval, None);
        }
    }

    fn refresh_preferences(&mut self, hwnd: HWND) -> Result<()> {
        let preferences = SystemVisualPreferences::query();
        if preferences == self.preferences {
            return Ok(());
        }
        self.preferences = preferences;
        self.motion = motion_policy(preferences);
        if self.phase != Phase::Hidden {
            let monitor = select_monitor(self.config.monitor.clone());
            self.rebuild_surface(monitor.as_ref())?;
            if self.motion == MotionPolicy::Reduced {
                self.phase = Phase::Holding;
                self.phase_started = Instant::now();
            }
            self.arm_timer(hwnd);
            self.render_frame(hwnd)?;
        }
        Ok(())
    }

    fn tick(&mut self, hwnd: HWND) {
        let now = Instant::now();
        if self.motion == MotionPolicy::Reduced {
            if now >= self.hold_until {
                self.phase = Phase::Hidden;
                unsafe {
                    let _ = KillTimer(Some(hwnd), TIMER_ID);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            return;
        }
        match self.phase {
            Phase::Appearing => {
                if now.duration_since(self.phase_started) >= Duration::from_millis(140) {
                    self.phase = Phase::Holding;
                    self.phase_started = now;
                }
            }
            Phase::Holding => {
                if now >= self.hold_until {
                    self.phase = Phase::Leaving;
                    self.phase_started = now;
                }
            }
            Phase::Leaving => {
                if now.duration_since(self.phase_started) >= Duration::from_millis(180) {
                    self.phase = Phase::Hidden;
                    unsafe {
                        let _ = KillTimer(Some(hwnd), TIMER_ID);
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                    return;
                }
            }
            Phase::Hidden => return,
        }
        if let Err(error) = self.render_frame(hwnd) {
            crate::warn_!("overlay frame failed: {error}");
        }
    }

    fn render_frame(&self, hwnd: HWND) -> Result<()> {
        let Some(surface) = &self.surface else {
            return Ok(());
        };
        let elapsed = Instant::now()
            .duration_since(self.phase_started)
            .as_secs_f32();
        let (alpha, slide_dip) = if self.motion == MotionPolicy::Reduced {
            (1.0, 0.0)
        } else {
            match self.phase {
                Phase::Appearing => {
                    let t = (elapsed / 0.14).clamp(0.0, 1.0);
                    let eased = 1.0 - (1.0 - t).powi(3);
                    (eased, 12.0 * (1.0 - eased))
                }
                Phase::Holding => (1.0, 0.0),
                Phase::Leaving => {
                    let t = (elapsed / 0.18).clamp(0.0, 1.0);
                    (1.0 - t * t, 8.0 * t)
                }
                Phase::Hidden => (0.0, 0.0),
            }
        };
        let opacity = if self.palette.opaque {
            255
        } else {
            (alpha * self.config.opacity.clamp(0.3, 1.0) * 255.0).round() as u8
        };
        let slide_px = (slide_dip * self.dpi as f32 / 96.0).round() as i32;
        let destination = POINT {
            x: self.base_position.x,
            y: self.base_position.y + slide_px,
        };
        surface.present(hwnd, destination, opacity)
    }
}

const RECT_FALLBACK: windows::Win32::Foundation::RECT = windows::Win32::Foundation::RECT {
    left: 0,
    top: 0,
    right: 1920,
    bottom: 1080,
};

struct LayeredSurface {
    hdc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    size: SIZE,
}

impl LayeredSurface {
    fn present(&self, hwnd: HWND, destination: POINT, alpha: u8) -> Result<()> {
        let source = POINT::default();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            UpdateLayeredWindow(
                hwnd,
                None,
                Some(&destination),
                Some(&self.size),
                Some(self.hdc),
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
            .map_err(|e| Error::win("UpdateLayeredWindow", &e))
        }
    }
}

impl Drop for LayeredSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.previous);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.hdc);
        }
    }
}

struct OverlayGraphics {
    factory: ID2D1Factory1,
    dwrite: IDWriteFactory,
    wic: IWICImagingFactory,
}

impl OverlayGraphics {
    fn create() -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory1 = D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                Some(&D2D1_FACTORY_OPTIONS::default()),
            )
            .map_err(|e| Error::win("D2D1CreateFactory(overlay)", &e))?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                .map_err(|e| Error::win("DWriteCreateFactory(overlay)", &e))?;
            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| Error::win("CoCreateInstance(WIC overlay)", &e))?;
            Ok(Self {
                factory,
                dwrite,
                wic,
            })
        }
    }

    fn render(
        &self,
        model: &OverlayModel,
        dpi: u32,
        scale: f32,
        palette: OverlayPalette,
    ) -> Result<LayeredSurface> {
        let scale = scale.clamp(0.7, 1.6);
        let logical_w = BASE_WIDTH * scale;
        let shadow_pad = if palette.shadow_enabled {
            SHADOW_PAD
        } else {
            0.0
        };
        let logical_h =
            (PAD * 2.0 + ROW_HEIGHT * model.rows.len() as f32) * scale + shadow_pad * 2.0;
        let px_scale = dpi as f32 / 96.0;
        let width = (logical_w * px_scale).ceil() as u32;
        let height = (logical_h * px_scale).ceil() as u32;

        unsafe {
            let bitmap: IWICBitmap = self
                .wic
                .CreateBitmap(
                    width,
                    height,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapCreateCacheOption(1),
                )
                .map_err(|e| Error::win("WIC CreateBitmap(overlay)", &e))?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi as f32,
                dpiY: dpi as f32,
                ..Default::default()
            };
            let target: ID2D1RenderTarget = self
                .factory
                .CreateWicBitmapRenderTarget(&bitmap, &props)
                .map_err(|e| Error::win("CreateWicBitmapRenderTarget(overlay)", &e))?;
            target.BeginDraw();
            target.Clear(None);
            draw_overlay(&target, &self.dwrite, model, scale, palette)?;
            target
                .EndDraw(None, None)
                .map_err(|e| Error::win("overlay EndDraw", &e))?;

            let stride = width * 4;
            let mut pixels = vec![0u8; (stride * height) as usize];
            bitmap
                .CopyPixels(std::ptr::null(), stride, &mut pixels)
                .map_err(|e| Error::win("overlay CopyPixels", &e))?;

            let bmi = BITMAPINFO {
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
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| Error::win("CreateDIBSection(overlay)", &e))?;
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast(), pixels.len());
            let hdc = CreateCompatibleDC(None);
            if hdc.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                return Err(Error::os("CreateCompatibleDC(overlay)", 0));
            }
            let previous = SelectObject(hdc, HGDIOBJ(bitmap.0));
            Ok(LayeredSurface {
                hdc,
                bitmap,
                previous,
                size: SIZE {
                    cx: width as i32,
                    cy: height as i32,
                },
            })
        }
    }
}

fn draw_overlay(
    target: &ID2D1RenderTarget,
    dwrite: &IDWriteFactory,
    model: &OverlayModel,
    scale: f32,
    palette: OverlayPalette,
) -> Result<()> {
    unsafe {
        let width = BASE_WIDTH * scale;
        let shadow_pad = if palette.shadow_enabled {
            SHADOW_PAD
        } else {
            0.0
        };
        let body_h = (PAD * 2.0 + ROW_HEIGHT * model.rows.len() as f32) * scale;
        let left = shadow_pad;
        let top = shadow_pad;
        let body = D2D_RECT_F {
            left,
            top,
            right: left + width,
            bottom: top + body_h,
        };

        if palette.shadow_enabled {
            for (spread, alpha) in [(8.0, 18u8), (5.0, 26u8), (2.0, 34u8)] {
                let shadow = color(Color::rgba(
                    palette.shadow.r,
                    palette.shadow.g,
                    palette.shadow.b,
                    alpha,
                ));
                let brush = target.CreateSolidColorBrush(&shadow, None)?;
                let rect = D2D_RECT_F {
                    left: body.left - spread,
                    top: body.top + 5.0 - spread,
                    right: body.right + spread,
                    bottom: body.bottom + 5.0 + spread,
                };
                target.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect,
                        radiusX: 16.0 + spread,
                        radiusY: 16.0 + spread,
                    },
                    &brush,
                );
            }
        }

        let surface = color(palette.surface);
        let surface_brush = target.CreateSolidColorBrush(&surface, None)?;
        target.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: body,
                radiusX: 14.0,
                radiusY: 14.0,
            },
            &surface_brush,
        );

        let border = color(palette.border);
        let border_brush = target.CreateSolidColorBrush(&border, None)?;
        target.DrawRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: body,
                radiusX: 14.0,
                radiusY: 14.0,
            },
            &border_brush,
            1.0,
            None,
        );

        let title_format = make_format(dwrite, 14.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
        let detail_format = make_format(dwrite, 12.0 * scale, DWRITE_FONT_WEIGHT_NORMAL)?;
        let text = color(palette.text);
        let secondary = color(palette.secondary);
        let text_brush = target.CreateSolidColorBrush(&text, None)?;
        let secondary_brush = target.CreateSolidColorBrush(&secondary, None)?;

        for (index, row) in model.rows.iter().enumerate() {
            let y = top + PAD * scale + index as f32 * ROW_HEIGHT * scale;
            if index > 0 {
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: left + 58.0 * scale,
                        Y: y,
                    },
                    windows_numerics::Vector2 {
                        X: body.right - 16.0 * scale,
                        Y: y,
                    },
                    &border_brush,
                    1.0,
                    None,
                );
            }
            let tone_color = match row.tone {
                OverlayTone::Muted => palette.tone_muted,
                OverlayTone::Active => palette.tone_active,
                OverlayTone::Changed => palette.tone_changed,
                OverlayTone::Unavailable => palette.tone_unavailable,
            };
            let tone = color(tone_color);
            let tone_brush = target.CreateSolidColorBrush(&tone, None)?;
            let icon_center_x = left + 34.0 * scale;
            let icon_center_y = y + ROW_HEIGHT * scale * 0.5;
            target.FillEllipse(
                &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                    point: windows_numerics::Vector2 {
                        X: icon_center_x,
                        Y: icon_center_y,
                    },
                    radiusX: 17.0 * scale,
                    radiusY: 17.0 * scale,
                },
                &tone_brush,
            );
            draw_icon(
                target,
                row.icon,
                icon_center_x,
                icon_center_y,
                scale,
                &surface_brush,
            );

            draw_text(
                target,
                &row.title,
                &title_format,
                D2D_RECT_F {
                    left: left + 62.0 * scale,
                    top: y + 8.0 * scale,
                    right: body.right - 18.0 * scale,
                    bottom: y + 32.0 * scale,
                },
                &text_brush,
            );
            draw_text(
                target,
                &row.detail,
                &detail_format,
                D2D_RECT_F {
                    left: left + 62.0 * scale,
                    top: y + 30.0 * scale,
                    right: body.right - 18.0 * scale,
                    bottom: y + 54.0 * scale,
                },
                if row.tone == OverlayTone::Unavailable {
                    &tone_brush
                } else {
                    &secondary_brush
                },
            );
        }
        Ok(())
    }
}

unsafe fn make_format(
    dwrite: &IDWriteFactory,
    size: f32,
    weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT,
) -> Result<windows::Win32::Graphics::DirectWrite::IDWriteTextFormat> {
    // SAFETY: DWrite factory/text-format COM calls on objects created by the caller.
    unsafe {
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
            .map_err(|e| Error::win("overlay CreateTextFormat", &e))?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        Ok(format)
    }
}

unsafe fn draw_text(
    target: &ID2D1RenderTarget,
    value: &str,
    format: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
    rect: D2D_RECT_F,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) {
    // SAFETY: Direct2D DrawText on a live render target; buffers sized locally.
    unsafe {
        let wide: Vec<u16> = value.encode_utf16().collect();
        target.DrawText(
            &wide,
            format,
            &rect,
            brush,
            windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
}

unsafe fn draw_icon(
    target: &ID2D1RenderTarget,
    icon: OverlayIcon,
    cx: f32,
    cy: f32,
    scale: f32,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) {
    // SAFETY: Direct2D geometry drawing on a live render target created above.
    unsafe {
        let w = 1.8 * scale;
        match icon {
            OverlayIcon::Microphone => {
                target.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: cx - 4.0 * scale,
                            top: cy - 9.0 * scale,
                            right: cx + 4.0 * scale,
                            bottom: cy + 4.0 * scale,
                        },
                        radiusX: 4.0 * scale,
                        radiusY: 4.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx - 8.0 * scale,
                        Y: cy,
                    },
                    windows_numerics::Vector2 {
                        X: cx - 8.0 * scale,
                        Y: cy + 1.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx - 8.0 * scale,
                        Y: cy + 1.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx,
                        Y: cy + 9.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx,
                        Y: cy + 9.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx + 8.0 * scale,
                        Y: cy + 1.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
            }
            OverlayIcon::Output => {
                let points = [
                    (
                        cx - 9.0 * scale,
                        cy - 4.0 * scale,
                        cx - 4.0 * scale,
                        cy - 4.0 * scale,
                    ),
                    (
                        cx - 4.0 * scale,
                        cy - 4.0 * scale,
                        cx + 2.0 * scale,
                        cy - 9.0 * scale,
                    ),
                    (
                        cx + 2.0 * scale,
                        cy - 9.0 * scale,
                        cx + 2.0 * scale,
                        cy + 9.0 * scale,
                    ),
                    (
                        cx + 2.0 * scale,
                        cy + 9.0 * scale,
                        cx - 4.0 * scale,
                        cy + 4.0 * scale,
                    ),
                    (
                        cx - 4.0 * scale,
                        cy + 4.0 * scale,
                        cx - 9.0 * scale,
                        cy + 4.0 * scale,
                    ),
                ];
                for (x1, y1, x2, y2) in points {
                    target.DrawLine(
                        windows_numerics::Vector2 { X: x1, Y: y1 },
                        windows_numerics::Vector2 { X: x2, Y: y2 },
                        brush,
                        w,
                        None,
                    );
                }
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx + 6.0 * scale,
                        Y: cy - 6.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx + 10.0 * scale,
                        Y: cy,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx + 10.0 * scale,
                        Y: cy,
                    },
                    windows_numerics::Vector2 {
                        X: cx + 6.0 * scale,
                        Y: cy + 6.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
            }
            OverlayIcon::Application => {
                target.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: cx - 9.0 * scale,
                            top: cy - 7.0 * scale,
                            right: cx + 9.0 * scale,
                            bottom: cy + 7.0 * scale,
                        },
                        radiusX: 2.0 * scale,
                        radiusY: 2.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx - 9.0 * scale,
                        Y: cy - 2.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx + 9.0 * scale,
                        Y: cy - 2.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
            }
            OverlayIcon::Info => {
                target.DrawEllipse(
                    &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                        point: windows_numerics::Vector2 { X: cx, Y: cy },
                        radiusX: 9.0 * scale,
                        radiusY: 9.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx,
                        Y: cy - 1.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx,
                        Y: cy + 5.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.FillEllipse(
                    &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                        point: windows_numerics::Vector2 {
                            X: cx,
                            Y: cy - 5.0 * scale,
                        },
                        radiusX: 1.2 * scale,
                        radiusY: 1.2 * scale,
                    },
                    brush,
                );
            }
        }
    }
}

fn color(color: Color) -> D2D1_COLOR_F {
    color.d2d()
}

fn select_monitor(choice: MonitorChoice) -> Option<crate::platform::monitor::MonitorGeometry> {
    match choice {
        MonitorChoice::Primary => crate::platform::monitor::primary(),
        // Stable identity (#26): device names survive topology changes;
        // fall back to primary with a warning when absent.
        MonitorChoice::Device(name) => {
            let found = crate::platform::monitor::all()
                .into_iter()
                .find(|m| m.device_name == name);
            if found.is_none() {
                crate::warn_!("overlay monitor {name} not present; using primary");
            }
            found.or_else(crate::platform::monitor::primary)
        }
        // Tray-triggered overlays must target the LAST external window's
        // monitor — the true foreground is WinShort itself (#26).
        MonitorChoice::Foreground => {
            let target =
                crate::platform::foreground::last_external_hwnd().unwrap_or_else(|| unsafe {
                    windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()
                });
            crate::platform::monitor::info_for(crate::platform::monitor::from_window(target))
                .or_else(crate::platform::monitor::primary)
        }
    }
}

fn position_for(
    work: windows::Win32::Foundation::RECT,
    size: SIZE,
    position: OverlayPosition,
    dpi: u32,
) -> POINT {
    let margin = (22.0 * dpi as f32 / 96.0).round() as i32;
    let left = work.left + margin;
    let right = work.right - margin - size.cx;
    let top = work.top + margin;
    let bottom = work.bottom - margin - size.cy;
    let center_x = work.left + ((work.right - work.left) - size.cx) / 2;
    let center_y = work.top + ((work.bottom - work.top) - size.cy) / 2;
    let (x, y) = match position {
        OverlayPosition::TopLeft => (left, top),
        OverlayPosition::TopCenter => (center_x, top),
        OverlayPosition::TopRight => (right, top),
        OverlayPosition::Center => (center_x, center_y),
        OverlayPosition::BottomLeft => (left, bottom),
        OverlayPosition::BottomCenter => (center_x, bottom),
        OverlayPosition::BottomRight => (right, bottom),
    };
    // Clamp into the work area; if the overlay cannot fit (absurd sizes),
    // re-center on the axis that overflows (#26).
    let x = x.clamp(work.left, (work.right - size.cx).max(work.left));
    let y = y.clamp(work.top, (work.bottom - size.cy).max(work.top));
    POINT { x, y }
}

unsafe extern "system" fn overlay_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: window handle is thread-valid; state access via WindowState cell.
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let state = Box::from_raw(create.lpCreateParams as *mut OverlayState);
            win::store_state_ptr(hwnd, win::WindowState::new(*state));
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        if msg == WM_NCDESTROY {
            drop(win::take_state::<OverlayState>(hwnd)); // outer unsafe scope
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let Some(cell) = win::state_cell::<OverlayState>(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            WM_SETTINGCHANGE | WM_SYSCOLORCHANGE | WM_THEMECHANGED => {
                if let Err(error) = cell.borrow_mut().refresh_preferences(hwnd) {
                    crate::warn_!("overlay visual preference refresh failed: {error}");
                }
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == TIMER_ID => {
                cell.borrow_mut().tick(hwnd);
                LRESULT(0)
            }
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            // PMv2 (#49): deliberately ignored. UpdateLayeredWindow owns the
            // window size/position and every show() re-renders at the target
            // monitor's DPI before repositioning; applying the suggested rect
            // here would fight that ownership.
            WM_DPICHANGED => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(icon: OverlayIcon, title: &str) -> OverlayRow {
        OverlayRow {
            icon,
            tone: OverlayTone::Active,
            title: title.into(),
            detail: "detail".into(),
        }
    }

    #[test]
    fn visual_preferences_select_reduced_motion_and_high_contrast_palette() {
        let preferences = SystemVisualPreferences {
            animations_enabled: false,
            high_contrast: true,
            high_contrast_background: VisualRgb { r: 1, g: 2, b: 3 },
            high_contrast_foreground: VisualRgb {
                r: 240,
                g: 241,
                b: 242,
            },
            high_contrast_accent: VisualRgb {
                r: 10,
                g: 20,
                b: 30,
            },
            ..SystemVisualPreferences::default()
        };

        assert_eq!(motion_policy(preferences), MotionPolicy::Reduced);
        let palette = palette_for(OverlayAppearance::System, preferences);
        assert_eq!(palette.surface, Color::rgb(1, 2, 3));
        assert_eq!(palette.text, Color::rgb(240, 241, 242));
        assert_eq!(palette.tone_changed, Color::rgb(10, 20, 30));
        assert!(!palette.shadow_enabled);
        assert!(palette.opaque);
    }

    #[test]
    fn appearance_policy_resolves_system_and_explicit_modes() {
        let preferences = SystemVisualPreferences {
            system_theme: ThemeMode::Light,
            ..SystemVisualPreferences::default()
        };

        let system = palette_for(OverlayAppearance::System, preferences);
        let explicit_dark = palette_for(OverlayAppearance::Dark, preferences);
        let explicit_light = palette_for(OverlayAppearance::Light, preferences);
        assert_eq!(system.surface, Color::rgba(255, 255, 255, 248));
        assert_eq!(explicit_dark.surface, Color::rgba(43, 43, 43, 248));
        assert_eq!(explicit_light.surface, Color::rgba(255, 255, 255, 248));
    }

    #[test]
    fn coalescer_replaces_same_icon_and_keeps_deterministic_order() {
        let current = OverlayModel {
            rows: vec![
                row(OverlayIcon::Output, "old output"),
                row(OverlayIcon::Application, "app"),
            ],
        };
        let incoming = OverlayModel::single(row(OverlayIcon::Microphone, "mic"));
        let merged = merge_overlay_models(&current, &incoming);
        assert_eq!(
            merged
                .rows
                .iter()
                .map(|value| value.icon)
                .collect::<Vec<_>>(),
            vec![
                OverlayIcon::Microphone,
                OverlayIcon::Output,
                OverlayIcon::Application
            ]
        );

        let replaced = merge_overlay_models(
            &merged,
            &OverlayModel::single(row(OverlayIcon::Output, "new output")),
        );
        assert_eq!(replaced.rows.len(), 3);
        assert_eq!(replaced.rows[1].title, "new output");
    }

    #[test]
    fn microphone_row_uses_volume_terminology() {
        let rendered = microphone_row(&crate::audio::AudioState::Active { volume_pct: 42 });
        assert!(rendered.detail.contains("input volume"));
        assert!(!rendered.detail.contains("input level"));
    }
}
