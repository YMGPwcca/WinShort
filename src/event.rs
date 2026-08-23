//! Strongly typed application events (spec §44) and the message-packing used
//! to move them across thread boundaries via `PostMessageW`.

use crate::audio::state::{AppAudioState, AudioState, OutputState};
use crate::desktop::backend::BackendStatus;

pub const WM_APP_TRAY: u32 = 0x8000; // WM_APP + 0: tray callback notifications
pub const WM_APP_ACTION: u32 = 0x8001; // keyboard hook recognized a binding
pub const WM_APP_EVENT: u32 = 0x8002; // boxed AppEvent payload in lParam

/// Actions produced by the keyboard engine. Small enough to pack into a WPARAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleMicrophone,
    ToggleOutput,
    ToggleForegroundAppAudio,
    /// Virtual desktop index, 0-based internally (desktops are numbered 1..=9).
    SwitchDesktop(u8),
}

impl HotkeyAction {
    pub fn pack(self) -> usize {
        match self {
            HotkeyAction::ToggleMicrophone => 1usize << 32,
            HotkeyAction::ToggleOutput => 2usize << 32,
            HotkeyAction::ToggleForegroundAppAudio => 3usize << 32,
            HotkeyAction::SwitchDesktop(n) => (4usize << 32) | n as usize,
        }
    }

    /// Inverse of [`pack`]; returns None for foreign messages.
    pub fn unpack(wparam: usize) -> Option<Self> {
        let kind = wparam >> 32;
        let arg = (wparam & 0xFFFF_FFFF) as u8;
        match kind {
            1 => Some(HotkeyAction::ToggleMicrophone),
            2 => Some(HotkeyAction::ToggleOutput),
            3 => Some(HotkeyAction::ToggleForegroundAppAudio),
            4 => Some(HotkeyAction::SwitchDesktop(arg)),
            _ => None,
        }
    }
}

/// Events routed through the main window. Workers post boxed payloads;
/// the main thread owns reconstruction.
#[derive(Debug)]
pub enum AppEvent {
    // Commands executed on the main thread.
    ShowSettings,
    ShowStatusOverlay,
    ConfigApplied(u64),
    SuspendToggled(bool),

    // State published by workers / callbacks.
    MicrophoneStateChanged(AudioState),
    OutputStateChanged(OutputState),
    ForegroundAudioChanged(AppAudioState),
    OverlayDismissed,

    /// Device list changed (added/removed/default switched): refresh pickers.
    DevicesChanged,
    DesktopBackendChanged(BackendStatus),

    Exit,
}

/// Send an event to the main window from any thread. The box is consumed;
/// the main thread frees it exactly once.
///
/// # Safety
/// `hwnd` must be a live window whose WndProc routes `WM_APP_EVENT` to the app.
pub unsafe fn post_event(hwnd: windows::Win32::Foundation::HWND, ev: AppEvent) -> bool {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
    let ptr = Box::into_raw(Box::new(ev));
    PostMessageW(Some(hwnd), WM_APP_EVENT, WPARAM(usize::MAX), LPARAM(ptr as isize)).is_ok()

}
/// Reconstruct a posted event. Called only by the main window's handler.
///
/// # Safety
/// `lparam` must be a pointer previously produced by [`post_event`] and not yet consumed.
pub unsafe fn take_event(lparam: usize) -> Box<AppEvent> {
    Box::from_raw(lparam as *mut AppEvent)
}
