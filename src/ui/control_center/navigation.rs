//! Navigation state transitions and user actions for the Control Center.

use super::native::post_main;
use super::state::SettingsUi;
use crate::ui::layout::ElementId;
use crate::ui::navigation::{search, Page};
use windows::Win32::Foundation::HWND;

impl SettingsUi {
    pub(super) fn set_page(&mut self, page: Page) {
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

    pub(super) fn activate_search_result(&mut self, hwnd: HWND, index: u8) {
        let results = search(&self.search_query);
        let Some(result) = results.get(index as usize) else {
            return;
        };
        self.open_search_destination(hwnd, result.item.page, result.item.target);
    }

    pub(super) fn open_search_destination(&mut self, hwnd: HWND, page: Page, target: ElementId) {
        self.set_page(page);
        if page == Page::Displays {
            self.refresh_display_outputs();
        }
        self.rebuild_layout(hwnd);
        let fallback = match page {
            Page::Displays => ElementId::DisplayProfilesEnabled,
            Page::Workspaces => ElementId::DesktopsEnabled,
            Page::Overlay => ElementId::OverlayEnabled,
            _ => ElementId::Nav(page),
        };
        let target = if self.layout.element(target).is_some() && !self.is_disabled(target) {
            target
        } else if self.layout.element(fallback).is_some() {
            fallback
        } else {
            ElementId::Nav(page)
        };
        self.focus.set_indicator_visible(true);
        self.focus.set_target(Some(target));
        self.scroll_focus_into_view(target);
        self.rebuild_layout(hwnd);
        self.sync_search_caret(hwnd);
        super::native::invalidate(hwnd);
        self.publish_automation_snapshot(hwnd);
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
