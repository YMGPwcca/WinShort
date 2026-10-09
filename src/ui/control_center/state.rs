//! Control Center session data and construction.

use super::config_access::ConfigAccess;
use super::display_inventory::DisplayInventory;
use super::focus_state::FocusState;
use super::search::SearchCaret;
use super::window::{DESIGN_HEIGHT, DESIGN_WIDTH};
use crate::config::model::Config;
use crate::config::validate::Violation;
use crate::keyboard::binding::ModifierMask;
use crate::ui::animation::Motion;
use crate::ui::control_center_automation::SettingsAutomation;
use crate::ui::layout::{ElementId, OnboardingStep, SettingsLayout};
use crate::ui::navigation::Page;
use crate::ui::presentation::DisplayWizardStep;
use crate::ui::renderer::Renderer;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DisplayRollbackStatus {
    Inactive,
    ActionFailed(String),
    Testing { error: Option<String> },
    Recovering,
    RecoveryFailed(String),
}

impl DisplayRollbackStatus {
    pub(crate) fn active(&self) -> bool {
        matches!(
            self,
            Self::Testing { .. } | Self::Recovering | Self::RecoveryFailed(_)
        )
    }

    pub(crate) fn keep_available(&self) -> bool {
        matches!(self, Self::Testing { .. })
    }

