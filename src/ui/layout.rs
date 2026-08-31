//! Deterministic DIP layout for the WinShort Control Center.
//!
//! One logical element list drives painting, hit testing, focus traversal, and
//! the UI Automation snapshot. The legacy `build` constructor remains a small
//! compatibility seam for pure layout tests; the window uses `build_shell`.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;

use crate::ui::navigation::{search, Page};
use crate::ui::theme::UiTokens;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    pub fn d2d(self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.x,
            top: self.y,
            right: self.right(),
            bottom: self.bottom(),
        }
    }

    pub fn inset(self, d: f32) -> Self {
        Self::new(self.x + d, self.y + d, self.w - d * 2.0, self.h - d * 2.0)
    }

    pub fn translated_y(self, dy: f32) -> Self {
        Self::new(self.x, self.y + dy, self.w, self.h)
    }

    pub fn intersects(self, other: Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementId {
    Search,
    Nav(Page),
    SearchResult(u8),
    HomeSpeaker,
    HomeCurrentDesktop,
    HomeMicrophone,
    HomePreviousDesktop,
    HomeSpecial,
    HomeDisplayProfile,
    HomeShortcutHealth,
    HomeDiagnostics,
    DisplayProfileCard(u8),
    OnboardingContinue,
    OnboardingOpen,
    StartWithWindows,
    StartHotkeysEnabled,
    MicHotkey,
    OutputHotkey,
    ForegroundHotkey,
    CycleInputHotkey,
    CycleOutputHotkey,
    ForegroundVolumeUpHotkey,
    ForegroundVolumeDownHotkey,
    InputDevice,
    OutputDevice,
    InputAllowlist,
    OutputAllowlist,
    DisplayProfilesEnabled,
    DisplayProfile,
    DisplayProfileHotkey,
    DisplayOutputs,
    DisplayTopology,
    DisplayRoute,
    EditDisplayRoute,
    NewDisplayProfile,
    UpdateDisplayProfile,
    RenameDisplayProfile,
    DuplicateDisplayProfile,
    TestApplyDisplayProfile,
    ApplyDisplayProfile,
    DeleteDisplayProfile,
    KeepDisplayChange,
    UndoDisplayChange,
    DiscardDisplayEdits,
    DebugLogging,
    DiagnosticsStatus,
    InputRole,
    OutputRole,
    DesktopsEnabled,
    WinNumberEnabled,
    DesktopNumberModifier,
    MoveDesktopModifier,
    SilentMoveDesktopModifier,
    PreviousDesktopHotkey,
    AssignScratchpadHotkey,
    ToggleScratchpadHotkey,
    OverlayEnabled,
    OverlayAppearance,
    OverlayExternalChanges,
    OverlayPosition,
    OverlayMonitor,
    OverlayDuration,
    OverlayOpacity,
    OverlayScale,
    OverlayPreview,
    OpenConfigFolder,
    ResetSettings,
    Cancel,
    Save,
}

impl ElementId {
    /// Compatibility order retained for policy tests and stable UIA indices.
    /// The active shell uses `SettingsLayout::focus_order` so navigation and
    /// profile cards are included without inventing a second hit-test model.
    #[allow(dead_code)]
    pub const FOCUS_ORDER: [ElementId; 53] = [
        ElementId::StartWithWindows,
        ElementId::StartHotkeysEnabled,
        ElementId::MicHotkey,
        ElementId::OutputHotkey,
        ElementId::ForegroundHotkey,
        ElementId::CycleInputHotkey,
        ElementId::CycleOutputHotkey,
        ElementId::ForegroundVolumeUpHotkey,
        ElementId::ForegroundVolumeDownHotkey,
        ElementId::InputDevice,
        ElementId::OutputDevice,
        ElementId::InputAllowlist,
        ElementId::OutputAllowlist,
        ElementId::InputRole,
        ElementId::OutputRole,
        ElementId::DisplayProfilesEnabled,
        ElementId::DisplayProfile,
        ElementId::DisplayProfileHotkey,
        ElementId::DisplayOutputs,
        ElementId::DisplayTopology,
        ElementId::DisplayRoute,
        ElementId::EditDisplayRoute,
        ElementId::NewDisplayProfile,
        ElementId::UpdateDisplayProfile,
        ElementId::RenameDisplayProfile,
        ElementId::DuplicateDisplayProfile,
        ElementId::TestApplyDisplayProfile,
        ElementId::ApplyDisplayProfile,
        ElementId::DeleteDisplayProfile,
        ElementId::KeepDisplayChange,
        ElementId::UndoDisplayChange,
        ElementId::DesktopsEnabled,
        ElementId::WinNumberEnabled,
        ElementId::DesktopNumberModifier,
        ElementId::MoveDesktopModifier,
        ElementId::SilentMoveDesktopModifier,
        ElementId::PreviousDesktopHotkey,
        ElementId::AssignScratchpadHotkey,
        ElementId::ToggleScratchpadHotkey,
        ElementId::OverlayEnabled,
        ElementId::OverlayAppearance,
        ElementId::OverlayExternalChanges,
        ElementId::OverlayPosition,
        ElementId::OverlayMonitor,
        ElementId::OverlayDuration,
        ElementId::OverlayOpacity,
        ElementId::OverlayScale,
        ElementId::OverlayPreview,
        ElementId::DebugLogging,
        ElementId::OpenConfigFolder,
        ElementId::ResetSettings,
        ElementId::Cancel,
        ElementId::Save,
    ];

    pub fn is_shell_chrome(self) -> bool {
        matches!(self, Self::Search | Self::Nav(_) | Self::SearchResult(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Toggle,
    Hotkey,
    Value,
    Slider,
    Action,
    ButtonSecondary,
    ButtonPrimary,
    Navigation,
    Search,
    Card,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub id: ElementId,
    pub kind: ElementKind,
    /// Rect in viewport logical coordinates after page scrolling is applied.
    pub rect: Rect,
    pub label: String,
    pub description: String,
    pub scrolls: bool,
}

#[derive(Debug, Clone)]
pub struct SectionLabel {
    pub title: String,
    pub description: String,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct SettingsLayout {
    pub width: f32,
    pub height: f32,
    pub page: Page,
    pub content_clip: Rect,
    pub footer: Rect,
    pub elements: Vec<Element>,
    pub sections: Vec<SectionLabel>,
    pub max_scroll: f32,
    pub scroll: f32,
    pub nav_width: f32,
    pub top_bar: Rect,
    pub search_rect: Rect,
}

impl SettingsLayout {
    /// Compatibility layout used by pure tests that predate the shell.
    #[cfg(test)]
    pub fn build(width: f32, height: f32, requested_scroll: f32) -> Self {
        let mut layout = Self::shell_base(width.max(610.0), height.max(500.0), Page::Home);
        let mut y = layout.content_clip.y + 20.0;
        for (id, kind, label, description) in legacy_rows() {
            layout.elements.push(Element {
                id,
                kind,
                rect: Rect::new(24.0, y, (width - 48.0).max(320.0), 64.0),
                label: label.into(),
                description: description.into(),
                scrolls: true,
            });
            y += 64.0;
        }
        layout.finish_content(y, requested_scroll);
        layout
    }

    pub fn build_shell(
        width: f32,
        height: f32,
        requested_scroll: f32,
        page: Page,
        query: &str,
        profile_count: usize,
        onboarding_step: Option<u8>,
    ) -> Self {
        let mut layout = Self::shell_base(width, height, page);
        layout.add_chrome(query);
        if let Some(step) = onboarding_step {
            add_onboarding(&mut layout, step);
        } else if query.trim().is_empty() {
            add_page(&mut layout, page, profile_count);
        } else {
            add_search_results(&mut layout, query);
        }
        let content_end = layout
            .elements
            .iter()
            .filter(|element| element.scrolls)
            .map(|element| element.rect.bottom())
            .max_by(f32::total_cmp)
            .unwrap_or(layout.content_clip.y);
        layout.finish_content(content_end + 32.0, requested_scroll);
        layout
    }

    fn shell_base(width: f32, height: f32, page: Page) -> Self {
        const TOP_BAR_HEIGHT: f32 = UiTokens::TOP_BAR_HEIGHT;
        const NAV_WIDTH: f32 = UiTokens::NAV_WIDTH;
        const FOOTER_HEIGHT: f32 = UiTokens::FOOTER_HEIGHT;
        let width = width.max(NAV_WIDTH + 360.0);
        let height = height.max(520.0);
        let footer = Rect::new(0.0, height - FOOTER_HEIGHT, width, FOOTER_HEIGHT);
        let top_bar = Rect::new(NAV_WIDTH, 0.0, width - NAV_WIDTH, TOP_BAR_HEIGHT);
        let content_clip = Rect::new(
            NAV_WIDTH,
            TOP_BAR_HEIGHT,
            (width - NAV_WIDTH).max(320.0),
            (height - TOP_BAR_HEIGHT - FOOTER_HEIGHT).max(180.0),
        );
        Self {
            width,
            height,
            page,
            content_clip,
            footer,
            elements: Vec::new(),
            sections: Vec::new(),
            max_scroll: 0.0,
            scroll: 0.0,
            nav_width: NAV_WIDTH,
            top_bar,
            search_rect: Rect::new(NAV_WIDTH + 32.0, 21.0, 360.0, 38.0),
        }
    }

    fn add_chrome(&mut self, query: &str) {
        let search_width = (self.width - self.nav_width - 64.0).clamp(220.0, 420.0);
        self.search_rect.w = search_width;
        self.elements.push(Element {
            id: ElementId::Search,
            kind: ElementKind::Search,
            rect: self.search_rect,
            label: "Find a setting".into(),
            description: "Search WinShort settings by what you want to do".into(),
            scrolls: false,
        });

        let mut y = 98.0;
        for page in Page::PRIMARY {
            self.elements.push(Element {
                id: ElementId::Nav(page),
                kind: ElementKind::Navigation,
                rect: Rect::new(16.0, y, self.nav_width - 32.0, 40.0),
                label: page.label().into(),
                description: page.description().into(),
                scrolls: false,
            });
            y += 44.0;
        }
        y += 18.0;
        for page in Page::SECONDARY {
            self.elements.push(Element {
                id: ElementId::Nav(page),
                kind: ElementKind::Navigation,
                rect: Rect::new(16.0, y, self.nav_width - 32.0, 40.0),
                label: page.label().into(),
                description: page.description().into(),
                scrolls: false,
            });
            y += 44.0;
        }

        if !query.trim().is_empty() {
            self.sections.push(SectionLabel {
                title: "Search results".into(),
                description: "Choose a result to open the right page.".into(),
                y: self.content_clip.y + 26.0,
            });
        }
    }

    fn finish_content(&mut self, content_end: f32, requested_scroll: f32) {
        let content_start = self.content_clip.y;
        let max_scroll = (content_end - content_start - self.content_clip.h).max(0.0);
        self.max_scroll = max_scroll;
        self.scroll = requested_scroll.clamp(0.0, max_scroll);
        let dy = -self.scroll;
        for element in &mut self.elements {
            if element.scrolls {
                element.rect = element.rect.translated_y(dy);
            }
        }
        for section in &mut self.sections {
            section.y += dy;
        }
    }

    pub fn focus_order(&self) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|element| !matches!(element.kind, ElementKind::Card))
            .map(|element| element.id)
            .collect()
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<ElementId> {
        self.elements
            .iter()
            .rev()
            .find(|element| {
                element.rect.contains(x, y)
                    && (!element.scrolls || self.content_clip.contains(x, y))
            })
            .map(|element| element.id)
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }
}
#[cfg(test)]
fn legacy_rows() -> Vec<(ElementId, ElementKind, &'static str, &'static str)> {
    vec![
        (
            ElementId::StartWithWindows,
            ElementKind::Toggle,
            "Start with Windows",
            "Launch WinShort quietly after sign-in",
        ),
        (
            ElementId::StartHotkeysEnabled,
            ElementKind::Toggle,
            "Pause shortcuts",
            "Temporarily stop global shortcuts",
        ),
        (
            ElementId::MicHotkey,
            ElementKind::Hotkey,
            "Mute microphone",
            "Toggle the microphone from any app",
        ),
        (
            ElementId::OutputHotkey,
            ElementKind::Hotkey,
            "Mute speakers",
            "Toggle speaker mute from any app",
        ),
        (
            ElementId::ForegroundHotkey,
            ElementKind::Hotkey,
            "Mute current app",
            "Toggle audio for the app in front",
        ),
        (
            ElementId::CycleInputHotkey,
            ElementKind::Hotkey,
            "Next microphone",
            "Switch to the next selected microphone",
        ),
        (
            ElementId::CycleOutputHotkey,
            ElementKind::Hotkey,
            "Next speaker",
            "Switch to the next selected speaker",
        ),
        (
            ElementId::ForegroundVolumeUpHotkey,
            ElementKind::Hotkey,
            "Current app volume up",
            "Raise the current app by five percent",
        ),
        (
            ElementId::ForegroundVolumeDownHotkey,
            ElementKind::Hotkey,
            "Current app volume down",
            "Lower the current app by five percent",
        ),
        (
            ElementId::InputDevice,
            ElementKind::Value,
            "Microphone",
            "Follow Windows or choose a microphone",
        ),
        (
            ElementId::OutputDevice,
            ElementKind::Value,
            "Speakers",
            "Follow Windows or choose speakers",
        ),
        (
            ElementId::InputAllowlist,
            ElementKind::Value,
            "Devices used by Next microphone",
            "Choose the microphones that can be selected",
        ),
        (
            ElementId::OutputAllowlist,
            ElementKind::Value,
            "Devices used by Next speaker",
            "Choose the speakers that can be selected",
        ),
        (
            ElementId::InputRole,
            ElementKind::Value,
            "Microphone device role",
            "Advanced Windows default behavior",
        ),
        (
            ElementId::OutputRole,
            ElementKind::Value,
            "Speaker device role",
            "Advanced Windows default behavior",
        ),
        (
            ElementId::DisplayProfilesEnabled,
            ElementKind::Toggle,
            "Display profiles",
            "Save monitor arrangements you can switch safely",
        ),
        (
            ElementId::DisplayProfile,
            ElementKind::Value,
            "Display profile",
            "Choose a saved display arrangement",
        ),
        (
            ElementId::DisplayProfileHotkey,
            ElementKind::Hotkey,
            "Display profile shortcut",
            "Activate the selected profile from any app",
        ),
        (
            ElementId::DisplayOutputs,
            ElementKind::Value,
            "Displays in profile",
            "Choose connected display routes",
        ),
        (
            ElementId::DisplayTopology,
            ElementKind::Value,
            "How displays work",
            "Extend or duplicate multiple displays",
        ),
        (
            ElementId::DisplayRoute,
            ElementKind::Value,
            "Display output",
            "Select an output for advanced editing",
        ),
        (
            ElementId::EditDisplayRoute,
            ElementKind::Action,
            "Advanced display output",
            "Edit exact position, mode, refresh, or rotation",
        ),
        (
            ElementId::NewDisplayProfile,
            ElementKind::Action,
            "New display profile",
            "Capture the current Windows arrangement",
        ),
        (
            ElementId::UpdateDisplayProfile,
            ElementKind::Action,
            "Update display profile",
            "Capture the current arrangement into this profile",
        ),
        (
            ElementId::RenameDisplayProfile,
            ElementKind::Action,
            "Rename display profile",
            "Change the name without changing its identity",
        ),
        (
            ElementId::DuplicateDisplayProfile,
            ElementKind::Action,
            "Duplicate display profile",
            "Create a new editable copy",
        ),
        (
            ElementId::TestApplyDisplayProfile,
            ElementKind::ButtonPrimary,
            "Test profile",
            "Try it for 15 seconds before keeping it",
        ),
        (
            ElementId::ApplyDisplayProfile,
            ElementKind::Action,
            "Activate profile",
            "Switch to a previously confirmed arrangement",
        ),
        (
            ElementId::DeleteDisplayProfile,
            ElementKind::ButtonSecondary,
            "Delete display profile",
            "Remove the selected profile and shortcut",
        ),
        (
            ElementId::KeepDisplayChange,
            ElementKind::ButtonPrimary,
            "Keep display setup",
            "Confirm the tested arrangement",
        ),
        (
            ElementId::UndoDisplayChange,
            ElementKind::ButtonSecondary,
            "Revert display setup",
            "Restore the arrangement from before testing",
        ),
        (
            ElementId::DesktopsEnabled,
            ElementKind::Toggle,
            "Workspace shortcuts",
            "Use WinShort desktop actions",
        ),
        (
            ElementId::WinNumberEnabled,
            ElementKind::Toggle,
            "Desktop number shortcuts",
            "Use the configured modifier with 1–9",
        ),
        (
            ElementId::DesktopNumberModifier,
            ElementKind::Value,
            "Desktop shortcut modifier",
            "Modifier family for Desktop 1–9",
        ),
        (
            ElementId::MoveDesktopModifier,
            ElementKind::Value,
            "Move window and follow",
            "Move the current window, then follow it",
        ),
        (
            ElementId::SilentMoveDesktopModifier,
            ElementKind::Value,
            "Move window quietly",
            "Move without leaving the current desktop",
        ),
        (
            ElementId::PreviousDesktopHotkey,
            ElementKind::Hotkey,
            "Previous desktop",
            "Return to the last normal desktop",
        ),
        (
            ElementId::AssignScratchpadHotkey,
            ElementKind::Hotkey,
            "Move window to Special",
            "Put the current window in Special Workspace",
        ),
        (
            ElementId::ToggleScratchpadHotkey,
            ElementKind::Hotkey,
            "Open / close Special",
            "Open Special Workspace or return",
        ),
        (
            ElementId::OverlayEnabled,
            ElementKind::Toggle,
            "Show status overlay",
            "Show compact feedback without stealing focus",
        ),
        (
            ElementId::OverlayAppearance,
            ElementKind::Value,
            "Overlay appearance",
            "Follow Windows, light, or dark",
        ),
        (
            ElementId::OverlayExternalChanges,
            ElementKind::Toggle,
            "Show changes made outside WinShort",
            "Keep the status card in sync with Windows",
        ),
        (
            ElementId::OverlayPosition,
            ElementKind::Value,
            "Overlay position",
            "Choose a corner or the center",
        ),
        (
            ElementId::OverlayMonitor,
            ElementKind::Value,
            "Overlay monitor",
            "Choose where the status card appears",
        ),
        (
            ElementId::OverlayDuration,
            ElementKind::Slider,
            "How long it stays visible",
            "Set the status card duration",
        ),
        (
            ElementId::OverlayOpacity,
            ElementKind::Slider,
            "Overlay opacity",
            "Set the status card transparency",
        ),
        (
            ElementId::OverlayScale,
            ElementKind::Slider,
            "Overlay size",
            "Set the status card size",
        ),
        (
            ElementId::OverlayPreview,
            ElementKind::Action,
            "Preview overlay",
            "See the current draft without saving",
        ),
        (
            ElementId::DebugLogging,
            ElementKind::Toggle,
            "Temporary debug logging",
            "Add troubleshooting detail until restart",
        ),
        (
            ElementId::DiagnosticsStatus,
            ElementKind::Action,
            "Diagnostics and support",
            "Inspect runtime details or create a support bundle",
        ),
        (
            ElementId::OpenConfigFolder,
            ElementKind::Action,
            "Open configuration folder",
            "Open WinShort's local files",
        ),
        (
            ElementId::ResetSettings,
            ElementKind::Action,
            "Reset WinShort",
            "Restore defaults after a second confirmation",
        ),
        (
            ElementId::Cancel,
            ElementKind::ButtonSecondary,
            "Cancel",
            "Discard draft changes",
        ),
        (
            ElementId::Save,
            ElementKind::ButtonPrimary,
            "Save",
            "Apply the complete draft",
        ),
    ]
}

fn add_page(layout: &mut SettingsLayout, page: Page, profile_count: usize) {
    match page {
        Page::Home => add_home(layout, profile_count),
        Page::Shortcuts => add_shortcuts(layout),
        Page::Audio => add_audio(layout),
        Page::Workspaces => add_workspaces(layout),
        Page::Displays => add_displays(layout, profile_count),
        Page::Overlay => add_overlay(layout),
        Page::System => add_system(layout),
        Page::Advanced => add_advanced(layout),
    }
}

fn add_heading(layout: &mut SettingsLayout, title: &str, description: &str, y: &mut f32) {
    layout.sections.push(SectionLabel {
        title: title.into(),
        description: description.into(),
        y: *y,
    });
    *y += 48.0;
}

fn add_row(
    layout: &mut SettingsLayout,
    y: &mut f32,
    id: ElementId,
    kind: ElementKind,
    label: &str,
    description: &str,
) {
    layout.elements.push(Element {
        id,
        kind,
        rect: Rect::new(
            layout.content_clip.x + UiTokens::PAGE_MARGIN,
            *y,
            layout.content_clip.w - UiTokens::PAGE_MARGIN * 2.0,
            UiTokens::ROW_HEIGHT,
        ),
        label: label.into(),
        description: description.into(),
        scrolls: true,
    });
    *y += UiTokens::ROW_HEIGHT + UiTokens::ROW_GAP;
}

fn add_card(
    layout: &mut SettingsLayout,
    y: &mut f32,
    id: ElementId,
    label: &str,
    description: &str,
) {
    layout.elements.push(Element {
        id,
        kind: ElementKind::Action,
        rect: Rect::new(
            layout.content_clip.x + UiTokens::PAGE_MARGIN,
            *y,
            layout.content_clip.w - UiTokens::PAGE_MARGIN * 2.0,
            UiTokens::CARD_HEIGHT,
        ),
        label: label.into(),
        description: description.into(),
        scrolls: true,
    });
    *y += UiTokens::CARD_HEIGHT + UiTokens::CARD_GAP;
}
fn add_card_pair(
    layout: &mut SettingsLayout,
    y: &mut f32,
    left: (ElementId, &str, &str),
    right: (ElementId, &str, &str),
) {
    let gap = UiTokens::CARD_GAP;
    let available = layout.content_clip.w - UiTokens::PAGE_MARGIN * 2.0;
    let card_w = ((available - gap) * 0.5).max(180.0);
    if card_w < 280.0 {
        add_card(layout, y, left.0, left.1, left.2);
        add_card(layout, y, right.0, right.1, right.2);
        return;
    }
    let x = layout.content_clip.x + UiTokens::PAGE_MARGIN;
    for (index, (id, label, description)) in [left, right].into_iter().enumerate() {
        layout.elements.push(Element {
            id,
            kind: ElementKind::Action,
            rect: Rect::new(
                x + index as f32 * (card_w + gap),
                *y,
                card_w,
                UiTokens::CARD_HEIGHT,
            ),
            label: label.into(),
            description: description.into(),
            scrolls: true,
        });
    }
    *y += UiTokens::CARD_HEIGHT + UiTokens::CARD_GAP;
}

fn add_profile_card(layout: &mut SettingsLayout, y: &mut f32, id: u8, index: usize) {
    let gap = UiTokens::CARD_GAP;
    let available = layout.content_clip.w - UiTokens::PAGE_MARGIN * 2.0;
    let card_w = ((available - gap) * 0.5).max(180.0);
    let x = layout.content_clip.x + UiTokens::PAGE_MARGIN + (index % 2) as f32 * (card_w + gap);
    let row_y = *y + (index / 2) as f32 * UiTokens::PROFILE_ROW_STEP;
    layout.elements.push(Element {
        id: ElementId::DisplayProfileCard(id),
        kind: ElementKind::Action,
        rect: Rect::new(x, row_y, card_w, UiTokens::PROFILE_CARD_HEIGHT),
        label: "Display profile".into(),
        description: "Select this saved arrangement".into(),
        scrolls: true,
    });
}

fn add_home(layout: &mut SettingsLayout, _profile_count: usize) {
    let mut y = layout.content_clip.y + 26.0;
    add_heading(
        layout,
        "Welcome back",
        "What WinShort is doing right now.",
        &mut y,
    );
    y += 20.0;
    add_heading(layout, "Audio", "Your current Windows devices.", &mut y);
    add_card_pair(
        layout,
        &mut y,
        (
            ElementId::HomeSpeaker,
            "Speakers",
            "Current playback device",
        ),
        (
            ElementId::HomeMicrophone,
            "Microphone",
            "Current recording device",
        ),
    );
    add_heading(
        layout,
        "Workspace",
        "Keep your windows within reach.",
        &mut y,
    );
    add_card_pair(
        layout,
        &mut y,
        (
            ElementId::HomeCurrentDesktop,
            "Current desktop",
            "Your current normal workspace",
        ),
        (
            ElementId::HomeSpecial,
            "Special Workspace",
            "Keep windows you want to bring back quickly",
        ),
    );
    add_row(
        layout,
        &mut y,
        ElementId::HomePreviousDesktop,
        ElementKind::Action,
        "Previous desktop",
        "Return to the last normal desktop",
    );
    add_heading(
        layout,
        "Display and shortcuts",
        "The two things you reach for most often.",
        &mut y,
    );
    add_card_pair(
        layout,
        &mut y,
        (
            ElementId::HomeDisplayProfile,
            "Display profile",
            "Open saved screen arrangements",
        ),
        (
            ElementId::HomeShortcutHealth,
            "Shortcut health",
            "Review active shortcuts and conflicts",
        ),
    );
    add_row(
        layout,
        &mut y,
        ElementId::HomeDiagnostics,
        ElementKind::Action,
        "Details and diagnostics",
        "See technical status when something needs attention",
    );
}

fn add_shortcuts(layout: &mut SettingsLayout) {
    let mut y = layout.content_clip.y + 26.0;
    add_heading(
        layout,
        "Shortcuts",
        "Choose a shortcut, press the keys you want, then use it immediately.",
        &mut y,
    );
    y += 20.0;
    add_heading(
        layout,
        "Audio",
        "Control the devices and app in front of you.",
        &mut y,
    );
    for (id, label, description) in [
        (
            ElementId::MicHotkey,
            "Mute microphone",
            "Toggle your microphone from any app",
        ),
        (
            ElementId::OutputHotkey,
            "Mute speakers",
            "Toggle speaker mute from any app",
        ),
        (
            ElementId::CycleInputHotkey,
            "Next microphone",
            "Switch to the next selected microphone",
        ),
        (
            ElementId::CycleOutputHotkey,
            "Next speaker",
            "Switch to the next selected speaker",
        ),
        (
            ElementId::ForegroundHotkey,
            "Mute current app",
            "Toggle audio for the app in front",
        ),
        (
            ElementId::ForegroundVolumeUpHotkey,
            "Current app volume up",
            "Raise the current app by five percent",
        ),
        (
            ElementId::ForegroundVolumeDownHotkey,
            "Current app volume down",
            "Lower the current app by five percent",
        ),
    ] {
        add_row(layout, &mut y, id, ElementKind::Hotkey, label, description);
    }
    add_heading(
        layout,
        "Workspaces",
        "Move between desktops with a predictable rhythm.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::WinNumberEnabled,
        ElementKind::Toggle,
        "Desktop 1–9 shortcuts",
        "Switch to numbered desktops with WinShort",
    );
    add_row(
        layout,
        &mut y,
        ElementId::DesktopNumberModifier,
        ElementKind::Value,
        "Desktop shortcut modifier",
        "Choose the modifier used with 1–9",
    );
    for (id, label, description) in [
        (
            ElementId::PreviousDesktopHotkey,
            "Previous desktop",
            "Return to the last normal desktop",
        ),
        (
            ElementId::AssignScratchpadHotkey,
            "Move window to Special",
            "Keep the current window out of the way",
        ),
        (
            ElementId::ToggleScratchpadHotkey,
            "Open / close Special",
            "Open Special Workspace or return",
        ),
    ] {
        add_row(layout, &mut y, id, ElementKind::Hotkey, label, description);
    }
    add_heading(
        layout,
        "Display profiles",
        "Give each saved arrangement a shortcut.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::DisplayProfileHotkey,
        ElementKind::Hotkey,
        "Selected display profile",
        "Activate the selected profile from any app",
    );
}

fn add_audio(layout: &mut SettingsLayout) {
    let mut y = layout.content_clip.y + 26.0;
    add_heading(
        layout,
        "Audio",
        "The devices WinShort uses and the app in front of you.",
        &mut y,
    );
    y += 20.0;
    y += 104.0;
    add_heading(
        layout,
        "Speakers",
        "Choose what Next speaker can use.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::OutputDevice,
        ElementKind::Value,
        "Current speaker",
        "Follow Windows or choose a specific speaker",
    );
    add_row(
        layout,
        &mut y,
        ElementId::OutputAllowlist,
        ElementKind::Value,
        "Devices used by Next speaker",
        "Use all active speakers or choose a set",
    );
    add_heading(
        layout,
        "Microphones",
        "Choose what Next microphone can use.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::InputDevice,
        ElementKind::Value,
        "Current microphone",
        "Follow Windows or choose a specific microphone",
    );
    add_row(
        layout,
        &mut y,
        ElementId::InputAllowlist,
        ElementKind::Value,
        "Devices used by Next microphone",
        "Use all active microphones or choose a set",
    );
    add_heading(
        layout,
        "Current app audio",
        "Control the app currently in front of you.",
        &mut y,
    );
    for (id, label, description) in [
        (
            ElementId::ForegroundHotkey,
            "Mute current app",
            "Toggle all audio sessions owned by the current app",
        ),
        (
            ElementId::ForegroundVolumeUpHotkey,
            "Current app volume up",
            "Raise the current app by five percent",
        ),
        (
            ElementId::ForegroundVolumeDownHotkey,
            "Current app volume down",
            "Lower the current app by five percent",
        ),
    ] {
        add_row(layout, &mut y, id, ElementKind::Hotkey, label, description);
    }
}

fn add_workspaces(layout: &mut SettingsLayout) {
    let mut y = layout.content_clip.y + 26.0;
    add_heading(
        layout,
        "Workspaces",
        "Switch desktops without learning the Shell.",
        &mut y,
    );
    y += 20.0;
    y += 104.0;
    add_heading(
        layout,
        "Desktops",
        "Your numbered desktop shortcuts stay in the familiar Win+1…9 family.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::DesktopsEnabled,
        ElementKind::Toggle,
        "Workspace shortcuts",
        "Use WinShort desktop actions",
    );
    add_row(
        layout,
        &mut y,
        ElementId::WinNumberEnabled,
        ElementKind::Toggle,
        "Desktop number shortcuts",
        "Use the configured modifier with 1–9",
    );
    add_row(
        layout,
        &mut y,
        ElementId::DesktopNumberModifier,
        ElementKind::Value,
        "Desktop shortcut modifier",
        "Modifier family for Desktop 1–9",
    );
    add_row(
        layout,
        &mut y,
        ElementId::MoveDesktopModifier,
        ElementKind::Value,
        "Move window and follow",
        "Move the current window, then follow it",
    );
    add_row(
        layout,
        &mut y,
        ElementId::SilentMoveDesktopModifier,
        ElementKind::Value,
        "Move window quietly",
        "Move without leaving the current desktop",
    );
    add_row(
        layout,
        &mut y,
        ElementId::PreviousDesktopHotkey,
        ElementKind::Hotkey,
        "Previous desktop",
        "Return to the last normal desktop",
    );
    add_heading(
        layout,
        "Special Workspace",
        "Keep windows you want to move out of the way and bring back quickly.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::AssignScratchpadHotkey,
        ElementKind::Hotkey,
        "Move window to Special",
        "Send the current window to Special Workspace",
    );
    add_row(
        layout,
        &mut y,
        ElementId::ToggleScratchpadHotkey,
        ElementKind::Hotkey,
        "Open / close Special",
        "Open Special Workspace or return to your previous desktop",
    );
}

fn add_displays(layout: &mut SettingsLayout, profile_count: usize) {
    let mut y = layout.content_clip.y + 26.0;
    add_heading(
        layout,
        "Displays",
        "Save the way your screens work, then test before you keep it.",
        &mut y,
    );
    y += 20.0;
    y += 104.0;
    add_row(
        layout,
        &mut y,
        ElementId::DisplayProfilesEnabled,
        ElementKind::Toggle,
        "Display profiles",
        "Save monitor arrangements you can switch safely",
    );
    add_heading(
        layout,
        "Your profiles",
        "Choose a profile to edit or activate.",
        &mut y,
    );
    if profile_count == 0 {
        add_card(
            layout,
            &mut y,
            ElementId::NewDisplayProfile,
            "Create your first profile",
            "Capture the arrangement Windows is using now",
        );
    } else {
        let count = profile_count.min(32);
        for index in 0..count {
            add_profile_card(layout, &mut y, index as u8, index);
        }
        y += count.div_ceil(2) as f32 * UiTokens::PROFILE_ROW_STEP;
        add_row(
            layout,
            &mut y,
            ElementId::NewDisplayProfile,
            ElementKind::Action,
            "New profile",
            "Capture the current arrangement and edit it",
        );
        add_row(
            layout,
            &mut y,
            ElementId::UpdateDisplayProfile,
            ElementKind::Action,
            "Capture current arrangement",
            "Replace the selected profile with Windows' current setup",
        );
        add_row(
            layout,
            &mut y,
            ElementId::RenameDisplayProfile,
            ElementKind::Action,
            "Rename profile",
            "Change its name without changing its identity",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DuplicateDisplayProfile,
            ElementKind::Action,
            "Duplicate profile",
            "Create a new editable copy",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DeleteDisplayProfile,
            ElementKind::ButtonSecondary,
            "Delete profile",
            "Remove the selected profile and shortcut",
        );
        add_heading(
            layout,
            "Selected profile",
            "Choose displays and how they work.",
            &mut y,
        );
        add_row(
            layout,
            &mut y,
            ElementId::DisplayProfile,
            ElementKind::Value,
            "Selected profile",
            "Choose a profile to edit",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DisplayOutputs,
            ElementKind::Value,
            "Which displays",
            "Select connected display routes for this profile",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DisplayTopology,
            ElementKind::Value,
            "How they work",
            "Extend or duplicate when more than one display is selected",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DisplayProfileHotkey,
            ElementKind::Hotkey,
            "Profile shortcut",
            "Activate the selected profile from any app",
        );
        add_row(
            layout,
            &mut y,
            ElementId::TestApplyDisplayProfile,
            ElementKind::ButtonPrimary,
            "Test profile",
            "Try it for 15 seconds before keeping it",
        );
        add_row(
            layout,
            &mut y,
            ElementId::ApplyDisplayProfile,
            ElementKind::Action,
            "Activate profile",
            "Switch to this previously confirmed arrangement",
        );
        add_row(
            layout,
            &mut y,
            ElementId::KeepDisplayChange,
            ElementKind::ButtonPrimary,
            "Keep display setup",
            "Confirm the tested arrangement before the timer ends",
        );
        add_row(
            layout,
            &mut y,
            ElementId::UndoDisplayChange,
            ElementKind::ButtonSecondary,
            "Revert display setup",
            "Restore the arrangement from before testing",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DisplayRoute,
            ElementKind::Value,
            "Advanced output",
            "Select an output before opening advanced editing",
        );
        add_row(
            layout,
            &mut y,
            ElementId::EditDisplayRoute,
            ElementKind::Action,
            "Advanced display output",
            "Edit exact position, mode, refresh, or rotation",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DiscardDisplayEdits,
            ElementKind::ButtonSecondary,
            "Discard display edits",
            "Return to the last saved profile",
        );
    }
}

fn add_overlay(layout: &mut SettingsLayout) {
    let mut y = layout.content_clip.y + 26.0;
    add_heading(
        layout,
        "Overlay",
        "A compact visual cue that never interrupts your work.",
        &mut y,
    );
    y += 20.0;
    y += 104.0;
    add_row(
        layout,
        &mut y,
        ElementId::OverlayEnabled,
        ElementKind::Toggle,
        "Show status overlay",
        "Show feedback without stealing focus",
    );
    add_heading(
        layout,
        "Appearance",
        "See the settings in a live preview.",
        &mut y,
    );
    for (id, kind, label, description) in [
        (
            ElementId::OverlayAppearance,
            ElementKind::Value,
            "Appearance",
            "Follow Windows, light, or dark",
        ),
        (
            ElementId::OverlayPosition,
            ElementKind::Value,
            "Position",
            "Choose a corner or the center",
        ),
        (
            ElementId::OverlayMonitor,
            ElementKind::Value,
            "Monitor",
            "Choose where the status card appears",
        ),
        (
            ElementId::OverlayExternalChanges,
            ElementKind::Toggle,
            "Show changes made outside WinShort",
            "Keep the status card in sync with Windows",
        ),
    ] {
        add_row(layout, &mut y, id, kind, label, description);
    }
    add_heading(
        layout,
        "Size and timing",
        "Use familiar ranges instead of numeric internals.",
        &mut y,
    );
    for (id, label, description) in [
        (ElementId::OverlayScale, "Size", "Small, normal, or large"),
        (ElementId::OverlayOpacity, "Opacity", "Low, normal, or high"),
        (
            ElementId::OverlayDuration,
            "Duration",
            "Short, normal, or long",
        ),
    ] {
        add_row(layout, &mut y, id, ElementKind::Slider, label, description);
    }
    add_row(
        layout,
        &mut y,
        ElementId::OverlayPreview,
        ElementKind::Action,
        "Preview overlay",
        "Show the edited overlay without saving",
    );
}

fn add_system(layout: &mut SettingsLayout) {
    let mut y = layout.content_clip.y + 26.0;
    add_heading(
        layout,
        "System",
        "Small choices that shape how WinShort lives on your PC.",
        &mut y,
    );
    y += 20.0;
    add_heading(
        layout,
        "Startup",
        "Decide whether WinShort is ready after sign-in.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::StartWithWindows,
        ElementKind::Toggle,
        "Start WinShort with Windows",
        "Launch quietly after you sign in",
    );
    add_heading(
        layout,
        "Behavior",
        "Pause everything without changing your shortcuts.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::StartHotkeysEnabled,
        ElementKind::Toggle,
        "Pause all shortcuts",
        "Temporarily stop global shortcut actions",
    );
    add_heading(
        layout,
        "Support",
        "Keep technical details available when you need them.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::DiagnosticsStatus,
        ElementKind::Action,
        "Diagnostics and support",
        "Inspect status, copy details, or create a sanitized bundle",
    );
    add_row(
        layout,
        &mut y,
        ElementId::OpenConfigFolder,
        ElementKind::Action,
        "Open configuration folder",
        "Open WinShort's local files",
    );
    add_heading(
        layout,
        "Reset",
        "Reset is deliberate and never happens in the background.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::ResetSettings,
        ElementKind::ButtonSecondary,
        "Reset WinShort",
        "Restore defaults after a second confirmation",
    );
    add_heading(
        layout,
        "About",
        concat!("WinShort · Version ", env!("CARGO_PKG_VERSION")),
        &mut y,
    );
}

fn add_advanced(layout: &mut SettingsLayout) {
    let mut y = layout.content_clip.y + 26.0;
    add_heading(
        layout,
        "Advanced",
        "Technical controls for troubleshooting and fine tuning.",
        &mut y,
    );
    y += 20.0;
    add_heading(layout, "Audio", "Windows default-device behavior.", &mut y);
    add_row(
        layout,
        &mut y,
        ElementId::InputRole,
        ElementKind::Value,
        "Microphone device role",
        "Used only when following the Windows default",
    );
    add_row(
        layout,
        &mut y,
        ElementId::OutputRole,
        ElementKind::Value,
        "Speaker device role",
        "Used only when following the Windows default",
    );
    add_heading(
        layout,
        "Display output editing",
        "Exact values are useful when a profile needs repair.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::DisplayRoute,
        ElementKind::Value,
        "Display output",
        "Select an output from the active display profile",
    );
    add_row(
        layout,
        &mut y,
        ElementId::EditDisplayRoute,
        ElementKind::Action,
        "Edit display output",
        "Position, resolution, refresh, and rotation",
    );
    add_heading(
        layout,
        "Troubleshooting",
        "Temporary diagnostics only; no setting is persisted here.",
        &mut y,
    );
    add_row(
        layout,
        &mut y,
        ElementId::DebugLogging,
        ElementKind::Toggle,
        "Temporary debug logging",
        "Add detail until WinShort restarts",
    );
    add_row(
        layout,
        &mut y,
        ElementId::DiagnosticsStatus,
        ElementKind::Action,
        "Diagnostics and support",
        "Inspect runtime and implementation detail",
    );
}

fn add_search_results(layout: &mut SettingsLayout, query: &str) {
    let mut y = layout.content_clip.y + 26.0;
    let matches = search(query);
    if matches.is_empty() {
        add_heading(
            layout,
            "No matching settings",
            "Try a word such as microphone, desktop, display, or overlay.",
            &mut y,
        );
        return;
    }
    add_heading(
        layout,
        "Find a setting",
        "Choose a result to open the right page.",
        &mut y,
    );
    y += 18.0;
    for (index, result) in matches.iter().enumerate() {
        add_row(
            layout,
            &mut y,
            ElementId::SearchResult(index as u8),
            ElementKind::Action,
            result.item.title,
            &format!("{}  ·  {}", result.item.page.label(), result.item.section),
        );
    }
}

fn add_onboarding(layout: &mut SettingsLayout, step: u8) {
    let mut y = layout.content_clip.y + 34.0;
    if step == 1 {
        add_heading(
            layout,
            "Welcome to WinShort",
            "A few choices make the most useful shortcuts feel right from the start.",
            &mut y,
        );
        y += 30.0;
        add_row(
            layout,
            &mut y,
            ElementId::OutputAllowlist,
            ElementKind::Value,
            "Choose speakers for Next speaker",
            "Use all available speakers or pick a set",
        );
        add_row(
            layout,
            &mut y,
            ElementId::InputAllowlist,
            ElementKind::Value,
            "Choose microphones for Next microphone",
            "Use all available microphones or pick a set",
        );
        add_row(
            layout,
            &mut y,
            ElementId::WinNumberEnabled,
            ElementKind::Toggle,
            "Enable desktop number shortcuts",
            "Use the familiar modifier + 1…9 workflow",
        );
        add_row(
            layout,
            &mut y,
            ElementId::OnboardingContinue,
            ElementKind::ButtonPrimary,
            "Continue",
            "Review your shortcuts",
        );
    } else {
        add_heading(
            layout,
            "You're ready",
            "These are the shortcuts currently configured for your everyday actions.",
            &mut y,
        );
        y += 26.0;
        add_row(
            layout,
            &mut y,
            ElementId::MicHotkey,
            ElementKind::Card,
            "Mute microphone",
            "Your configured shortcut",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DesktopNumberModifier,
            ElementKind::Card,
            "Switch desktop",
            "Your configured desktop shortcut",
        );
        add_row(
            layout,
            &mut y,
            ElementId::ToggleScratchpadHotkey,
            ElementKind::Card,
            "Special Workspace",
            "Your configured Special shortcut",
        );
        add_row(
            layout,
            &mut y,
            ElementId::OnboardingOpen,
            ElementKind::ButtonPrimary,
            "Open WinShort",
            "Go to the Control Center",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{ElementId, ElementKind, SettingsLayout};
    use crate::ui::navigation::Page;

    #[test]
    fn shell_has_primary_navigation_and_search() {
        let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
        assert!(layout.element(ElementId::Search).is_some());
        assert!(layout.element(ElementId::Nav(Page::Audio)).is_some());
        assert!(layout.element(ElementId::HomeSpeaker).is_some());
    }
    #[test]
    fn home_cards_use_dashboard_grid_then_stack_when_narrow() {
        let wide = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
        let wide_speaker = wide
            .element(ElementId::HomeSpeaker)
            .expect("wide speaker card");
        let wide_microphone = wide
            .element(ElementId::HomeMicrophone)
            .expect("wide microphone card");
        assert_eq!(wide_speaker.rect.y, wide_microphone.rect.y);
        assert!(wide_microphone.rect.x > wide_speaker.rect.x);

        let narrow = SettingsLayout::build_shell(760.0, 660.0, 0.0, Page::Home, "", 0, None);
        let narrow_speaker = narrow
            .element(ElementId::HomeSpeaker)
            .expect("narrow speaker card");
        let narrow_microphone = narrow
            .element(ElementId::HomeMicrophone)
            .expect("narrow microphone card");
        assert!(narrow_microphone.rect.y > narrow_speaker.rect.y);
    }

    #[test]
    fn content_hit_testing_excludes_scrolled_elements_outside_viewport() {
        let layout = SettingsLayout::build_shell(960.0, 660.0, 500.0, Page::Shortcuts, "", 0, None);
        assert!(layout
            .elements
            .iter()
            .filter(|element| element.scrolls)
            .all(|element| !element.rect.contains(0.0, 0.0)));
    }

    #[test]
    fn compatibility_layout_keeps_phase_one_hotkeys() {
        let layout = SettingsLayout::build(610.0, 720.0, 0.0);
        for id in [
            ElementId::CycleInputHotkey,
            ElementId::CycleOutputHotkey,
            ElementId::ForegroundVolumeUpHotkey,
            ElementId::ForegroundVolumeDownHotkey,
        ] {
            assert!(layout.element(id).is_some());
        }
        assert!(matches!(
            layout
                .element(ElementId::OverlayDuration)
                .map(|element| element.kind),
            Some(ElementKind::Slider)
        ));
    }
    #[test]
    fn every_page_has_a_heading_and_reachable_navigation() {
        let pages = Page::PRIMARY.into_iter().chain(Page::SECONDARY);
        for page in pages {
            let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, page, "", 0, None);
            let expected = if page == Page::Home {
                "Welcome back"
            } else {
                page.label()
            };
            assert_eq!(
                layout
                    .sections
                    .first()
                    .map(|section| section.title.as_str()),
                Some(expected)
            );
            assert!(layout.element(ElementId::Nav(page)).is_some());
            assert!(!layout.focus_order().is_empty());
        }
    }

    #[test]
    fn display_profiles_use_a_responsive_two_column_grid() {
        let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Displays, "", 3, None);
        let first = layout
            .element(ElementId::DisplayProfileCard(0))
            .expect("first profile card");
        let second = layout
            .element(ElementId::DisplayProfileCard(1))
            .expect("second profile card");
        let third = layout
            .element(ElementId::DisplayProfileCard(2))
            .expect("third profile card");
        assert!(second.rect.x > first.rect.x);
        assert_eq!(third.rect.y, first.rect.y + 136.0);
        assert!(first.rect.w > 200.0);
    }

    #[test]
    fn onboarding_steps_expose_real_choices_and_shortcut_values() {
        let first = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, Some(1));
        assert!(first.element(ElementId::OutputAllowlist).is_some());
        assert!(first.element(ElementId::OnboardingContinue).is_some());
        let ready = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, Some(2));
        assert!(ready.element(ElementId::MicHotkey).is_some());
        assert!(ready.element(ElementId::OnboardingOpen).is_some());
    }
}
