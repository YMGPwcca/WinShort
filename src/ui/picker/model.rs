//! Model for the picker.

use crate::config::model::{
    DeviceSelection, DisplayTopology, EndpointRole, MonitorChoice, OverlayAppearance,
    OverlayPosition,
};
use crate::ui::presentation::AllowlistMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerKind {
    InputDevice,
    OutputDevice,
    InputAllowlist,
    OutputAllowlist,
    DisplayProfile,
    DisplayOutputs,
    DisplayTopology,
    DisplayRoute,
    InputRole,
    OutputRole,
    DesktopNumberModifier,
    MoveDesktopModifier,
    SilentMoveDesktopModifier,
    OverlayPosition,
    OverlayAppearance,
    OverlayMonitor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PickerValue {
    /// `None` means all active endpoints; `Some(empty)` means none.
    Allowlist(Option<Vec<String>>),
    /// A visual mode row in an allowlist popup. Device rows remain regular
    /// `Allowlist(Some([endpoint]))` values so the persisted representation is
    /// unchanged when the selection is committed.
    AllowlistMode(AllowlistMode),
    Device(DeviceSelection),
    Role(EndpointRole),
    DisplayProfile(Option<String>),
    DisplayOutput(crate::display::DisplayRoute),
    DisplayOutputs(Vec<crate::display::DisplayRoute>),
    DisplayTopology(DisplayTopology),
    DisplayRoute(usize),
    Modifier(crate::keyboard::binding::ModifierMask),
    Position(OverlayPosition),
    Appearance(OverlayAppearance),
    Monitor(MonitorChoice),
}

#[derive(Debug, Clone)]
pub(crate) struct PickerChoice {
    pub label: String,
    pub value: PickerValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PopupRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PickerKind {
    pub(crate) const fn is_multi_select(self) -> bool {
        matches!(
            self,
            Self::InputAllowlist | Self::OutputAllowlist | Self::DisplayOutputs
        )
    }

    pub(crate) const fn is_allowlist(self) -> bool {
        matches!(self, Self::InputAllowlist | Self::OutputAllowlist)
    }
}

impl PopupRect {
    pub(crate) const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub(crate) const fn width(self) -> i32 {
        self.right - self.left
    }

    pub(crate) const fn height(self) -> i32 {
        self.bottom - self.top
    }
}
