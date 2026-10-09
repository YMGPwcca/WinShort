//! Configuration draft transactions and rollback of failed local edits.

use super::native::{invalidate, start_timer};
use super::state::{ControlCenterRuntimeSnapshot, SettingsUi};
use crate::config::model::Config;
use crate::config::validate::Violation;
use crate::ui::animation::MotionChannel;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn dirty(&self) -> bool {
        self.draft != *self.config_access.current()
    }

    pub(super) fn replace_draft(&mut self, draft: Config) {
        self.draft = draft;
        self.display.close();
        self.interaction.confirmations_mut().clear();
        self.motion.clear_channel(MotionChannel::ToggleState);
    }

    pub(super) fn discard_uncommitted_draft(&mut self) {
        if self.dirty() || self.display.is_editing() {
            self.replace_draft((*self.config_access.current()).clone());
        }
    }

    pub(super) fn set_display_rollback_state(&mut self, active: bool, keep_available: bool) {
        self.runtime
            .set_display_rollback_phase(active, keep_available);
    }

    pub(super) fn set_runtime_snapshot(&mut self, snapshot: ControlCenterRuntimeSnapshot) {
        self.runtime = snapshot;
    }

    pub(super) fn commit_local_change(&mut self, hwnd: HWND, before: Config) -> bool {
        if self.display.is_dirty() {
            self.draft = before;
            self.validation = vec![Violation {
                field: "Displays".into(),
                message:
                    "Test or discard the current display edits before changing another setting"
                        .into(),
            }];
            invalidate(hwnd);
            return false;
        }
        match self.config_access.commit(self.draft.clone()) {
            Ok(()) => {
                self.display.clear_dirty();
                self.validation.clear();
                self.applied_until = Some(Instant::now() + Duration::from_secs(2));
                self.applied_message = super::painting::APPLIED_STATUS;
                start_timer(hwnd);
                true
            }
            Err(error) => {
                self.draft = before;
                self.validation = vec![Violation {
                    field: "Changes".into(),
                    message: error.to_string(),
                }];
                invalidate(hwnd);
                false
            }
        }
    }
}
