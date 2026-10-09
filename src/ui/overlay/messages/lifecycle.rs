//! Lifecycle message handling for the overlay.

use super::super::backend::{render_current_frame, render_prepared_frame};
use super::super::state::OverlayState;
use super::super::timeline::{prepare_state_plan, TickPlan};
use super::super::window::{apply_frame_plan, apply_hide_window, set_timer};

use windows::Win32::Foundation::{HWND, LRESULT};

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_settingchange() -> LRESULT {
    crate::event::post_main(crate::event::AppEvent::OverlayVisualRefresh);
    LRESULT(0)
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_timer(
    cell: &std::cell::RefCell<OverlayState>,
    hwnd: HWND,
    timer_id: usize,
) -> LRESULT {
    if cell.borrow().timer_id != timer_id {
        return LRESULT(0);
    }
    {
        let plan = prepare_state_plan(cell, |state| state.prepare_tick());
        match plan {
            Some(TickPlan::Hide) => {
                let (entry_id, generation) = {
                    let state = cell.borrow();
                    (state.entry_id, state.generation)
                };
                apply_hide_window(hwnd, timer_id);
                if entry_id != 0 {
                    crate::event::post_main(crate::event::AppEvent::OverlayCardExpired {
                        entry_id,
                        generation,
                    });
                }
            }
            Some(TickPlan::Frame(plan)) => {
                let apply_region = cell.borrow().requires_window_region();
                if let Err(error) = apply_frame_plan(hwnd, plan, apply_region) {
                    crate::warn_!("overlay frame placement failed: {error}");
                }
                if let Err(error) = set_timer(
                    hwnd,
                    plan.timer_id,
                    plan.timer_interval,
                    plan.animation_active,
                ) {
                    crate::warn_!("overlay frame timer update failed: {error}");
                }
                if let Err(error) = render_prepared_frame(cell, hwnd, plan) {
                    crate::warn_!("overlay frame failed: {error}");
                }
                if plan.layout_changed {
                    crate::event::post_main(crate::event::AppEvent::OverlayVisualRefresh);
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
pub(super) unsafe fn handle_dpichanged() -> LRESULT {
    crate::event::post_main(crate::event::AppEvent::OverlayVisualRefresh);
    LRESULT(0)
}