    pub(crate) fn error(&self) -> Option<&str> {
        match self {
            Self::ActionFailed(error) | Self::RecoveryFailed(error) => Some(error),
            Self::Testing { error: Some(error) } => Some(error),
            Self::Inactive | Self::Testing { error: None } | Self::Recovering => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ControlCenterRuntimeSnapshot {
    pub microphone: crate::audio::AudioState,
    /// Resolved capture endpoint name, when the audio worker has one.
    pub microphone_name: Option<String>,
    pub output: crate::audio::OutputState,
    pub foreground: crate::audio::AppAudioState,
    pub desktop: crate::desktop::BackendStatus,
    pub degraded: Vec<(String, String)>,
    // App integration still writes these transport fields. Control Center code
    // must consume `display_rollback_status()` so invalid boolean combinations
    // do not leak into UI policy or presentation.
    pub display_rollback_active: bool,
    pub display_keep_available: bool,
    pub display_rollback_error: Option<String>,
}

impl ControlCenterRuntimeSnapshot {
    pub(crate) fn display_rollback_status(&self) -> DisplayRollbackStatus {
        match (
            self.display_rollback_active,
            self.display_keep_available,
            self.display_rollback_error.as_ref(),
        ) {
            (false, _, None) => DisplayRollbackStatus::Inactive,
            (false, _, Some(error)) => DisplayRollbackStatus::ActionFailed(error.clone()),
            (true, true, error) => DisplayRollbackStatus::Testing {
                error: error.cloned(),
            },
            (true, false, None) => DisplayRollbackStatus::Recovering,
            (true, false, Some(error)) => DisplayRollbackStatus::RecoveryFailed(error.clone()),
        }
    }

    pub(crate) fn set_display_rollback_phase(&mut self, active: bool, keep_available: bool) {
        self.display_rollback_active = active;
        self.display_keep_available = active && keep_available;
    }
}

impl Default for ControlCenterRuntimeSnapshot {
    fn default() -> Self {
        Self {
            microphone: crate::audio::AudioState::Unavailable {
                reason: "Audio is starting".into(),
            },
            microphone_name: None,
            output: crate::audio::OutputState::Unavailable {
                reason: "Audio is starting".into(),
            },
            foreground: crate::audio::AppAudioState::no_external(),
            desktop: crate::desktop::BackendStatus {
                native: crate::desktop::BackendAvailability::Failed {
                    reason: "Workspace service is starting".into(),
                },
                fallback: crate::desktop::BackendAvailability::Available,
                active: crate::desktop::BackendKind::KeyboardFallback,
                desktop_count: None,
                current_desktop: None,
                last_served: None,
            },
            degraded: Vec::new(),
            display_rollback_active: false,
            display_keep_available: false,
            display_rollback_error: None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum DisplayEditState {
    #[default]
    Closed,
    Editing {
        step: DisplayWizardStep,
        dirty: bool,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct DisplaySession {
    edit: DisplayEditState,
    selected_route: Option<usize>,
}

impl DisplaySession {
    pub(super) fn is_editing(&self) -> bool {
        matches!(self.edit, DisplayEditState::Editing { .. })
    }

    pub(super) fn is_dirty(&self) -> bool {
        matches!(self.edit, DisplayEditState::Editing { dirty: true, .. })
    }

    pub(super) fn step(&self) -> Option<DisplayWizardStep> {
        match self.edit {
            DisplayEditState::Closed => None,
            DisplayEditState::Editing { step, .. } => Some(step),
        }
    }

    pub(super) fn open(&mut self, step: DisplayWizardStep, dirty: bool) {
        self.edit = DisplayEditState::Editing { step, dirty };
    }

    pub(super) fn close(&mut self) {
        self.edit = DisplayEditState::Closed;
        self.selected_route = None;
    }

    pub(super) fn set_step(&mut self, step: DisplayWizardStep) {
        if let DisplayEditState::Editing { step: current, .. } = &mut self.edit {
            *current = step;
        }
    }

    pub(super) fn mark_dirty(&mut self) {
        if let DisplayEditState::Editing { dirty, .. } = &mut self.edit {
            *dirty = true;
        }
    }

    pub(super) fn clear_dirty(&mut self) {
        if let DisplayEditState::Editing { dirty, .. } = &mut self.edit {
            *dirty = false;
        }
    }

    pub(super) fn selected_route(&self) -> Option<usize> {
        self.selected_route
    }

    pub(super) fn select_route(&mut self, route: Option<usize>) {
        self.selected_route = route;
    }

    pub(super) fn select_first_route(&mut self, route_count: usize) {
        self.selected_route = (route_count > 0).then_some(0);
    }

    pub(super) fn clamp_selected_route(&mut self, route_count: usize) {
        self.selected_route = match (self.selected_route, route_count) {
            (_, 0) => None,
            (Some(index), count) => Some(index.min(count - 1)),
            (None, _) => None,
        };
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum HotkeyCaptureState {
    #[default]
    Idle,
    Recording {
        target: ElementId,
        modifiers: ModifierMask,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfirmationTarget {
    ResetSettings,
    DeleteDisplayProfile,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct ConfirmationState {
    pending: Option<ConfirmationTarget>,
}

impl ConfirmationState {
    pub(super) fn is_pending(&self, target: ConfirmationTarget) -> bool {
        self.pending == Some(target)
    }

    pub(super) fn request_or_consume(&mut self, target: ConfirmationTarget) -> bool {
        if self.pending == Some(target) {
            self.pending = None;
            true
        } else {
            self.pending = Some(target);
            false
        }
    }

    pub(super) fn retain_only(&mut self, target: Option<ConfirmationTarget>) {
        if self.pending != target {
            self.pending = None;
        }
    }

    pub(super) fn clear(&mut self) -> bool {
        self.pending.take().is_some()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(super) struct InteractionState {
    hovered: Option<ElementId>,
    pressed: Option<ElementId>,
    scroll_drag_offset: Option<f32>,
    mouse_tracking: bool,
    closing: bool,
    capture: HotkeyCaptureState,
    confirmations: ConfirmationState,
}

impl InteractionState {
    pub(super) fn hovered(&self) -> Option<ElementId> {
        self.hovered
    }

    pub(super) fn set_hovered(&mut self, value: Option<ElementId>) {
        self.hovered = value;
    }

    pub(super) fn pressed(&self) -> Option<ElementId> {
        self.pressed
    }

    pub(super) fn set_pressed(&mut self, value: Option<ElementId>) {
        self.pressed = value;
    }

    pub(super) fn take_pressed(&mut self) -> Option<ElementId> {
        self.pressed.take()
    }

    pub(super) fn scroll_drag_offset(&self) -> Option<f32> {
        self.scroll_drag_offset
    }

    pub(super) fn set_scroll_drag_offset(&mut self, value: Option<f32>) {
        self.scroll_drag_offset = value;
    }

    pub(super) fn take_scroll_drag_offset(&mut self) -> Option<f32> {
        self.scroll_drag_offset.take()
    }

    pub(super) fn mouse_tracking(&self) -> bool {
        self.mouse_tracking
    }

    pub(super) fn set_mouse_tracking(&mut self, value: bool) {
        self.mouse_tracking = value;
    }

    pub(super) fn begin_close(&mut self) -> bool {
        if self.closing {
            false
        } else {
            self.closing = true;
            true
        }
    }

    pub(super) fn reopen(&mut self) {
        self.closing = false;
    }

    pub(super) fn closing(&self) -> bool {
        self.closing
    }

    pub(super) fn start_capture(&mut self, target: ElementId) {
        self.capture = HotkeyCaptureState::Recording {
            target,
            modifiers: ModifierMask::NONE,
        };
    }

    pub(super) fn capture_active(&self) -> bool {
        matches!(self.capture, HotkeyCaptureState::Recording { .. })
    }

    pub(super) fn capture_target(&self) -> Option<ElementId> {
        match self.capture {
            HotkeyCaptureState::Idle => None,
            HotkeyCaptureState::Recording { target, .. } => Some(target),
        }
    }

    pub(super) fn capture_modifiers(&self) -> ModifierMask {
        match self.capture {
            HotkeyCaptureState::Idle => ModifierMask::NONE,
            HotkeyCaptureState::Recording { modifiers, .. } => modifiers,
        }
    }

    pub(super) fn set_capture_modifiers(&mut self, modifiers: ModifierMask) {
        if let HotkeyCaptureState::Recording {
            modifiers: current, ..
        } = &mut self.capture
        {
            *current = modifiers;
        }
    }

    pub(super) fn clear_capture(&mut self) -> bool {
        let was_active = self.capture_active();
        self.capture = HotkeyCaptureState::Idle;
        was_active
    }

    pub(super) fn confirmations(&self) -> &ConfirmationState {
        &self.confirmations
    }

    pub(super) fn confirmations_mut(&mut self) -> &mut ConfirmationState {
        &mut self.confirmations
    }
}

pub(crate) struct SettingsUi {
    pub(super) config_access: ConfigAccess,
    pub(super) access: super::config_access::ControlCenterAccess,
    pub(super) focus: FocusState,
    pub(super) caret: SearchCaret,
    pub(super) inventory: DisplayInventory,
    pub(super) dpi: u32,
    pub(super) renderer: Option<Renderer>,
    pub(super) layout: SettingsLayout,
    pub(super) page: Page,
    pub(super) search_query: String,
    pub(super) draft: Config,
    pub(super) display: DisplaySession,
    pub(super) onboarding_step: Option<OnboardingStep>,
    pub(super) startup_enabled: bool,
    pub(super) debug_logging_enabled: bool,
    pub(super) runtime: ControlCenterRuntimeSnapshot,
    pub(super) devices: crate::audio::devices::DeviceLists,
    pub(super) overlay_preview_aspect: (u32, u32),
    pub(super) validation: Vec<Violation>,
    pub(super) interaction: InteractionState,
    pub(super) scroll: f32,
    pub(super) motion: Motion,
    pub(super) applied_until: Option<Instant>,
    pub(super) applied_message: &'static str,
    pub(super) automation: Option<SettingsAutomation>,
}

impl SettingsUi {
    pub(super) fn new(
        dpi: u32,
        devices: crate::audio::devices::DeviceLists,
        draft: Config,
        onboarding_step: Option<OnboardingStep>,
        config_access: ConfigAccess,
        access: super::config_access::ControlCenterAccess,
    ) -> Self {
        let page = Page::Home;
        Self {
            config_access,
            access,
            inventory: DisplayInventory::Unqueried,
            caret: SearchCaret::Hidden,
            focus: FocusState::default(),
            dpi,
            renderer: None,
            layout: SettingsLayout::build_shell(
                DESIGN_WIDTH,
                DESIGN_HEIGHT,
                0.0,
                page,
                "",
                draft.display_profiles.profiles.len(),
                onboarding_step,
            ),
            page,
            search_query: String::new(),
            draft,
            display: DisplaySession::default(),
            onboarding_step,
            startup_enabled: access.startup_enabled(),
            debug_logging_enabled: access.debug_logging_enabled(),
            runtime: ControlCenterRuntimeSnapshot::default(),
            devices,
            overlay_preview_aspect: (16, 9),
            validation: Vec::new(),
            interaction: InteractionState::default(),
            scroll: 0.0,
            motion: Motion::default(),
            applied_until: None,
            applied_message: super::painting::APPLIED_STATUS,
            automation: None,
        }
    }

    pub(super) fn begin_close(&mut self) -> bool {
        self.interaction.begin_close()
    }

    pub(super) fn picker_activation_allowed(&self) -> bool {
        !self.interaction.closing()
    }

    pub(super) fn display_rollback_status(&self) -> DisplayRollbackStatus {
        self.runtime.display_rollback_status()
    }
}
