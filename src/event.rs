//! Strongly typed application events (spec §44) and the transport used
//! to move them across thread boundaries: an owned queue woken by
//! `PostMessageW`, plus WPARAM packing for keyboard actions.

use std::collections::VecDeque;
use std::sync::Mutex;
use crate::audio::state::{AppAudioState, AudioState, OutputState};
use crate::desktop::backend::BackendStatus;

pub const WM_APP_TRAY: u32 = 0x8000; // WM_APP + 0: tray callback notifications
pub const WM_APP_ACTION: u32 = 0x8001; // keyboard hook recognized a binding
pub const WM_APP_EVENT: u32 = 0x8002; // wake-only; payload lives in [`EVENTS`]

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
    PostMessageW(Some(hwnd), WM_APP_EVENT, WPARAM(0), LPARAM(0)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_drains_in_order_then_empties() {
        let q = EventQueue::new();
        q.push(AppEvent::ShowSettings);
        q.push(AppEvent::OverlayDismissed);
        q.push(AppEvent::Exit);
        let drained = q.drain();
        assert_eq!(drained.len(), 3);
        assert!(matches!(drained[0], AppEvent::ShowSettings));
        assert!(matches!(drained[1], AppEvent::OverlayDismissed));
        assert!(matches!(drained[2], AppEvent::Exit));
        assert!(q.drain().is_empty());
    }
}
