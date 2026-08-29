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
}

impl HotkeyAction {
    /// Fixed-width 32-bit encoding (issue #28): kind in the high half, arg in
    /// the low byte. Safe on any pointer width — no `usize`-shift assumptions.
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
        match self {
            HotkeyAction::ToggleMicrophone => KIND_MIC << 16,
            HotkeyAction::ToggleOutput => KIND_OUT << 16,
            HotkeyAction::ToggleForegroundAppAudio => KIND_FG << 16,
            HotkeyAction::SwitchDesktop(n) => (KIND_DESKTOP << 16) | n as u32,
            HotkeyAction::CycleInputDevice => KIND_CYCLE_INPUT << 16,
            HotkeyAction::CycleOutputDevice => KIND_CYCLE_OUTPUT << 16,
            HotkeyAction::ForegroundVolumeUp => KIND_FOREGROUND_VOLUME_UP << 16,
            HotkeyAction::ForegroundVolumeDown => KIND_FOREGROUND_VOLUME_DOWN << 16,
        }
    }

    /// Inverse of [`pack`]; returns None for foreign messages.
    pub fn unpack(wparam: usize) -> Option<Self> {
        Self::unpack_u32(wparam as u32)
    }

    pub fn unpack_u32(word: u32) -> Option<Self> {
        let kind = word >> 16;
        let arg = word as u8;
        match kind {
            1 => Some(HotkeyAction::ToggleMicrophone),
            2 => Some(HotkeyAction::ToggleOutput),
            3 => Some(HotkeyAction::ToggleForegroundAppAudio),
            4 => Some(HotkeyAction::SwitchDesktop(arg)),
            5 => Some(HotkeyAction::CycleInputDevice),
            6 => Some(HotkeyAction::CycleOutputDevice),
            7 => Some(HotkeyAction::ForegroundVolumeUp),
            8 => Some(HotkeyAction::ForegroundVolumeDown),
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
}

/// Events routed through the main window. Payloads live in the process-wide
/// queue; `PostMessageW` carries only a wake-up message or packed action.
#[derive(Debug)]
pub enum AppEvent {
    // Commands executed on the main thread.
    ShowSettings,
    ShowDiagnostics,
    OpenSettingsPicker(crate::ui::picker::PickerKind),
    ShowStatusOverlay,
    /// Render the Settings draft overlay without persisting it.
    PreviewOverlay {
        config: crate::config::model::OverlayCfg,
    },
    FocusSettingsFromPicker {
        reverse: bool,
    },
    CommitSettingsPicker {
        kind: crate::ui::picker::PickerKind,
        value: crate::ui::picker::PickerValue,
    },
    CancelSettingsPicker {
        popup_hwnd: isize,
        restore_focus: bool,
    },
    SettingsWindowClosed,
    RunDiagnosticsSelfTest,
    CopyDiagnostics,
    OpenDiagnosticsLogs,
    CreateSupportBundle,
    ConfigApplied {
        seq: u64,
        stamp: crate::config::ConfigRevisionStamp,
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
    /// Transient "default output changed" presentation (#17b): the overlay
    /// shows a one-shot card; persistent state stays OutputState::Current.
    DefaultOutputChanged(crate::audio::state::DeviceId),
    ForegroundAudioChanged {
        state: AppAudioState,
        origin: AudioEventOrigin,
    },
    ForegroundVolumeChanged {
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
    if let Some(hwnd) = crate::app::main_hwnd() {
        unsafe {
            let _ = post_event(hwnd, ev);
        }
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
        assert_eq!(HotkeyAction::ForegroundVolumeDown.pack_u32(), 8 << 16);
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
