//! Model for the overlay.

use crate::config::model::{OverlayNotificationCategory, OverlayNotifications};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OverlayIcon {
    Microphone,
    Output,
    Application,
    Executable(std::sync::Arc<super::exe_icon::ExecutableIcon>),
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
    /// Cached unscaled DirectWrite measurement, refreshed when rows change.
    pub(super) width_dip: Option<f32>,
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
            width_dip: None,
        }
    }

    pub(crate) fn preview(row: OverlayRow) -> Self {
        Self {
            rows: vec![row],
            bypass_categories: true,
            width_dip: None,
        }
    }

    pub(crate) fn from_rows(rows: Vec<OverlayRow>) -> Self {
        Self {
            rows,
            bypass_categories: false,
            width_dip: None,
        }
    }

    pub(crate) fn filter_enabled(self, notifications: OverlayNotifications) -> Self {
        if self.bypass_categories {
            return self;
        }
        let rows = self
            .rows
            .into_iter()
            .filter(|row| {
                row.category
                    .is_none_or(|category| notifications.is_enabled(category))
            })
            .collect::<Vec<_>>();
        Self {
            rows,
            bypass_categories: false,
            width_dip: None,
        }
    }
}
