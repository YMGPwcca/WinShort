//! Display actions for the Control Center.

use super::native::post_main;
use super::state::{DisplayEditorState, SettingsUi};

use crate::ui::presentation::DisplayWizardStep;

use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn activate_display_profile_card(&mut self, hwnd: HWND, index: u8) {
        {
            if let Some(profile) = self.draft.display_profiles.profiles.get(index as usize) {
                let id = profile.id.clone();
                let ready = self.inventory.profile_readiness(profile).is_ready();
                let already_selected = self
                    .draft
                    .display_profiles
                    .active_profile
                    .as_deref()
                    .is_some_and(|active| active.eq_ignore_ascii_case(&id));
                if already_selected && ready {
                    post_main(crate::event::AppEvent::ApplyDisplayProfile {
                        profile: profile.clone(),
                    });
                } else if already_selected {
                    self.display_editor = Some(DisplayEditorState {
                        step: DisplayWizardStep::Review,
                    });
                    self.display_draft_dirty = false;
                } else {
                    let before = self.draft.clone();
                    self.draft.display_profiles.active_profile = Some(id);
                    self.selected_display_route = 0;
                    self.commit_local_change(hwnd, before);
                }
            }
        }
    }

    pub(super) fn request_display_route_edit(&mut self) {
        {
            if let (Some(profile), Some(initial)) = (
                self.draft.display_profiles.active(),
                self.selected_display_route_edit_value(),
            ) {
                post_main(crate::event::AppEvent::OpenDisplayRouteEditPrompt {
                    profile_id: profile.id.clone(),
                    route_index: self.selected_display_route,
                    initial,
                });
            }
        }
    }

    pub(super) fn request_display_profile_rename(&mut self) {
        {
            if let Some(profile) = self.draft.display_profiles.active() {
                post_main(crate::event::AppEvent::OpenDisplayRenamePrompt {
                    profile_id: profile.id.clone(),
                    current_name: profile.name.clone(),
                });
            }
        }
    }

    pub(super) fn activate_display_profile_duplicate(&mut self, hwnd: HWND) {
        {
            let before = self.draft.clone();
            self.duplicate_active_display_profile();
            if self.draft != before {
                self.commit_local_change(hwnd, before);
            }
        }
    }

    pub(super) fn activate_display_profile_delete(&mut self, hwnd: HWND) {
        {
            if Self::consume_reset_confirmation(&mut self.delete_profile_confirm) {
                let before = self.draft.clone();
                self.delete_active_display_profile();
                if self.draft != before {
                    self.commit_local_change(hwnd, before);
                }
            }
        }
    }
}
