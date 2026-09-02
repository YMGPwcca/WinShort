//! DWM-backed status HUD. One persistent HWND updates in place for every event;
//! it is topmost, no-activate, tool-window, click-through, and hidden when idle
//! (spec §30–§33).

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory1, ID2D1HwndRenderTarget, ID2D1RenderTarget,
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_FACTORY_OPTIONS,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMSBT_NONE,
    DWMSBT_TRANSIENTWINDOW, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY,
    DWMWA_SYSTEMBACKDROP_TYPE,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateRoundRectRgn, DeleteObject, EndPaint, SetWindowRgn, HGDIOBJ, PAINTSTRUCT,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, KillTimer, SetTimer, SetWindowPos, ShowWindow,
    CREATESTRUCTW, HTTRANSPARENT, HWND_TOPMOST, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SW_HIDE, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DPICHANGED,
    WM_DWMCOMPOSITIONCHANGED, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY,
    WM_NCHITTEST, WM_PAINT, WM_SETTINGCHANGE, WM_SIZE, WM_SYSCOLORCHANGE, WM_THEMECHANGED,
    WM_TIMER, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
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
const APPEAR_MS: u64 = 140;
const LEAVE_MS: u64 = 180;
const BASE_WIDTH: f32 = 372.0;
const ROW_HEIGHT: f32 = 62.0;
const PAD: f32 = 16.0;
const CARD_CORNER_RADIUS_DIP: f32 = 14.0;
#[derive(Debug, Clone, Copy, PartialEq)]
struct SurfaceGeometry {
    width: f32,
    height: f32,
    body_left: f32,
    body_top: f32,
    body_right: f32,
    body_bottom: f32,
}

impl SurfaceGeometry {
    fn pixel_size(self, dpi: u32) -> SIZE {
        let scale = dpi as f32 / 96.0;
        SIZE {
            cx: (self.width * scale).ceil() as i32,
            cy: (self.height * scale).ceil() as i32,
        }
    }
}

fn surface_geometry(scale: f32, row_count: usize) -> SurfaceGeometry {
    let scale = scale.clamp(0.7, 1.6);
    let body_width = BASE_WIDTH * scale;
    let body_height = (PAD * 2.0 + ROW_HEIGHT * row_count as f32) * scale;
    SurfaceGeometry {
        width: body_width,
        height: body_height,
        body_left: 0.0,
        body_top: 0.0,
        body_right: body_width,
        body_bottom: body_height,
    }
}
fn window_region_for(size: SIZE, dpi: u32) -> WindowRegion {
    let scale = dpi.max(96) as f32 / 96.0;
    WindowRegion {
        width: size.cx,
        height: size.cy,
        inset: 0,
        corner_diameter: (CARD_CORNER_RADIUS_DIP * 2.0 * scale).round().max(2.0) as i32,
    }
}

static REGISTERED: OnceLock<u16> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayIcon {
    Microphone,
    Output,
    Application,
    Workspace,
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
    opaque: bool,
    icon: Color,
    changed_icon: Color,
    tone_muted: Color,
    tone_active: Color,
    tone_changed: Color,
    tone_unavailable: Color,
    unavailable_text: Color,
}
fn motion_policy(preferences: SystemVisualPreferences) -> MotionPolicy {
    if preferences.animations_enabled {
        MotionPolicy::Animated
    } else {
        MotionPolicy::Reduced
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackdropMode {
    Acrylic,
    Opaque,
}

fn backdrop_mode(preferences: SystemVisualPreferences, api_available: bool) -> BackdropMode {
    if preferences.high_contrast || preferences.disable_overlapped_content || !api_available {
        BackdropMode::Opaque
    } else {
        BackdropMode::Acrylic
    }
}

fn configure_backdrop(hwnd: HWND, preferences: SystemVisualPreferences) -> bool {
    suppress_dwm_nonclient(hwnd);
    disable_dwm_border(hwnd);
    if backdrop_mode(preferences, true) == BackdropMode::Opaque {
        return disable_backdrop(hwnd);
    }
    let backdrop = DWMSBT_TRANSIENTWINDOW;
    let applied = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            std::ptr::from_ref(&backdrop).cast(),
            std::mem::size_of_val(&backdrop) as u32,
        )
    }
    .is_ok();
    if !applied {
        return disable_backdrop(hwnd);
    }
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    if unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) }.is_ok() {
        true
    } else {
        disable_backdrop(hwnd)
    }
}

fn disable_backdrop(hwnd: HWND) -> bool {
    disable_dwm_border(hwnd);
    let backdrop = DWMSBT_NONE;
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            std::ptr::from_ref(&backdrop).cast(),
            std::mem::size_of_val(&backdrop) as u32,
        )
    };
    let margins = MARGINS::default();
    let _ = unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) };
    false
}

fn suppress_dwm_nonclient(hwnd: HWND) {
    let policy = DWMNCRP_DISABLED;
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            std::ptr::from_ref(&policy).cast(),
            std::mem::size_of_val(&policy) as u32,
        )
    };
}

