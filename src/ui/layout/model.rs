//! Model for the layout.

use super::geometry::Rect;
use crate::ui::navigation::Page;
use crate::ui::presentation::{AllowlistMode, DisplayWizardStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum HotkeySlot {
    Microphone,
    Output,
    Foreground,
    CycleInput,
    CycleOutput,
    ForegroundVolumeUp,
    ForegroundVolumeDown,
    PreviousDesktop,
    AssignSpecial,
    ToggleSpecial,
    DisplayProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ElementId {
    Search,
    Nav(Page),
    SearchResult(u8),
    ResumeDisplayDraft,
    WindowClose,
    HomeSpeaker,
    HomeCurrentDesktop,
    HomeMicrophone,
    HomePreviousDesktop,
    HomeSpecial,
    HomeDisplayProfile,
    HomeShortcutHealth,
    HomeDiagnostics,
    DisplayProfileCard(u8),
    DisplayProfileAction(u8),
    DisplayOutputCard(u8),
    DisplayTopologyChoice(u8),
    InputCycleMode(u8),
    OutputCycleMode(u8),
    InputCycleDevice(u8),
    OutputCycleDevice(u8),
    OverlayPositionCell(u8),
    DisplayWizardBack,
    DisplayWizardNext,
    DisplayWizardCancel,
    DisplayWizardSummary,
    EditDisplayProfile,
    OnboardingContinue,
    OnboardingOpen,
    StartWithWindows,
    StartHotkeysEnabled,
    HotkeyCard(HotkeySlot),
    HotkeyEnabled(HotkeySlot),
    HotkeyUnassign(HotkeySlot),
    MicHotkey,
    OutputHotkey,
    ForegroundHotkey,
    CycleInputHotkey,
    CycleOutputHotkey,
    ForegroundVolumeUpHotkey,
    ForegroundVolumeDownHotkey,
    InputDevice,
    OutputDevice,
    InputAllowlist,
    OutputAllowlist,
    DisplayProfilesEnabled,
    DisplayProfile,
    DisplayProfileHotkey,
    DisplayOutputs,
    DisplayTopology,
    DisplayRoute,
    EditDisplayRoute,
    NewDisplayProfile,
    UpdateDisplayProfile,
    RenameDisplayProfile,
    DuplicateDisplayProfile,
    TestApplyDisplayProfile,
    DeleteDisplayProfile,
    KeepDisplayChange,
    UndoDisplayChange,
    DiscardDisplayEdits,
    DebugLogging,
    DiagnosticsStatus,
    InputRole,
    OutputRole,
    DesktopsEnabled,
    WinNumberEnabled,
    DesktopNumberModifier,
    MoveDesktopModifier,
    SilentMoveDesktopModifier,
    PreviousDesktopHotkey,
    AssignScratchpadHotkey,
    ToggleScratchpadHotkey,
    OverlayEnabled,
    OverlayAppearance,
    OverlayMicrophone,
    OverlaySpeaker,
    OverlayCurrentAppAudio,
    OverlayWorkspace,
    OverlayDisplayProfile,
    OverlayPosition,
    OverlayMonitor,
    OverlayDuration,
    OverlayBlur,
    OverlayScale,
    OverlayHoverOpacity,
    OverlayPreview,
    OpenConfigFolder,
    ResetSettings,
    CopyVersionInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElementKind {
    Toggle,
    Checkbox,
    Choice,
    Hotkey,
    Value,
    Slider,
    Action,
    ButtonSecondary,
    ButtonPrimary,
    ButtonDanger,
    Navigation,
    Search,
    Card,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegionKind {
    WorkspaceNotice,
    PauseNotice,
    AudioCurrentApp,
    OverlayPreview,
    DisplaySafety,
    DisplayWizardSteps,
    DisplayWizardSummary,
    SelectedDisplayProfile,
}

#[derive(Debug, Clone)]
pub(crate) struct Element {
    pub id: ElementId,
    pub kind: ElementKind,
    /// Rect in viewport logical coordinates after page scrolling is applied.
    pub rect: Rect,
    pub label: String,
    pub description: String,
    pub scrolls: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct VisualRegion {
    pub kind: RegionKind,
    pub rect: Rect,
    pub scrolls: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SectionLabel {
    pub title: String,
    pub description: String,
    pub y: f32,
    pub height: f32,
    pub page_header: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayoutContext {
    pub profile_count: usize,
    pub selected_profile_index: Option<usize>,
    pub display_output_count: usize,
    pub display_route_count: usize,
    pub display_editor_step: Option<DisplayWizardStep>,
    pub display_profiles_enabled: bool,
    pub display_draft_dirty: bool,
    pub display_rollback_active: bool,
    pub display_keep_available: bool,
    pub display_inventory_unknown: bool,
    pub workspace_enabled: bool,
    pub paused: bool,
    pub input_cycle_mode: AllowlistMode,
    pub output_cycle_mode: AllowlistMode,
    pub input_device_count: usize,
    pub output_device_count: usize,
    pub current_app_audio_available: bool,
    pub overlay_preview_aspect: (u32, u32),
}

impl Default for LayoutContext {
    fn default() -> Self {
        Self {
            profile_count: 0,
            selected_profile_index: None,
            display_output_count: 0,
            display_route_count: 0,
            display_editor_step: None,
            display_profiles_enabled: true,
            display_draft_dirty: false,
            display_rollback_active: false,
            display_keep_available: false,
            display_inventory_unknown: false,
            workspace_enabled: true,
            paused: false,
            input_cycle_mode: AllowlistMode::All,
            output_cycle_mode: AllowlistMode::All,
            input_device_count: 0,
            output_device_count: 0,
            current_app_audio_available: false,
            overlay_preview_aspect: (16, 9),
        }
    }
}

impl HotkeySlot {
    pub(crate) fn from_capture_id(id: ElementId) -> Option<Self> {
        Some(match id {
            ElementId::MicHotkey => Self::Microphone,
            ElementId::OutputHotkey => Self::Output,
            ElementId::ForegroundHotkey => Self::Foreground,
            ElementId::CycleInputHotkey => Self::CycleInput,
            ElementId::CycleOutputHotkey => Self::CycleOutput,
            ElementId::ForegroundVolumeUpHotkey => Self::ForegroundVolumeUp,
            ElementId::ForegroundVolumeDownHotkey => Self::ForegroundVolumeDown,
            ElementId::PreviousDesktopHotkey => Self::PreviousDesktop,
            ElementId::AssignScratchpadHotkey => Self::AssignSpecial,
            ElementId::ToggleScratchpadHotkey => Self::ToggleSpecial,
            ElementId::DisplayProfileHotkey => Self::DisplayProfile,
            _ => return None,
        })
    }
}
