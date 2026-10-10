//! Window for the overlay.

use super::backend::{render_prepared_frame, OverlayGraphics, OverlaySurface};
use super::composition::{CompositionHost, CompositionRuntime};
use super::layout::surface_geometry;
use super::messages::overlay_wndproc;
use super::palette::{acceptance_forces_composition_failure, resolved_theme_mode};
use super::state::{OverlayState, ShowRequest};
use super::timeline::{
    prepare_state_plan, timer_id_for_generation, Phase, ShowPlan, WindowRegion, TIMER_ID,
};
use crate::error::{Error, Result};
use crate::platform::visual::SystemVisualPreferences;
use crate::platform::window as win;
use crate::ui::theme::ThemeMode;
use std::sync::OnceLock;
use std::time::SystemTime;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowRect,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE,
    HWND_TOPMOST, LWA_ALPHA, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WINDOW_STYLE, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

pub(crate) const CLASS_NAME: &str = "WinShort.Overlay";

static REGISTERED: OnceLock<u16> = OnceLock::new();

pub(crate) struct OverlayWindow {
    pub hwnd: HWND,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OverlayRuntimeStatus {
    pub window_available: bool,
    pub active_card_count: usize,
    pub permanent_card_count: usize,
    pub toast_card_count: usize,
    /// None, one resolved device name, or a deterministic multiple (N) summary.
    pub active_monitor_summary: Option<String>,
    pub resolved_appearance: Option<String>,
    pub animations_enabled: Option<bool>,
    pub high_contrast: Option<bool>,
    pub disable_overlapped_content: Option<bool>,
    /// Legacy field with manager aggregate semantics: one monitor identity or
    /// a deterministic multiple (N) summary, never an arbitrary first HWND.
    pub target_monitor: Option<String>,
    /// Some only when every active window reports the same render DPI.
    pub render_dpi: Option<u32>,
    pub last_shown: Option<SystemTime>,
}

fn commit_prepared_show(hwnd: HWND, plan: ShowPlan) -> Result<()> {
    if let Err(error) = set_timer(
        hwnd,
        plan.timer_id,
        plan.timer_interval,
        plan.animation_active,
    ) {
        apply_hide_window(hwnd, plan.timer_id);
        return Err(error);
    }
    unsafe {
        // ShowWindow reports the previous visibility state, not operation failure.
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        if let Some(badge) = plan.behind_badge {
            // The manager supplies a live overlay it owns on this UI thread.
            // Keep expanding feedback below the badge that remains muted.
            SetWindowPos(
                hwnd,
                Some(badge),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            )
            .map_err(|error| Error::win("SetFeedbackBelowBadge", &error))?;
        }
    }
    Ok(())
}

pub(super) fn set_timer(
    hwnd: HWND,
    timer_id: usize,
    interval: Option<u32>,
    animated: bool,
) -> Result<()> {
    // SAFETY: every caller runs on this overlay HWND's owning UI thread. Clone
    // the clock and release the state borrow before signaling its worker.
    let Some(cell) = (unsafe { win::state_cell::<OverlayState>(hwnd) }) else {
        return Ok(());
    };
    let (clock, period) = {
        let state = cell.borrow();
        (state.graphics.clock.clone(), state.frame_period)
    };
    let mode = interval.map(|delay| {
        if animated {
            super::frame_clock::WakeMode::Frame(period)
        } else {
            super::frame_clock::WakeMode::Deadline(std::time::Duration::from_millis(delay as u64))
        }
    });
    clock.arm(hwnd, timer_id, mode)
}

pub(super) fn apply_window_region(hwnd: HWND, region: WindowRegion) {
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
            Some(plan.behind_badge.unwrap_or(HWND_TOPMOST)),
            plan.position.x,
            plan.position.y,
            plan.size.cx,
            plan.size.cy,
            SWP_NOACTIVATE,
        )
        .map_err(|error| Error::win("SetWindowPos(overlay)", &error))?;
    }
    super::hover::refresh(hwnd);
    Ok(())
}

