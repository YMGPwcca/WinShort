//! Shared layout model and page construction; no window ownership lives here.

mod advanced;
mod audio;
mod builder;
mod chrome;
mod display_wizard;
mod displays;
mod geometry;
mod home;
mod model;
mod onboarding;
mod overlay;
mod pages;
mod search;
mod shell;
mod shortcuts;
mod system;
mod workspaces;

#[cfg(test)]
mod legacy;

#[cfg(test)]
pub(crate) use chrome::brand_row_geometry;
pub(crate) use chrome::{top_chrome_geometry, top_chrome_separator_rect, TopChromeGeometry};
pub(crate) use geometry::Rect;
pub(crate) use model::{Element, ElementId, ElementKind, HotkeySlot, LayoutContext, RegionKind};
pub(crate) use overlay::{overlay_placement_geometry, overlay_preview_canvas_rect};
#[cfg(test)]
pub(crate) use overlay::{
    overlay_position_grid_rect, overlay_preview_canvas_size, overlay_status_row_rects,
};
pub(crate) use shell::SettingsLayout;

#[cfg(test)]
use self::overlay::OVERLAY_PLACEMENT_GAP;

#[cfg(test)]
use crate::ui::theme::UiTokens;

#[cfg(test)]
mod tests;
