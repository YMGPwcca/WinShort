//! Layout construction primitives shared by actual repeated row and card patterns.

use super::geometry::Rect;
use super::model::{
    Element, ElementId, ElementKind, HotkeySlot, RegionKind, SectionLabel, VisualRegion,
};
use super::shell::SettingsLayout;
use crate::ui::theme::UiTokens;

pub(super) fn add_heading(
    layout: &mut SettingsLayout,
    title: &str,
    description: &str,
    y: &mut f32,
) {
    let page_header = layout.sections.is_empty();
    if !page_header {
        *y += UiTokens::SECTION_GAP;
    }
    let height = if page_header {
        UiTokens::PAGE_HEADER_HEIGHT
    } else {
        if description.is_empty() {
            32.0
        } else {
            UiTokens::SECTION_HEADER_HEIGHT
        }
    };
    layout.sections.push(SectionLabel {
        title: title.into(),
        description: description.into(),
        y: *y,
        height,
        page_header,
    });
    *y += height;
}

pub(super) fn add_section_content_gap(y: &mut f32) {
    *y += UiTokens::SECTION_CONTENT_GAP;
}

pub(super) fn add_region(
    layout: &mut SettingsLayout,
    kind: RegionKind,
    y: &mut f32,
    height: f32,
) -> Rect {
    let rect = Rect::new(layout.content_column.x, *y, layout.content_column.w, height);
    layout.regions.push(VisualRegion {
        kind,
        rect,
        scrolls: true,
    });
    *y += height + UiTokens::ROW_GAP;
    rect
}

pub(super) fn add_element(
    layout: &mut SettingsLayout,
    y: &mut f32,
    id: ElementId,
    kind: ElementKind,
    label: impl Into<String>,
    description: impl Into<String>,
    rect: Rect,
) {
    layout.elements.push(Element {
        id,
        kind,
        rect,
        label: label.into(),
        description: description.into(),
        scrolls: true,
    });
    *y = (*y).max(rect.bottom() + UiTokens::ROW_GAP);
}

pub(super) fn add_row(
    layout: &mut SettingsLayout,
    y: &mut f32,
    id: ElementId,
    kind: ElementKind,
    label: &str,
    description: &str,
) {
    add_element(
        layout,
        y,
        id,
        kind,
        label,
        description,
        Rect::new(
            layout.content_column.x,
            *y,
            layout.content_column.w,
            UiTokens::ROW_HEIGHT,
        ),
    );
}

pub(super) fn add_card(
    layout: &mut SettingsLayout,
    y: &mut f32,
    id: ElementId,
    label: &str,
    description: &str,
) {
    add_element(
        layout,
        y,
        id,
        if id == ElementId::NewDisplayProfile {
            ElementKind::ButtonPrimary
        } else {
            ElementKind::Action
        },
        label,
        description,
        Rect::new(
            layout.content_column.x,
            *y,
            layout.content_column.w,
            UiTokens::CARD_HEIGHT,
        ),
    );
}

pub(super) fn add_card_pair(
    layout: &mut SettingsLayout,
    y: &mut f32,
    left: (ElementId, &str, &str),
    right: (ElementId, &str, &str),
) {
    let gap = UiTokens::CARD_COLUMN_GAP;
    let available = layout.content_column.w;
    let card_w = ((available - gap) * 0.5).max(180.0);
    if card_w < 280.0 {
        add_card(layout, y, left.0, left.1, left.2);
        add_card(layout, y, right.0, right.1, right.2);
        return;
    }
    let row_y = *y;
    for (index, (id, label, description)) in [left, right].into_iter().enumerate() {
        add_element(
            layout,
            y,
            id,
            ElementKind::Action,
            label,
            description,
            Rect::new(
                layout.content_column.x + index as f32 * (card_w + gap),
                row_y,
                card_w,
                UiTokens::CARD_HEIGHT,
            ),
        );
    }
    *y = row_y + UiTokens::CARD_HEIGHT + UiTokens::ROW_GAP;
}

