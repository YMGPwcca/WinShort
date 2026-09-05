//! Display edit for the control center.

use super::display_routes::{next_display_profile_identity, parse_display_route_values};
use super::state::{DisplayEditorState, SettingsUi};
use crate::config::validate::Violation;
use crate::ui::presentation::DisplayWizardStep;
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn start_display_editor(&mut self, _hwnd: HWND, update_selected: bool) {
        if self.display_editor.is_some() {
            return;
        }
        self.refresh_display_outputs();
        let before = self.draft.clone();
        self.capture_display_profile(_hwnd, update_selected);
        if self.draft != before {
            self.display_editor = Some(DisplayEditorState {
                step: DisplayWizardStep::Displays,
            });
            self.display_draft_dirty = true;
            self.selected_display_route = 0;
            self.validation.clear();
        }
    }

    pub(super) fn start_existing_display_editor(&mut self) {
        if self
            .draft
            .display_profiles
            .active()
            .is_some_and(|profile| !profile.routes.is_empty())
        {
            self.display_editor = Some(DisplayEditorState {
                step: DisplayWizardStep::Displays,
            });
            self.display_draft_dirty = false;
            self.selected_display_route = 0;
            self.validation.clear();
        }
    }

    pub(super) fn move_display_editor(&mut self, _hwnd: HWND, forward: bool) {
        let Some(editor) = self.display_editor.as_mut() else {
            return;
        };
        if forward && editor.step == DisplayWizardStep::Displays {
            let has_route = self
                .draft
                .display_profiles
                .active()
                .is_some_and(|profile| !profile.routes.is_empty());
            if !has_route {
                self.validation = vec![Violation {
                    field: "display_profiles.routes".into(),
                    message: "select at least one screen before continuing".into(),
                }];
                return;
            }
        }
        let next = if forward {
            editor.step.next()
        } else {
            editor.step.previous()
        };
        if let Some(step) = next {
            editor.step = step;
            self.reset_scroll();
            self.validation.clear();
        }
    }

    pub(super) fn cancel_display_editor(&mut self) {
        if self.display_editor.is_some() {
            self.replace_draft((*self.config_access.current()).clone());
            self.validation.clear();
        }
    }

    pub(super) fn toggle_display_output(&mut self, index: u8) {
        let Some((_, _, route, _)) = self
            .display_route_candidates()
            .into_iter()
            .nth(index as usize)
        else {
            return;
        };
        let Some(profile) = self.draft.display_profiles.active() else {
            return;
        };
        let profile_id = profile.id.clone();
        if let Some(profile) = self
            .draft
            .display_profiles
            .profiles
            .iter_mut()
            .find(|profile| profile.id.eq_ignore_ascii_case(&profile_id))
        {
            if let Some(existing) = profile
                .routes
                .iter()
                .position(|configured| crate::display::same_output(configured, &route))
            {
                profile.routes.remove(existing);
            } else {
                profile.routes.push(route);
            }
            profile.confirmed = false;
            if profile.routes.len() <= 1 {
                profile.topology = crate::display::DisplayTopology::Custom;
            } else if !matches!(
                profile.topology,
                crate::display::DisplayTopology::Extend | crate::display::DisplayTopology::Clone
            ) {
                profile.topology = crate::display::DisplayTopology::Extend;
            }
            self.selected_display_route = self
                .selected_display_route
                .min(profile.routes.len().saturating_sub(1));
            self.display_draft_dirty = true;
            self.validation.clear();
        }
    }

    pub(super) fn set_display_topology(&mut self, index: u8) {
        let Some(topology) = super::display_routes::topology_choice(index) else {
            return;
        };
        if let Some(profile) = self
            .draft
            .display_profiles
            .active_profile
            .as_deref()
            .and_then(|id| {
                self.draft
                    .display_profiles
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id.eq_ignore_ascii_case(id))
            })
        {
            if profile.routes.len() > 1 {
                profile.topology = topology;
                profile.confirmed = false;
                self.display_draft_dirty = true;
                self.validation.clear();
            }
        }
    }

    pub(super) fn capture_display_profile(&mut self, _hwnd: HWND, update_selected: bool) {
        let (id, name) = if update_selected {
            let Some(profile) = self.draft.display_profiles.active() else {
                self.validation = vec![Violation {
                    field: "display_profiles.active_profile".into(),
                    message: "select a profile before Update from Current".into(),
                }];
                return;
            };
            (profile.id.clone(), profile.name.clone())
        } else {
            let Some(identity) =
                next_display_profile_identity(&self.draft.display_profiles.profiles)
            else {
                self.validation = vec![Violation {
                    field: "display_profiles.profiles".into(),
                    message: "profile identity space exhausted".into(),
                }];
                return;
            };
            identity
        };
        match crate::display::capture_current_profile(&id, &name) {
            Ok(mut profile) => {
                profile.confirmed = false;
                if self.draft.display_profiles.upsert(profile) {
                    self.selected_display_route = 0;
                    self.validation.clear();
                } else {
                    self.validation = vec![Violation {
                        field: "display_profiles.profiles".into(),
                        message: format!(
                            "at most {} display profiles are supported",
                            crate::display::MAX_PROFILES
                        ),
                    }];
                }
            }
            Err(error) => {
                let reason = error.to_string();
                crate::error_!("display profile capture failed: {reason}");
                self.validation = vec![Violation {
                    field: "display_profiles".into(),
                    message: reason,
                }];
            }
        }
    }

    pub(super) fn duplicate_active_display_profile(&mut self) {
        let Some(source) = self.draft.display_profiles.active().cloned() else {
            return;
        };
        let Some((id, _)) = next_display_profile_identity(&self.draft.display_profiles.profiles)
        else {
            self.validation = vec![Violation {
                field: "display_profiles.profiles".into(),
                message: "profile identity space exhausted".into(),
            }];
            return;
        };
        let mut name = format!("{} Copy", source.name.trim());
        let base = name.clone();
        let mut suffix = 2usize;
        while self
            .draft
            .display_profiles
            .profiles
            .iter()
            .any(|profile| profile.name.eq_ignore_ascii_case(&name))
        {
            name = format!("{base} {suffix}");
            suffix += 1;
        }
        let mut duplicate = source;
        duplicate.id = id;
        duplicate.name = name;
        duplicate.confirmed = false;
        if self.draft.display_profiles.upsert(duplicate) {
            self.selected_display_route = 0;
            self.validation.clear();
        } else {
            self.validation = vec![Violation {
                field: "display_profiles.profiles".into(),
                message: format!(
                    "at most {} display profiles are supported",
                    crate::display::MAX_PROFILES
                ),
            }];
        }
    }

    pub(super) fn rename_display_profile(&mut self, profile_id: &str, name: &str) {
        let normalized = name.trim();
        if normalized.is_empty() {
            self.validation = vec![Violation {
                field: "display_profiles.profiles.name".into(),
                message: "profile name must not be empty".into(),
            }];
            return;
        }
        if self.draft.display_profiles.profiles.iter().any(|profile| {
            !profile.id.eq_ignore_ascii_case(profile_id)
                && profile.name.eq_ignore_ascii_case(normalized)
        }) {
            self.validation = vec![Violation {
                field: "display_profiles.profiles.name".into(),
                message: format!("profile name `{normalized}` is already in use"),
            }];
            return;
        }
        let Some(profile) = self
            .draft
            .display_profiles
            .profiles
            .iter_mut()
            .find(|profile| profile.id.eq_ignore_ascii_case(profile_id))
        else {
            self.validation = vec![Violation {
                field: "display_profiles.active_profile".into(),
                message: "selected profile no longer exists".into(),
            }];
            return;
        };
        profile.name = normalized.to_string();
        if self.display_editor.is_some() {
            self.display_draft_dirty = true;
        }
        self.validation.clear();
    }

    pub(super) fn edit_display_route(&mut self, profile_id: &str, route_index: usize, value: &str) {
        let edit = match parse_display_route_values(value) {
            Ok(values) => values,
            Err(error) => {
                self.validation = vec![Violation {
                    field: "display_profiles.routes".into(),
                    message: error.into(),
                }];
                return;
            }
        };

        let Some(profile) = self
            .draft
            .display_profiles
            .profiles
            .iter_mut()
            .find(|profile| profile.id.eq_ignore_ascii_case(profile_id))
        else {
            self.validation = vec![Violation {
                field: "display_profiles.active_profile".into(),
                message: "selected profile no longer exists".into(),
            }];
            return;
        };
        let Some(route) = profile.routes.get_mut(route_index) else {
            self.validation = vec![Violation {
                field: "display_profiles.routes".into(),
                message: "selected display route no longer exists".into(),
            }];
            return;
        };
        edit.apply_to(route);
        profile.confirmed = false;
        self.selected_display_route = route_index;
        self.display_draft_dirty = true;
        if self.display_editor.is_none() {
            self.display_editor = Some(DisplayEditorState {
                step: DisplayWizardStep::Review,
            });
        }
        self.validation.clear();
    }

    pub(super) fn delete_active_display_profile(&mut self) {
        if let Some(active) = self.draft.display_profiles.active_profile.clone() {
            self.draft.display_profiles.remove(&active);
            self.draft
                .hotkeys
                .display_profiles
                .retain(|binding| !binding.profile_id.eq_ignore_ascii_case(&active));
            self.draft
                .hotkeys
                .clear_disabled_hotkey(&format!("display_profile:{active}"));
            self.selected_display_route = 0;
        }
        self.validation.clear();
    }
}
