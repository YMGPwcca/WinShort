//! Strongly typed application events (spec §44) and the transport used
//! to move them across thread boundaries: an owned queue woken by
//! `PostMessageW`, plus WPARAM packing for keyboard actions.

use crate::audio::state::{
    AppAudioState, AppVolumeState, AudioState, DeviceCycleResult, OutputState,
};
use crate::desktop::backend::BackendStatus;
use std::collections::VecDeque;
use std::sync::Mutex;

pub const WM_APP_TRAY: u32 = 0x8000; // WM_APP + 0: tray callback notifications
pub const WM_APP_ACTION: u32 = 0x8001; // keyboard hook recognized a binding
pub const WM_APP_EVENT: u32 = 0x8002; // wake-only; payload lives in [`EVENTS`]
/// Harness-only messages used to exercise the real UI paths in an isolated
/// release-process acceptance run.
pub const WM_APP_UI_ACCEPTANCE_SHOW_DETERMINISTIC_OVERLAY: u32 = 0x8003;
pub const WM_APP_UI_ACCEPTANCE_HIDE_ALL_OVERLAYS: u32 = 0x8004;
pub const WM_APP_UI_ACCEPTANCE_SHOW_MULTI_OVERLAY: u32 = 0x8005;
pub const WM_APP_UI_ACCEPTANCE_REPLACE_SPEAKER_OVERLAY: u32 = 0x8006;

/// Actions produced by the keyboard engine. Small enough to pack into a WPARAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleMicrophone,
    ToggleOutput,
    ToggleForegroundAppAudio,
    /// Cycle WinShort's configured capture endpoint.
    CycleInputDevice,
    /// Cycle WinShort's configured render endpoint.
    CycleOutputDevice,
    /// Raise the foreground application's session volume by five percentage points.
    ForegroundVolumeUp,
    /// Lower the foreground application's session volume by five percentage points.
    ForegroundVolumeDown,
    /// Virtual desktop index, 0-based internally (desktops are numbered 1..=9).
    SwitchDesktop(u8),
    /// Switch to the previously active virtual desktop.
    SwitchPreviousDesktop,
    /// Move the foreground view to a numbered desktop and follow it.
    MoveForegroundToDesktop(u8),
    /// Move the foreground view silently without switching desktops.
    MoveForegroundToDesktopSilent(u8),
    /// Assign the current foreground window to the runtime scratchpad.
    AssignScratchpad,
    /// Toggle the runtime scratchpad window.
    ToggleScratchpad,
    /// Apply a confirmed profile-hotkey binding by stable profile-ID key.
    ApplyDisplayProfile(u16),
}

impl HotkeyAction {
    /// Fixed-width 32-bit encoding (issue #28): kind in the high half and
    /// arguments in the low 16 bits. Desktop arguments remain bounded to 1..9.
    pub fn pack(self) -> usize {
        (self.pack_u32()) as usize
    }

