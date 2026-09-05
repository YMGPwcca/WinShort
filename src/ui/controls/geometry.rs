//! Geometry for the controls.

use super::row::COMPACT_MONITOR_CONTROL_WIDTH;
use crate::ui::layout::{ElementId, ElementKind, Rect};
use crate::ui::theme::UiTokens;

pub(super) const CONTROL_RADIUS: f32 = UiTokens::CONTROL_RADIUS;

pub(super) const CONTROL_WIDTH: f32 = UiTokens::CONTROL_WIDTH;

pub(super) fn control_rect(row: Rect, width: f32) -> Rect {
    Rect::new(
        row.right() - 18.0 - width,
        row.y + (row.h - 34.0) * 0.5,
        width,
        34.0,
    )
}

pub(crate) fn value_control_rect(row: Rect, _kind: ElementKind) -> Rect {
    control_rect(row, CONTROL_WIDTH)
}

pub(crate) fn value_control_rect_for(row: Rect, id: ElementId, kind: ElementKind) -> Rect {
    let width = if id == ElementId::OverlayMonitor && kind == ElementKind::Value {
        COMPACT_MONITOR_CONTROL_WIDTH
    } else {
        CONTROL_WIDTH
    };
    control_rect(row, width)
}

pub(super) const VALUE_TEXT_PADDING: f32 = 10.0;

const VALUE_CHEVRON_RESERVE: f32 = 30.0;

pub(super) fn value_text_rect_for(row: Rect, kind: ElementKind, control_width: f32) -> Rect {
    let control = control_rect(row, control_width);
    let left = control.x + VALUE_TEXT_PADDING;
    let top = control.y + VALUE_TEXT_PADDING;
    let right_padding = if kind == ElementKind::Value {
        VALUE_CHEVRON_RESERVE
    } else {
        VALUE_TEXT_PADDING
    };
    let right = (control.right() - right_padding).max(left);
    let bottom = (control.bottom() - VALUE_TEXT_PADDING).max(top);
    Rect::new(left, top, right - left, bottom - top)
}

#[cfg(test)]
pub(crate) fn value_text_rect(row: Rect, kind: ElementKind) -> Rect {
    value_text_rect_for(row, kind, CONTROL_WIDTH)
}
