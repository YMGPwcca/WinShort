//! Model for the overlay.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayIcon {
    Microphone,
    Output,
    Application,
    Workspace,
    /// Reserved for informational rows (tone ladder completeness).
    #[allow(dead_code)]
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
        };
    }
    let incoming_row = &incoming.rows[0];
    let mut rows = current.rows.clone();
    if let Some(existing) = rows.iter_mut().find(|row| row.icon == incoming_row.icon) {
        *existing = incoming_row.clone();
    } else {
        rows.push(incoming_row.clone());
    }
    rows.sort_by_key(|row| row_rank(row.icon));
    rows.truncate(3);
    OverlayModel { rows }
}

#[derive(Debug, Clone)]
pub(crate) struct OverlayRow {
    pub icon: OverlayIcon,
    pub tone: OverlayTone,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OverlayModel {
    pub rows: Vec<OverlayRow>,
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
        Self { rows: vec![row] }
    }
}
