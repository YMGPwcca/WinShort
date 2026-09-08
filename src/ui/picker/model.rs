//! Typed picker model and commit boundary.

use crate::config::model::{
    DeviceSelection, DisplayTopology, EndpointRole, MonitorChoice, OverlayAppearance,
    OverlayPosition,
};
use crate::keyboard::binding::ModifierMask;
use crate::ui::presentation::{AllowlistMode, DeviceCycleSelection};

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

/// A picker result already names the domain operation it represents. There is
/// no independent `(PickerKind, value)` pair that can describe an impossible
/// combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PickerCommit {
    InputDevice(DeviceSelection),
    OutputDevice(DeviceSelection),
    InputAllowlist(DeviceCycleSelection),
    OutputAllowlist(DeviceCycleSelection),
    DisplayProfile(Option<String>),
    DisplayOutputs(Vec<crate::display::DisplayRoute>),
    DisplayTopology(DisplayTopology),
    DisplayRoute(usize),
    InputRole(EndpointRole),
    OutputRole(EndpointRole),
    DesktopNumberModifier(ModifierMask),
    MoveDesktopModifier(ModifierMask),
    SilentMoveDesktopModifier(ModifierMask),
    OverlayPosition(OverlayPosition),
    OverlayAppearance(OverlayAppearance),
    OverlayMonitor(MonitorChoice),
}

impl PickerCommit {
    pub(crate) const fn kind(&self) -> PickerKind {
        match self {
            Self::InputDevice(_) => PickerKind::InputDevice,
            Self::OutputDevice(_) => PickerKind::OutputDevice,
            Self::InputAllowlist(_) => PickerKind::InputAllowlist,
            Self::OutputAllowlist(_) => PickerKind::OutputAllowlist,
            Self::DisplayProfile(_) => PickerKind::DisplayProfile,
            Self::DisplayOutputs(_) => PickerKind::DisplayOutputs,
            Self::DisplayTopology(_) => PickerKind::DisplayTopology,
            Self::DisplayRoute(_) => PickerKind::DisplayRoute,
            Self::InputRole(_) => PickerKind::InputRole,
            Self::OutputRole(_) => PickerKind::OutputRole,
            Self::DesktopNumberModifier(_) => PickerKind::DesktopNumberModifier,
            Self::MoveDesktopModifier(_) => PickerKind::MoveDesktopModifier,
            Self::SilentMoveDesktopModifier(_) => PickerKind::SilentMoveDesktopModifier,
            Self::OverlayPosition(_) => PickerKind::OverlayPosition,
            Self::OverlayAppearance(_) => PickerKind::OverlayAppearance,
            Self::OverlayMonitor(_) => PickerKind::OverlayMonitor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PickerChoiceValue {
    Commit(PickerCommit),
    AllowlistMode(AllowlistMode),
    AllowlistEndpoint(String),
    DisplayOutput(crate::display::DisplayRoute),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PickerChoice {
    pub(super) label: String,
    pub(super) value: PickerChoiceValue,
}

impl PickerChoice {
    pub(crate) fn commit(label: impl Into<String>, commit: PickerCommit) -> Self {
        Self {
            label: label.into(),
            value: PickerChoiceValue::Commit(commit),
        }
    }

    pub(crate) fn allowlist_mode(label: impl Into<String>, mode: AllowlistMode) -> Self {
        Self {
            label: label.into(),
            value: PickerChoiceValue::AllowlistMode(mode),
        }
    }

    pub(crate) fn allowlist_endpoint(label: impl Into<String>, endpoint: String) -> Self {
        Self {
            label: label.into(),
            value: PickerChoiceValue::AllowlistEndpoint(endpoint),
        }
    }

    pub(crate) fn display_output(
        label: impl Into<String>,
        route: crate::display::DisplayRoute,
    ) -> Self {
        Self {
            label: label.into(),
            value: PickerChoiceValue::DisplayOutput(route),
        }
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn value(&self) -> &PickerChoiceValue {
        &self.value
    }

    pub(crate) fn commit_value(&self) -> Option<&PickerCommit> {
        match &self.value {
            PickerChoiceValue::Commit(commit) => Some(commit),
            PickerChoiceValue::AllowlistMode(_)
            | PickerChoiceValue::AllowlistEndpoint(_)
            | PickerChoiceValue::DisplayOutput(_) => None,
        }
    }

    fn valid_for(&self, kind: PickerKind) -> bool {
        match &self.value {
            PickerChoiceValue::Commit(commit) => commit.kind() == kind && !kind.is_multi_select(),
            PickerChoiceValue::AllowlistMode(_) | PickerChoiceValue::AllowlistEndpoint(_) => {
                kind.is_allowlist()
            }
            PickerChoiceValue::DisplayOutput(_) => kind == PickerKind::DisplayOutputs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PickerModel {
    pub(super) kind: PickerKind,
    pub(super) choices: Vec<PickerChoice>,
    pub(super) current: Option<usize>,
    pub(super) selected_indices: Vec<usize>,
}

impl PickerModel {
    pub(crate) fn new(
        kind: PickerKind,
        choices: Vec<PickerChoice>,
        current: Option<usize>,
        selected_indices: Vec<usize>,
    ) -> Result<Self, &'static str> {
        if choices.iter().any(|choice| !choice.valid_for(kind)) {
            return Err("picker choice does not belong to picker kind");
        }
        if current.is_some_and(|index| index >= choices.len()) {
            return Err("picker current selection is out of range");
        }
        if selected_indices.iter().any(|index| *index >= choices.len()) {
            return Err("picker multi-selection is out of range");
        }
        if !kind.is_multi_select() && !selected_indices.is_empty() {
            return Err("single-select picker cannot have multi-selection indices");
        }
        Ok(Self {
            kind,
            choices,
            current,
            selected_indices,
        })
    }

    pub(crate) fn choices(&self) -> &[PickerChoice] {
        &self.choices
    }

    #[cfg(test)]
    pub(crate) const fn current(&self) -> Option<usize> {
        self.current
    }

    #[cfg(test)]
    pub(crate) fn selected_indices(&self) -> &[usize] {
        &self.selected_indices
    }

    pub(super) fn into_parts(self) -> (PickerKind, Vec<PickerChoice>, Option<usize>, Vec<usize>) {
        (self.kind, self.choices, self.current, self.selected_indices)
    }
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
