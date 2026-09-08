//! Control Center boundary. Commands, presentation, local state and native dispatch are distinct responsibilities.

mod appearance;
mod audio;
mod audio_view;
mod automation;
mod availability;
mod chrome;
mod commands;
mod config_access;
mod config_toggle;
mod display_actions;
mod display_edit;
mod display_inventory;
mod display_routes;
mod display_view;
mod draft;
mod focus;
mod focus_state;
mod home;
mod interaction;
mod layout;
mod messages;
mod native;
mod navigation;
mod overlay_preview;
mod pages;
mod painting;
mod picker_apply;
mod picker_choices;
mod placement;
mod scroll;
mod search;
mod settings_actions;
mod shortcuts;
mod sliders;
mod state;
mod values;
mod window;

#[cfg(test)]
pub(crate) use placement::clamp_window_rect;
pub(crate) use state::ControlCenterRuntimeSnapshot;
#[cfg(test)]
pub(crate) use state::SettingsUi;
pub(crate) use window::ControlCenterWindow;
#[cfg(test)]
pub(crate) use window::{DESIGN_HEIGHT, DESIGN_WIDTH};

#[cfg(test)]
use self::appearance::settings_theme_for;
#[cfg(test)]
use self::chrome::{blocked_fixed_window_command, chrome_hit_test_dip};
#[cfg(test)]
use self::display_routes::parse_display_route_values;
#[cfg(test)]
use self::native::close_picker_before_settings_hide;
#[cfg(test)]
use self::overlay_preview::{overlay_preview_card_rect, work_area_aspect};
#[cfg(test)]
use self::painting::APPLIED_STATUS;
#[cfg(test)]
use self::picker_choices::picker_model;
#[cfg(test)]
use self::placement::{
    client_rect_from_dip, fixed_window_size, picker_height_px, picker_width_dip,
};
#[cfg(test)]
use self::scroll::{
    page_scroll_target, scroll_after_wheel, settings_wheel_action, SettingsWheelAction,
};
#[cfg(test)]
use self::state::ConfirmationTarget;
#[cfg(test)]
use self::window::CONTROL_CENTER_STYLE;
#[cfg(test)]
use crate::config::model::{Config, DeviceSelection, MonitorChoice, OverlayPosition};
#[cfg(test)]
use crate::keyboard::binding::{Hotkey, ModifierMask, VirtualKey};
#[cfg(test)]
use crate::platform::visual::SystemVisualPreferences;
#[cfg(test)]
use crate::ui::animation::{Motion, MotionChannel};
#[cfg(test)]
use crate::ui::control_center_automation::{node_has_invoke, AutomationFocusOwner};
#[cfg(test)]
use crate::ui::controls;
#[cfg(test)]
use crate::ui::controls::ControlValue;
#[cfg(test)]
use crate::ui::layout::{ElementId, ElementKind, HotkeySlot, Rect as UiRect, SettingsLayout};
#[cfg(test)]
use crate::ui::navigation::Page;
#[cfg(test)]
use crate::ui::picker::{PickerChoice, PickerCommit, PickerKind, PopupRect};
#[cfg(test)]
use crate::ui::presentation::DisplayWizardStep;
#[cfg(test)]
use crate::ui::theme::{Color, Theme};
#[cfg(test)]
use windows::Win32::Foundation::{HWND, RECT};
#[cfg(test)]
use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, HTCLIENT};

#[cfg(test)]
mod interaction_tests;
#[cfg(test)]
mod state_tests;

pub(crate) use config_access::{ConfigAccess, ControlCenterAccess};
