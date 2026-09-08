//! Pages for the layout.

use super::advanced::add_advanced;
use super::audio::add_audio;
use super::displays::add_displays;
use super::home::add_home;
use super::model::LayoutContext;
use super::overlay::add_overlay;
use super::shell::SettingsLayout;
use super::shortcuts::add_shortcuts;
use super::system::add_system;
use super::workspaces::add_workspaces;
use crate::ui::navigation::Page;

pub(super) fn add_page(layout: &mut SettingsLayout, page: Page, context: &LayoutContext) {
    match page {
        Page::Home => add_home(layout),
        Page::Shortcuts => add_shortcuts(layout, context),
        Page::Audio => add_audio(layout, context),
        Page::Workspaces => add_workspaces(layout, context),
        Page::Displays => add_displays(layout, context),
        Page::Overlay => add_overlay(layout, context),
        Page::System => add_system(layout),
        Page::Advanced => add_advanced(layout),
    }
}
