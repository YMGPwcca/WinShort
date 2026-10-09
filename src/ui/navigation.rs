//! User-facing navigation and deterministic local search descriptors.
//!
//! The shell searches the language people use, not the compatibility names in
//! the configuration file. Results point to stable page/control destinations.

use crate::ui::layout::ElementId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Page {
    #[default]
    Home,
    Shortcuts,
    Audio,
    Workspaces,
    Displays,
    Overlay,
    System,
    Advanced,
}

impl Page {
    pub const PRIMARY: [Self; 7] = [
        Self::Home,
        Self::Shortcuts,
        Self::Audio,
        Self::Workspaces,
        Self::Displays,
        Self::Overlay,
        Self::System,
    ];

    pub const SECONDARY: [Self; 1] = [Self::Advanced];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Shortcuts => "Shortcuts",
            Self::Audio => "Audio",
            Self::Workspaces => "Workspaces",
            Self::Displays => "Displays",
            Self::Overlay => "Overlay",
            Self::System => "System",
            Self::Advanced => "Advanced",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Home => "See what WinShort is doing right now.",
            Self::Shortcuts => "Make everyday actions feel like second nature.",
            Self::Audio => "Choose devices and control the app in front of you.",
            Self::Workspaces => "Move through desktops without losing your place.",
            Self::Displays => "Save and safely switch the way your screens work.",
            Self::Overlay => "Shape the small status card that appears on screen.",
            Self::System => "Startup, behavior, support, and reset.",
            Self::Advanced => "Technical options for troubleshooting and fine tuning.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchItem {
    pub title: &'static str,
    pub keywords: &'static str,
    pub page: Page,
    pub section: &'static str,
    pub target: ElementId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    pub item: &'static SearchItem,
    pub score: u8,
}

