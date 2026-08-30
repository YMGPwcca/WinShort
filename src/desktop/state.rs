//! Process-lifetime virtual desktop navigation and focus state.
//!
//! Desktop ordinals are mutable when desktops are inserted, removed, or
//! reordered. This state therefore keys remembered windows and navigation by
//! the Shell desktop GUID instead of an ordinal index. It contains no Win32
//! calls so transition and stale-state policy stays deterministic and tested.

use std::collections::HashMap;

use windows_core::GUID;

/// Raw HWND value kept only while this process is alive.
pub(crate) type WindowHandle = isize;

#[derive(Debug, Default)]
pub(crate) struct DesktopHistory {
    last_focused: HashMap<GUID, WindowHandle>,
    current_desktop: Option<GUID>,
    previous_desktop: Option<GUID>,
}

impl DesktopHistory {
    pub(crate) fn remember(&mut self, desktop: GUID, hwnd: WindowHandle) {
        self.last_focused.insert(desktop, hwnd);
    }

    pub(crate) fn remembered(&self, desktop: GUID) -> Option<WindowHandle> {
        self.last_focused.get(&desktop).copied()
    }

    pub(crate) fn forget_window(&mut self, hwnd: WindowHandle) {
        self.last_focused
            .retain(|_, remembered| *remembered != hwnd);
    }

    /// Observe the active desktop from an event-driven shell/foreground
    /// sample. A changed identity becomes the Previous Desktop target.
    pub(crate) fn observe_desktop(&mut self, desktop: GUID) {
        let previous = self.current_desktop.replace(desktop);
        if previous != Some(desktop) {
            self.previous_desktop = previous;
        }
    }

    pub(crate) fn note_numbered_switch(&mut self, previous: Option<GUID>, target: GUID) {
        if previous != Some(target) {
            self.previous_desktop = previous;
        }
        self.current_desktop = Some(target);
    }

    pub(crate) fn note_previous_switch(&mut self, current: GUID, target: GUID) {
        self.previous_desktop = Some(current);
        self.current_desktop = Some(target);
    }

    pub(crate) fn previous(&self) -> Option<GUID> {
        self.previous_desktop
    }

    pub(crate) fn clear_previous(&mut self) {
        self.previous_desktop = None;
    }

    pub(crate) fn clear_identity(&mut self) {
        self.current_desktop = None;
        self.previous_desktop = None;
    }

    pub(crate) fn previous_if_present(&self, desktops: &[GUID]) -> Option<GUID> {
        self.previous_desktop
            .filter(|previous| desktops.contains(previous))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_driven_identity_changes_update_back_and_forth_history() {
        let mut history = DesktopHistory::default();
        let first = desktop(1);
        let second = desktop(2);

        history.observe_desktop(first);
        assert_eq!(history.previous(), None);
        history.observe_desktop(second);
        assert_eq!(history.previous(), Some(first));
        history.observe_desktop(first);
        assert_eq!(history.previous(), Some(second));
    }
    fn desktop(value: u128) -> GUID {
        GUID::from_u128(value)
    }

    #[test]
    fn remembers_one_window_per_stable_desktop_identity() {
        let mut history = DesktopHistory::default();
        let first = desktop(1);
        let second = desktop(2);

        history.remember(first, 101);
        history.remember(second, 202);

        assert_eq!(history.remembered(first), Some(101));
        assert_eq!(history.remembered(second), Some(202));
    }

    #[test]
    fn successful_numbered_switch_records_the_actual_previous_identity() {
        let mut history = DesktopHistory::default();
        let first = desktop(1);
        let second = desktop(2);

        history.note_numbered_switch(Some(first), second);
        assert_eq!(history.previous(), Some(first));
        history.note_numbered_switch(Some(second), first);
        assert_eq!(history.previous(), Some(second));
    }

    #[test]
    fn switching_to_the_current_desktop_does_not_destroy_back_and_forth_state() {
        let mut history = DesktopHistory::default();
        let first = desktop(1);
        let second = desktop(2);

        history.note_numbered_switch(Some(first), second);
        history.note_numbered_switch(Some(second), second);

        assert_eq!(history.previous(), Some(first));
    }

    #[test]
    fn failed_switches_do_not_mutate_navigation_state() {
        let mut history = DesktopHistory::default();
        let first = desktop(1);
        let second = desktop(2);
        history.note_numbered_switch(Some(first), second);

        // A failed operation never calls either success transition method.
        assert_eq!(history.previous(), Some(first));
    }

    #[test]
    fn previous_identity_is_unavailable_after_deletion() {
        let mut history = DesktopHistory::default();
        let first = desktop(1);
        let second = desktop(2);
        history.note_numbered_switch(Some(first), second);

        assert_eq!(history.previous_if_present(&[second]), None);
        assert_eq!(history.previous(), Some(first));
        history.clear_previous();
        assert_eq!(history.previous(), None);
    }

    #[test]
    fn desktop_ordinal_reordering_does_not_change_identity_lookup() {
        let mut history = DesktopHistory::default();
        let first = desktop(1);
        let second = desktop(2);
        let third = desktop(3);
        history.remember(second, 202);

        let reordered = [third, first, second];
        let target = reordered
            .iter()
            .position(|id| *id == second)
            .map(|_| second)
            .and_then(|id| history.remembered(id));

        assert_eq!(target, Some(202));
    }
    #[test]
    fn stale_window_is_removed_from_all_desktops() {
        let mut history = DesktopHistory::default();
        let first = desktop(1);
        let second = desktop(2);
        history.remember(first, 404);
        history.remember(second, 404);

        history.forget_window(404);

        assert_eq!(history.remembered(first), None);
        assert_eq!(history.remembered(second), None);
    }
}
