//! Painting for the picker.

use super::appearance::{picker_colors, picker_colors_for_state, picker_item_state, to_colorref};
use super::geometry::picker_corner_diameter_px;
use crate::platform::visual::SystemVisualPreferences;
use crate::ui::theme::Theme;
use windows::Win32::Foundation::{HWND, LRESULT, RECT};
use windows::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, FrameRect, GetTextMetricsW,
    RoundRect, SelectObject, SetBkMode, SetTextColor, BACKGROUND_MODE, DT_END_ELLIPSIS, DT_LEFT,
    DT_NOPREFIX, DT_SINGLELINE, HDC, HFONT, HGDIOBJ, PS_SOLID, TEXTMETRICW,
};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_FOCUS, ODS_SELECTED};
use windows::Win32::UI::HiDpi::GetDpiForWindow;

pub(super) unsafe fn draw_picker_surface(hwnd: HWND, hdc: HDC) {
    let mut rect = RECT::default();
    unsafe {
        if windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect).is_err() {
            crate::warn_!("picker client area unavailable during paint");
            return;
        }
        let colors = picker_colors(false);
        let background = CreateSolidBrush(to_colorref(colors.background));
        let border = CreatePen(PS_SOLID, 1, to_colorref(colors.border));
        let old_brush = SelectObject(hdc, background.into());
        let old_pen = SelectObject(hdc, border.into());
        let dpi = GetDpiForWindow(hwnd).max(96);
        let diameter = picker_corner_diameter_px(dpi);
        let _ = RoundRect(
            hdc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            diameter,
            diameter,
        );
        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ(background.0));
        let _ = DeleteObject(HGDIOBJ(border.0));
    }
}

pub(super) unsafe fn draw_picker_item(
    item: &DRAWITEMSTRUCT,
    label: &str,
    hovered: bool,
    font: HFONT,
    multi_select: bool,
    allowlist_mode: bool,
) -> LRESULT {
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    let focus = item.itemState.0 & ODS_FOCUS.0 != 0;
    let state = picker_item_state(selected, hovered, false);
    let colors = picker_colors_for_state(state, SystemVisualPreferences::query(), Theme::current());
    let background = unsafe { CreateSolidBrush(to_colorref(colors.background)) };
    let border = unsafe { CreateSolidBrush(to_colorref(colors.border)) };
    let _ = unsafe { FillRect(item.hDC, &item.rcItem, background) };
    let mut text_rect = item.rcItem;
    text_rect.left += if multi_select { 34 } else { 12 };
    text_rect.right -= 12;
    if multi_select {
        let is_mode = allowlist_mode && item.itemID < 3;
        let mut mark = item.rcItem;
        mark.left += 10;
        mark.right = mark.left + 14;
        mark.top += (mark.bottom - mark.top - 14) / 2;
        mark.bottom = mark.top + 14;
        if is_mode {
            let center_x = (mark.left + mark.right) / 2;
            let center_y = (mark.top + mark.bottom) / 2;
            let marker_pen = unsafe { CreatePen(PS_SOLID, 1, to_colorref(colors.border)) };
            let marker_brush = unsafe { CreateSolidBrush(to_colorref(colors.background)) };
            let old_pen = unsafe { SelectObject(item.hDC, marker_pen.into()) };
            let old_brush = unsafe { SelectObject(item.hDC, marker_brush.into()) };
            let _ = unsafe {
                windows::Win32::Graphics::Gdi::Ellipse(
                    item.hDC,
                    mark.left,
                    mark.top,
                    mark.right,
                    mark.bottom,
                )
            };
            if selected {
                let selected_brush = unsafe { CreateSolidBrush(to_colorref(colors.border)) };
                let previous_brush = unsafe { SelectObject(item.hDC, selected_brush.into()) };
                let _ = unsafe {
                    windows::Win32::Graphics::Gdi::Ellipse(
                        item.hDC,
                        center_x - 4,
                        center_y - 4,
                        center_x + 4,
                        center_y + 4,
                    )
                };
                unsafe {
                    let _ = SelectObject(item.hDC, previous_brush);
                    let _ = DeleteObject(HGDIOBJ(selected_brush.0));
                }
            }
            unsafe {
                let _ = SelectObject(item.hDC, old_brush);
                let _ = SelectObject(item.hDC, old_pen);
                let _ = DeleteObject(HGDIOBJ(marker_brush.0));
                let _ = DeleteObject(HGDIOBJ(marker_pen.0));
            }
        } else {
            let _ = unsafe { FrameRect(item.hDC, &mark, border) };
            if selected {
                let mut check = mark;
                check.left += 3;
                check.top += 3;
                check.right -= 3;
                check.bottom -= 3;
                let _ = unsafe { FillRect(item.hDC, &check, border) };
            }
        }
    }
    let mut text = label.encode_utf16().collect::<Vec<_>>();
    let old_font = if !font.is_invalid() {
        unsafe { SelectObject(item.hDC, font.into()) }
    } else {
        HGDIOBJ::default()
    };
    unsafe {
        let _ = SetBkMode(item.hDC, BACKGROUND_MODE(1));
        let _ = SetTextColor(item.hDC, to_colorref(colors.foreground));
        let mut metrics = TEXTMETRICW::default();
        let item_height = (item.rcItem.bottom - item.rcItem.top).max(1);
        let measured_height = if GetTextMetricsW(item.hDC, &mut metrics).as_bool() {
            metrics.tmHeight.max(1).min(item_height)
        } else {
            item_height
        };
        text_rect.top = item.rcItem.top + (item_height - measured_height) / 2;
        text_rect.bottom = text_rect.top + measured_height;
        let _ = DrawTextW(
            item.hDC,
            &mut text,
            &mut text_rect,
            DT_END_ELLIPSIS | DT_LEFT | DT_NOPREFIX | DT_SINGLELINE,
        );
        if focus || hovered {
            let _ = FrameRect(item.hDC, &item.rcItem, border);
        }
        if !old_font.is_invalid() {
            let _ = SelectObject(item.hDC, old_font);
        }
        let _ = DeleteObject(HGDIOBJ(background.0));
        let _ = DeleteObject(HGDIOBJ(border.0));
    }
    LRESULT(1)
}