fn disable_dwm_border(hwnd: HWND) {
    let border = DWMWA_COLOR_NONE;
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            std::ptr::from_ref(&border).cast(),
            std::mem::size_of_val(&border) as u32,
        )
    };
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShowTiming {
    phase: Phase,
    restart_phase: bool,
    hold_after_now_ms: u64,
}
#[derive(Debug, Clone, Copy)]
struct ShowPlan {
    position: POINT,
    size: SIZE,
    region: WindowRegion,
    alpha: f32,
    timer_interval: u32,
}
#[derive(Debug, Clone, Copy)]
struct WindowRegion {
    width: i32,
    height: i32,
    inset: i32,
    corner_diameter: i32,
}

#[derive(Debug, Clone, Copy)]
enum TickPlan {
    Hide,
    Frame(ShowPlan),
}

/// Return an owned plan before any caller performs HWND work.
///
/// Keeping this boundary in one helper makes it impossible for the state
/// borrow used to prepare a plan to accidentally span a reentrant call.
fn prepare_state_plan<State, Plan, Prepare>(
    cell: &std::cell::RefCell<State>,
    prepare: Prepare,
) -> Plan
where
    Prepare: FnOnce(&mut State) -> Plan,
{
    let mut state = cell.borrow_mut();
    prepare(&mut state)
}

