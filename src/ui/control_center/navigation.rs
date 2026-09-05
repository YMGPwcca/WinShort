//! Navigation state transitions and user actions for the Control Center.

use super::native::post_main;
use super::state::SettingsUi;
use crate::ui::layout::ElementId;
use crate::ui::navigation::{search, Page};
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn set_page(&mut self, page: Page) {
        if page != Page::Displays && self.display_editor.is_some() {
            self.replace_draft((*self.config_access.current()).clone());
        }
        self.page = page;
        if page == Page::Overlay {
            self.refresh_overlay_preview_aspect();
        }
        self.search_query.clear();
        self.reset_scroll();
        self.validation.clear();
    }

    pub(super) fn activate_navigation(&mut self, page: Page) {
        self.set_page(page);
        self.focus.set_target(Some(ElementId::Nav(page)));
        if page == Page::Displays {
            self.refresh_display_outputs();
        }
    }

    pub(super) fn activate_search_result(&mut self, index: u8) {
        let results = search(&self.search_query);
        let Some(result) = results.get(index as usize) else {
            return;
        };
        let page = result.item.page;
        let target = result.item.target;
        self.set_page(page);
        self.focus.set_target(Some(target));
        if page == Page::Displays {
            self.refresh_display_outputs();
        }
    }

    pub(super) fn activate_special_workspace(&mut self, hwnd: HWND) {
        if !self.draft.virtual_desktops.enabled {
            self.activate(hwnd, ElementId::DesktopsEnabled);
        } else if matches!(
            &self.runtime.desktop.native,
            crate::desktop::BackendAvailability::Available
        ) {
            post_main(crate::event::AppEvent::ToggleSpecialWorkspaceFromUi);
        } else {
            self.set_page(Page::Workspaces);
        }
    }
}