pub(super) fn set_hover_opacity(hwnd: HWND, opacity: f32) -> Result<()> {
    // SAFETY: callers own the layered overlay HWND on this thread. This changes
    // only its constant alpha and preserves the transparent hit-test style.
    unsafe {
        SetLayeredWindowAttributes(
            hwnd,
            COLORREF(0),
            (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            LWA_ALPHA,
        )
    }
    .map_err(|error| Error::win("SetLayeredWindowAttributes(overlay hover)", &error))
}

pub(super) fn update_hover_pointer(
    cell: &std::cell::RefCell<OverlayState>,
    hwnd: HWND,
    point: POINT,
) -> Result<()> {
    let mut rect = RECT::default();
    // SAFETY: hwnd is the dispatching overlay's HWND and rect is a writable
    // stack RECT; no state borrow spans the native operation.
    unsafe { GetWindowRect(hwnd, &mut rect) }
        .map_err(|error| Error::win("GetWindowRect(overlay hover)", &error))?;
    let inside = point.x >= rect.left
        && point.x < rect.right
        && point.y >= rect.top
        && point.y < rect.bottom;
    let plan = prepare_state_plan(cell, |state| {
        if state.phase == Phase::Hidden || state.parked {
            return None;
        }
        let opacity = if inside && !state.preferences.high_contrast {
            state.config.hover_opacity
        } else {
            1.0
        };
        state
            .hover
            .set_target(opacity, state.motion, std::time::Instant::now())
            .then(|| state.frame_plan())
    });
    if let Some(plan) = plan {
        set_timer(
            hwnd,
            plan.timer_id,
            plan.timer_interval,
            plan.animation_active,
        )?;
        render_prepared_frame(cell, hwnd, plan)?;
    }
    Ok(())
}

pub(super) fn apply_hide_window(hwnd: HWND, timer_id: usize) {
    super::hover::unregister(hwnd);
    let _ = set_timer(hwnd, timer_id, None, false);
    unsafe {
        // Hide/teardown is intentionally best-effort: a timer can already be
        // absent and ShowWindow returns prior visibility rather than an error.
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
    pub(super) fn create_with_graphics(
        entry_id: u64,
        graphics: OverlayGraphics,
        composition_runtime: Option<CompositionRuntime>,
    ) -> Result<Self> {
        let _atom = win::register_class_once(&REGISTERED, CLASS_NAME, Some(overlay_wndproc))?;
        let mut state = win::WindowCreation::new(OverlayState::new(graphics.clone(), entry_id));
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
                        | WS_EX_LAYERED.0
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
        // WS_EX_TRANSPARENT on a layered HWND makes User32 skip this window
        // for hit testing across threads/processes. HTTRANSPARENT alone only
        // searches windows belonging to this window's thread.
        unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA) }
            .map_err(|error| Error::win("SetLayeredWindowAttributes(overlay)", &error))?;

        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let initial_size = surface_geometry(1.0, 1).pixel_size(dpi);
        let composition = composition_runtime.and_then(|runtime| {
            match CompositionHost::create(hwnd, initial_size, dpi, runtime) {
                Ok(host) => Some(host),
                Err(error) => {
                    crate::warn_!(
                        "overlay Composition target unavailable; using opaque D2D fallback for this card: {error}"
                    );
                    None
                }
            }
        });
        let surface = if let Some(host) = composition {
            OverlaySurface::Composition(host)
        } else {
            remove_no_redirection_bitmap(hwnd);
            OverlaySurface::Hwnd(graphics.create_surface(hwnd, dpi, initial_size)?)
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

    pub(crate) fn show_at(&self, request: ShowRequest) -> Result<()> {
        if request.model.rows.is_empty() || !request.config.enabled {
            return Ok(());
        }
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(self.hwnd) }) else {
            return Err(Error::internal("overlay state missing"));
        };
        let previous_timer_id = cell.borrow().timer_id;
        let preferences = SystemVisualPreferences::query();
        let plan = prepare_state_plan(cell, |state| state.prepare_show(request, preferences))?;
        let Some(plan) = plan else {
            return Ok(());
        };
        if previous_timer_id != plan.timer_id {
            set_timer(self.hwnd, previous_timer_id, None, false)?;
        }
        let apply_region = cell.borrow().requires_window_region();
        apply_frame_plan(self.hwnd, plan, apply_region)?;
        if let Err(error) = render_prepared_frame(cell, self.hwnd, plan) {
            apply_hide_window(self.hwnd, plan.timer_id);
            return Err(error);
        }
        commit_prepared_show(self.hwnd, plan)?;
        let hover_enabled = {
            let state = cell.borrow();
            state.config.hover_opacity < 1.0 && !state.preferences.high_contrast
        };
        if let Err(error) = super::hover::register(self.hwnd, hover_enabled) {
            crate::warn_!("overlay hover observation unavailable: {error}");
        }
        let mut point = POINT::default();
        if unsafe { GetCursorPos(&mut point) }.is_ok() {
            update_hover_pointer(cell, self.hwnd, point)?;
        }
        Ok(())
    }

    pub(super) fn set_entry_id(&self, entry_id: u64, generation: u64) -> Result<()> {
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(self.hwnd) }) else {
            return Err(Error::internal("overlay state missing"));
        };
        let mut state = cell.borrow_mut();
        state.phase = Phase::Hidden;
        state.expires_at = None;
        state.position_tween = None;
        state.badge = super::badge::BadgeMotion::default();
        state.content = super::content::ContentMotion::default();
        state.cluster = super::group::ClusterMotion::default();
        state.join = None;
        state.compacted_at = None;
        state.parked = false;
        state.behind_badge = None;
        state.hover = super::hover::HoverMotion::default();
        state.layout_size = SIZE::default();
        state.model = super::model::OverlayModel::default();
        state.last_target_monitor = None;
        state.last_render_dpi = None;
        state.last_shown = None;
        state.entry_id = entry_id;
        state.generation = generation;
        state.timer_id = timer_id_for_generation(generation);
        Ok(())
    }

    pub(super) fn destroy(self) {
        let hwnd = self.hwnd;
        self.hide();
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    pub(crate) fn hide(&self) {
        let timer_id = if let Some(cell) = unsafe { win::state_cell::<OverlayState>(self.hwnd) } {
            {
                let mut state = cell.borrow_mut();
                state.phase = Phase::Hidden;
                state.expires_at = None;
                state.position_tween = None;
                state.badge = super::badge::BadgeMotion::default();
                state.content = super::content::ContentMotion::default();
                state.cluster = super::group::ClusterMotion::default();
                state.join = None;
                state.compacted_at = None;
                state.parked = false;
                state.behind_badge = None;
                state.hover = super::hover::HoverMotion::default();
                state.timer_id
            }
        } else {
            TIMER_ID
        };
        apply_hide_window(self.hwnd, timer_id);
    }

    pub(super) fn compacted_at(&self) -> Option<std::time::Instant> {
        // SAFETY: manager getters run on the HWND's owning UI thread.
        unsafe { win::state_cell::<OverlayState>(self.hwnd) }
            .and_then(|cell| cell.borrow().compacted_at)
    }

    /// Hide only the peer's native surface; its independent audio entry and
    /// settled badge state must survive so unmute can reuse this same HWND.
    pub(super) fn park(&self, request: ShowRequest) -> Result<()> {
        let position = request.position;
        // SAFETY: this object owns the live HWND on the UI thread. No state
        // borrow spans the native hide or worker-clock operation.
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(self.hwnd) }) else {
            return Ok(());
        };
        let preferences = SystemVisualPreferences::query();
        prepare_state_plan(cell, |state| state.prepare_show(request, preferences))?;
        let timer_id = {
            let mut state = cell.borrow_mut();
            state.parked = true;
            state.base_position = position;
            state.position_tween = None;
            state.join = None;
            state.timer_id
        };
        super::hover::unregister(self.hwnd);
        set_timer(self.hwnd, timer_id, None, false)?;
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        Ok(())
    }

    pub(crate) fn status(&self) -> OverlayRuntimeStatus {
        let Some(cell) = (unsafe { win::state_cell::<OverlayState>(self.hwnd) }) else {
            return OverlayRuntimeStatus::default();
        };
        let state = cell.borrow();
        let resolved = resolved_theme_mode(state.config.appearance, state.preferences);
        let assigned = state.entry_id != 0;
        let is_permanent = assigned && state.expires_at.is_none();
        OverlayRuntimeStatus {
            window_available: true,
            active_card_count: usize::from(assigned),
            permanent_card_count: usize::from(is_permanent),
            toast_card_count: usize::from(assigned && !is_permanent),
            active_monitor_summary: assigned
                .then(|| state.last_target_monitor.clone())
                .flatten(),
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

    pub(super) fn reserves_compact(&self) -> bool {
        unsafe { win::state_cell::<OverlayState>(self.hwnd) }
            .is_some_and(|cell| cell.borrow().badge.reserves_compact())
    }
}
