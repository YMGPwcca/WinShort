//! Lifecycle message handling for the overlay.

use super::super::backend::{render_current_frame, render_prepared_frame, resize_surface};
use super::super::layout::select_monitor;
use super::super::state::OverlayState;
use super::super::timeline::{prepare_state_plan, TickPlan};
use super::super::window::{apply_frame_plan, apply_hide_window, set_timer};
use crate::platform::visual::SystemVisualPreferences;

use windows::Win32::Foundation::{HWND, LRESULT, WPARAM};

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_settingchange(
    cell: &std::cell::RefCell<OverlayState>,
    hwnd: HWND,
) -> LRESULT {
    {
        let preferences = SystemVisualPreferences::query();
        let monitor_choice = { cell.borrow().config.monitor.clone() };
        let monitor = select_monitor(monitor_choice);
        let plan = prepare_state_plan(cell, |state| {
            state.prepare_visual_refresh(preferences, monitor)
        });
        match plan {
            Ok(Some(plan)) => {
                let apply_region = cell.borrow().requires_window_region();
                if let Err(error) = apply_frame_plan(hwnd, plan, apply_region) {
                    crate::warn_!("overlay visual refresh placement failed: {error}");
                }
                if let Err(error) = set_timer(hwnd, plan.timer_interval) {
                    crate::warn_!("overlay visual refresh timer failed: {error}");
                }
                if let Err(error) = render_prepared_frame(cell, hwnd, plan) {
                    crate::warn_!("overlay visual refresh failed: {error}");
                }
            }
            Ok(None) => {}
            Err(error) => crate::warn_!("overlay visual refresh failed: {error}"),
        }
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_timer(cell: &std::cell::RefCell<OverlayState>, hwnd: HWND) -> LRESULT {
    {
        let plan = prepare_state_plan(cell, |state| state.prepare_tick());
        match plan {
            Some(TickPlan::Hide) => apply_hide_window(hwnd),
            Some(TickPlan::PruneToStickyOwner) => {
                let monitor_choice = { cell.borrow().config.monitor.clone() };
                let monitor = select_monitor(monitor_choice);
                let plan =
                    prepare_state_plan(cell, |state| state.prepare_prune_to_sticky_owner(monitor));
                match plan {
                    Some(plan) => {
                        let apply_region = cell.borrow().requires_window_region();
                        if let Err(error) = apply_frame_plan(hwnd, plan, apply_region) {
                            crate::warn_!("overlay sticky-owner prune placement failed: {error}");
                        }
                        if let Err(error) = render_prepared_frame(cell, hwnd, plan) {
                            crate::warn_!("overlay sticky-owner prune render failed: {error}");
                        }
                    }
                    None => apply_hide_window(hwnd),
                }
            }
            Some(TickPlan::Frame(plan)) => {
                if let Err(error) = apply_frame_plan(hwnd, plan, false) {
                    crate::warn_!("overlay frame placement failed: {error}");
                }
                if let Err(error) = render_prepared_frame(cell, hwnd, plan) {
                    crate::warn_!("overlay frame failed: {error}");
                }
            }
            None => {}
        }
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_paint(cell: &std::cell::RefCell<OverlayState>, hwnd: HWND) -> LRESULT {
    {
        let _paint = crate::platform::window::PaintSession::begin(hwnd);
        let result = render_current_frame(cell, hwnd);
        if let Err(error) = result {
            crate::warn_!("overlay paint failed: {error}");
        }
        LRESULT(0)
    }
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_dpichanged(
    cell: &std::cell::RefCell<OverlayState>,
    hwnd: HWND,
    wparam: WPARAM,
) -> LRESULT {
    {
        let new_dpi = ((wparam.0 >> 16) as u32).max(96);
        {
            let mut state = cell.borrow_mut();
            state.dpi = new_dpi;
            state.last_render_dpi = Some(new_dpi);
        }
        if let Err(error) = resize_surface(cell, hwnd) {
            crate::warn_!("overlay DPI resize failed: {error}");
        }
        LRESULT(0)
    }
}