fn timing_after_show(
    phase: Phase,
    appearance_elapsed_ms: u64,
    motion: MotionPolicy,
    coalesced: bool,
    duration_ms: u64,
) -> ShowTiming {
    if motion == MotionPolicy::Reduced {
        return ShowTiming {
            phase: Phase::Holding,
            restart_phase: true,
            hold_after_now_ms: duration_ms,
        };
    }
    if !coalesced {
        return ShowTiming {
            phase: Phase::Appearing,
            restart_phase: true,
            hold_after_now_ms: APPEAR_MS + duration_ms,
        };
    }
    match phase {
        Phase::Appearing => ShowTiming {
            phase: Phase::Appearing,
            restart_phase: false,
            hold_after_now_ms: APPEAR_MS.saturating_sub(appearance_elapsed_ms) + duration_ms,
        },
        Phase::Holding => ShowTiming {
            phase: Phase::Holding,
            restart_phase: false,
            hold_after_now_ms: duration_ms,
        },
        Phase::Leaving | Phase::Hidden => ShowTiming {
            phase: Phase::Holding,
            restart_phase: true,
            hold_after_now_ms: duration_ms,
        },
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
        let highlight = color_from_visual(preferences.high_contrast_highlight);
        let highlight_foreground =
            color_from_visual(preferences.high_contrast_highlight_foreground);
        return OverlayPalette {
            surface: background,
            border: foreground,
            text: foreground,
            secondary: foreground,
            opaque: true,
            icon: foreground,
            changed_icon: highlight_foreground,
            tone_muted: background,
            tone_active: background,
            tone_changed: highlight,
            tone_unavailable: background,
            unavailable_text: foreground,
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
    let surface = Color::rgba(
        theme.card.r,
        theme.card.g,
        theme.card.b,
        if simple { 255 } else { 248 },
    );
    OverlayPalette {
        surface,
        border: theme.border_strong,
        text: theme.text,
        secondary: theme.text_secondary,
        opaque: simple,
        icon: surface,
        changed_icon: surface,
        tone_muted: theme.danger,
        tone_active: theme.success,
        tone_changed: theme.accent,
        tone_unavailable: theme.text_disabled,
        unavailable_text: theme.text_disabled,
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
        OverlayIcon::Workspace => 3,
        OverlayIcon::Info => 4,
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
            title: "Microphone muted".into(),
            detail: format!("{volume_pct}% input volume"),
        },
        AudioState::Active { volume_pct } => OverlayRow {
            icon: OverlayIcon::Microphone,
            tone: OverlayTone::Active,
            title: "Microphone".into(),
            detail: format!("Ready · {volume_pct}% input volume"),
        },
        AudioState::Unavailable { .. } => OverlayRow {
            icon: OverlayIcon::Microphone,
            tone: OverlayTone::Unavailable,
            title: "Microphone unavailable".into(),
            detail: "Windows Audio is not available".into(),
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
            title: if *muted {
                "Speaker muted".into()
            } else {
                concise(&device.name)
            },
            detail: if *muted {
                "Speaker output is muted".into()
            } else {
                format!("Default speaker · {volume_pct}% volume")
            },
        },
        OutputState::Unavailable { .. } => OverlayRow {
            icon: OverlayIcon::Output,
            tone: OverlayTone::Unavailable,
            title: "Speakers unavailable".into(),
            detail: "Windows Audio is not available".into(),
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
            "Couldn't update current app audio".to_string(),
        ),
        Aggregate::NoExternalApp => (
            OverlayTone::Unavailable,
            "No current app with audio".to_string(),
        ),
    };
    OverlayRow {
        icon: OverlayIcon::Application,
        tone,
        title: "Current app audio".into(),
        detail: if let Some(app_name) = &state.app_name {
            format!("{app_name} · {detail}")
        } else {
            detail
        },
    }
}

pub fn device_cycle_row(
    flow: crate::audio::DeviceCycleFlow,
    device: &crate::audio::DeviceId,
) -> OverlayRow {
    let input = matches!(flow, crate::audio::DeviceCycleFlow::Input);
    OverlayRow {
        icon: if input {
            OverlayIcon::Microphone
        } else {
            OverlayIcon::Output
        },
        tone: OverlayTone::Changed,
        title: if input {
            "Next microphone".into()
        } else {
            "Next speaker".into()
        },
        detail: concise(&device.name),
    }
}

pub fn device_cycle_no_devices_row(flow: crate::audio::DeviceCycleFlow) -> OverlayRow {
    let input = matches!(flow, crate::audio::DeviceCycleFlow::Input);
    OverlayRow {
        icon: if input {
            OverlayIcon::Microphone
        } else {
            OverlayIcon::Output
        },
        tone: OverlayTone::Unavailable,
        title: if input {
            "Next microphone unavailable".into()
        } else {
            "Next speaker unavailable".into()
        },
        detail: if input {
            "No active microphones are available".into()
        } else {
            "No active speakers are available".into()
        },
    }
}

pub fn device_cycle_error_row(flow: crate::audio::DeviceCycleFlow, _error: &str) -> OverlayRow {
    let input = matches!(flow, crate::audio::DeviceCycleFlow::Input);
    OverlayRow {
        icon: if input {
            OverlayIcon::Microphone
        } else {
            OverlayIcon::Output
        },
        tone: OverlayTone::Unavailable,
        title: if input {
            "Next microphone unavailable".into()
        } else {
            "Next speaker unavailable".into()
        },
        detail: "Couldn't change the device. Open Diagnostics for help".into(),
    }
}

pub fn application_volume_row(state: &crate::audio::AppVolumeState) -> OverlayRow {
    let (tone, detail) = if state.app_name.is_none() {
        (OverlayTone::Unavailable, "No current app with audio".into())
    } else {
        match (
            state.min_volume_pct,
            state.max_volume_pct,
            state.sessions,
            state.error.is_some(),
        ) {
            (Some(min), Some(max), _, has_error) => {
                let value = if min == max {
                    format!("Volume {min}%")
                } else {
                    format!("Volume {min}–{max}%")
                };
                (
                    if has_error {
                        OverlayTone::Unavailable
                    } else {
                        OverlayTone::Changed
                    },
                    if has_error {
                        format!("{value} · Some sessions couldn't be updated")
                    } else {
                        value
                    },
                )
            }
            (_, _, 0, true) => (
                OverlayTone::Unavailable,
                "Couldn't change current app audio".into(),
            ),
            (_, _, 0, false) => (OverlayTone::Unavailable, "No active audio session".into()),
            _ => (
                OverlayTone::Unavailable,
                "Current app volume unavailable".into(),
            ),
        }
    };
    OverlayRow {
        icon: OverlayIcon::Application,
        tone,
        title: "Current app audio".into(),
        detail: if let Some(app_name) = &state.app_name {
            format!("{app_name} · {detail}")
        } else {
            detail
        },
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
                    WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TRANSPARENT.0,
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
        if model.rows.is_empty() || !config.enabled {
            return Ok(());
        }
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(self.hwnd) }) else {
            return Err(Error::internal("overlay state missing"));
        };
        let preferences = SystemVisualPreferences::query();
        let backdrop_enabled = configure_backdrop(self.hwnd, preferences);
        let plan = prepare_state_plan(cell, |state| {
            state.prepare_show(self.hwnd, model, config, preferences, backdrop_enabled)
        })?;
        let Some(plan) = plan else {
            return Ok(());
        };
        apply_show_plan(self.hwnd, plan);
        render_prepared_frame(cell, plan)
    }

    pub fn hide(&self) {
        if let Some(cell) = unsafe { win::state_cell::<OverlayState>(self.hwnd) } {
            {
                cell.borrow_mut().phase = Phase::Hidden;
            }
        }
        apply_hide_window(self.hwnd);
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

fn apply_show_plan(hwnd: HWND, plan: ShowPlan) {
    apply_frame_plan(hwnd, plan, true);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    set_timer(hwnd, plan.timer_interval);
}

fn set_timer(hwnd: HWND, interval: u32) {
    unsafe {
        let _ = SetTimer(Some(hwnd), TIMER_ID, interval, None);
    }
}
fn apply_window_region(hwnd: HWND, region: WindowRegion) {
    let handle = unsafe {
        CreateRoundRectRgn(
            region.inset,
            region.inset,
            (region.width - region.inset).max(region.inset + 1),
            (region.height - region.inset).max(region.inset + 1),
            region.corner_diameter,
            region.corner_diameter,
        )
    };
    if handle.is_invalid() {
        return;
    }
    if unsafe { SetWindowRgn(hwnd, Some(handle), true) } == 0 {
        let _ = unsafe { DeleteObject(HGDIOBJ(handle.0)) };
    } else {
        disable_dwm_border(hwnd);
    }
}

fn apply_frame_plan(hwnd: HWND, plan: ShowPlan, apply_region: bool) {
    if apply_region {
        apply_window_region(hwnd, plan.region);
    }
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            plan.position.x,
            plan.position.y,
            plan.size.cx,
            plan.size.cy,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

fn apply_hide_window(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_ID);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

fn render_prepared_frame(cell: &std::cell::RefCell<OverlayState>, plan: ShowPlan) -> Result<()> {
    let state = cell.borrow();
    if state.phase == Phase::Hidden {
        return Ok(());
    }
    state.render_surface(plan.alpha)
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
    surface: Option<OverlaySurface>,
    model: OverlayModel,
    config: OverlayCfg,
    preferences: SystemVisualPreferences,
    palette: OverlayPalette,
    backdrop_enabled: bool,
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
            backdrop_enabled: false,
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

    fn prepare_show(
        &mut self,
        hwnd: HWND,
        model: OverlayModel,
        config: OverlayCfg,
        preferences: SystemVisualPreferences,
        backdrop_enabled: bool,
    ) -> Result<Option<ShowPlan>> {
        if model.rows.is_empty() || !config.enabled {
            return Ok(None);
        }
        self.preferences = preferences;
        self.motion = motion_policy(self.preferences);
        self.config = config;
        self.backdrop_enabled = backdrop_enabled;
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
        self.rebuild_surface(hwnd, monitor.as_ref())?;
        let appearance_elapsed_ms = now
            .duration_since(self.phase_started)
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let timing = timing_after_show(
            self.phase,
            appearance_elapsed_ms,
            self.motion,
            coalesce,
            self.config.duration_ms as u64,
        );
        self.phase = timing.phase;
        if timing.restart_phase {
            self.phase_started = now;
        }
        self.hold_until = now + Duration::from_millis(timing.hold_after_now_ms);
        self.frame_plan()
            .map(Some)
            .ok_or_else(|| Error::internal("overlay surface missing after show preparation"))
    }

    fn frame_plan(&self) -> Option<ShowPlan> {
        let surface = self.surface.as_ref()?;
        let (alpha, slide_dip) = self.frame_values();
        let slide_px = (slide_dip * self.dpi as f32 / 96.0).round() as i32;
        Some(ShowPlan {
            position: POINT {
                x: self.base_position.x,
                y: self.base_position.y + slide_px,
            },
            size: surface.size,
            region: window_region_for(surface.size, self.dpi),
            alpha,
            timer_interval: self.timer_interval(),
        })
    }

    fn timer_interval(&self) -> u32 {
        if self.motion == MotionPolicy::Reduced {
            self.hold_until
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, u32::MAX as u128) as u32
        } else {
            TIMER_MS
        }
    }

    fn rebuild_surface(
        &mut self,
        hwnd: HWND,
        monitor: Option<&crate::platform::monitor::MonitorGeometry>,
    ) -> Result<()> {
        self.dpi = crate::platform::dpi::effective_render_dpi(monitor.map(|value| value.dpi));
        self.last_target_monitor = monitor.map(|value| value.device_name.clone());
        self.last_render_dpi = Some(self.dpi);
        self.last_shown = Some(SystemTime::now());
        let mut palette = palette_for(self.config.appearance, self.preferences);
        if !self.backdrop_enabled && !palette.opaque {
            palette.surface = Color::rgb(palette.surface.r, palette.surface.g, palette.surface.b);
            palette.opaque = true;
        }
        self.palette = palette;
        self.surface = Some(self.graphics.create_surface(
            hwnd,
            self.dpi,
            self.config.scale,
            self.model.rows.len(),
        )?);
        let size = self.surface.as_ref().expect("surface").size;
        self.base_position = position_for(
            monitor.map(|value| value.work).unwrap_or(RECT_FALLBACK),
            size,
            self.config.position,
            self.dpi,
        );
        Ok(())
    }
    fn resize_surface(&mut self, hwnd: HWND) -> Result<()> {
        if let Some(surface) = &mut self.surface {
            surface.resize_to_client(hwnd)?;
        }
        Ok(())
    }

    fn prepare_visual_refresh(
        &mut self,
        hwnd: HWND,
        preferences: SystemVisualPreferences,
        backdrop_enabled: bool,
    ) -> Result<Option<ShowPlan>> {
        if preferences == self.preferences && backdrop_enabled == self.backdrop_enabled {
            return Ok(None);
        }
        self.preferences = preferences;
        self.motion = motion_policy(preferences);
        self.backdrop_enabled = backdrop_enabled;
        if self.phase == Phase::Hidden {
            return Ok(None);
        }
        let monitor = select_monitor(self.config.monitor.clone());
        self.rebuild_surface(hwnd, monitor.as_ref())?;
        if self.motion == MotionPolicy::Reduced {
            self.phase = Phase::Holding;
            self.phase_started = Instant::now();
        }
        self.frame_plan()
            .map(Some)
            .ok_or_else(|| Error::internal("overlay surface missing after visual refresh"))
    }

    fn prepare_tick(&mut self) -> Option<TickPlan> {
        let now = Instant::now();
        if self.motion == MotionPolicy::Reduced {
            if now >= self.hold_until {
                self.phase = Phase::Hidden;
                return Some(TickPlan::Hide);
            }
            return None;
        }
        match self.phase {
            Phase::Appearing => {
                if now.duration_since(self.phase_started) >= Duration::from_millis(APPEAR_MS) {
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
                if now.duration_since(self.phase_started) >= Duration::from_millis(LEAVE_MS) {
                    self.phase = Phase::Hidden;
                    return Some(TickPlan::Hide);
                }
            }
            Phase::Hidden => return None,
        }
        self.frame_plan().map(TickPlan::Frame)
    }

    fn frame_values(&self) -> (f32, f32) {
        let elapsed = Instant::now()
            .duration_since(self.phase_started)
            .as_secs_f32();
        if self.motion == MotionPolicy::Reduced {
            return (1.0, 0.0);
        }
        match self.phase {
            Phase::Appearing => {
                let t = (elapsed / (APPEAR_MS as f32 / 1000.0)).clamp(0.0, 1.0);
                let eased = 1.0 - (1.0 - t).powi(3);
                (eased, 12.0 * (1.0 - eased))
            }
            Phase::Holding => (1.0, 0.0),
            Phase::Leaving => {
                let t = (elapsed / (LEAVE_MS as f32 / 1000.0)).clamp(0.0, 1.0);
                (1.0 - t * t, 8.0 * t)
            }
            Phase::Hidden => (0.0, 0.0),
        }
    }

    fn render_surface(&self, alpha: f32) -> Result<()> {
        let Some(surface) = &self.surface else {
            return Ok(());
        };
        let content_alpha = alpha
            * if self.palette.opaque {
                1.0
            } else {
                self.config.opacity.clamp(0.3, 1.0)
            };
        surface.render(
            &self.graphics.dwrite,
            &self.model,
            self.config.scale,
            self.palette,
            content_alpha,
        )
    }

    fn repaint(&self) -> Result<()> {
        let (alpha, _) = self.frame_values();
        self.render_surface(alpha)
    }
}

const RECT_FALLBACK: windows::Win32::Foundation::RECT = windows::Win32::Foundation::RECT {
    left: 0,
    top: 0,
    right: 1920,
    bottom: 1080,
};

struct OverlaySurface {
    target: ID2D1HwndRenderTarget,
    size: SIZE,
}

impl OverlaySurface {
    fn render(
        &self,
        dwrite: &IDWriteFactory,
        model: &OverlayModel,
        scale: f32,
        palette: OverlayPalette,
        alpha: f32,
    ) -> Result<()> {
        unsafe {
            self.target.BeginDraw();
            let clear = if palette.opaque {
                palette.surface.d2d()
            } else {
                Color::rgba(0, 0, 0, 0).d2d()
            };
            self.target.Clear(Some(&clear));
            draw_overlay(&self.target, dwrite, model, scale, palette, alpha)?;
            self.target
                .EndDraw(None, None)
                .map_err(|e| Error::win("overlay EndDraw", &e))
        }
    }
    fn resize_to_client(&mut self, hwnd: HWND) -> Result<()> {
        let mut client = windows::Win32::Foundation::RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd, &mut client);
        }
        let width = (client.right - client.left).max(0) as u32;
        let height = (client.bottom - client.top).max(0) as u32;
        if width == 0 || height == 0 {
            return Ok(());
        }
        if self.size.cx as u32 == width && self.size.cy as u32 == height {
            return Ok(());
        }
        unsafe {
            self.target
                .Resize(&D2D_SIZE_U { width, height })
                .map_err(|e| Error::win("ID2D1HwndRenderTarget::Resize(overlay)", &e))?;
        }
        self.size = SIZE {
            cx: width as i32,
            cy: height as i32,
        };
        Ok(())
    }
}

