//! Settings actions for the Control Center.

use super::native::start_timer;
use super::state::SettingsUi;
use crate::config::model::Config;
use crate::config::validate::Violation;

use crate::ui::layout::ElementId;

use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn toggle_startup_registration(&mut self, hwnd: HWND) {
        {
            let enable = !self.startup_enabled;
            if let Err(error) = crate::platform::startup::set_enabled(enable) {
                self.validation = vec![Violation {
                    field: "Startup".into(),
                    message: error.to_string(),
                }];
            } else {
                self.startup_enabled = enable;
                self.applied_until = Some(Instant::now() + Duration::from_secs(2));
                start_timer(hwnd);
            }
        }
    }

    pub(super) fn toggle_debug_logging(&mut self, hwnd: HWND, id: ElementId) {
        {
            let enabled = !crate::diagnostics::logging::debug_logging_enabled();
            crate::diagnostics::logging::set_debug_logging(enabled);
            self.applied_until = Some(Instant::now() + Duration::from_secs(2));
            start_timer(hwnd);
            self.animate_toggle(hwnd, id, enabled);
        }
    }

    pub(super) fn activate_reset_settings(&mut self, hwnd: HWND) {
        {
            if Self::consume_reset_confirmation(&mut self.reset_confirm) {
                let before = self.draft.clone();
                self.replace_draft(Config::default());
                self.commit_local_change(hwnd, before);
            }
        }
    }
}
