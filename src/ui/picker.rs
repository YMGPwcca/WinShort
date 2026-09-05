//! Native picker API; selection rules do not own windows or paint resources.

mod appearance;
mod focus;
mod font;
mod geometry;
mod messages;
mod model;
mod painting;
mod selection;
mod window;

pub(crate) use geometry::place_popup;
pub(crate) use model::{PickerChoice, PickerKind, PickerValue, PopupRect};
#[cfg(test)]
pub(crate) use window::{hiword, loword};
pub(crate) use window::{PickerPopup, ITEM_HEIGHT_DIP};

#[cfg(test)]
use self::appearance::{
    picker_colors_for, picker_colors_for_state, picker_item_state, PickerItemState,
};
#[cfg(test)]
use self::focus::{claim_close, picker_focus_snapshot, should_close_after_focus_loss};
#[cfg(test)]
use self::geometry::{picker_corner_diameter_px, picker_list_rect};
#[cfg(test)]
use self::window::{picker_item_height_px, PickerCloseAction, PickerUi, PICKER_HOST_STYLE};

#[cfg(test)]
use crate::platform::visual::SystemVisualPreferences;
#[cfg(test)]
use crate::ui::theme::{Color, Theme};

#[cfg(test)]
use windows::Win32::UI::WindowsAndMessaging::{WS_CHILD, WS_CLIPCHILDREN};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod wm_command_tests;

#[cfg(test)]
mod focus_loss_tests;

#[cfg(test)]
use self::font::{picker_font_height, PICKER_FONT_FALLBACK, PICKER_FONT_FAMILY};
