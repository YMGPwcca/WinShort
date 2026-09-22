//! Layout for the diagnostics.

use super::model::{Action, Layout, Line, ReportSection};
use super::native::client_size_dip;
use super::state::DiagnosticsUi;
use super::window::{FOOTER_HEIGHT, HEADER_HEIGHT, ROW_HEIGHT};
use crate::error::Result;
use crate::ui::layout::{titlebar_geometry, Rect};
use crate::ui::theme::UiTokens;
use windows::Win32::Foundation::HWND;

pub(super) const CONTENT_PADDING_X: f32 = UiTokens::PAGE_MARGIN;
pub(super) const CONTENT_TOP_INSET: f32 = 16.0;
pub(super) const CONTENT_BOTTOM_INSET: f32 = 16.0;
pub(super) const SECTION_HEADER_HEIGHT: f32 = 48.0;
pub(super) const SECTION_BOTTOM_PADDING: f32 = 12.0;
pub(super) const SECTION_GAP: f32 = UiTokens::CARD_COLUMN_GAP;

impl DiagnosticsUi {
    pub(super) fn layout(&self, hwnd: HWND) -> Result<Layout> {
        let (width, height) = if let Some(renderer) = self.renderer.as_ref() {
            renderer.client_size_dip()
        } else {
            client_size_dip(hwnd, self.dpi)?
        };
        let content = Rect::new(
            0.0,
            HEADER_HEIGHT,
            width,
            (height - HEADER_HEIGHT - FOOTER_HEIGHT).max(80.0),
        );
        let sections = report_sections(self.lines(), content, width);
        let content_end = sections.last().map_or(content.y, |section| {
            section.rect.bottom() + CONTENT_BOTTOM_INSET
        });
        let max_scroll = (content_end - content.bottom()).max(0.0);
        let scroll = self.scroll.clamp(0.0, max_scroll);
        let footer = Rect::new(0.0, height - FOOTER_HEIGHT, width, FOOTER_HEIGHT);
        let button_widths = [150.0, 110.0, 166.0, 138.0];
        let gap = 10.0;
        let mut x = CONTENT_PADDING_X;
        let y = footer.y + 48.0;
        let mut buttons = Vec::with_capacity(Action::FOOTER_ACTIONS.len());
        for (action, button_width) in Action::FOOTER_ACTIONS.into_iter().zip(button_widths) {
            buttons.push((action, Rect::new(x, y, button_width, 34.0)));
            x += button_width + gap;
        }
        Ok(Layout {
            width,
            chrome: titlebar_geometry(width, 0.0),
            content,
            footer,
            sections,
            max_scroll,
            scroll,
            buttons,
        })
    }
}

fn report_sections(lines: Vec<Line>, content: Rect, width: f32) -> Vec<ReportSection> {
    let mut sections = Vec::new();
    for line in lines {
        if line.section {
            sections.push(ReportSection {
                title: line.key,
                rows: Vec::new(),
                rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            });
        } else if let Some(section) = sections.last_mut() {
            section.rows.push(line);
        }
    }

    let card_width = (width - CONTENT_PADDING_X * 2.0).max(1.0);
    let mut y = content.y + CONTENT_TOP_INSET;
    for section in &mut sections {
        let height =
            SECTION_HEADER_HEIGHT + section.rows.len() as f32 * ROW_HEIGHT + SECTION_BOTTOM_PADDING;
        section.rect = Rect::new(CONTENT_PADDING_X, y, card_width, height);
        y += height + SECTION_GAP;
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::snapshot::Health;

    fn lines_for_section(title: &str, key: &str) -> Vec<Line> {
        vec![
            Line {
                section: true,
                key: title.into(),
                value: String::new(),
                health: None,
            },
            Line {
                section: false,
                key: key.into(),
                value: "value".into(),
                health: Some(Health::Healthy),
            },
        ]
    }

    #[test]
    fn report_sections_become_separated_cards_without_losing_rows() {
        let mut lines = lines_for_section("Application", "Version");
        lines.extend(lines_for_section("Audio", "Output"));
        let content = Rect::new(0.0, 124.0, 860.0, 400.0);
        let sections = report_sections(lines, content, 860.0);

        assert_eq!(sections.len(), 2);
        assert_eq!(
            sections
                .iter()
                .map(|section| section.rows.len())
                .sum::<usize>(),
            2
        );
        assert_eq!(sections[0].rect.x, CONTENT_PADDING_X);
        assert_eq!(sections[0].rect.w, 796.0);
        assert_eq!(sections[1].rect.y, sections[0].rect.bottom() + SECTION_GAP);
    }

    #[test]
    fn report_layout_scrolls_from_card_bounds() {
        let content = Rect::new(0.0, 124.0, 860.0, 40.0);
        let lines = vec![Line {
            section: true,
            key: "Application".into(),
            value: String::new(),
            health: None,
        }];
        let sections = report_sections(lines, content, 860.0);
        let content_end = sections[0].rect.bottom() + CONTENT_BOTTOM_INSET;
        assert_eq!(
            (content_end - content.bottom()).max(0.0),
            CONTENT_TOP_INSET
                + SECTION_HEADER_HEIGHT
                + SECTION_BOTTOM_PADDING
                + CONTENT_BOTTOM_INSET
                - content.h,
        );
    }

    #[test]
    fn footer_actions_keep_close_in_the_shared_titlebar() {
        let content = Rect::new(0.0, 124.0, 860.0, 400.0);
        let sections = report_sections(Vec::new(), content, 860.0);
        assert!(sections.is_empty());
        assert_eq!(Action::FOOTER_ACTIONS.len(), 4);
        assert!(!Action::FOOTER_ACTIONS.contains(&Action::Close));
    }
}
