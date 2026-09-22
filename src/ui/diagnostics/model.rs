//! Model for the diagnostics.

use crate::diagnostics::snapshot::Health;
use crate::ui::layout::{Rect, TitlebarGeometry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Copy,
    OpenLogs,
    Bundle,
    SelfTest,
    Close,
}

#[derive(Debug, Clone)]
pub(super) struct Line {
    pub(super) section: bool,
    pub(super) key: String,
    pub(super) value: String,
    pub(super) health: Option<Health>,
}

impl Action {
    pub(super) const FOOTER_ACTIONS: [Self; 4] =
        [Self::Copy, Self::OpenLogs, Self::Bundle, Self::SelfTest];

    pub(super) const ALL: [Self; 5] = [
        Self::Copy,
        Self::OpenLogs,
        Self::Bundle,
        Self::SelfTest,
        Self::Close,
    ];

    pub(super) fn label(self, busy: bool) -> &'static str {
        match self {
            Self::Copy => "Copy Diagnostics",
            Self::OpenLogs => "Open Logs",
            Self::Bundle if busy => "Creating…",
            Self::Bundle => "Create Support Bundle",
            Self::SelfTest => "Run Self-Test",
            Self::Close => "Close",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ReportSection {
    pub(super) title: String,
    pub(super) rows: Vec<Line>,
    pub(super) rect: Rect,
}

#[derive(Debug, Clone)]
pub(super) struct Layout {
    pub(super) width: f32,
    pub(super) chrome: TitlebarGeometry,
    pub(super) content: Rect,
    pub(super) footer: Rect,
    pub(super) sections: Vec<ReportSection>,
    pub(super) max_scroll: f32,
    pub(super) scroll: f32,
    pub(super) buttons: Vec<(Action, Rect)>,
}