    pub fn pack_u32(self) -> u32 {
        const KIND_MIC: u32 = 1;
        const KIND_OUT: u32 = 2;
        const KIND_FG: u32 = 3;
        const KIND_DESKTOP: u32 = 4;
        const KIND_CYCLE_INPUT: u32 = 5;
        const KIND_CYCLE_OUTPUT: u32 = 6;
        const KIND_FOREGROUND_VOLUME_UP: u32 = 7;
        const KIND_FOREGROUND_VOLUME_DOWN: u32 = 8;
        const KIND_PREVIOUS_DESKTOP: u32 = 9;
        const KIND_MOVE_FOREGROUND: u32 = 10;
        const KIND_MOVE_FOREGROUND_SILENT: u32 = 11;
        const KIND_ASSIGN_SCRATCHPAD: u32 = 12;
        const KIND_TOGGLE_SCRATCHPAD: u32 = 13;
        const KIND_APPLY_DISPLAY_PROFILE: u32 = 14;
        match self {
            HotkeyAction::ToggleMicrophone => KIND_MIC << 16,
            HotkeyAction::ToggleOutput => KIND_OUT << 16,
            HotkeyAction::ToggleForegroundAppAudio => KIND_FG << 16,
            HotkeyAction::SwitchDesktop(n) => (KIND_DESKTOP << 16) | n as u32,
            HotkeyAction::CycleInputDevice => KIND_CYCLE_INPUT << 16,
            HotkeyAction::CycleOutputDevice => KIND_CYCLE_OUTPUT << 16,
            HotkeyAction::ForegroundVolumeUp => KIND_FOREGROUND_VOLUME_UP << 16,
            HotkeyAction::ForegroundVolumeDown => KIND_FOREGROUND_VOLUME_DOWN << 16,
            HotkeyAction::SwitchPreviousDesktop => KIND_PREVIOUS_DESKTOP << 16,
            HotkeyAction::MoveForegroundToDesktop(n) => (KIND_MOVE_FOREGROUND << 16) | n as u32,
            HotkeyAction::MoveForegroundToDesktopSilent(n) => {
                (KIND_MOVE_FOREGROUND_SILENT << 16) | n as u32
            }
            HotkeyAction::AssignScratchpad => KIND_ASSIGN_SCRATCHPAD << 16,
            HotkeyAction::ToggleScratchpad => KIND_TOGGLE_SCRATCHPAD << 16,
            HotkeyAction::ApplyDisplayProfile(index) => {
                (KIND_APPLY_DISPLAY_PROFILE << 16) | index as u32
            }
        }
    }

    /// Inverse of [`pack`]; returns None for foreign messages.
    pub fn unpack(wparam: usize) -> Option<Self> {
        Self::unpack_u32(wparam as u32)
    }

    pub fn unpack_u32(word: u32) -> Option<Self> {
        let kind = word >> 16;
        let arg = word as u16;
        match kind {
            1 => Some(HotkeyAction::ToggleMicrophone),
            2 => Some(HotkeyAction::ToggleOutput),
            3 => Some(HotkeyAction::ToggleForegroundAppAudio),
            4 if arg < 9 => Some(HotkeyAction::SwitchDesktop(arg as u8)),
            5 => Some(HotkeyAction::CycleInputDevice),
            6 => Some(HotkeyAction::CycleOutputDevice),
            7 => Some(HotkeyAction::ForegroundVolumeUp),
            8 => Some(HotkeyAction::ForegroundVolumeDown),
            9 => Some(HotkeyAction::SwitchPreviousDesktop),
            10 if arg < 9 => Some(HotkeyAction::MoveForegroundToDesktop(arg as u8)),
            11 if arg < 9 => Some(HotkeyAction::MoveForegroundToDesktopSilent(arg as u8)),
            12 => Some(HotkeyAction::AssignScratchpad),
            13 => Some(HotkeyAction::ToggleScratchpad),
            14 if arg != 0 => Some(HotkeyAction::ApplyDisplayProfile(arg)),
            _ => None,
        }
    }
}

