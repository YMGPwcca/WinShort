//! Focus for the control center.

use super::state::SettingsUi;
use crate::ui::control_center_automation::AutomationFocusOwner;
use crate::ui::focus::next_focus_target;
use crate::ui::layout::{ElementId, ElementKind};
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn set_pointer_focus(&mut self, target: Option<ElementId>) {
        {
            let value = false;
            self.focus.set_indicator_visible(value);
        };
        {
            let value = target.filter(|id| {
                let Some(element) = self.layout.element(*id) else {
                    return false;
                };
                !matches!(element.kind, ElementKind::Card | ElementKind::Info)
                    && !self.is_disabled(*id)
            });
            self.focus.set_target(value);
        };
    }

    pub(super) fn clear_search_focus_without_settings_window(&mut self) {
        self.focus.clear_search_outside_window();
    }

    pub(super) fn repair_focus(&mut self) {
        if self.focus.owner() != AutomationFocusOwner::Settings {
            return;
        }
        let Some(current) = self.focus.target() else {
            return;
        };
        if self.layout.element(current).is_some() && !self.is_disabled(current) {
            return;
        }
        let order = self.layout.focus_order();
        let next = next_focus_target(&order, Some(current), false, |id| self.is_disabled(id));
        self.focus.set_target(next);
    }

    pub(super) fn focus_next(&mut self, hwnd: HWND, reverse: bool) {
        let order = self.layout.focus_order();
        let next = next_focus_target(&order, self.focus.target(), reverse, |id| {
            self.is_disabled(id)
        });
        let Some(next) = next else {
            self.focus.set_target(None);
            self.sync_search_caret(hwnd);
            self.publish_automation_snapshot(hwnd);
            return;
        };
        self.focus.set_indicator_visible(true);
        self.scroll_focus_into_view(next);
        self.rebuild_layout(hwnd);
        self.focus.set_target(Some(next));
        self.sync_search_caret(hwnd);
        self.publish_automation_snapshot(hwnd);
    }

    pub(super) fn scroll_focus_into_view(&mut self, id: ElementId) {
        let Some(element) = self.layout.element(id) else {
            return;
        };
        if !element.scrolls {
            return;
        }
        let top = self.layout.content_clip.y + 28.0;
        let bottom = self.layout.content_clip.bottom() - 12.0;
        let current = self.scroll;
        let target = if element.rect.y < top {
            (current - (top - element.rect.y)).clamp(0.0, self.layout.max_scroll)
        } else if element.rect.bottom() > bottom {
            (current + (element.rect.bottom() - bottom)).clamp(0.0, self.layout.max_scroll)
        } else {
            current
        };
        if (target - current).abs() >= 0.001 {
            self.scroll = target;
        }
    }

    pub(super) fn sync_focus_after_set_focus(&mut self, hwnd: HWND, actual: HWND) {
        self.focus.sync_native(hwnd, actual);
        self.finish_focus_transition(hwnd);
    }

    pub(super) fn on_window_focus(&mut self, hwnd: HWND, focused: bool, next: HWND) {
        self.focus.window_focus_changed(focused, next);
        self.finish_focus_transition(hwnd);
    }

    pub(super) fn set_picker_open(
        &mut self,
        hwnd: HWND,
        owner: ElementId,
        picker_hwnd: HWND,
        picker_list_hwnd: HWND,
        actual: HWND,
    ) {
        self.focus.open_picker(owner, picker_hwnd, picker_list_hwnd);
        self.focus.sync_native(hwnd, actual);
        self.finish_focus_transition(hwnd);
    }

    pub(super) fn sync_picker_focus(&mut self, hwnd: HWND, actual: HWND) {
        self.focus.sync_native(hwnd, actual);
        self.finish_focus_transition(hwnd);
    }

    pub(super) fn set_picker_closed(&mut self, hwnd: HWND, owner: Option<ElementId>, actual: HWND) {
        self.focus.close_picker(owner);
        self.focus.sync_native(hwnd, actual);
        self.finish_focus_transition(hwnd);
    }
}

impl SettingsUi {
    fn finish_focus_transition(&mut self, hwnd: HWND) {
        self.clear_search_focus_without_settings_window();
        self.sync_search_caret(hwnd);
        self.publish_automation_snapshot(hwnd);
    }
}
