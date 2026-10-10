//! Executable-keyed badge state survives foreground-window changes.
use crate::audio::state::ApplicationAudioInfo;
use crate::audio::Aggregate;
use crate::ui::overlay::{OverlayIcon, OverlayKey, OverlayModel, OverlayRequest};

#[derive(Default)]
pub(super) struct MutedApplications {
    next_id: u64,
    entries: Vec<TrackedApplication>,
}
#[derive(Clone)]
struct TrackedApplication {
    id: u64,
    info: ApplicationAudioInfo,
    icon: OverlayIcon,
    muted: bool,
}
impl TrackedApplication {
    fn model(&self) -> OverlayModel {
        let mut row = crate::ui::overlay::application_row(&self.info.state);
        row.icon = self.icon.clone();
        OverlayModel::single(row)
    }
}
impl MutedApplications {
    #[cfg(test)]
    pub(super) fn muted_ids(&self) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|entry| entry.muted)
            .map(|entry| entry.id)
            .collect()
    }

    fn observe(&mut self, info: ApplicationAudioInfo) -> u64 {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.info.identity == info.identity)
        {
            if !matches!(
                info.state.aggregate,
                Aggregate::Error | Aggregate::NoSession
            ) {
                entry.muted = info.state.aggregate == Aggregate::AllMuted;
                entry.info = info;
            }
            return entry.id;
        }
        self.next_id += 1;
        let icon = info
            .image_path
            .as_deref()
            .and_then(crate::ui::overlay::executable_icon)
            .unwrap_or(OverlayIcon::Application);
        let id = self.next_id;
        let muted = info.state.aggregate == Aggregate::AllMuted;
        self.entries.push(TrackedApplication {
            id,
            info,
            icon,
            muted,
        });
        id
    }

    fn snapshot(&mut self, applications: Vec<ApplicationAudioInfo>) {
        for entry in &mut self.entries {
            entry.muted = false;
        }
        for app in applications {
            self.observe(app);
        }
    }
}

impl super::App {
    pub(super) fn handle_muted_applications(&mut self, applications: Vec<ApplicationAudioInfo>) {
        self.muted_applications.snapshot(applications);
        self.reconcile_app_audio_overlay(false);
    }

    pub(super) fn reconcile_foreground_app_audio(&mut self, show_feedback: bool) {
        let Some(pid) = self.foreground_pid else {
            if show_feedback {
                self.show_overlay(OverlayRequest::toast(
                    OverlayKey::CurrentAppAudio,
                    OverlayModel::single(crate::ui::overlay::application_row(
                        &self.foreground_state,
                    )),
                ));
            }
            return;
        };
        let info = ApplicationAudioInfo::from_process(pid, self.foreground_state.clone());
        let id = self.muted_applications.observe(info.clone());
        if show_feedback && info.state.aggregate != Aggregate::AllMuted {
            let entry = self
                .muted_applications
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .unwrap();
            let mut row = crate::ui::overlay::application_row(&info.state);
            row.icon = entry.icon.clone();
            let request =
                OverlayRequest::toast(OverlayKey::ApplicationToast(id), OverlayModel::single(row));
            let request = if matches!(
                info.state.aggregate,
                Aggregate::Error | Aggregate::NoSession
            ) {
                request
            } else {
                request.replacing(OverlayKey::ApplicationPermanent(id))
            };
            self.show_overlay(request);
        }
        self.reconcile_app_audio_overlay(false);
    }

    pub(super) fn reconcile_app_audio_overlay(&mut self, _show_feedback: bool) {
        let config = crate::app::config();
        let enabled = !self.acceptance_overlay_only
            && config.overlay.enabled
            && config.overlay.notifications.current_app_audio;
        // Clone plain presentation data before entering native window operations.
        let entries = self.muted_applications.entries.clone();
        for entry in entries {
            if enabled && entry.muted {
                self.show_overlay_with_config(
                    OverlayRequest::permanent(
                        OverlayKey::ApplicationPermanent(entry.id),
                        entry.model(),
                    )
                    .replacing(OverlayKey::ApplicationToast(entry.id)),
                    config.overlay.clone(),
                );
            } else {
                self.remove_overlay_key(
                    OverlayKey::ApplicationPermanent(entry.id),
                    &config.overlay,
                );
                if !enabled {
                    self.remove_overlay_key(
                        OverlayKey::ApplicationToast(entry.id),
                        &config.overlay,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn info(pid: u32, path: Option<&str>, aggregate: Aggregate) -> ApplicationAudioInfo {
        ApplicationAudioInfo::new(
            pid,
            path.map(Into::into),
            crate::audio::AppAudioState {
                app_name: Some("Player".into()),
                aggregate,
                sessions: 1,
                error: None,
            },
        )
    }
    #[test]
    fn independent_mutes_survive_focus_and_only_the_unmuted_member_is_removed() {
        let mut tracker = MutedApplications::default();
        let first = tracker.observe(info(1, None, Aggregate::AllMuted));
        let second = tracker.observe(info(2, None, Aggregate::AllMuted));
        assert_ne!(first, second);
        tracker.observe(info(1, None, Aggregate::AllActive));
        assert!(!tracker.entries[0].muted);
        assert!(tracker.entries[1].muted);
        assert_eq!(tracker.observe(info(1, None, Aggregate::AllMuted)), first);
    }
    #[test]
    fn snapshot_removes_exited_apps_and_error_does_not_invent_an_unmute() {
        let mut tracker = MutedApplications::default();
        tracker.observe(info(1, None, Aggregate::AllMuted));
        tracker.observe(info(1, None, Aggregate::Error));
        assert!(tracker.entries[0].muted);
        tracker.snapshot(vec![]);
        assert!(!tracker.entries[0].muted);
    }
}
