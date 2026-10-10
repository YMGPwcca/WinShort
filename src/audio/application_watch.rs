//! Keep the exact mute target scope used by the shortcut when observing in the background.
use super::state::{Aggregate, AppAudioState, ApplicationAudioInfo};

#[derive(Default)]
pub(super) struct ApplicationWatch {
    entries: Vec<ApplicationAudioInfo>,
}
pub(super) enum WatchResult {
    State(AppAudioState),
    Gone,
    Unavailable,
}

impl ApplicationWatch {
    pub(super) fn observe(&mut self, info: ApplicationAudioInfo) {
        if info.state.aggregate == Aggregate::Error {
            return;
        }
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.identity == info.identity)
        {
            if info.state.aggregate != Aggregate::NoSession {
                *entry = info;
            }
        } else if info.state.aggregate == Aggregate::AllMuted {
            self.entries.push(info);
        }
    }
    pub(super) fn discover(
        &mut self,
        inventory: &[ApplicationAudioInfo],
        mut query: impl FnMut(&ApplicationAudioInfo) -> WatchResult,
    ) {
        for app in inventory {
            if app.state.aggregate == Aggregate::AllActive
                || self
                    .entries
                    .iter()
                    .any(|known| known.identity == app.identity)
            {
                continue;
            }
            if let WatchResult::State(state) = query(app) {
                if state.aggregate == Aggregate::AllMuted {
                    let mut scoped = app.clone();
                    scoped.state = state;
                    self.observe(scoped);
                }
            }
        }
    }
    pub(super) fn refresh(&mut self, mut query: impl FnMut(&ApplicationAudioInfo) -> WatchResult) {
        self.entries.retain_mut(|entry| match query(entry) {
            WatchResult::Gone => false,
            WatchResult::Unavailable => true,
            WatchResult::State(state) => {
                if state.aggregate != Aggregate::NoSession {
                    entry.state = state;
                }
                true
            }
        });
    }
    pub(super) fn merge_inventory(
        &self,
        mut global: Vec<ApplicationAudioInfo>,
    ) -> Vec<ApplicationAudioInfo> {
        // The global sweep can see unrelated/inactive sessions that the mute
        // command never targeted. It must not overrule a known target's scope.
        global.retain(|app| {
            !self
                .entries
                .iter()
                .any(|known| known.identity == app.identity)
        });
        global.extend(
            self.entries
                .iter()
                .filter(|app| app.state.aggregate == Aggregate::AllMuted)
                .cloned(),
        );
        global.sort_by(|a, b| a.identity.cmp(&b.identity));
        global
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn info(pid: u32, name: &str, aggregate: Aggregate) -> ApplicationAudioInfo {
        ApplicationAudioInfo::new(
            pid,
            None,
            AppAudioState {
                app_name: Some(name.into()),
                aggregate,
                sessions: 1,
                error: None,
            },
        )
    }
    #[test]
    fn startup_discovers_a_muted_scope_even_when_other_endpoints_make_the_executable_mixed() {
        let mut watch = ApplicationWatch::default();
        let raw = info(10, "Zen", Aggregate::Mixed);
        let scoped = info(10, "Zen", Aggregate::AllMuted);
        watch.discover(&[raw], |_| WatchResult::State(scoped.state.clone()));
        assert_eq!(watch.merge_inventory(vec![]), vec![scoped]);
    }

    #[test]
    fn unrelated_global_sessions_cannot_erase_zen_when_spotify_changes() {
        let mut watch = ApplicationWatch::default();
        let zen = info(10, "Zen", Aggregate::AllMuted);
        let spotify = info(20, "Spotify", Aggregate::AllMuted);
        watch.observe(zen.clone());
        assert_eq!(watch.merge_inventory(vec![]), vec![zen.clone()]);
        watch.refresh(|_| WatchResult::State(info(10, "Zen", Aggregate::NoSession).state));
        assert_eq!(
            watch.merge_inventory(vec![]),
            vec![zen.clone()],
            "an idle audio stream is not an unmute"
        );
        watch.observe(spotify.clone());
        assert_eq!(watch.merge_inventory(vec![spotify.clone()]).len(), 2);
        watch.observe(info(20, "Spotify", Aggregate::AllActive));
        assert_eq!(watch.merge_inventory(vec![]), vec![zen.clone()]);
        watch.refresh(|entry| {
            if entry.pid == 10 {
                WatchResult::Unavailable
            } else {
                WatchResult::Gone
            }
        });
        assert_eq!(watch.merge_inventory(vec![]), vec![zen]);
        watch.refresh(|_| WatchResult::Gone);
        assert!(watch.merge_inventory(vec![]).is_empty());
    }
}