static SEARCH_ITEMS: &[SearchItem] = &[
    SearchItem {
        title: "Mute microphone shortcut",
        keywords: "microphone mic mute hotkey input",
        page: Page::Shortcuts,
        section: "Audio",
        target: ElementId::MicHotkey,
    },
    SearchItem {
        title: "Mute speakers shortcut",
        keywords: "speaker speakers output mute hotkey",
        page: Page::Shortcuts,
        section: "Audio",
        target: ElementId::OutputHotkey,
    },
    SearchItem {
        title: "Next microphone shortcut",
        keywords: "microphone mic input next cycle device hotkey",
        page: Page::Shortcuts,
        section: "Audio",
        target: ElementId::CycleInputHotkey,
    },
    SearchItem {
        title: "Next speaker shortcut",
        keywords: "speaker speakers output next cycle device hotkey",
        page: Page::Shortcuts,
        section: "Audio",
        target: ElementId::CycleOutputHotkey,
    },
    SearchItem {
        title: "Current app audio shortcut",
        keywords: "foreground app application mute volume audio",
        page: Page::Shortcuts,
        section: "Audio",
        target: ElementId::ForegroundHotkey,
    },
    SearchItem {
        title: "Devices used by Next microphone",
        keywords: "microphone mic input devices allowlist available selected cycle",
        page: Page::Audio,
        section: "Microphones",
        target: ElementId::InputCycleMode(0),
    },
    SearchItem {
        title: "Devices used by Next speaker",
        keywords: "speaker speakers output devices allowlist available selected cycle",
        page: Page::Audio,
        section: "Speakers",
        target: ElementId::OutputCycleMode(0),
    },
    SearchItem {
        title: "Microphone device",
        keywords: "microphone mic input recording device",
        page: Page::Audio,
        section: "Microphones",
        target: ElementId::InputDevice,
    },
    SearchItem {
        title: "Speaker device",
        keywords: "speaker speakers output playback device",
        page: Page::Audio,
        section: "Speakers",
        target: ElementId::OutputDevice,
    },
    SearchItem {
        title: "Desktop number shortcuts",
        keywords: "desktop workspace win numbers 1 2 3 4 5 6 7 8 9",
        page: Page::Workspaces,
        section: "Desktops",
        target: ElementId::WinNumberEnabled,
    },
    SearchItem {
        title: "Previous desktop shortcut",
        keywords: "desktop workspace previous back history",
        page: Page::Workspaces,
        section: "Desktops",
        target: ElementId::PreviousDesktopHotkey,
    },
    SearchItem {
        title: "Move window to Special Desktop shortcut",
        keywords: "special desktop workspace window move away",
        page: Page::Workspaces,
        section: "Special Desktop",
        target: ElementId::AssignScratchpadHotkey,
    },
    SearchItem {
        title: "Open / close Special Desktop shortcut",
        keywords: "special desktop workspace open close toggle return",
        page: Page::Workspaces,
        section: "Special Desktop",
        target: ElementId::ToggleScratchpadHotkey,
    },
    SearchItem {
        title: "Display profile",
        keywords: "display monitor screen profile layout setup arrange",
        page: Page::Displays,
        section: "Display profiles",
        target: ElementId::NewDisplayProfile,
    },
    SearchItem {
        title: "Overlay style",
        keywords: "status card toast hud light dark system appearance",
        page: Page::Overlay,
        section: "Overlay",
        target: ElementId::OverlayAppearance,
    },
    SearchItem {
        title: "Overlay blur",
        keywords: "status card backdrop transparent blur acrylic solid background",
        page: Page::Overlay,
        section: "Overlay",
        target: ElementId::OverlayBlur,
    },
    SearchItem {
        title: "Display profile notifications",
        keywords: "overlay display profile notifications alerts",
        page: Page::Overlay,
        section: "Notifications",
        target: ElementId::OverlayDisplayProfile,
    },
    SearchItem {
        title: "Microphone notifications",
        keywords: "overlay alerts mic muted input notifications",
        page: Page::Overlay,
        section: "Notifications",
        target: ElementId::OverlayMicrophone,
    },
    SearchItem {
        title: "Speaker notifications",
        keywords: "overlay alerts speaker output notifications",
        page: Page::Overlay,
        section: "Notifications",
        target: ElementId::OverlaySpeaker,
    },
    SearchItem {
        title: "Current app audio notifications",
        keywords: "overlay alerts current app audio notifications",
        page: Page::Overlay,
        section: "Notifications",
        target: ElementId::OverlayCurrentAppAudio,
    },
    SearchItem {
        title: "Workspace notifications",
        keywords: "overlay alerts workspace desktop notifications",
        page: Page::Overlay,
        section: "Notifications",
        target: ElementId::OverlayWorkspace,
    },
    SearchItem {
        title: "Overlay size",
        keywords: "status card scale size",
        page: Page::Overlay,
        section: "Size and timing",
        target: ElementId::OverlayScale,
    },
    SearchItem {
        title: "Overlay duration",
        keywords: "status card timeout duration time seconds",
        page: Page::Overlay,
        section: "Size and timing",
        target: ElementId::OverlayDuration,
    },
    SearchItem {
        title: "Opacity on hover",
        keywords: "overlay hover mouse pointer opacity fade transparency",
        page: Page::Overlay,
        section: "Size and timing",
        target: ElementId::OverlayHoverOpacity,
    },
    SearchItem {
        title: "Version and build info",
        keywords: "about version build date revision copy",
        page: Page::System,
        section: "About",
        target: ElementId::CopyVersionInfo,
    },
    SearchItem {
        title: "Overlay position",
        keywords: "status card toast hud corner monitor grid top bottom center",
        page: Page::Overlay,
        section: "Position",
        target: ElementId::OverlayPositionCell(0),
    },
    SearchItem {
        title: "Start WinShort with Windows",
        keywords: "startup launch sign in boot automatically",
        page: Page::System,
        section: "Startup",
        target: ElementId::StartWithWindows,
    },
    SearchItem {
        title: "Pause shortcuts",
        keywords: "pause suspend hotkeys behavior",
        page: Page::System,
        section: "Behavior",
        target: ElementId::StartHotkeysEnabled,
    },
    SearchItem {
        title: "Diagnostics and support",
        keywords: "diagnostics logs support self test troubleshooting",
        page: Page::System,
        section: "Support",
        target: ElementId::DiagnosticsStatus,
    },
    SearchItem {
        title: "Advanced audio settings",
        keywords: "audio role console multimedia communications expert",
        page: Page::Advanced,
        section: "Audio",
        target: ElementId::InputRole,
    },
];

