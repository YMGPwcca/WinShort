//! Stateless controls. Each control family consumes layout and interaction values.

mod audio;
mod buttons;
mod cards;
mod choices;
mod displays;
mod geometry;
mod headings;
mod icons;
mod navigation;
mod overlay;
mod row;
mod scrollbar;
mod search;
mod shortcuts;
mod sliders;
mod state;
mod surface;
mod topology;
mod values;

#[cfg(test)]
pub(crate) use audio::device_value_rect;
pub(crate) use audio::draw_device_row;
#[cfg(test)]
pub(crate) use buttons::titlebar_glyph_bounds;
pub(crate) use buttons::{
    draw_button, draw_button_style, draw_close_button, draw_close_button_rect,
};
pub(crate) use cards::{draw_home_card, draw_profile_card, draw_profile_name_row};
pub(crate) use choices::{draw_choice, draw_labeled_choice};
pub(crate) use displays::draw_display_route_card;
#[cfg(test)]
pub(crate) use geometry::value_control_rect;
pub(crate) use geometry::value_control_rect_for;
#[cfg(test)]
pub(crate) use geometry::value_text_rect;
pub(crate) use headings::{draw_page_header, draw_section_header};
#[cfg(test)]
pub(crate) use headings::{section_accent_rect, section_divider_y, section_title_text_rect};
#[cfg(test)]
pub(crate) use icons::shortcut_icon_geometry;
pub(crate) use icons::{draw_app_mark, draw_icon};
pub(crate) use navigation::draw_nav_item;
pub(crate) use overlay::draw_position_cell;
pub(crate) use row::draw_row;
pub(crate) use scrollbar::{
    draw_scrollbar, scroll_from_scrollbar_pointer, scrollbar_hit_rect, scrollbar_thumb_rect,
};
pub(crate) use search::draw_search_box;
#[cfg(test)]
pub(crate) use search::{search_caret_rect, search_text_rect};
pub(crate) use shortcuts::{draw_hotkey_card, draw_hotkey_keycap};
#[cfg(test)]
pub(crate) use sliders::slider_cluster_geometry;
pub(crate) use sliders::slider_track_rect;
#[cfg(test)]
pub(crate) use state::{interaction_state, InteractionState};
pub(crate) use state::{ButtonStyle, ControlValue, IconKind, Interaction};
pub(crate) use topology::draw_topology_choice;

#[cfg(test)]
use self::geometry::{value_text_rect_for, CONTROL_WIDTH, VALUE_TEXT_PADDING};
#[cfg(test)]
use self::row::{row_control_width, BODY_LEFT, COMPACT_MONITOR_CONTROL_WIDTH};

#[cfg(test)]
use crate::ui::theme::UiTokens;

#[cfg(test)]
mod tests;
