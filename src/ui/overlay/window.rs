//! Window for the overlay.

use super::backend::{render_prepared_frame, OverlayGraphics, OverlaySurface};
use super::composition::CompositionHost;
use super::layout::{select_monitor, surface_geometry};
use super::messages::overlay_wndproc;
use super::model::OverlayModel;
use super::palette::{acceptance_forces_composition_failure, resolved_theme_mode};
use super::state::OverlayState;
use super::timeline::{prepare_state_plan, Phase, ShowPlan, WindowRegion};
use crate::config::model::OverlayCfg;
use crate::error::{Error, Result};
use crate::platform::visual::SystemVisualPreferences;
use crate::platform::window as win;
use crate::ui::theme::ThemeMode;
use std::sync::OnceLock;
use std::time::SystemTime;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetClientRect, GetWindowLongPtrW, KillTimer, SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
    WINDOW_EX_STYLE, WINDOW_STYLE, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

pub(crate) const CLASS_NAME: &str = "WinShort.Overlay";

pub(super) const TIMER_ID: usize = 2;

static REGISTERED: OnceLock<u16> = OnceLock::new();

pub(crate) struct OverlayWindow {
    pub hwnd: HWND,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OverlayRuntimeStatus {
    pub window_available: bool,
    pub resolved_appearance: Option<String>,
    pub animations_enabled: Option<bool>,
    pub high_contrast: Option<bool>,
    pub disable_overlapped_content: Option<bool>,
    pub target_monitor: Option<String>,
    pub render_dpi: Option<u32>,
    pub last_shown: Option<SystemTime>,
}

fn apply_show_plan(hwnd: HWND, plan: ShowPlan) -> Result<()> {
    apply_frame_plan(hwnd, plan, true)?;
    if let Err(error) = set_timer(hwnd, plan.timer_interval) {
        apply_hide_window(hwnd);
        return Err(error);
    }
    unsafe {
        // ShowWindow reports the previous visibility state, not operation failure.
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    Ok(())
}

pub(super) fn set_timer(hwnd: HWND, interval: u32) -> Result<()> {
    let timer = unsafe { SetTimer(Some(hwnd), TIMER_ID, interval, None) };
    if timer == 0 {
        Err(Error::internal("SetTimer(overlay) returned zero"))
    } else {
        Ok(())
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
        crate::warn_!("could not create overlay window region; using rectangular window");
        return;
    }
    if unsafe { SetWindowRgn(hwnd, Some(handle), true) } == 0 {
        // Ownership transfers to the window only on success.
        let _ = unsafe { DeleteObject(HGDIOBJ(handle.0)) };
        crate::warn_!("could not apply overlay window region; using previous region");
    }
}

pub(super) fn apply_frame_plan(hwnd: HWND, plan: ShowPlan, apply_region: bool) -> Result<()> {
    if apply_region {
        apply_window_region(hwnd, plan.region);
    }
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            plan.position.x,
            plan.position.y,
            plan.size.cx,
            plan.size.cy,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
        .map_err(|error| Error::win("SetWindowPos(overlay)", &error))?;
    }
    Ok(())
}

pub(super) fn apply_hide_window(hwnd: HWND) {
    unsafe {
        // Hide/teardown is intentionally best-effort: a timer can already be
        // absent and ShowWindow returns prior visibility rather than an error.
        let _ = KillTimer(Some(hwnd), TIMER_ID);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub(super) fn remove_no_redirection_bitmap(hwnd: HWND) {
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    #[cfg(target_pointer_width = "64")]
    let no_redirection = WS_EX_NOREDIRECTIONBITMAP.0 as isize;
    #[cfg(target_pointer_width = "32")]
    let no_redirection = WS_EX_NOREDIRECTIONBITMAP.0 as i32;
    let updated = style & !no_redirection;
    if style == updated {
        return;
    }
    unsafe {
        // SetWindowLongPtrW has an ambiguous zero return unless LastError is
        // managed around the call, so the observable fallible operation here is
        // the required frame refresh below.
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        if let Err(error) = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        ) {
            crate::warn_!("overlay fallback frame refresh failed: {error}");
        }
    }
}

pub(super) fn client_size(hwnd: HWND) -> Result<Option<SIZE>> {
    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) }
        .map_err(|error| Error::win("GetClientRect(overlay)", &error))?;
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    Ok((width > 0 && height > 0).then_some(SIZE {
        cx: width,
        cy: height,
    }))
}

