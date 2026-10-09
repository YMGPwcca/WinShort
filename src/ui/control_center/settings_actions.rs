//! Settings actions for the Control Center.

use super::native::start_timer;
use super::state::{ConfirmationTarget, SettingsUi};
use crate::config::model::Config;
use crate::config::validate::Violation;
use crate::ui::layout::ElementId;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn toggle_startup_registration(&mut self, hwnd: HWND) {
        let enable = !self.startup_enabled;
        if let Err(error) = self.access.set_startup_enabled(enable) {
            self.validation = vec![Violation {
                field: "Startup".into(),
                message: error.to_string(),
            }];
        } else {
            self.startup_enabled = enable;
            self.applied_message = super::painting::APPLIED_STATUS;
            self.applied_until = Some(Instant::now() + Duration::from_secs(2));
            start_timer(hwnd);
        }
    }

    pub(super) fn toggle_debug_logging(&mut self, hwnd: HWND, id: ElementId) {
        let enabled = !self.debug_logging_enabled;
        self.access.set_debug_logging(enabled);
        self.debug_logging_enabled = enabled;
        self.applied_message = super::painting::APPLIED_STATUS;
        self.applied_until = Some(Instant::now() + Duration::from_secs(2));
        start_timer(hwnd);
        self.animate_toggle(hwnd, id, enabled);
    }

    pub(super) fn activate_reset_settings(&mut self, hwnd: HWND) {
        if self
            .interaction
            .confirmations_mut()
            .request_or_consume(ConfirmationTarget::ResetSettings)
        {
            let before = self.draft.clone();
            self.replace_draft(Config::default());
            self.commit_local_change(hwnd, before);
        }
    }

    pub(super) fn copy_version_info(&mut self, hwnd: HWND) {
        match crate::diagnostics::support::copy_unicode_text(hwnd, &crate::version::info()) {
            Ok(()) => {
                self.validation.clear();
                self.applied_message = "Version info copied";
                self.applied_until = Some(Instant::now() + Duration::from_secs(2));
                start_timer(hwnd);
            }
            Err(_) => {
                self.validation = vec![Violation {
                    field: "Clipboard".into(),
                    message: "Couldn't copy version info. Try again.".into(),
                }]
            }
        }
    }
}
