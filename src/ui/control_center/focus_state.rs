//! Logical focus survives native focus transfers; picker handles are observed atomically.

use crate::ui::focus::FocusOwner;
use crate::ui::layout::ElementId;
use windows::Win32::Foundation::HWND;

#[derive(Debug, Clone, Copy)]
struct PickerBinding {
    window: HWND,
    list: HWND,
}

impl PickerBinding {
    fn contains(self, window: HWND) -> bool {
        self.window == window || self.list == window
    }
}

#[derive(Debug, Default)]
pub(super) struct FocusState {
    target: Option<ElementId>,
    indicator_visible: bool,
    owner: FocusOwner,
    picker: Option<PickerBinding>,
}

impl FocusState {
    pub(super) fn target(&self) -> Option<ElementId> {
        self.target
    }

    pub(super) fn set_target(&mut self, target: Option<ElementId>) {
        self.target = target;
    }

    pub(super) fn indicator_visible(&self) -> bool {
        self.indicator_visible
    }

    pub(super) fn set_indicator_visible(&mut self, visible: bool) {
        self.indicator_visible = visible;
    }

    pub(super) fn owner(&self) -> FocusOwner {
        self.owner
    }

    #[cfg(test)]
    pub(super) fn set_owner(&mut self, owner: FocusOwner) {
        self.owner = owner;
    }

    /// The logical picker owner is the focus target while the native picker
    /// binding exists. It is not copied into the binding or window owner.
    pub(super) fn picker_owner(&self) -> Option<ElementId> {
        self.picker.as_ref().and(self.target)
    }

    // These are observed child handles, not owned resources. Installation and
    // removal stay atomic in open_picker/close_picker.
    pub(super) fn picker_window(&self) -> Option<HWND> {
        self.picker.map(|picker| picker.window)
    }

    pub(super) fn picker_list_window(&self) -> Option<HWND> {
        self.picker.map(|picker| picker.list)
    }

    fn picker_contains(&self, window: HWND) -> bool {
        self.picker.is_some_and(|picker| picker.contains(window))
    }

    pub(super) fn sync_native(&mut self, settings: HWND, actual: HWND) {
        self.owner = if actual == settings {
            FocusOwner::Settings
        } else if self.picker_contains(actual) {
            FocusOwner::Picker
        } else {
            FocusOwner::Outside
        };
    }

    pub(super) fn window_focus_changed(&mut self, focused: bool, next: HWND) {
        self.owner = if focused {
            FocusOwner::Settings
        } else if self.picker_contains(next) {
            FocusOwner::Picker
        } else {
            FocusOwner::Outside
        };
    }

    pub(super) fn open_picker(&mut self, owner: ElementId, window: HWND, list: HWND) {
        self.picker = Some(PickerBinding { window, list });
        self.target = Some(owner);
    }

    pub(super) fn close_picker(&mut self) {
        self.picker = None;
    }

    pub(super) fn clear_search_outside_window(&mut self) {
        if self.owner != FocusOwner::Settings && self.target == Some(ElementId::Search) {
            self.target = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(value: usize) -> HWND {
        HWND(value as *mut _)
    }

    #[test]
    fn picker_binding_observes_handles_without_copying_logical_owner() {
        let mut focus = FocusState::default();
        focus.open_picker(ElementId::InputDevice, window(2), window(3));
        focus.sync_native(window(1), window(3));
        assert_eq!(focus.picker_owner(), Some(ElementId::InputDevice));
        assert_eq!(focus.owner(), FocusOwner::Picker);
        focus.close_picker();
        focus.sync_native(window(1), window(3));
        assert_eq!(focus.picker_owner(), None);
        assert_eq!(focus.owner(), FocusOwner::Outside);
        assert_eq!(focus.target(), Some(ElementId::InputDevice));
    }

    #[test]
    fn losing_native_focus_clears_search_but_keeps_the_return_target() {
        let mut focus = FocusState::default();
        focus.set_target(Some(ElementId::Search));
        focus.sync_native(window(1), window(2));
        focus.clear_search_outside_window();
        assert_eq!(focus.target(), None);
        focus.set_target(Some(ElementId::OutputDevice));
        focus.clear_search_outside_window();
        assert_eq!(focus.target(), Some(ElementId::OutputDevice));
    }
}
