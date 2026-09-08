//! Model for the overlay.

use crate::config::model::{OverlayNotificationCategory, OverlayNotifications};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayIcon {
    Microphone,
    Output,
    Application,
    Workspace,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayTone {
    Muted,
    Active,
    Changed,
    Unavailable,
}

fn row_rank(icon: OverlayIcon) -> usize {
    match icon {
        OverlayIcon::Microphone => 0,
        OverlayIcon::Output => 1,
        OverlayIcon::Application => 2,
        OverlayIcon::Workspace => 3,
        OverlayIcon::Info => 4,
    }
}

pub(super) fn merge_overlay_models(
    current: &OverlayModel,
    incoming: &OverlayModel,
) -> OverlayModel {
    if incoming.rows.len() != 1 {
        return OverlayModel {
            rows: incoming.rows.iter().take(3).cloned().collect(),
            bypass_categories: incoming.bypass_categories,
        };
    }
    let bypass_categories = current.bypass_categories || incoming.bypass_categories;
    let incoming_row = &incoming.rows[0];
    let mut rows = current.rows.clone();
    if let Some(existing) = rows.iter_mut().find(|row| row.icon == incoming_row.icon) {
        *existing = incoming_row.clone();
    } else {
        rows.push(incoming_row.clone());
    }
    rows.sort_by_key(|row| row_rank(row.icon));
    rows.truncate(3);
    OverlayModel {
        rows,
        bypass_categories,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OverlayRow {
    pub category: Option<OverlayNotificationCategory>,
    pub icon: OverlayIcon,
    pub tone: OverlayTone,
    pub title: String,
    pub detail: String,
}

impl OverlayRow {
    pub(crate) fn preview(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            category: None,
            icon: OverlayIcon::Info,
            tone: OverlayTone::Changed,
            title: title.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OverlayModel {
    pub rows: Vec<OverlayRow>,
    pub(super) bypass_categories: bool,
}

pub(super) fn concise(value: &str) -> String {
    const MAX: usize = 58;
    if value.chars().count() <= MAX {
        value.to_owned()
    } else {
        format!("{}…", value.chars().take(MAX - 1).collect::<String>())
    }
}

impl OverlayModel {
    pub(crate) fn single(row: OverlayRow) -> Self {
        Self {
            rows: vec![row],
            bypass_categories: false,
        }
    }

    pub(crate) fn preview(row: OverlayRow) -> Self {
        Self {
            rows: vec![row],
            bypass_categories: true,
        }
    }

    pub(crate) fn from_rows(rows: Vec<OverlayRow>) -> Self {
        Self {
            rows,
            bypass_categories: false,
        }
    }

    pub(crate) fn filter_enabled(self, notifications: OverlayNotifications) -> Self {
        if self.bypass_categories {
            return self;
        }
        Self {
            rows: self
                .rows
                .into_iter()
                .filter(|row| {
                    row.category
                        .is_none_or(|category| notifications.is_enabled(category))
                })
                .collect(),
            bypass_categories: false,
        }
    }
}
