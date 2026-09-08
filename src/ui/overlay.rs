//! Status overlay API. Window lifetime, state, drawing and Composition have separate owners.

mod backend;
mod composition;
mod drawing;
mod effect;
mod icons;
mod layout;
mod messages;
mod model;
mod palette;
mod presentation;
mod state;
mod timeline;
mod window;

pub(crate) use model::{OverlayIcon, OverlayModel, OverlayRow, OverlayTone};
pub(crate) use presentation::{
    application_row, application_volume_row, device_cycle_error_row, device_cycle_no_devices_row,
    device_cycle_row, microphone_row, output_row,
};
pub(crate) use window::{OverlayRuntimeStatus, OverlayWindow};

#[cfg(test)]
use self::layout::{position_for, surface_geometry};
#[cfg(test)]
use self::model::merge_overlay_models;
#[cfg(test)]
use self::palette::{
    backdrop_mode, composition_tint_alpha, palette_for, resolved_theme_mode, BackdropMode,
};
#[cfg(test)]
use self::timeline::{
    motion_policy, prepare_state_plan, timing_after_show, MotionPolicy, Phase, ShowPlan,
    WindowRegion, TIMER_MS,
};
#[cfg(test)]
use crate::config::model::{OverlayAppearance, OverlayBlur, OverlayPosition};
#[cfg(test)]
use crate::platform::visual::{SystemVisualPreferences, VisualRgb};
#[cfg(test)]
use crate::ui::theme::{Color, ThemeMode};
#[cfg(test)]
use windows::Win32::Foundation::{POINT, SIZE};

#[cfg(test)]
mod tests;