pub fn search_items() -> &'static [SearchItem] {
    SEARCH_ITEMS
}

#[cfg(test)]
mod destination_regressions {
    use super::*;

    #[test]
    fn overlay_settings_search_points_to_the_matching_controls() {
        for (query, target) in [
            (
                "display profile notifications",
                ElementId::OverlayDisplayProfile,
            ),
            ("opacity hover", ElementId::OverlayHoverOpacity),
            ("overlay size", ElementId::OverlayScale),
            ("overlay duration", ElementId::OverlayDuration),
        ] {
            assert_eq!(search(query).first().unwrap().item.target, target);
        }
    }
}

fn page_priority(page: Page) -> u8 {
    match page {
        Page::Shortcuts => 0,
        Page::Audio => 1,
        Page::Workspaces => 2,
        Page::Displays => 3,
        Page::Overlay => 4,
        Page::System => 5,
        Page::Advanced => 6,
        Page::Home => 7,
    }
}

pub fn search(query: &str) -> Vec<SearchMatch> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }

    let mut matches: Vec<SearchMatch> = search_items()
        .iter()
        .filter_map(|item| {
            let title = item.title.to_ascii_lowercase();
            let keywords = item.keywords.to_ascii_lowercase();
            let mut score = 0u8;
            for term in &terms {
                let term_score = if title == *term {
                    0
                } else if title.split_whitespace().any(|word| word == term) {
                    1
                } else if title.contains(term) {
                    2
                } else if keywords.split_whitespace().any(|word| word == term) {
                    3
                } else if keywords.contains(term) {
                    4
                } else {
                    return None;
                };
                score = score.saturating_add(term_score);
            }
            Some(SearchMatch { item, score })
        })
        .collect();
    matches.sort_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then_with(|| page_priority(left.item.page).cmp(&page_priority(right.item.page)))
            .then_with(|| left.item.title.cmp(right.item.title))
    });
    matches.truncate(8);
    matches
}

#[cfg(test)]
mod tests {
    use super::{search, search_items, Page};

    #[test]
    fn search_is_case_insensitive_and_prefers_human_titles() {
        let results = search("MICROPHONE");
        assert!(!results.is_empty());
        assert_eq!(results[0].item.page, Page::Shortcuts);
        assert!(results.iter().any(|result| result.item.page == Page::Audio));
        assert!(results
            .iter()
            .all(|result| !result.item.title.contains("scratchpad")));
    }

    #[test]
    fn search_requires_all_terms_and_is_deterministic() {
        let first = search("display profile");
        let second = search("display profile");
        assert_eq!(first, second);
        assert!(first
            .iter()
            .all(|result| result.item.title.contains("Display")));
        assert!(search("not-a-winshort-setting").is_empty());
    }

    #[test]
    fn descriptors_do_not_expose_wire_names() {
        for item in search_items() {
            assert!(!item.title.contains("cycle_"));
            assert!(!item.title.contains("scratchpad"));
            assert!(!item.keywords.contains("scratchpad"));
        }
    }

    #[test]
    fn workspaces_product_copy_uses_special_desktop_terminology() {
        let forbidden = ["special", "workspace"].join(" ");
        let layout = crate::ui::layout::SettingsLayout::build_shell(
            960.0,
            900.0,
            0.0,
            Page::Workspaces,
            "",
            0,
            None,
        );
        let mut copy = vec![Page::Workspaces.description().to_string()];
        copy.extend(
            search_items()
                .iter()
                .filter(|item| item.page == Page::Workspaces)
                .flat_map(|item| [item.title, item.section, item.keywords])
                .map(str::to_string),
        );
        copy.extend(
            layout
                .sections
                .iter()
                .flat_map(|section| [section.title.clone(), section.description.clone()]),
        );
        copy.extend(
            layout
                .elements
                .iter()
                .flat_map(|element| [element.label.clone(), element.description.clone()]),
        );
        assert!(copy
            .iter()
            .all(|text| !text.to_ascii_lowercase().contains(&forbidden)));
        assert!(search("special desktop")
            .iter()
            .any(|result| result.item.page == Page::Workspaces));
    }
}
