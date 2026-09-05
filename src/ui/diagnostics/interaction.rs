//! Interaction for the diagnostics.

use super::model::Action;
use super::native::post_main;
use super::state::DiagnosticsUi;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

impl DiagnosticsUi {
    pub(super) fn action_at(&self, hwnd: HWND, x: f32, y: f32) -> Option<Action> {
        self.layout(hwnd)
            .buttons
            .into_iter()
            .find(|(_, rect)| rect.contains(x, y))
            .map(|(action, _)| action)
    }

    pub(super) fn activate(&mut self, hwnd: HWND, action: Action) {
        if action == Action::Bundle && self.bundle_running {
            return;
        }
        match action {
            Action::Copy => post_main(crate::event::AppEvent::CopyDiagnostics),
            Action::OpenLogs => post_main(crate::event::AppEvent::OpenDiagnosticsLogs),
            Action::Bundle => post_main(crate::event::AppEvent::CreateSupportBundle),
            Action::SelfTest => post_main(crate::event::AppEvent::RunDiagnosticsSelfTest),
            Action::Close => unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            },
        }
    }

    pub(super) fn focus_next(&mut self, reverse: bool) {
        let current = self
            .focused
            .and_then(|value| Action::ALL.iter().position(|action| *action == value));
        let mut index = current.unwrap_or(if reverse { 0 } else { Action::ALL.len() - 1 });
        for _ in 0..Action::ALL.len() {
            index = if reverse {
                if index == 0 {
                    Action::ALL.len() - 1
                } else {
                    index - 1
                }
            } else {
                (index + 1) % Action::ALL.len()
            };
            if !(Action::ALL[index] == Action::Bundle && self.bundle_running) {
                self.focused = Some(Action::ALL[index]);
                break;
            }
        }
    }
}
