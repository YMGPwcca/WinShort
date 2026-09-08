//! Immutable accessibility snapshots shared with provider threads.

use super::capabilities::node_has_value;
use crate::ui::layout::{ElementId, ElementKind, Rect as UiRect};
use windows::Win32::Foundation::POINT;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AutomationRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for AutomationRect {
    fn default() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AutomationRange {
    pub value: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub small_change: f64,
    pub large_change: f64,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SettingsAutomationNode {
    pub id: ElementId,
    pub name: String,
    pub help_text: String,
    pub enabled: bool,
    pub focused: bool,
    pub offscreen: bool,
    pub bounds: AutomationRect,
    pub kind: ElementKind,
    pub value: String,
    pub toggle: Option<bool>,
    pub range: Option<AutomationRange>,
}

pub(crate) use crate::ui::focus::FocusOwner as AutomationFocusOwner;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SettingsAutomationSnapshot {
    pub window: AutomationRect,
    pub nodes: Vec<SettingsAutomationNode>,
    pub focused: Option<ElementId>,
    pub focus_owner: AutomationFocusOwner,
    pub picker_open_for: Option<ElementId>,
    pub page: crate::ui::navigation::Page,
}

impl AutomationRect {
    pub(super) fn from_ui(rect: UiRect, scale: f64, origin: POINT) -> Self {
        Self {
            left: origin.x as f64 + rect.x as f64 * scale,
            top: origin.y as f64 + rect.y as f64 * scale,
            width: rect.w as f64 * scale,
            height: rect.h as f64 * scale,
        }
    }

    pub(super) fn contains(self, x: f64, y: f64) -> bool {
        x >= self.left && y >= self.top && x < self.left + self.width && y < self.top + self.height
    }
}

impl SettingsAutomationSnapshot {
    pub(crate) fn set_focus_state(
        &mut self,
        focus_owner: AutomationFocusOwner,
        picker_open_for: Option<ElementId>,
    ) {
        self.focus_owner = focus_owner;
        self.picker_open_for = picker_open_for;
        self.focused = self
            .focused
            .filter(|id| self.nodes.iter().any(|node| node.id == *id && node.enabled));
        for node in &mut self.nodes {
            node.focused =
                focus_owner == AutomationFocusOwner::Settings && self.focused == Some(node.id);
        }
    }
}

impl SettingsAutomationNode {
    pub(super) fn is_value_pattern_available(&self) -> bool {
        node_has_value(self.kind)
    }
}