pub use crate::config::ConfigCommitOrigin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEventOrigin {
    Initial,
    External,
    Config(ConfigCommitOrigin),
    WinShortAction(u64),
    StatusRequest(u64),
    /// Read-only state for the selected external application, without a toast.
    ForegroundSelection(u64),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopActionKind {
    Switched,
    MovedAndFollowed,
    MovedSilently,
    Previous,
    SentToSpecial,
    EnteredSpecial,
    LeftSpecial,
}

/// Events routed through the main window. Payloads live in the process-wide
/// queue; `PostMessageW` carries only a wake-up message or packed action.
#[derive(Debug)]
pub enum AppEvent {
    // Commands executed on the main thread.
    ShowSettings,
    SwitchPreviousDesktopFromUi,
    ToggleSpecialWorkspaceFromUi,
    ShowDiagnostics,
    OpenSettingsPicker(crate::ui::picker::PickerKind),
    OpenConfigFolder,
    ShowStatusOverlay,
    /// Render the Settings draft overlay without persisting it.
    PreviewOverlay {
        config: crate::config::model::OverlayCfg,
    },
    /// A card's own native timer completed its leave animation.
    OverlayCardExpired {
        entry_id: u64,
        generation: u64,
    },
    /// Recompute all card geometry after a system visual/DPI change.
    OverlayVisualRefresh,
    FocusSettingsFromPicker {
        reverse: bool,
    },
    CommitSettingsPicker {
        commit: crate::ui::picker::PickerCommit,
    },
    CancelSettingsPicker {
        popup_hwnd: isize,
        restore_focus: bool,
    },
    ControlCenterWindowClosed,
    OpenDisplayRenamePrompt {
        profile_id: String,
        current_name: String,
    },
    DisplayProfileRenameSubmitted {
        profile_id: String,
        name: String,
    },
    OpenDisplayRouteEditPrompt {
        profile_id: String,
        route_index: usize,
        initial: String,
    },
    DisplayProfileRouteEditSubmitted {
        profile_id: String,
        route_index: usize,
        value: String,
    },
    DisplayProfileRenameCancelled,
    TestApplyDisplayProfile {
        profile: crate::config::model::DisplayProfile,
    },
    ApplyDisplayProfile {
        profile: crate::config::model::DisplayProfile,
    },
    KeepDisplayProfile,
    RevertDisplayProfile,
    RunDiagnosticsSelfTest,
    CopyDiagnostics,
    OpenDiagnosticsLogs,
    DiagnosticsLogsOpenFinished {
        error: Option<String>,
    },
    CreateSupportBundle,
    ConfigApplied {
        seq: u64,
        stamp: crate::config::ConfigRevisionStamp,
    },
    /// Event-driven foreground HWND sample for desktop focus bookkeeping.
    ForegroundWindowChanged {
        hwnd_raw: isize,
    },
    DesktopActionCompleted {
        kind: DesktopActionKind,
    },
    DesktopActionFailed {
        action: String,
        reason: String,
    },
    DeviceCycleResolved {
        request_id: u64,
        result: DeviceCycleResult,
    },
    // State published by workers / callbacks.
    MicrophoneStateChanged {
        state: AudioState,
        origin: AudioEventOrigin,
    },
    OutputStateChanged {
        state: OutputState,
        origin: AudioEventOrigin,
    },
    /// Legacy default-output notification retained for worker compatibility.
    /// The application deliberately does not surface a duplicate endpoint OSD.
    DefaultOutputChanged(crate::audio::state::DeviceId),
    ForegroundAudioChanged {
        pid: Option<u32>,
        state: AppAudioState,
        origin: AudioEventOrigin,
    },
    MutedApplicationsChanged {
        applications: Vec<crate::audio::state::ApplicationAudioInfo>,
    },
    ForegroundVolumeChanged {
        pid: Option<u32>,
        state: AppVolumeState,
        origin: AudioEventOrigin,
    },
    /// Device list changed (added/removed/default switched): refresh pickers.
    DevicesChanged,
    DesktopBackendChanged(BackendStatus),
    SupportBundleFinished {
        path: Option<std::path::PathBuf>,
        error: Option<String>,
    },
}

#[derive(Debug)]
pub(crate) enum RoutedAppEvent {
    ControlCenter(ControlCenterEvent),
    Display(DisplayEvent),
    Diagnostics(DiagnosticsEvent),
    Overlay(OverlayEvent),
    Config(ConfigEvent),
    Desktop(DesktopEvent),
    Audio(AudioRuntimeEvent),
}

#[derive(Debug)]
pub(crate) enum ControlCenterEvent {
    Show,
    OpenPicker(crate::ui::picker::PickerKind),
    OpenConfigFolder,
    FocusFromPicker {
        reverse: bool,
    },
    CommitPicker {
        commit: crate::ui::picker::PickerCommit,
    },
    CancelPicker {
        popup_hwnd: isize,
        restore_focus: bool,
    },
    WindowClosed,
}

#[derive(Debug)]
pub(crate) enum DisplayEvent {
    OpenRenamePrompt {
        profile_id: String,
        current_name: String,
    },
    RenameSubmitted {
        profile_id: String,
        name: String,
    },
    RenameCancelled,
    OpenRouteEditPrompt {
        profile_id: String,
        route_index: usize,
        initial: String,
    },
    RouteEditSubmitted {
        profile_id: String,
        route_index: usize,
        value: String,
    },
    TestApply {
        profile: crate::config::model::DisplayProfile,
    },
    Apply {
        profile: crate::config::model::DisplayProfile,
    },
    Keep,
    Revert,
}

#[derive(Debug)]
pub(crate) enum DiagnosticsEvent {
    Show,
    RunSelfTest,
    Copy,
    OpenLogs,
    LogsOpenFinished {
        error: Option<String>,
    },
    CreateSupportBundle,
    SupportBundleFinished {
        path: Option<std::path::PathBuf>,
        error: Option<String>,
    },
}

#[derive(Debug)]
pub(crate) enum OverlayEvent {
    ShowStatus,
    Preview {
        config: crate::config::model::OverlayCfg,
    },
    CardExpired {
        entry_id: u64,
        generation: u64,
    },
    VisualRefresh,
}

#[derive(Debug)]
pub(crate) enum ConfigEvent {
    Applied {
        seq: u64,
        stamp: crate::config::ConfigRevisionStamp,
    },
}

#[derive(Debug)]
pub(crate) enum DesktopEvent {
    SwitchPreviousFromUi,
    ToggleSpecialFromUi,
    ForegroundWindowChanged { hwnd_raw: isize },
    ActionCompleted { kind: DesktopActionKind },
    ActionFailed { action: String, reason: String },
    BackendChanged(BackendStatus),
}

#[derive(Debug)]
pub(crate) enum AudioRuntimeEvent {
    DeviceCycleResolved {
        request_id: u64,
        result: DeviceCycleResult,
    },
    MicrophoneStateChanged {
        state: AudioState,
        origin: AudioEventOrigin,
    },
    OutputStateChanged {
        state: OutputState,
        origin: AudioEventOrigin,
    },
    DefaultOutputChanged(crate::audio::state::DeviceId),
    DevicesChanged,
    ForegroundAudioChanged {
        pid: Option<u32>,
        state: AppAudioState,
        origin: AudioEventOrigin,
    },
    MutedApplicationsChanged {
        applications: Vec<crate::audio::state::ApplicationAudioInfo>,
    },
    ForegroundVolumeChanged {
        pid: Option<u32>,
        state: AppVolumeState,
        origin: AudioEventOrigin,
    },
}

impl From<AppEvent> for RoutedAppEvent {
    fn from(event: AppEvent) -> Self {
        match event {
            AppEvent::ShowSettings => Self::ControlCenter(ControlCenterEvent::Show),
            AppEvent::OpenSettingsPicker(kind) => {
                Self::ControlCenter(ControlCenterEvent::OpenPicker(kind))
            }
            AppEvent::OpenConfigFolder => Self::ControlCenter(ControlCenterEvent::OpenConfigFolder),
            AppEvent::FocusSettingsFromPicker { reverse } => {
                Self::ControlCenter(ControlCenterEvent::FocusFromPicker { reverse })
            }
            AppEvent::CommitSettingsPicker { commit } => {
                Self::ControlCenter(ControlCenterEvent::CommitPicker { commit })
            }
            AppEvent::CancelSettingsPicker {
                popup_hwnd,
                restore_focus,
            } => Self::ControlCenter(ControlCenterEvent::CancelPicker {
                popup_hwnd,
                restore_focus,
            }),
            AppEvent::ControlCenterWindowClosed => {
                Self::ControlCenter(ControlCenterEvent::WindowClosed)
            }
            AppEvent::OpenDisplayRenamePrompt {
                profile_id,
                current_name,
            } => Self::Display(DisplayEvent::OpenRenamePrompt {
                profile_id,
                current_name,
            }),
            AppEvent::DisplayProfileRenameSubmitted { profile_id, name } => {
                Self::Display(DisplayEvent::RenameSubmitted { profile_id, name })
            }
            AppEvent::DisplayProfileRenameCancelled => Self::Display(DisplayEvent::RenameCancelled),
            AppEvent::OpenDisplayRouteEditPrompt {
                profile_id,
                route_index,
                initial,
            } => Self::Display(DisplayEvent::OpenRouteEditPrompt {
                profile_id,
                route_index,
                initial,
            }),
            AppEvent::DisplayProfileRouteEditSubmitted {
                profile_id,
                route_index,
                value,
            } => Self::Display(DisplayEvent::RouteEditSubmitted {
                profile_id,
                route_index,
                value,
            }),
            AppEvent::TestApplyDisplayProfile { profile } => {
                Self::Display(DisplayEvent::TestApply { profile })
            }
            AppEvent::ApplyDisplayProfile { profile } => {
                Self::Display(DisplayEvent::Apply { profile })
            }
            AppEvent::KeepDisplayProfile => Self::Display(DisplayEvent::Keep),
            AppEvent::RevertDisplayProfile => Self::Display(DisplayEvent::Revert),
            AppEvent::ShowDiagnostics => Self::Diagnostics(DiagnosticsEvent::Show),
            AppEvent::RunDiagnosticsSelfTest => Self::Diagnostics(DiagnosticsEvent::RunSelfTest),
            AppEvent::CopyDiagnostics => Self::Diagnostics(DiagnosticsEvent::Copy),
            AppEvent::OpenDiagnosticsLogs => Self::Diagnostics(DiagnosticsEvent::OpenLogs),
            AppEvent::DiagnosticsLogsOpenFinished { error } => {
                Self::Diagnostics(DiagnosticsEvent::LogsOpenFinished { error })
            }
            AppEvent::CreateSupportBundle => {
                Self::Diagnostics(DiagnosticsEvent::CreateSupportBundle)
            }
            AppEvent::SupportBundleFinished { path, error } => {
                Self::Diagnostics(DiagnosticsEvent::SupportBundleFinished { path, error })
            }
            AppEvent::ShowStatusOverlay => Self::Overlay(OverlayEvent::ShowStatus),
            AppEvent::PreviewOverlay { config } => Self::Overlay(OverlayEvent::Preview { config }),
            AppEvent::OverlayCardExpired {
                entry_id,
                generation,
            } => Self::Overlay(OverlayEvent::CardExpired {
                entry_id,
                generation,
            }),
            AppEvent::OverlayVisualRefresh => Self::Overlay(OverlayEvent::VisualRefresh),
            AppEvent::ConfigApplied { seq, stamp } => {
                Self::Config(ConfigEvent::Applied { seq, stamp })
            }
            AppEvent::SwitchPreviousDesktopFromUi => {
                Self::Desktop(DesktopEvent::SwitchPreviousFromUi)
            }
            AppEvent::ToggleSpecialWorkspaceFromUi => {
                Self::Desktop(DesktopEvent::ToggleSpecialFromUi)
            }
            AppEvent::ForegroundWindowChanged { hwnd_raw } => {
                Self::Desktop(DesktopEvent::ForegroundWindowChanged { hwnd_raw })
            }
            AppEvent::DesktopActionCompleted { kind } => {
                Self::Desktop(DesktopEvent::ActionCompleted { kind })
            }
            AppEvent::DesktopActionFailed { action, reason } => {
                Self::Desktop(DesktopEvent::ActionFailed { action, reason })
            }
            AppEvent::DesktopBackendChanged(status) => {
                Self::Desktop(DesktopEvent::BackendChanged(status))
            }
            AppEvent::DeviceCycleResolved { request_id, result } => {
                Self::Audio(AudioRuntimeEvent::DeviceCycleResolved { request_id, result })
            }
            AppEvent::MicrophoneStateChanged { state, origin } => {
                Self::Audio(AudioRuntimeEvent::MicrophoneStateChanged { state, origin })
            }
            AppEvent::OutputStateChanged { state, origin } => {
                Self::Audio(AudioRuntimeEvent::OutputStateChanged { state, origin })
            }
            AppEvent::DefaultOutputChanged(device) => {
                Self::Audio(AudioRuntimeEvent::DefaultOutputChanged(device))
            }
            AppEvent::DevicesChanged => Self::Audio(AudioRuntimeEvent::DevicesChanged),
            AppEvent::ForegroundAudioChanged { pid, state, origin } => {
                Self::Audio(AudioRuntimeEvent::ForegroundAudioChanged { pid, state, origin })
            }
            AppEvent::MutedApplicationsChanged { applications } => {
                Self::Audio(AudioRuntimeEvent::MutedApplicationsChanged { applications })
            }
            AppEvent::ForegroundVolumeChanged { pid, state, origin } => {
                Self::Audio(AudioRuntimeEvent::ForegroundVolumeChanged { pid, state, origin })
            }
        }
    }
}

/// Process-wide event queue. Producers push from any thread; the main thread
/// drains on every `WM_APP_EVENT` wake. No heap pointers cross `PostMessageW`.
pub struct EventQueue(Mutex<VecDeque<AppEvent>>);

impl EventQueue {
    pub const fn new() -> Self {
        Self(Mutex::new(VecDeque::new()))
    }

    pub fn push(&self, ev: AppEvent) {
        // Lock poisoning cannot leave the deque invalid; recover the data.
        match self.0.lock() {
            Ok(mut q) => q.push_back(ev),
            Err(p) => p.into_inner().push_back(ev),
        }
    }

    /// Remove all queued events in posting order.
    pub fn drain(&self) -> Vec<AppEvent> {
        match self.0.lock() {
            Ok(mut q) => q.drain(..).collect(),
            Err(p) => p.into_inner().drain(..).collect(),
        }
    }
}

pub static EVENTS: std::sync::OnceLock<EventQueue> = std::sync::OnceLock::new();

fn events() -> &'static EventQueue {
    EVENTS.get_or_init(EventQueue::new)
}

