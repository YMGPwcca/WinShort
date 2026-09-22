//! Pointer message handling for the control center.

use super::super::native::{invalidate, post_main};
use super::super::placement::mouse_point;
use super::super::scroll::{settings_wheel_action, SettingsWheelAction};
use super::super::state::SettingsUi;
use crate::platform::window as win;
use crate::ui::controls;
use crate::ui::layout::ElementId;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_mousemove(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    let (x, y, needs_track) = {
        let ui = cell.borrow();
        let (x, y) = mouse_point(lparam, ui.dpi);
        (x, y, !ui.interaction.mouse_tracking())
    };
    if needs_track {
        let mut track = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        let _ = unsafe { TrackMouseEvent(&mut track) };
        cell.borrow_mut().interaction.set_mouse_tracking(true);
    }
    let mut ui = cell.borrow_mut();
    if let Some(offset) = ui.interaction.scroll_drag_offset() {
        ui.scroll = controls::scroll_from_scrollbar_pointer(
            ui.layout.content_clip,
            y,
            offset,
            ui.layout.max_scroll,
        );
        ui.rebuild_layout(hwnd);
        ui.publish_automation_snapshot(hwnd);
        invalidate(hwnd);
        return LRESULT(0);
    }
    ui.update_hover(hwnd, x, y);
    if let Some(
        id @ (ElementId::OverlayDuration | ElementId::OverlayBlur | ElementId::OverlayScale),
    ) = ui.interaction.pressed()
    {
        ui.set_slider_from_x(id, x);
        ui.publish_automation_snapshot(hwnd);
        invalidate(hwnd);
    }
    LRESULT(0)
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_lbuttondown(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    let (picker_hwnd, close_hit) = {
        let mut ui = cell.borrow_mut();
        let picker_hwnd = ui.focus.picker_window();
        let close_hit = picker_hwnd.is_some_and(|_| {
            let (x, y) = mouse_point(lparam, ui.dpi);
            ui.rebuild_layout(hwnd);
            ui.layout.hit_test(x, y) == Some(ElementId::WindowClose)
        });
        (picker_hwnd, close_hit)
    };
    if let Some(popup_hwnd) = picker_hwnd {
        post_main(crate::event::AppEvent::CancelSettingsPicker {
            popup_hwnd: popup_hwnd.0 as isize,
            restore_focus: true,
        });
        if !close_hit {
            return LRESULT(0);
        }
    }
    let (focus_requested, capture_requested) = {
        let mut ui = cell.borrow_mut();
        let (x, y) = mouse_point(lparam, ui.dpi);
        ui.rebuild_layout(hwnd);
        let mut focus_requested = false;
        let mut capture_requested = false;
        let scrollbar_hit = ui.layout.max_scroll > 0.0
            && controls::scrollbar_hit_rect(ui.layout.content_clip).contains(x, y);
        if scrollbar_hit {
            ui.set_pointer_focus(None);
            ui.sync_search_caret(hwnd);
            if let Some(thumb) = controls::scrollbar_thumb_rect(
                ui.layout.content_clip,
                ui.scroll,
                ui.layout.max_scroll,
            ) {
                let offset = if thumb.contains(x, y) {
                    y - thumb.y
                } else {
                    thumb.h * 0.5
                };
                ui.interaction.set_scroll_drag_offset(Some(offset));
                ui.scroll = controls::scroll_from_scrollbar_pointer(
                    ui.layout.content_clip,
                    y,
                    offset,
                    ui.layout.max_scroll,
                );
                ui.rebuild_layout(hwnd);
                capture_requested = true;
                invalidate(hwnd);
            }
        } else {
            let hit = ui.layout.hit_test(x, y);
            ui.set_pointer_focus(hit);
            ui.sync_search_caret(hwnd);
            if let Some(id) = hit {
                if !ui.is_disabled(id) {
                    focus_requested = true;
                    capture_requested = true;
                    ui.interaction.set_pressed(Some(id));
                    if matches!(
                        id,
                        ElementId::OverlayDuration
                            | ElementId::OverlayBlur
                            | ElementId::OverlayScale
                    ) {
                        ui.set_slider_from_x(id, x);
                    }
                    invalidate(hwnd);
                }
            }
        }
        invalidate(hwnd);
        ui.publish_automation_snapshot(hwnd);
        (focus_requested, capture_requested)
    };
    if capture_requested {
        let _ = unsafe { SetCapture(hwnd) };
    }
    if focus_requested {
        let _ = unsafe { SetFocus(Some(hwnd)) };
        let actual = unsafe { GetFocus() };
        cell.borrow_mut().sync_focus_after_set_focus(hwnd, actual);
    }
    LRESULT(0)
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_lbuttonup(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    if cell
        .borrow_mut()
        .interaction
        .take_scroll_drag_offset()
        .is_some()
    {
        let _ = unsafe { ReleaseCapture() };
        invalidate(hwnd);
        return LRESULT(0);
    }
    let (slider, activate_id) = {
        let mut ui = cell.borrow_mut();
        let (x, y) = mouse_point(lparam, ui.dpi);
        let pressed = ui.interaction.take_pressed();
        let slider = pressed.is_some_and(|id| {
            matches!(
                id,
                ElementId::OverlayDuration | ElementId::OverlayBlur | ElementId::OverlayScale
            )
        });
        let activate_id = pressed.filter(|id| !slider && ui.layout.hit_test(x, y) == Some(*id));
        (slider, activate_id)
    };
    let _ = unsafe { ReleaseCapture() };
    if slider {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(hwnd) } {
            let mut ui = cell.borrow_mut();
            let before = (*ui.config_access.current()).clone();
            ui.commit_local_change(hwnd, before);
            ui.publish_automation_snapshot(hwnd);
        }
    } else if let Some(id) = activate_id {
        cell.borrow_mut().activate(hwnd, id);
    }
    invalidate(hwnd);
    LRESULT(0)
}

/// # Safety
/// Called synchronously by this window's dispatcher on its owning thread.
/// Native message pointers and the state cell must remain valid for the call.
pub(super) unsafe fn handle_mousewheel(
    cell: &std::cell::RefCell<SettingsUi>,
    hwnd: HWND,
    wparam: WPARAM,
) -> LRESULT {
    let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
    let (picker_hwnd, scroll, max_scroll) = {
        let ui = cell.borrow();
        (ui.focus.picker_window(), ui.scroll, ui.layout.max_scroll)
    };
    match settings_wheel_action(picker_hwnd, scroll, delta, max_scroll) {
        SettingsWheelAction::ClosePicker(popup_hwnd) => {
            post_main(crate::event::AppEvent::CancelSettingsPicker {
                popup_hwnd: popup_hwnd.0 as isize,
                restore_focus: true,
            });
        }
        SettingsWheelAction::Scroll(scroll) => {
            let mut ui = cell.borrow_mut();
            ui.set_scroll(scroll, hwnd);
            ui.publish_automation_snapshot(hwnd);
            invalidate(hwnd);
        }
    }
    LRESULT(0)
}
