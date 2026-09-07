//! Layout for the diagnostics.

use super::model::{Action, Layout};
use super::native::client_size_dip;
use super::state::DiagnosticsUi;
use super::window::{FOOTER_HEIGHT, HEADER_HEIGHT, ROW_HEIGHT};
use crate::error::Result;
use crate::ui::layout::Rect;
use windows::Win32::Foundation::HWND;

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
        let lines = self.lines();
        let raw_height: f32 = lines
            .iter()
            .map(|line| if line.section { 34.0 } else { ROW_HEIGHT })
            .sum();
        let max_scroll = (raw_height - content.h + 12.0).max(0.0);
        let scroll = self.scroll.clamp(0.0, max_scroll);
        let footer = Rect::new(0.0, height - FOOTER_HEIGHT, width, FOOTER_HEIGHT);
        let button_widths = [150.0, 110.0, 166.0, 138.0, 86.0];
        let gap = 10.0;
        let mut x = 24.0;
        let y = footer.y + 48.0;
        let mut buttons = Vec::with_capacity(Action::ALL.len());
        for (action, button_width) in Action::ALL.into_iter().zip(button_widths) {
            buttons.push((action, Rect::new(x, y, button_width, 34.0)));
            x += button_width + gap;
        }
        Ok(Layout {
            width,
            content,
            footer,
            lines,
            max_scroll,
            scroll,
            buttons,
        })
    }
}