/// Send an event to the main window from any thread.
///
/// The event is stored in the process-wide [`EVENTS`] queue and a wake-only
/// `WM_APP_EVENT` (wparam = 0, lparam = 0) is posted. If the post fails the
/// event stays queued and is delivered with the next successful wake; events
/// still queued at shutdown are dropped.
///
/// # Safety
/// `hwnd` must be a live window whose WndProc routes `WM_APP_EVENT` to the app.
pub unsafe fn post_event(hwnd: windows::Win32::Foundation::HWND, ev: AppEvent) -> bool {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
    events().push(ev);
    // SAFETY: hwnd contract documented above; wake-only message.
    unsafe { PostMessageW(Some(hwnd), WM_APP_EVENT, WPARAM(0), LPARAM(0)).is_ok() }
}

pub(crate) fn post_main(ev: AppEvent) {
    let kind = std::mem::discriminant(&ev);
    let Some(hwnd) = crate::app::main_hwnd() else {
        crate::warn_!("dropping app event {kind:?}: main window is unavailable");
        return;
    };
    // `post_event` queues before waking. A failed wake therefore delays delivery
    // until another wake succeeds; it does not justify silently losing evidence.
    if !unsafe { post_event(hwnd, ev) } {
        crate::warn_!("app event {kind:?} queued but main-window wake failed");
    }
}

