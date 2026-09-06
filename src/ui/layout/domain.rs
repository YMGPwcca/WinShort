//! Typed domain classification for flat layout identities.
//!
//! `ElementId` remains the stable geometry/UIA identity. Command and policy
//! code consume the narrower types below, so family membership is mapped once
//! and exhaustively instead of being copied across modules.

use super::{ElementId, HotkeySlot};
use crate::ui::navigation::Page;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElementDomain {
    Shell(ShellElement),
    Home(HomeElement),
    Audio(AudioElement),
    Displays(DisplayElement),
    Shortcuts(ShortcutElement),
    Workspaces(WorkspaceElement),
    Overlay(OverlayElement),
    System(SystemElement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellElement {
    Search,
    Nav(Page),
    SearchResult(u8),
    WindowClose,
    OnboardingContinue,
    OnboardingOpen,
    Cancel,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HomeElement {
    Speaker,
    CurrentDesktop,
    Microphone,
    PreviousDesktop,
    Special,
    DisplayProfile,
    ShortcutHealth,
    Diagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioElement {
    InputCycleMode(u8),
    OutputCycleMode(u8),
    InputCycleDevice(u8),
    OutputCycleDevice(u8),
    InputDevice,
    OutputDevice,
    InputAllowlist,
    OutputAllowlist,
    InputRole,
    OutputRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayElement {
    ProfileCard(u8),
    OutputCard(u8),
    TopologyChoice(u8),
    WizardBack,
    WizardNext,
    WizardCancel,
    WizardSummary,
    ProfilesEnabled,
    EditProfile,
    Profile,
    Outputs,
    Topology,
    Route,
    EditRoute,
    NewProfile,
    UpdateProfile,
    RenameProfile,
    DuplicateProfile,
    TestApply,
    Apply,
    DeleteProfile,
    KeepChange,
    UndoChange,
    DiscardEdits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutElement {
    Card(HotkeySlot),
    Enabled(HotkeySlot),
    Unassign(HotkeySlot),
    Capture(ShortcutCaptureElement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutCaptureElement {
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

impl ShortcutCaptureElement {
    pub(crate) const fn id(self) -> ElementId {
        match self {
            Self::Microphone => ElementId::MicHotkey,
            Self::Output => ElementId::OutputHotkey,
            Self::Foreground => ElementId::ForegroundHotkey,
            Self::CycleInput => ElementId::CycleInputHotkey,
            Self::CycleOutput => ElementId::CycleOutputHotkey,
            Self::ForegroundVolumeUp => ElementId::ForegroundVolumeUpHotkey,
            Self::ForegroundVolumeDown => ElementId::ForegroundVolumeDownHotkey,
            Self::PreviousDesktop => ElementId::PreviousDesktopHotkey,
            Self::AssignSpecial => ElementId::AssignScratchpadHotkey,
            Self::ToggleSpecial => ElementId::ToggleScratchpadHotkey,
            Self::DisplayProfile => ElementId::DisplayProfileHotkey,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceElement {
    Enabled,
    WinNumberEnabled,
    DesktopNumberModifier,
    MoveDesktopModifier,
    SilentMoveDesktopModifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayElement {
    Enabled,
    ExternalChanges,
    PositionCell(u8),
    Appearance,
    Position,
    Monitor,
    Duration,
    Opacity,
    Scale,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SystemElement {
    StartWithWindows,
    StartHotkeysEnabled,
    DebugLogging,
    DiagnosticsStatus,
    OpenConfigFolder,
    ResetSettings,
}

impl ElementId {
    pub(crate) const fn domain(self) -> ElementDomain {
        match self {
            Self::Search => ElementDomain::Shell(ShellElement::Search),
            Self::Nav(page) => ElementDomain::Shell(ShellElement::Nav(page)),
            Self::SearchResult(index) => ElementDomain::Shell(ShellElement::SearchResult(index)),
            Self::WindowClose => ElementDomain::Shell(ShellElement::WindowClose),
            Self::OnboardingContinue => ElementDomain::Shell(ShellElement::OnboardingContinue),
            Self::OnboardingOpen => ElementDomain::Shell(ShellElement::OnboardingOpen),
            Self::Cancel => ElementDomain::Shell(ShellElement::Cancel),
            Self::Save => ElementDomain::Shell(ShellElement::Save),

            Self::HomeSpeaker => ElementDomain::Home(HomeElement::Speaker),
            Self::HomeCurrentDesktop => ElementDomain::Home(HomeElement::CurrentDesktop),
            Self::HomeMicrophone => ElementDomain::Home(HomeElement::Microphone),
            Self::HomePreviousDesktop => ElementDomain::Home(HomeElement::PreviousDesktop),
            Self::HomeSpecial => ElementDomain::Home(HomeElement::Special),
            Self::HomeDisplayProfile => ElementDomain::Home(HomeElement::DisplayProfile),
            Self::HomeShortcutHealth => ElementDomain::Home(HomeElement::ShortcutHealth),
            Self::HomeDiagnostics => ElementDomain::Home(HomeElement::Diagnostics),

            Self::InputCycleMode(index) => {
                ElementDomain::Audio(AudioElement::InputCycleMode(index))
            }
            Self::OutputCycleMode(index) => {
                ElementDomain::Audio(AudioElement::OutputCycleMode(index))
            }
            Self::InputCycleDevice(index) => {
                ElementDomain::Audio(AudioElement::InputCycleDevice(index))
            }
            Self::OutputCycleDevice(index) => {
                ElementDomain::Audio(AudioElement::OutputCycleDevice(index))
            }
            Self::InputDevice => ElementDomain::Audio(AudioElement::InputDevice),
            Self::OutputDevice => ElementDomain::Audio(AudioElement::OutputDevice),
            Self::InputAllowlist => ElementDomain::Audio(AudioElement::InputAllowlist),
            Self::OutputAllowlist => ElementDomain::Audio(AudioElement::OutputAllowlist),
            Self::InputRole => ElementDomain::Audio(AudioElement::InputRole),
            Self::OutputRole => ElementDomain::Audio(AudioElement::OutputRole),

            Self::DisplayProfileCard(index) => {
                ElementDomain::Displays(DisplayElement::ProfileCard(index))
            }
            Self::DisplayOutputCard(index) => {
                ElementDomain::Displays(DisplayElement::OutputCard(index))
            }
            Self::DisplayTopologyChoice(index) => {
                ElementDomain::Displays(DisplayElement::TopologyChoice(index))
            }
            Self::DisplayWizardBack => ElementDomain::Displays(DisplayElement::WizardBack),
            Self::DisplayWizardNext => ElementDomain::Displays(DisplayElement::WizardNext),
            Self::DisplayWizardCancel => ElementDomain::Displays(DisplayElement::WizardCancel),
            Self::DisplayWizardSummary => ElementDomain::Displays(DisplayElement::WizardSummary),
            Self::DisplayProfilesEnabled => {
                ElementDomain::Displays(DisplayElement::ProfilesEnabled)
            }
            Self::EditDisplayProfile => ElementDomain::Displays(DisplayElement::EditProfile),
            Self::DisplayProfile => ElementDomain::Displays(DisplayElement::Profile),
            Self::DisplayOutputs => ElementDomain::Displays(DisplayElement::Outputs),
            Self::DisplayTopology => ElementDomain::Displays(DisplayElement::Topology),
            Self::DisplayRoute => ElementDomain::Displays(DisplayElement::Route),
            Self::EditDisplayRoute => ElementDomain::Displays(DisplayElement::EditRoute),
            Self::NewDisplayProfile => ElementDomain::Displays(DisplayElement::NewProfile),
            Self::UpdateDisplayProfile => ElementDomain::Displays(DisplayElement::UpdateProfile),
            Self::RenameDisplayProfile => ElementDomain::Displays(DisplayElement::RenameProfile),
            Self::DuplicateDisplayProfile => {
                ElementDomain::Displays(DisplayElement::DuplicateProfile)
            }
            Self::TestApplyDisplayProfile => ElementDomain::Displays(DisplayElement::TestApply),
            Self::ApplyDisplayProfile => ElementDomain::Displays(DisplayElement::Apply),
            Self::DeleteDisplayProfile => ElementDomain::Displays(DisplayElement::DeleteProfile),
            Self::KeepDisplayChange => ElementDomain::Displays(DisplayElement::KeepChange),
            Self::UndoDisplayChange => ElementDomain::Displays(DisplayElement::UndoChange),
            Self::DiscardDisplayEdits => ElementDomain::Displays(DisplayElement::DiscardEdits),

            Self::HotkeyCard(slot) => ElementDomain::Shortcuts(ShortcutElement::Card(slot)),
            Self::HotkeyEnabled(slot) => ElementDomain::Shortcuts(ShortcutElement::Enabled(slot)),
            Self::HotkeyUnassign(slot) => ElementDomain::Shortcuts(ShortcutElement::Unassign(slot)),
            Self::MicHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::Microphone,
            )),
            Self::OutputHotkey => {
                ElementDomain::Shortcuts(ShortcutElement::Capture(ShortcutCaptureElement::Output))
            }
            Self::ForegroundHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::Foreground,
            )),
            Self::CycleInputHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::CycleInput,
            )),
            Self::CycleOutputHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::CycleOutput,
            )),
            Self::ForegroundVolumeUpHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::ForegroundVolumeUp,
            )),
            Self::ForegroundVolumeDownHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::ForegroundVolumeDown,
            )),
            Self::PreviousDesktopHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::PreviousDesktop,
            )),
            Self::AssignScratchpadHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::AssignSpecial,
            )),
            Self::ToggleScratchpadHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::ToggleSpecial,
            )),
            Self::DisplayProfileHotkey => ElementDomain::Shortcuts(ShortcutElement::Capture(
                ShortcutCaptureElement::DisplayProfile,
            )),

            Self::DesktopsEnabled => ElementDomain::Workspaces(WorkspaceElement::Enabled),
            Self::WinNumberEnabled => ElementDomain::Workspaces(WorkspaceElement::WinNumberEnabled),
            Self::DesktopNumberModifier => {
                ElementDomain::Workspaces(WorkspaceElement::DesktopNumberModifier)
            }
            Self::MoveDesktopModifier => {
                ElementDomain::Workspaces(WorkspaceElement::MoveDesktopModifier)
            }
            Self::SilentMoveDesktopModifier => {
                ElementDomain::Workspaces(WorkspaceElement::SilentMoveDesktopModifier)
            }

            Self::OverlayEnabled => ElementDomain::Overlay(OverlayElement::Enabled),
            Self::OverlayExternalChanges => ElementDomain::Overlay(OverlayElement::ExternalChanges),
            Self::OverlayPositionCell(index) => {
                ElementDomain::Overlay(OverlayElement::PositionCell(index))
            }
            Self::OverlayAppearance => ElementDomain::Overlay(OverlayElement::Appearance),
            Self::OverlayPosition => ElementDomain::Overlay(OverlayElement::Position),
            Self::OverlayMonitor => ElementDomain::Overlay(OverlayElement::Monitor),
            Self::OverlayDuration => ElementDomain::Overlay(OverlayElement::Duration),
            Self::OverlayOpacity => ElementDomain::Overlay(OverlayElement::Opacity),
            Self::OverlayScale => ElementDomain::Overlay(OverlayElement::Scale),
            Self::OverlayPreview => ElementDomain::Overlay(OverlayElement::Preview),

            Self::StartWithWindows => ElementDomain::System(SystemElement::StartWithWindows),
            Self::StartHotkeysEnabled => ElementDomain::System(SystemElement::StartHotkeysEnabled),
            Self::DebugLogging => ElementDomain::System(SystemElement::DebugLogging),
            Self::DiagnosticsStatus => ElementDomain::System(SystemElement::DiagnosticsStatus),
            Self::OpenConfigFolder => ElementDomain::System(SystemElement::OpenConfigFolder),
            Self::ResetSettings => ElementDomain::System(SystemElement::ResetSettings),
        }
    }
}
