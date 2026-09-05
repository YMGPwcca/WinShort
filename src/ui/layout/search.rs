//! Search for the layout.

use super::builder::{add_heading, add_region, add_row, add_section_content_gap};
use super::model::{ElementId, ElementKind, RegionKind};
use super::shell::SettingsLayout;
use crate::ui::navigation::search;

pub(super) fn add_search_results(layout: &mut SettingsLayout, query: &str) {
    let mut y = layout.content_column.y + 28.0;
    let matches = search(query);
    if matches.is_empty() {
        add_heading(
            layout,
            "No matching settings",
            "Try microphone, desktop, display, or overlay.",
            &mut y,
        );
        add_section_content_gap(&mut y);
        add_region(layout, RegionKind::WorkspaceNotice, &mut y, 76.0);
        return;
    }
    add_heading(
        layout,
        "Find a setting",
        "Choose a result to open the right page.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    for (index, result) in matches.iter().enumerate() {
        add_row(
            layout,
            &mut y,
            ElementId::SearchResult(index as u8),
            ElementKind::Action,
            result.item.title,
            &format!("{}  ·  {}", result.item.page.label(), result.item.section),
        );
    }
}
