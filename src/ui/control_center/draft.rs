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
        self.display_draft_dirty = false;
        self.display_editor = None;
        self.delete_profile_confirm = false;
        self.selected_display_route = 0;
        self.motion.clear_channel(MotionChannel::ToggleState);
    }

    pub(super) fn discard_uncommitted_draft(&mut self) {
        if self.dirty() || self.display_draft_dirty || self.display_editor.is_some() {
            self.replace_draft((*self.config_access.current()).clone());
        }
    }

    pub(super) fn set_display_rollback_state(&mut self, active: bool, keep_available: bool) {
        self.display_rollback_active = active;
        self.display_keep_available = keep_available;
    }

    pub(super) fn set_runtime_snapshot(&mut self, snapshot: ControlCenterRuntimeSnapshot) {
        self.display_rollback_active = snapshot.display_rollback_active;
        self.display_keep_available = snapshot.display_keep_available;
        self.runtime = snapshot;
    }

    pub(super) fn commit_local_change(&mut self, hwnd: HWND, before: Config) -> bool {
        if self.display_draft_dirty {
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
                self.display_draft_dirty = false;
                self.validation.clear();
                self.applied_until = Some(Instant::now() + Duration::from_secs(2));
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
