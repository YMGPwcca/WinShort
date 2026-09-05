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
use crate::ui::layout::{ElementId, SettingsLayout};
use crate::ui::navigation::Page;
use crate::ui::presentation::DisplayWizardStep;
use crate::ui::renderer::Renderer;
use std::time::Instant;

#[derive(Debug, Clone)]
pub(crate) struct ControlCenterRuntimeSnapshot {
    pub microphone: crate::audio::AudioState,
    /// Resolved capture endpoint name, when the audio worker has one.
    pub microphone_name: Option<String>,
    pub output: crate::audio::OutputState,
    pub foreground: crate::audio::AppAudioState,
    pub desktop: crate::desktop::BackendStatus,
    pub degraded: Vec<(String, String)>,
    pub display_rollback_active: bool,
    pub display_keep_available: bool,
    pub display_rollback_error: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DisplayEditorState {
    pub(super) step: DisplayWizardStep,
}

pub(crate) struct SettingsUi {
    pub(super) config_access: ConfigAccess,
    pub(super) focus: FocusState,
    pub(super) caret: SearchCaret,
    pub(super) inventory: DisplayInventory,
    pub(super) dpi: u32,

    pub(super) renderer: Option<Renderer>,
    pub(super) layout: SettingsLayout,
    pub(super) page: Page,
    pub(super) search_query: String,
    pub(super) draft: Config,
    pub(super) selected_display_route: usize,
    /// Unsaved topology/name edits stay local until the profile is kept.
    pub(super) display_draft_dirty: bool,
    /// Display profile editor remains local until the test/keep workflow ends.
    pub(super) display_editor: Option<DisplayEditorState>,
    pub(super) onboarding_step: Option<u8>,
    pub(super) startup_enabled: bool,
    pub(super) display_rollback_active: bool,
    pub(super) display_keep_available: bool,
    pub(super) runtime: ControlCenterRuntimeSnapshot,
    pub(super) devices: crate::audio::devices::DeviceLists,
    pub(super) overlay_preview_aspect: (u32, u32),
    pub(super) validation: Vec<Violation>,
    pub(super) hovered: Option<ElementId>,
    pub(super) pressed: Option<ElementId>,
    pub(super) recording_modifiers: ModifierMask,

    pub(super) recording: Option<ElementId>,
    pub(super) capture_armed: bool,
    pub(super) reset_confirm: bool,
    pub(super) delete_profile_confirm: bool,
    pub(super) scroll: f32,
    pub(super) scroll_drag_offset: Option<f32>,
    pub(super) motion: Motion,
    pub(super) applied_until: Option<Instant>,
    pub(super) closing: bool,
    pub(super) mouse_tracking: bool,
    pub(super) automation: Option<SettingsAutomation>,
}

impl SettingsUi {
    pub(super) fn new(
        dpi: u32,
        devices: crate::audio::devices::DeviceLists,
        draft: Config,
        onboarding_step: Option<u8>,
        config_access: ConfigAccess,
    ) -> Self {
        let page = Page::Home;
        Self {
            config_access,
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
            selected_display_route: 0,

            display_draft_dirty: false,
            display_editor: None,
            onboarding_step,
            startup_enabled: false,
            display_rollback_active: false,
            display_keep_available: false,
            runtime: ControlCenterRuntimeSnapshot::default(),
            devices,
            overlay_preview_aspect: (16, 9),
            validation: Vec::new(),
            hovered: None,
            pressed: None,
            recording_modifiers: ModifierMask::NONE,

            recording: None,
            capture_armed: false,
            reset_confirm: false,
            delete_profile_confirm: false,
            scroll: 0.0,
            scroll_drag_offset: None,
            motion: Motion::default(),
            applied_until: None,
            closing: false,
            mouse_tracking: false,
            automation: None,
        }
    }

    pub(super) fn begin_close(&mut self) -> bool {
        if self.closing {
            false
        } else {
            self.closing = true;
            true
        }
    }

    pub(super) fn picker_activation_allowed(&self) -> bool {
        !self.closing
    }
}
