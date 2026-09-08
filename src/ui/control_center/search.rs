//! Search state, caret timing and interaction for the Control Center.

use super::native::{invalidate, start_timer};
use super::state::SettingsUi;
use crate::ui::control_center_automation::AutomationFocusOwner;
use crate::ui::controls;
use crate::ui::layout::{ElementId, ElementKind};
use crate::ui::navigation::search;
use crate::ui::renderer::Renderer;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);

#[derive(Debug, Default)]
pub(super) enum SearchCaret {
    #[default]
    Hidden,
    Blinking {
        visible: bool,
        deadline: Instant,
    },
}

impl SearchCaret {
    pub(super) fn active(now: Instant) -> Self {
        Self::Blinking {
            visible: true,
            deadline: now + CARET_BLINK_INTERVAL,
        }
    }

    pub(super) fn visible(&self) -> bool {
        matches!(self, Self::Blinking { visible: true, .. })
    }

    pub(super) fn sync_focus(&mut self, focused: bool, now: Instant) {
        if !focused {
            *self = Self::Hidden;
        } else if matches!(self, Self::Hidden) {
            *self = Self::active(now);
        }
    }

    /// The native timer remains active while the caret is blinking.
    pub(super) fn tick(&mut self, now: Instant) -> bool {
        let Self::Blinking { visible, deadline } = self else {
            return false;
        };
        if now >= *deadline {
            *visible = !*visible;
            *deadline = now + CARET_BLINK_INTERVAL;
        }
        true
    }

    #[cfg(test)]
    pub(super) fn deadline(&self) -> Option<Instant> {
        match self {
            Self::Hidden => None,
            Self::Blinking { deadline, .. } => Some(*deadline),
        }
    }
}

impl SettingsUi {
    pub(super) fn search_has_focus(&self) -> bool {
        self.focus.target() == Some(ElementId::Search)
            && self.focus.owner() == AutomationFocusOwner::Settings
    }

    pub(super) fn sync_search_caret(&mut self, hwnd: HWND) {
        let focused = self.search_has_focus();
        self.caret.sync_focus(focused, Instant::now());
        if focused {
            start_timer(hwnd);
        }
    }

    pub(super) fn restart_search_caret(&mut self, hwnd: HWND) {
        if self.search_has_focus() {
            self.caret = SearchCaret::active(Instant::now());
            start_timer(hwnd);
        } else {
            self.caret = SearchCaret::Hidden;
        }
    }

    pub(super) fn tick_search_caret(&mut self, now: Instant) -> bool {
        if !self.search_has_focus() {
            self.caret = SearchCaret::Hidden;
        }
        self.caret.tick(now)
    }

    pub(super) fn handle_search_key(&mut self, hwnd: HWND, vk: u16) -> bool {
        if !self.search_has_focus() {
            return false;
        }
        match vk {
            0x08 => self.search_query.pop(),
            0x1B if !self.search_query.is_empty() => {
                self.search_query.clear();
                None
            }
            0x0D => {
                if let Some(result) = search(&self.search_query).first() {
                    let page = result.item.page;
                    let target = result.item.target;
                    self.set_page(page);
                    {
                        let value = Some(target);
                        self.focus.set_target(value);
                    };
                }
                None
            }
            _ => return false,
        };
        self.restart_search_caret(hwnd);
        self.reset_scroll();
        self.rebuild_layout(hwnd);
        invalidate(hwnd);
        self.publish_automation_snapshot(hwnd);
        true
    }

    pub(super) fn handle_search_char(&mut self, hwnd: HWND, ch: u16) -> bool {
        if !self.search_has_focus() || ch < 0x20 {
            return false;
        }
        if let Some(character) = char::from_u32(u32::from(ch)) {
            if !character.is_control() && self.search_query.chars().count() < 256 {
                self.search_query.push(character);
                self.restart_search_caret(hwnd);
                self.reset_scroll();
                self.rebuild_layout(hwnd);
                invalidate(hwnd);
                self.publish_automation_snapshot(hwnd);
                return true;
            }
        }
        false
    }

    pub(super) fn draw_search_results(&self, renderer: &Renderer) {
        for element in &self.layout.elements {
            if !element.scrolls
                || !self.layout.content_clip.intersects(element.rect)
                || element.kind == ElementKind::Card
            {
                continue;
            }
            controls::draw_row(
                renderer,
                element,
                self.value_for(element.id),
                self.interaction(element.id, false),
            );
        }
    }
}