#[cfg(test)]
mod pack_tests {
    use super::*;

    #[test]
    fn packing_is_32_bit_and_round_trips() {
        let all = [
            HotkeyAction::ToggleMicrophone,
            HotkeyAction::ToggleOutput,
            HotkeyAction::ToggleForegroundAppAudio,
            HotkeyAction::CycleInputDevice,
            HotkeyAction::CycleOutputDevice,
            HotkeyAction::ForegroundVolumeUp,
            HotkeyAction::ForegroundVolumeDown,
            HotkeyAction::SwitchPreviousDesktop,
            HotkeyAction::MoveForegroundToDesktop(0),
            HotkeyAction::MoveForegroundToDesktop(8),
            HotkeyAction::MoveForegroundToDesktopSilent(0),
            HotkeyAction::MoveForegroundToDesktopSilent(8),
            HotkeyAction::AssignScratchpad,
            HotkeyAction::ToggleScratchpad,
            HotkeyAction::ApplyDisplayProfile(1),
            HotkeyAction::ApplyDisplayProfile(u16::MAX),
            HotkeyAction::SwitchDesktop(0),
            HotkeyAction::SwitchDesktop(8),
        ];
        assert_eq!(HotkeyAction::ToggleMicrophone.pack_u32(), 1 << 16);
        assert_eq!(HotkeyAction::ToggleOutput.pack_u32(), 2 << 16);
        assert_eq!(HotkeyAction::ToggleForegroundAppAudio.pack_u32(), 3 << 16);
        assert_eq!(HotkeyAction::SwitchDesktop(8).pack_u32(), (4 << 16) | 8);
        assert_eq!(HotkeyAction::CycleInputDevice.pack_u32(), 5 << 16);
        assert_eq!(HotkeyAction::CycleOutputDevice.pack_u32(), 6 << 16);
        assert_eq!(HotkeyAction::ForegroundVolumeUp.pack_u32(), 7 << 16);
        assert_eq!(HotkeyAction::SwitchPreviousDesktop.pack_u32(), 9 << 16);
        assert_eq!(
            HotkeyAction::MoveForegroundToDesktop(8).pack_u32(),
            (10 << 16) | 8
        );
        assert_eq!(
            HotkeyAction::MoveForegroundToDesktopSilent(8).pack_u32(),
            (11 << 16) | 8
        );
        assert_eq!(HotkeyAction::AssignScratchpad.pack_u32(), 12 << 16);
        assert_eq!(HotkeyAction::ToggleScratchpad.pack_u32(), 13 << 16);
        assert_eq!(
            HotkeyAction::ApplyDisplayProfile(255).pack_u32(),
            (14 << 16) | 255
        );
        for action in all {
            let packed = action.pack();
            assert!(
                packed <= u32::MAX as usize,
                "{action:?} packs beyond 32 bits"
            );
            assert_eq!(HotkeyAction::unpack(packed), Some(action));
        }
        // Distinct encodings.
        let packed: Vec<usize> = all.iter().map(|a| a.pack()).collect();
        for (i, a) in packed.iter().enumerate() {
            for b in &packed[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_drains_in_order_then_empties() {
        let q = EventQueue::new();
        q.push(AppEvent::ShowSettings);
        q.push(AppEvent::ConfigApplied {
            seq: 7,
            stamp: crate::config::ConfigRevisionStamp {
                revision: 7,
                origin: ConfigCommitOrigin::Settings,
            },
        });
        q.push(AppEvent::DevicesChanged);
        let drained = q.drain();
        assert_eq!(drained.len(), 3);
        assert!(matches!(drained[0], AppEvent::ShowSettings));
        assert!(matches!(
            drained[1],
            AppEvent::ConfigApplied {
                seq: 7,
                stamp: crate::config::ConfigRevisionStamp {
                    revision: 7,
                    origin: ConfigCommitOrigin::Settings,
                }
            }
        ));
        assert!(matches!(drained[2], AppEvent::DevicesChanged));
        assert!(q.drain().is_empty());
    }

    #[test]
    fn open_config_folder_routes_as_a_typed_control_center_event() {
        assert!(matches!(
            RoutedAppEvent::from(AppEvent::OpenConfigFolder),
            RoutedAppEvent::ControlCenter(ControlCenterEvent::OpenConfigFolder)
        ));
    }

    #[test]
    fn endpoint_state_events_preserve_audio_origin_when_routed() {
        assert!(matches!(
            RoutedAppEvent::from(AppEvent::MicrophoneStateChanged {
                state: AudioState::Active { volume_pct: 40 },
                origin: AudioEventOrigin::WinShortAction(41),
            }),
            RoutedAppEvent::Audio(AudioRuntimeEvent::MicrophoneStateChanged {
                origin: AudioEventOrigin::WinShortAction(41),
                ..
            })
        ));
        assert!(matches!(
            RoutedAppEvent::from(AppEvent::OutputStateChanged {
                state: OutputState::Unavailable {
                    reason: "test".into(),
                },
                origin: AudioEventOrigin::External,
            }),
            RoutedAppEvent::Audio(AudioRuntimeEvent::OutputStateChanged {
                origin: AudioEventOrigin::External,
                ..
            })
        ));
    }

    #[test]
    fn overlay_lifecycle_events_route_as_typed_overlay_events() {
        assert!(matches!(
            RoutedAppEvent::from(AppEvent::OverlayCardExpired {
                entry_id: 7,
                generation: 3,
            }),
            RoutedAppEvent::Overlay(OverlayEvent::CardExpired {
                entry_id: 7,
                generation: 3,
            })
        ));
        assert!(matches!(
            RoutedAppEvent::from(AppEvent::OverlayVisualRefresh),
            RoutedAppEvent::Overlay(OverlayEvent::VisualRefresh)
        ));
    }

    #[test]
    fn open_diagnostics_logs_routes_as_a_typed_diagnostics_event() {
        assert!(matches!(
            RoutedAppEvent::from(AppEvent::OpenDiagnosticsLogs),
            RoutedAppEvent::Diagnostics(DiagnosticsEvent::OpenLogs)
        ));
    }

    #[test]
    fn diagnostics_logs_completion_routes_as_a_typed_diagnostics_event() {
        let event = AppEvent::DiagnosticsLogsOpenFinished {
            error: Some("test failure".into()),
        };
        assert!(matches!(
            RoutedAppEvent::from(event),
            RoutedAppEvent::Diagnostics(DiagnosticsEvent::LogsOpenFinished { error })
                if error.as_deref() == Some("test failure")
        ));
    }

    #[test]
    fn concurrent_producers_preserve_per_producer_order() {
        // #37: N producer threads push tagged events; the main-thread drain
        // must observe every event exactly once, with each producer's own
        // subsequence in order. No total cross-producer order is asserted.
        use std::sync::Arc;
        let q = Arc::new(EventQueue::new());
        let producers = 4u32;
        let per = 200u32;
        let mut handles = Vec::new();
        for p in 0..producers {
            let q = Arc::clone(&q);
            handles.push(std::thread::spawn(move || {
                for i in 0..per {
                    q.push(AppEvent::ConfigApplied {
                        seq: ((p * 1000 + i) + 1) as u64,
                        stamp: crate::config::ConfigRevisionStamp {
                            revision: ((p * 1000 + i) + 1) as u64,
                            origin: ConfigCommitOrigin::Settings,
                        },
                    });
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let drained = q.drain();
        assert_eq!(drained.len(), (producers * per) as usize, "no lost events");
        // Per-producer subsequences are strictly increasing.
        let mut last = [0u64; 4];
        for ev in drained {
            if let AppEvent::ConfigApplied { seq, .. } = ev {
                let p = (seq / 1000) as usize;
                assert!(seq > last[p], "producer {p} order violated");
                last[p] = seq;
            }
        }
        assert!(q.drain().is_empty(), "drain empties the queue");
    }

    #[test]
    fn repeated_push_drain_cycles_remain_usable() {
        let q = EventQueue::new();
        for cycle in 0..50 {
            for i in 0..10 {
                q.push(AppEvent::ConfigApplied {
                    seq: cycle * 100 + i,
                    stamp: crate::config::ConfigRevisionStamp {
                        revision: cycle * 100 + i,
                        origin: ConfigCommitOrigin::Settings,
                    },
                });
            }
            assert_eq!(q.drain().len(), 10);
            assert!(q.drain().is_empty());
        }
    }
}
