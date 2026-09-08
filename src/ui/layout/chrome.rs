//! Chrome for the layout.

use super::geometry::Rect;

use crate::ui::theme::UiTokens;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BrandRowGeometry {
    pub row: Rect,
    pub icon: Rect,
    pub text: Rect,
}

pub(crate) fn brand_row_geometry(nav_width: f32) -> BrandRowGeometry {
    let row = Rect::new(0.0, 0.0, nav_width, UiTokens::BRAND_ROW_HEIGHT);
    let icon = Rect::new(
        UiTokens::BRAND_ROW_LEFT,
        row.y + (row.h - UiTokens::BRAND_ICON_SIZE) * 0.5,
        UiTokens::BRAND_ICON_SIZE,
        UiTokens::BRAND_ICON_SIZE,
    );
    let text_x = icon.right() + UiTokens::BRAND_TEXT_GAP;
    let text = Rect::new(
        text_x,
        row.y + (row.h - UiTokens::BRAND_TEXT_HEIGHT) * 0.5,
        (nav_width - text_x - UiTokens::BRAND_ROW_RIGHT).max(1.0),
        UiTokens::BRAND_TEXT_HEIGHT,
    );
    BrandRowGeometry { row, icon, text }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TopChromeGeometry {
    pub row: Rect,
    pub search: Rect,
    pub close: Rect,
    pub caption: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TitlebarGeometry {
    pub row: Rect,
    pub close: Rect,
    pub caption: Rect,
}

pub(crate) fn titlebar_geometry(width: f32, left: f32) -> TitlebarGeometry {
    let row = Rect::new(left, 0.0, (width - left).max(1.0), UiTokens::TOP_BAR_HEIGHT);
    let control_y = row.y + UiTokens::TITLEBAR_BUTTON_TOP;
    let close = Rect::new(
        width - UiTokens::TITLEBAR_BUTTON_RIGHT - UiTokens::TITLEBAR_BUTTON_WIDTH,
        control_y,
        UiTokens::TITLEBAR_BUTTON_WIDTH,
        UiTokens::TITLEBAR_BUTTON_HEIGHT,
    );
    let caption = Rect::new(row.x, row.y, (close.x - row.x).max(0.0), row.h);
    TitlebarGeometry {
        row,
        close,
        caption,
    }
}

pub(crate) fn top_chrome_geometry(width: f32, nav_width: f32) -> TopChromeGeometry {
    let titlebar = titlebar_geometry(width, nav_width);
    let row = titlebar.row;
    let close = titlebar.close;
    let control_y = row.y + UiTokens::TITLEBAR_BUTTON_TOP;
    let search_x = nav_width + 32.0;
    let search_available = (close.x - search_x - UiTokens::TOP_CHROME_SEARCH_GAP).max(1.0);
    let search_width = search_available.clamp(180.0_f32.min(search_available), 440.0);
    let search = Rect::new(search_x, control_y, search_width, 32.0);
    let caption = Rect::new(
        search.right(),
        row.y,
        (close.x - search.right()).max(0.0),
        row.h,
    );
    TopChromeGeometry {
        row,
        search,
        close,
        caption,
    }
}

pub(crate) fn top_chrome_separator_rect(top_bar: Rect) -> Rect {
    Rect::new(top_bar.x, top_bar.bottom(), top_bar.w, 1.0)
}
