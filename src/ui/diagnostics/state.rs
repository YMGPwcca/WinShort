//! State for the diagnostics.

use super::model::Action;
use crate::diagnostics::snapshot::{DiagnosticsSnapshot, SelfTestReport};
use crate::ui::renderer::Renderer;

pub(super) struct DiagnosticsUi {
    pub(super) dpi: u32,
    pub(super) renderer: Option<Renderer>,
    pub(super) snapshot: DiagnosticsSnapshot,
    pub(super) self_test: Option<SelfTestReport>,
    pub(super) action_status: Option<String>,
    pub(super) bundle_running: bool,
    pub(super) scroll: f32,
    pub(super) hovered: Option<Action>,
    pub(super) pressed: Option<Action>,
    pub(super) focused: Option<Action>,
    pub(super) mouse_tracking: bool,
}

impl DiagnosticsUi {
    pub(super) fn new(dpi: u32, snapshot: DiagnosticsSnapshot) -> Self {
        Self {
            dpi,
            renderer: None,
            snapshot,
            self_test: None,
            action_status: None,
            bundle_running: false,
            scroll: 0.0,
            hovered: None,
            pressed: None,
            focused: None,
            mouse_tracking: false,
        }
    }

    pub(super) fn set_snapshot(
        &mut self,
        snapshot: DiagnosticsSnapshot,
        self_test: Option<SelfTestReport>,
    ) {
        self.snapshot = snapshot;
        self.self_test = self_test;
        self.scroll = 0.0;
    }
}
