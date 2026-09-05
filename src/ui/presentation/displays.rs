//! Displays for the presentation.

use super::audio::{strip_index_wrapper, FriendlyLabel};
use crate::config::model::MonitorChoice;

pub(crate) fn display_output_label(
    monitor_name: &str,
    adapter_name: &str,
    connector_name: &str,
    active: bool,
) -> FriendlyLabel {
    let (monitor_name, _) = strip_index_wrapper(monitor_name.trim());
    let primary = if monitor_name.is_empty() {
        "Display".to_string()
    } else {
        monitor_name.to_string()
    };
    let mut details = Vec::new();
    for value in [adapter_name, connector_name] {
        let (value, _) = strip_index_wrapper(value.trim());
        if !value.is_empty()
            && !details
                .iter()
                .any(|item: &&str| item.eq_ignore_ascii_case(value))
        {
            details.push(value);
        }
    }
    if active {
        details.push("Active now");
    }
    FriendlyLabel {
        primary,
        detail: (!details.is_empty()).then(|| details.join(" · ")),
    }
}

pub(crate) fn monitor_choice_label(choice: &MonitorChoice) -> String {
    match choice {
        MonitorChoice::Cursor => "Cursor position".into(),
        MonitorChoice::Primary => "Primary".into(),
        MonitorChoice::Device(_) => "Saved monitor".into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayWizardStep {
    Displays,
    Arrangement,
    NameAndShortcut,
    Review,
}

impl DisplayWizardStep {
    pub(crate) const ALL: [Self; 4] = [
        Self::Displays,
        Self::Arrangement,
        Self::NameAndShortcut,
        Self::Review,
    ];

    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Displays => "Displays",
            Self::Arrangement => "Arrangement",
            Self::NameAndShortcut => "Name & shortcut",
            Self::Review => "Review",
        }
    }

    pub(crate) const fn number(self) -> u8 {
        match self {
            Self::Displays => 1,
            Self::Arrangement => 2,
            Self::NameAndShortcut => 3,
            Self::Review => 4,
        }
    }

    pub(crate) const fn previous(self) -> Option<Self> {
        match self {
            Self::Displays => None,
            Self::Arrangement => Some(Self::Displays),
            Self::NameAndShortcut => Some(Self::Arrangement),
            Self::Review => Some(Self::NameAndShortcut),
        }
    }

    pub(crate) const fn next(self) -> Option<Self> {
        match self {
            Self::Displays => Some(Self::Arrangement),
            Self::Arrangement => Some(Self::NameAndShortcut),
            Self::NameAndShortcut => Some(Self::Review),
            Self::Review => None,
        }
    }
}

/// The display query and a saved profile determine one readiness state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileReadiness {
    Unchecked,
    Unknown,
    MissingScreens(std::num::NonZeroUsize),
    Empty,
    NeedsTest,
    Ready,
}

impl ProfileReadiness {
    pub(crate) fn is_ready(self) -> bool {
        self == Self::Ready
    }
    pub(crate) fn needs_attention(self) -> bool {
        matches!(
            self,
            Self::Unchecked | Self::Unknown | Self::MissingScreens(_)
        )
    }
}

/// The same presentation feeds the card painter and its accessibility snapshot.
pub(crate) struct DisplayProfileCard {
    pub name: String,
    pub summary: String,
    pub shortcut: String,
    pub readiness: ProfileReadiness,
    pub selected: bool,
}

pub(crate) struct DisplayOutputCard {
    pub primary: String,
    pub detail: String,
    pub selected: bool,
    pub available: bool,
}