struct OverlayGraphics {
    factory: ID2D1Factory1,
    dwrite: IDWriteFactory,
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
            Ok(Self { factory, dwrite })
        }
    }

    fn create_surface(
        &self,
        hwnd: HWND,
        dpi: u32,
        scale: f32,
        row_count: usize,
    ) -> Result<OverlaySurface> {
        let scale = scale.clamp(0.7, 1.6);
        let geometry = surface_geometry(scale, row_count);
        let size = geometry.pixel_size(dpi);
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
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
            Ok(OverlaySurface { target, size })
        }
    }
}

fn draw_overlay(
    target: &ID2D1RenderTarget,
    dwrite: &IDWriteFactory,
    model: &OverlayModel,
    scale: f32,
    palette: OverlayPalette,
    content_alpha: f32,
) -> Result<()> {
    unsafe {
        let scale = scale.clamp(0.7, 1.6);
        let geometry = surface_geometry(scale, model.rows.len());
        let body = D2D_RECT_F {
            left: geometry.body_left,
            top: geometry.body_top,
            right: geometry.body_right,
            bottom: geometry.body_bottom,
        };
        let left = geometry.body_left;
        let top = geometry.body_top;

        let surface = color(with_alpha(palette.surface, content_alpha));
        let surface_brush = target.CreateSolidColorBrush(&surface, None)?;
        target.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: body,
                radiusX: CARD_CORNER_RADIUS_DIP,
                radiusY: CARD_CORNER_RADIUS_DIP,
            },
            &surface_brush,
        );

        let border = color(with_alpha(palette.border, content_alpha));
        let border_brush = target.CreateSolidColorBrush(&border, None)?;
        target.DrawRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: body,
                radiusX: CARD_CORNER_RADIUS_DIP,
                radiusY: CARD_CORNER_RADIUS_DIP,
            },
            &border_brush,
            1.0,
            None,
        );

        let title_format = make_format(dwrite, 14.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
        let detail_format = make_format(dwrite, 12.0 * scale, DWRITE_FONT_WEIGHT_NORMAL)?;
        let text = color(with_alpha(palette.text, content_alpha));
        let secondary = color(with_alpha(palette.secondary, content_alpha));
        let text_brush = target.CreateSolidColorBrush(&text, None)?;
        let secondary_brush = target.CreateSolidColorBrush(&secondary, None)?;
        let unavailable_text = color(with_alpha(palette.unavailable_text, content_alpha));
        let unavailable_brush = target.CreateSolidColorBrush(&unavailable_text, None)?;

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
            let tone = color(with_alpha(tone_color, content_alpha));
            let tone_brush = target.CreateSolidColorBrush(&tone, None)?;
            let icon_color = if row.tone == OverlayTone::Changed {
                palette.changed_icon
            } else {
                palette.icon
            };
            let icon_brush_color = color(with_alpha(icon_color, content_alpha));
            let icon_brush = target.CreateSolidColorBrush(&icon_brush_color, None)?;
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
                &icon_brush,
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
                    &unavailable_brush
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
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
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
                            top: cy - 10.0 * scale,
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
                for (x1, y1, x2, y2) in [
                    (-8.0, 1.0, -8.0, 3.0),
                    (-8.0, 3.0, -6.0, 6.0),
                    (-6.0, 6.0, 0.0, 8.0),
                    (0.0, 8.0, 6.0, 6.0),
                    (6.0, 6.0, 8.0, 3.0),
                    (8.0, 3.0, 8.0, 1.0),
                    (0.0, 8.0, 0.0, 11.0),
                    (-4.0, 11.0, 4.0, 11.0),
                ] {
                    target.DrawLine(
                        windows_numerics::Vector2 {
                            X: cx + x1 * scale,
                            Y: cy + y1 * scale,
                        },
                        windows_numerics::Vector2 {
                            X: cx + x2 * scale,
                            Y: cy + y2 * scale,
                        },
                        brush,
                        w,
                        None,
                    );
                }
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
            OverlayIcon::Workspace => {
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
                        X: cx,
                        Y: cy - 6.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx,
                        Y: cy + 6.0 * scale,
                    },
                    brush,
                    w * 0.8,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx - 7.0 * scale,
                        Y: cy,
                    },
                    windows_numerics::Vector2 {
                        X: cx + 7.0 * scale,
                        Y: cy,
                    },
                    brush,
                    w * 0.8,
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