impl OverlayWindow {
    pub(crate) fn create() -> Result<Self> {
        let _atom = win::register_class_once(&REGISTERED, CLASS_NAME, Some(overlay_wndproc))?;
        let graphics = OverlayGraphics::create()?;
        let mut state = win::WindowCreation::new(OverlayState::new(graphics.clone()));
        let no_redirection = if acceptance_forces_composition_failure() {
            0
        } else {
            WS_EX_NOREDIRECTIONBITMAP.0
        };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(
                    WS_EX_TOPMOST.0
                        | WS_EX_TOOLWINDOW.0
                        | WS_EX_NOACTIVATE.0
                        | WS_EX_TRANSPARENT.0
                        | no_redirection,
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
                Some(state.parameter()),
            )
        }
        .map_err(|e| Error::win("CreateWindowExW(overlay)", &e))?;
        // SAFETY: this constructor exclusively owns the newly created HWND.
        let construction = unsafe { win::WindowConstructionGuard::new(hwnd) };

        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let initial_size = surface_geometry(1.0, 1).pixel_size(dpi);
        let composition = if acceptance_forces_composition_failure() {
            Err(Error::internal("forced acceptance Composition failure"))
        } else {
            CompositionHost::create(hwnd, initial_size, dpi, &graphics)
        };
        let surface = match composition {
            Ok(host) => OverlaySurface::Composition(host),
            Err(error) => {
                crate::warn_!(
                    "overlay Composition unavailable; using opaque D2D fallback: {error}"
                );
                remove_no_redirection_bitmap(hwnd);
                match graphics.create_surface(hwnd, dpi, initial_size) {
                    Ok(surface) => OverlaySurface::Hwnd(surface),
                    Err(fallback_error) => {
                        return Err(fallback_error);
                    }
                }
            }
        };
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(hwnd) }) else {
            return Err(Error::internal("overlay state missing after creation"));
        };
        {
            let mut state = cell.borrow_mut();
            state.surface = Some(surface);
            state.surface_size = initial_size;
            state.dpi = dpi;
        }
        let hwnd = construction.complete();
        Ok(Self { hwnd })
    }

    pub(crate) fn show(&self, model: OverlayModel, config: OverlayCfg) -> Result<()> {
        if model.rows.is_empty() || !config.enabled {
            return Ok(());
        }
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(self.hwnd) }) else {
            return Err(Error::internal("overlay state missing"));
        };
        let preferences = SystemVisualPreferences::query();
        let monitor = select_monitor(config.monitor.clone());
        let plan = prepare_state_plan(cell, |state| {
            state.prepare_show(model, config, preferences, monitor)
        })?;
        let Some(plan) = plan else {
            return Ok(());
        };
        apply_show_plan(self.hwnd, plan)?;
        render_prepared_frame(cell, self.hwnd, plan)
    }

    pub(crate) fn hide(&self) {
        if let Some(cell) = unsafe { win::state_cell::<OverlayState>(self.hwnd) } {
            {
                cell.borrow_mut().phase = Phase::Hidden;
            }
        }
        apply_hide_window(self.hwnd);
    }

    pub(crate) fn status(&self) -> OverlayRuntimeStatus {
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(self.hwnd) }) else {
            return OverlayRuntimeStatus::default();
        };
        let state = cell.borrow();
        let resolved = resolved_theme_mode(state.config.appearance, state.preferences);
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