fn add_managed_hotkey(
    layout: &mut SettingsLayout,
    card: Rect,
    slot: HotkeySlot,
    capture_id: ElementId,
    label: &str,
    description: &str,
) {
    layout.elements.push(Element {
        id: ElementId::HotkeyCard(slot),
        kind: ElementKind::Card,
        rect: card,
        label: label.into(),
        description: description.into(),
        scrolls: true,
    });
    let control_x = card.right() - 18.0 - UiTokens::CONTROL_WIDTH;
    let keycap_y = card.y + 10.0;
    let keycap_height = 32.0;
    let actions_y = keycap_y + keycap_height + UiTokens::ROW_GAP;
    layout.elements.push(Element {
        id: capture_id,
        kind: ElementKind::Hotkey,
        rect: Rect::new(control_x, keycap_y, UiTokens::CONTROL_WIDTH, keycap_height),
        label: format!("{label} shortcut"),
        description: "Record a new shortcut".into(),
        scrolls: true,
    });
    let button_gap = UiTokens::ROW_GAP;
    let button_width = (UiTokens::CONTROL_WIDTH - button_gap) * 0.5;
    layout.elements.push(Element {
        id: ElementId::HotkeyEnabled(slot),
        kind: ElementKind::ButtonSecondary,
        rect: Rect::new(control_x, actions_y, button_width, 28.0),
        label: "Shortcut state".into(),
        description: format!("Enable or disable {label}"),
        scrolls: true,
    });
    layout.elements.push(Element {
        id: ElementId::HotkeyUnassign(slot),
        kind: ElementKind::ButtonSecondary,
        rect: Rect::new(
            control_x + button_width + button_gap,
            actions_y,
            button_width,
            28.0,
        ),
        label: "Unassign".into(),
        description: format!("Remove the shortcut for {label}"),
        scrolls: true,
    });
}

pub(super) fn add_hotkey_grid(
    layout: &mut SettingsLayout,
    y: &mut f32,
    items: &[(ElementId, &str, &str)],
) {
    let column_gap = UiTokens::CARD_COLUMN_GAP;
    let row_gap = UiTokens::ROW_GAP;
    let columns = if layout.content_column.w >= 980.0 {
        2
    } else {
        1
    };
    let card_w = if columns == 1 {
        layout.content_column.w
    } else {
        (layout.content_column.w - column_gap) * 0.5
    };
    let row_h = 88.0;
    let start = *y;
    for (index, (capture_id, label, description)) in items.iter().copied().enumerate() {
        let Some(slot) = HotkeySlot::from_capture_id(capture_id) else {
            continue;
        };
        let row = index / columns;
        let column = index % columns;
        let card = Rect::new(
            layout.content_column.x + column as f32 * (card_w + column_gap),
            start + row as f32 * (row_h + row_gap),
            card_w,
            row_h,
        );
        add_managed_hotkey(layout, card, slot, capture_id, label, description);
    }
    if !items.is_empty() {
        let rows = items.len().div_ceil(columns);
        *y = start + rows as f32 * row_h + rows.saturating_sub(1) as f32 * row_gap;
    }
}

pub(super) fn add_button_grid(
    layout: &mut SettingsLayout,
    y: &mut f32,
    items: &[(ElementId, ElementKind, &str, &str)],
) {
    let column_gap = UiTokens::CARD_COLUMN_GAP;
    let row_gap = UiTokens::ROW_GAP;
    let widths = [142.0, 142.0, 142.0];
    let mut x = layout.content_column.x;
    let mut row_y = *y;
    let row_height = 36.0;
    for (index, (id, kind, label, description)) in items.iter().copied().enumerate() {
        let width = widths[index % widths.len()];
        if index > 0 && x + width > layout.content_column.right() {
            x = layout.content_column.x;
            row_y += row_height + row_gap;
        }
        add_element(
            layout,
            y,
            id,
            kind,
            label,
            description,
            Rect::new(x, row_y, width.min(layout.content_column.w), row_height),
        );
        x += width + column_gap;
    }
    if !items.is_empty() {
        *y = row_y + row_height;
    }
}

pub(super) fn add_profile_card(
    layout: &mut SettingsLayout,
    y: &mut f32,
    id: u8,
    index: usize,
    columns: usize,
) {
    let column_gap = UiTokens::CARD_COLUMN_GAP;
    let available = layout.content_column.w;
    let columns = columns.max(1);
    let card_w = if columns == 1 {
        available
    } else {
        (available - column_gap) * 0.5
    };
    let x = layout.content_column.x + (index % columns) as f32 * (card_w + column_gap);
    let row_y = *y + (index / columns) as f32 * UiTokens::PROFILE_ROW_STEP;
    layout.elements.push(Element {
        id: ElementId::DisplayProfileCard(id),
        kind: ElementKind::Action,
        rect: Rect::new(x, row_y, card_w, UiTokens::PROFILE_CARD_HEIGHT),
        label: "Display profile".into(),
        description: "Select this saved arrangement".into(),
        scrolls: true,
    });
    layout.elements.push(Element {
        id: ElementId::DisplayProfileAction(id),
        kind: ElementKind::ButtonSecondary,
        rect: Rect::new(x + card_w - 136.0, row_y + 64.0, 116.0, 30.0),
        label: "Activate or review profile".into(),
        description: "Apply a tested profile or review its setup".into(),
        scrolls: true,
    });
}