fn with_alpha(value: Color, multiplier: f32) -> Color {
    Color::rgba(
        value.r,
        value.g,
        value.b,
        (value.a as f32 * multiplier.clamp(0.0, 1.0)).round() as u8,
    )
}

fn color(color: Color) -> D2D1_COLOR_F {
    color.d2d()
}

fn select_monitor(choice: MonitorChoice) -> Option<crate::platform::monitor::MonitorGeometry> {
    match choice {
        MonitorChoice::Primary => crate::platform::monitor::primary(),
        MonitorChoice::Cursor => crate::platform::monitor::cursor(),
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
        OverlayPosition::CenterLeft => (left, center_y),
        OverlayPosition::Center => (center_x, center_y),
        OverlayPosition::CenterRight => (right, center_y),
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
            crate::event::WM_APP_UI_ACCEPTANCE_HIDE_OVERLAY
                if std::env::var_os("WINSHORT_UI_ACCEPTANCE").is_some() =>
            {
                {
                    cell.borrow_mut().phase = Phase::Hidden;
                }
                apply_hide_window(hwnd);
                LRESULT(0)
            }
            WM_SETTINGCHANGE | WM_SYSCOLORCHANGE | WM_THEMECHANGED | WM_DWMCOMPOSITIONCHANGED => {
                let preferences = SystemVisualPreferences::query();
                let backdrop_enabled = configure_backdrop(hwnd, preferences);
                let plan = prepare_state_plan(cell, |state| {
                    state.prepare_visual_refresh(hwnd, preferences, backdrop_enabled)
                });
                match plan {
                    Ok(Some(plan)) => {
                        apply_frame_plan(hwnd, plan, true);
                        set_timer(hwnd, plan.timer_interval);
                        if let Err(error) = render_prepared_frame(cell, plan) {
                            crate::warn_!("overlay visual refresh failed: {error}");
                        }
                    }
                    Ok(None) => {}
                    Err(error) => crate::warn_!("overlay visual refresh failed: {error}"),
                }
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == TIMER_ID => {
                let plan = prepare_state_plan(cell, |state| state.prepare_tick());
                match plan {
                    Some(TickPlan::Hide) => apply_hide_window(hwnd),
                    Some(TickPlan::Frame(plan)) => {
                        apply_frame_plan(hwnd, plan, false);
                        if let Err(error) = render_prepared_frame(cell, plan) {
                            crate::warn_!("overlay frame failed: {error}");
                        }
                    }
                    None => {}
                }
                LRESULT(0)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut paint);
                let result = {
                    let state = cell.borrow();
                    if state.phase != Phase::Hidden {
                        state.repaint()
                    } else {
                        Ok(())
                    }
                };
                if let Err(error) = result {
                    crate::warn_!("overlay paint failed: {error}");
                }
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_SIZE => {
                if let Err(error) = cell.borrow_mut().resize_surface(hwnd) {
                    crate::warn_!("overlay resize failed: {error}");
                }
                LRESULT(0)
            }
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_DPICHANGED => {
                let new_dpi = ((wparam.0 >> 16) as u32).max(96);
                let mut state = cell.borrow_mut();
                state.dpi = new_dpi;
                if let Some(surface) = &state.surface {
                    surface.target.SetDpi(new_dpi as f32, new_dpi as f32);
                }
                if let Err(error) = state.resize_surface(hwnd) {
                    crate::warn_!("overlay DPI resize failed: {error}");
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::RECT;

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
            high_contrast_background: VisualRgb {
                r: 10,
                g: 20,
                b: 30,
            },
            high_contrast_foreground: VisualRgb {
                r: 240,
                g: 200,
                b: 160,
            },
            high_contrast_highlight: VisualRgb {
                r: 50,
                g: 100,
                b: 150,
            },
            high_contrast_highlight_foreground: VisualRgb { r: 1, g: 2, b: 3 },
            ..SystemVisualPreferences::default()
        };

        assert_eq!(motion_policy(preferences), MotionPolicy::Reduced);
        let palette = palette_for(OverlayAppearance::System, preferences);
        assert_eq!(palette.surface, Color::rgb(10, 20, 30));
        assert_eq!(palette.unavailable_text, Color::rgb(240, 200, 160));
        assert_eq!(palette.text, Color::rgb(240, 200, 160));
        assert_eq!(palette.tone_muted, Color::rgb(10, 20, 30));
        assert_eq!(palette.tone_changed, Color::rgb(50, 100, 150));
        assert_eq!(palette.icon, Color::rgb(240, 200, 160));
        assert_eq!(palette.changed_icon, Color::rgb(1, 2, 3));
        assert!(palette.opaque);
    }
    #[test]
    fn backdrop_policy_falls_back_for_accessibility_or_missing_api() {
        let normal = SystemVisualPreferences::default();
        assert_eq!(backdrop_mode(normal, true), BackdropMode::Acrylic);
        assert_eq!(backdrop_mode(normal, false), BackdropMode::Opaque);
        assert_eq!(
            backdrop_mode(
                SystemVisualPreferences {
                    high_contrast: true,
                    ..normal
                },
                true
            ),
            BackdropMode::Opaque
        );
        assert_eq!(
            backdrop_mode(
                SystemVisualPreferences {
                    disable_overlapped_content: true,
                    ..normal
                },
                true
            ),
            BackdropMode::Opaque
        );
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

    #[test]
    fn coalesced_timing_preserves_full_settled_hold() {
        let appearing_early =
            timing_after_show(Phase::Appearing, 10, MotionPolicy::Animated, true, 1300);
        assert_eq!(appearing_early.phase, Phase::Appearing);
        assert!(!appearing_early.restart_phase);
        assert_eq!(appearing_early.hold_after_now_ms, 1430);

        let appearing_late =
            timing_after_show(Phase::Appearing, 139, MotionPolicy::Animated, true, 1300);
        assert_eq!(appearing_late.hold_after_now_ms, 1301);

        let holding = timing_after_show(Phase::Holding, 0, MotionPolicy::Animated, true, 1300);
        assert_eq!(holding.phase, Phase::Holding);
        assert!(!holding.restart_phase);
        assert_eq!(holding.hold_after_now_ms, 1300);

        let leaving = timing_after_show(Phase::Leaving, 40, MotionPolicy::Animated, true, 1300);
        assert_eq!(leaving.phase, Phase::Holding);
        assert!(leaving.restart_phase);
        assert_eq!(leaving.hold_after_now_ms, 1300);

        let reduced = timing_after_show(Phase::Appearing, 10, MotionPolicy::Reduced, true, 1300);
        assert_eq!(reduced.phase, Phase::Holding);
        assert!(reduced.restart_phase);
        assert_eq!(reduced.hold_after_now_ms, 1300);

        let fresh = timing_after_show(Phase::Hidden, 0, MotionPolicy::Animated, false, 1300);
        assert_eq!(fresh.phase, Phase::Appearing);
        assert!(fresh.restart_phase);
        assert_eq!(fresh.hold_after_now_ms, 1440);
    }

    #[test]
    fn state_plan_preparation_releases_borrow_before_reentrant_window_work() {
        let cell = std::cell::RefCell::new(0_u8);
        let plan = prepare_state_plan(&cell, |state| {
            *state = 1;
            ShowPlan {
                position: POINT { x: 40, y: 80 },
                size: SIZE { cx: 320, cy: 180 },
                region: WindowRegion {
                    width: 320,
                    height: 180,
                    inset: 0,
                    corner_diameter: 28,
                },
                alpha: 1.0,
                timer_interval: TIMER_MS,
            }
        });

        assert_eq!(plan.position, POINT { x: 40, y: 80 });
        assert_eq!(plan.size, SIZE { cx: 320, cy: 180 });
        let mut reentrant = cell
            .try_borrow_mut()
            .expect("state borrow must end before window work");
        *reentrant = 2;
        assert_eq!(*reentrant, 2);
    }

    #[test]
    fn surface_geometry_keeps_body_inside_final_surface() {
        for scale in [0.7, 1.0, 1.6] {
            for dpi in [96, 144, 192] {
                let geometry = surface_geometry(scale, 3);
                let right_padding = geometry.width - geometry.body_right;
                let bottom_padding = geometry.height - geometry.body_bottom;
                assert!(geometry.body_left >= 0.0);
                assert!(geometry.body_top >= 0.0);
                assert!(geometry.body_right <= geometry.width);
                assert!(geometry.body_bottom <= geometry.height);
                assert!((geometry.body_left - right_padding).abs() < f32::EPSILON);
                assert!((geometry.body_top - bottom_padding).abs() < f32::EPSILON);
                assert!(geometry.body_left.abs() < f32::EPSILON);
                let pixels = geometry.pixel_size(dpi);
                let dpi_scale = dpi as f32 / 96.0;
                assert!(geometry.body_right * dpi_scale <= pixels.cx as f32);
                assert!(geometry.body_bottom * dpi_scale <= pixels.cy as f32);
            }
        }
    }
    #[test]
    fn device_cycle_rows_report_real_system_endpoint() {
        let device = crate::audio::DeviceId {
            endpoint: "opaque-id".into(),
            name: "USB Microphone".into(),
        };
        let row = device_cycle_row(crate::audio::DeviceCycleFlow::Input, &device);
        assert_eq!(row.title, "Next microphone");
        assert_eq!(row.detail, "USB Microphone");
    }
    #[test]
    fn long_output_names_are_bounded_before_overlay_rendering() {
        let row = output_row(&crate::audio::OutputState::Current {
            device: crate::audio::DeviceId {
                endpoint: "opaque-id".into(),
                name: "A".repeat(100),
            },
            muted: false,
            volume_pct: 50,
        });
        assert_eq!(row.title.chars().count(), 58);
        assert!(row.title.ends_with('…'));
    }

    #[test]
    fn volume_rows_show_exact_values_ranges_and_no_session_state() {
        let exact = application_volume_row(&crate::audio::AppVolumeState {
            app_name: Some("Player".into()),
            sessions: 1,
            min_volume_pct: Some(65),
            max_volume_pct: Some(65),
            error: None,
        });
        assert_eq!(exact.detail, "Player · Volume 65%");

        let range = application_volume_row(&crate::audio::AppVolumeState {
            app_name: Some("Player".into()),
            sessions: 2,
            min_volume_pct: Some(45),
            max_volume_pct: Some(70),
            error: None,
        });
        assert_eq!(range.detail, "Player · Volume 45–70%");

        let empty = application_volume_row(&crate::audio::AppVolumeState::no_session(Some(
            "Player".into(),
        )));
        assert_eq!(empty.detail, "Player · No active audio session");
        let no_external = application_volume_row(&crate::audio::AppVolumeState::no_external());
        assert_eq!(no_external.detail, "No current app with audio");
    }

    #[test]
    fn center_edge_positions_use_the_work_area_axes() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 800,
        };
        let size = SIZE { cx: 100, cy: 80 };
        assert_eq!(
            position_for(work, size, OverlayPosition::CenterLeft, 96),
            POINT { x: 22, y: 360 }
        );
        assert_eq!(
            position_for(work, size, OverlayPosition::CenterRight, 96),
            POINT { x: 878, y: 360 }
        );
    }
}
