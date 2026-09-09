//! Status overlay API. Window lifetime, state, drawing and Composition have separate owners.

mod backend;
mod composition;
mod drawing;
mod effect;
mod icons;
mod layout;
mod manager;
mod messages;
mod model;
mod palette;
mod presentation;
mod state;
mod timeline;
mod window;

pub(crate) use manager::{OverlayKey, OverlayManager, OverlayRequest};
pub(crate) use model::{OverlayIcon, OverlayModel, OverlayRow, OverlayTone};
pub(crate) use presentation::{
    application_row, application_volume_row, device_cycle_error_row, device_cycle_no_devices_row,
    device_cycle_row, microphone_row, output_row,
};
pub(crate) use window::OverlayRuntimeStatus;

#[cfg(test)]
use self::layout::{layout_cards, position_for, surface_geometry, LayoutInput};
#[cfg(test)]
use self::palette::{
    backdrop_mode, composition_tint_alpha, palette_for, resolved_theme_mode, BackdropMode,
};
#[cfg(test)]
use self::timeline::{
    motion_policy, prepare_state_plan, timing_after_show, toast_deadline, MotionPolicy, Phase,
    PositionTween, ShowMode, ShowPlan, WindowRegion, APPEAR_MS, POSITION_TWEEN_MS, TIMER_MS,
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
