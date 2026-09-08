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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum OverlayLifetime {
    #[default]
    Transient,
    Sticky(OverlayIcon),
}

impl OverlayLifetime {
    fn retain_for(self, rows: &[OverlayRow]) -> Self {
        match self {
            Self::Sticky(icon) if rows.iter().any(|row| row.icon == icon) => self,
            _ => Self::Transient,
        }
    }

    pub(crate) fn is_sticky(self) -> bool {
        matches!(self, Self::Sticky(_))
    }

    fn sticky_owner(self) -> Option<OverlayIcon> {
        match self {
            Self::Sticky(icon) => Some(icon),
            Self::Transient => None,
        }
    }
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
        let rows = incoming.rows.iter().take(3).cloned().collect::<Vec<_>>();
        return OverlayModel {
            lifetime: incoming.lifetime.retain_for(&rows),
            rows,
            bypass_categories: incoming.bypass_categories,
        };
    }
    let bypass_categories = current.bypass_categories || incoming.bypass_categories;
    let incoming_row = &incoming.rows[0];
    let lifetime = match incoming.lifetime {
        OverlayLifetime::Sticky(icon) => OverlayLifetime::Sticky(icon),
        OverlayLifetime::Transient => match current.lifetime {
            OverlayLifetime::Sticky(icon) if incoming_row.icon != icon => {
                OverlayLifetime::Sticky(icon)
            }
            _ => OverlayLifetime::Transient,
        },
    };
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
        lifetime,
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
    pub(super) lifetime: OverlayLifetime,
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
        Self::single_with_lifetime(row, OverlayLifetime::Transient)
    }

    pub(crate) fn single_with_lifetime(row: OverlayRow, lifetime: OverlayLifetime) -> Self {
        let rows = vec![row];
        Self {
            lifetime: lifetime.retain_for(&rows),
            rows,
            bypass_categories: false,
        }
    }

    pub(crate) fn preview(row: OverlayRow) -> Self {
        Self {
            rows: vec![row],
            bypass_categories: true,
            lifetime: OverlayLifetime::Transient,
        }
    }

    pub(crate) fn from_rows(rows: Vec<OverlayRow>) -> Self {
        Self::from_rows_with_lifetime(rows, OverlayLifetime::Transient)
    }

    pub(crate) fn from_rows_with_lifetime(
        rows: Vec<OverlayRow>,
        lifetime: OverlayLifetime,
    ) -> Self {
        Self {
            lifetime: lifetime.retain_for(&rows),
            rows,
            bypass_categories: false,
        }
    }

    pub(crate) fn is_sticky(&self) -> bool {
        self.lifetime.is_sticky()
    }

    pub(super) fn sticky_owner(&self) -> Option<OverlayIcon> {
        self.lifetime.sticky_owner()
    }

    pub(super) fn has_transient_rows(&self) -> bool {
        if !self.is_sticky() {
            return false;
        }
        let Some(owner) = self.sticky_owner() else {
            return false;
        };
        self.rows.iter().any(|row| row.icon != owner)
    }

    pub(super) fn retain_sticky_owner(self) -> Self {
        let Some(owner) = self.sticky_owner() else {
            return self;
        };
        let rows = self
            .rows
            .into_iter()
            .filter(|row| row.icon == owner)
            .collect::<Vec<_>>();
        Self {
            lifetime: self.lifetime.retain_for(&rows),
            rows,
            bypass_categories: self.bypass_categories,
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
            lifetime: self.lifetime.retain_for(&rows),
            rows,
            bypass_categories: false,
        }
    }
}
