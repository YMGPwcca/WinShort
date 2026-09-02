//! Deterministic DIP layout for the WinShort Control Center.
//!
//! The layout is the single logical model shared by Direct2D painting, pointer
//! hit testing, keyboard focus, and UI Automation. Pages choose structures for
//! the task instead of flattening every configuration value into a row.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;

use crate::ui::navigation::{search, Page};
use crate::ui::presentation::{AllowlistMode, DisplayWizardStep};
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
pub enum HotkeySlot {
    Microphone,
    Output,
    Foreground,
    CycleInput,
    CycleOutput,
    ForegroundVolumeUp,
    ForegroundVolumeDown,
    PreviousDesktop,
    AssignSpecial,
    ToggleSpecial,
    DisplayProfile,
}

impl HotkeySlot {
    pub fn from_capture_id(id: ElementId) -> Option<Self> {
        Some(match id {
            ElementId::MicHotkey => Self::Microphone,
            ElementId::OutputHotkey => Self::Output,
            ElementId::ForegroundHotkey => Self::Foreground,
            ElementId::CycleInputHotkey => Self::CycleInput,
            ElementId::CycleOutputHotkey => Self::CycleOutput,
            ElementId::ForegroundVolumeUpHotkey => Self::ForegroundVolumeUp,
            ElementId::ForegroundVolumeDownHotkey => Self::ForegroundVolumeDown,
            ElementId::PreviousDesktopHotkey => Self::PreviousDesktop,
            ElementId::AssignScratchpadHotkey => Self::AssignSpecial,
            ElementId::ToggleScratchpadHotkey => Self::ToggleSpecial,
            ElementId::DisplayProfileHotkey => Self::DisplayProfile,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementId {
    Search,
    Nav(Page),
    SearchResult(u8),
    WindowClose,
    HomeSpeaker,
    HomeCurrentDesktop,
    HomeMicrophone,
    HomePreviousDesktop,
    HomeSpecial,
    HomeDisplayProfile,
    HomeShortcutHealth,
    HomeDiagnostics,
    DisplayProfileCard(u8),
    DisplayOutputCard(u8),
    DisplayTopologyChoice(u8),
    InputCycleMode(u8),
    OutputCycleMode(u8),
    InputCycleDevice(u8),
    OutputCycleDevice(u8),
    OverlayPositionCell(u8),
    DisplayWizardBack,
    DisplayWizardNext,
    DisplayWizardCancel,
    DisplayWizardSummary,
    EditDisplayProfile,
    OnboardingContinue,
    OnboardingOpen,
    StartWithWindows,
    StartHotkeysEnabled,
    HotkeyCard(HotkeySlot),
    HotkeyEnabled(HotkeySlot),
    HotkeyUnassign(HotkeySlot),
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
    /// The active shell uses `SettingsLayout::focus_order` so new task controls
    /// are included without inventing a second hit-test model.
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
        matches!(
            self,
            Self::Search | Self::Nav(_) | Self::SearchResult(_) | Self::WindowClose
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Toggle,
    Checkbox,
    Choice,
    Hotkey,
    Value,
    Slider,
    Action,
    ButtonSecondary,
    ButtonPrimary,
    ButtonDanger,
    Navigation,
    Search,
    Card,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    WorkspaceNotice,
    PauseNotice,
    AudioCurrentApp,
    OverlayPreview,
    DisplaySafety,
    DisplayWizardSteps,
    DisplayWizardSummary,
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
pub struct VisualRegion {
    pub kind: RegionKind,
    pub rect: Rect,
    pub scrolls: bool,
}

#[derive(Debug, Clone)]
pub struct SectionLabel {
    pub title: String,
    pub description: String,
    pub y: f32,
    pub height: f32,
    pub page_header: bool,
}
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

pub(crate) fn top_chrome_geometry(width: f32, nav_width: f32) -> TopChromeGeometry {
    let row = Rect::new(
        nav_width,
        0.0,
        (width - nav_width).max(1.0),
        UiTokens::TOP_BAR_HEIGHT,
    );
    let control_y = row.y + UiTokens::TITLEBAR_BUTTON_TOP;
    let close = Rect::new(
        width - UiTokens::TITLEBAR_BUTTON_RIGHT - UiTokens::TITLEBAR_BUTTON_WIDTH,
        control_y,
        UiTokens::TITLEBAR_BUTTON_WIDTH,
        UiTokens::TITLEBAR_BUTTON_HEIGHT,
    );
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutContext {
    pub profile_count: usize,
    pub display_output_count: usize,
    pub display_route_count: usize,
    pub display_editor_step: Option<DisplayWizardStep>,
    pub display_profiles_enabled: bool,
    pub display_draft_dirty: bool,
    pub display_rollback_active: bool,
    pub display_keep_available: bool,
    pub display_inventory_unknown: bool,
    pub workspace_enabled: bool,
    pub paused: bool,
    pub input_cycle_mode: AllowlistMode,
    pub output_cycle_mode: AllowlistMode,
    pub input_device_count: usize,
    pub output_device_count: usize,
    pub overlay_preview_aspect: (u32, u32),
}

impl Default for LayoutContext {
    fn default() -> Self {
        Self {
            profile_count: 0,
            display_output_count: 0,
            display_route_count: 0,
            display_editor_step: None,
            display_profiles_enabled: true,
            display_draft_dirty: false,
            display_rollback_active: false,
            display_keep_available: false,
            display_inventory_unknown: false,
            workspace_enabled: true,
            paused: false,
            input_cycle_mode: AllowlistMode::All,
            output_cycle_mode: AllowlistMode::All,
            input_device_count: 0,
            output_device_count: 0,
            overlay_preview_aspect: (16, 9),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsLayout {
    pub width: f32,
    pub height: f32,
    pub page: Page,
    pub content_clip: Rect,
    pub content_column: Rect,
    pub footer: Rect,
    pub elements: Vec<Element>,
    pub regions: Vec<VisualRegion>,
    pub sections: Vec<SectionLabel>,
    pub max_scroll: f32,
    pub scroll: f32,
    pub nav_width: f32,
    pub brand: BrandRowGeometry,
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
                rect: Rect::new(layout.content_column.x, y, layout.content_column.w, 64.0),
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
        let mut context = LayoutContext {
            profile_count,
            ..LayoutContext::default()
        };
        if page == Page::Displays {
            context.display_profiles_enabled = true;
        }
        Self::build_shell_with_context(
            width,
            height,
            requested_scroll,
            page,
            query,
            context,
            onboarding_step,
        )
    }

    pub fn build_shell_with_context(
        width: f32,
        height: f32,
        requested_scroll: f32,
        page: Page,
        query: &str,
        context: LayoutContext,
        onboarding_step: Option<u8>,
    ) -> Self {
        let mut layout = Self::shell_base(width, height, page);
        layout.add_chrome();
        if let Some(step) = onboarding_step {
            add_onboarding(&mut layout, step);
        } else if query.trim().is_empty() {
            add_page(&mut layout, page, &context);
        } else {
            add_search_results(&mut layout, query);
        }
        let content_end = layout
            .elements
            .iter()
            .filter(|element| element.scrolls)
            .map(|element| element.rect.bottom())
            .chain(
                layout
                    .regions
                    .iter()
                    .filter(|region| region.scrolls)
                    .map(|region| region.rect.bottom()),
            )
            .chain(
                layout
                    .sections
                    .iter()
                    .map(|section| section.y + section.height),
            )
            .max_by(f32::total_cmp)
            .unwrap_or(layout.content_column.y);
        layout.finish_content(content_end + 32.0, requested_scroll);
        layout
    }

    fn shell_base(width: f32, height: f32, page: Page) -> Self {
        let width = width.max(UiTokens::NAV_WIDTH + 360.0);
        let height = height.max(520.0);
        let footer = Rect::new(
            0.0,
            height - UiTokens::FOOTER_HEIGHT,
            width,
            UiTokens::FOOTER_HEIGHT,
        );
        let top_bar = Rect::new(
            UiTokens::NAV_WIDTH,
            0.0,
            width - UiTokens::NAV_WIDTH,
            UiTokens::TOP_BAR_HEIGHT,
        );
        let viewport_top = UiTokens::TOP_BAR_HEIGHT + UiTokens::VIEWPORT_TOP_INSET;
        let content_clip = Rect::new(
            UiTokens::NAV_WIDTH,
            viewport_top,
            (width - UiTokens::NAV_WIDTH).max(320.0),
            (height - viewport_top - UiTokens::FOOTER_HEIGHT).max(180.0),
        );
        let max_width = UiTokens::content_max_width(page);
        let column_width = (content_clip.w - UiTokens::PAGE_MARGIN * 2.0)
            .max(260.0)
            .min(max_width);
        let content_column = Rect::new(
            content_clip.x + UiTokens::PAGE_MARGIN,
            content_clip.y,
            column_width,
            content_clip.h,
        );
        Self {
            width,
            height,
            page,
            content_clip,
            content_column,
            footer,
            elements: Vec::new(),
            regions: Vec::new(),
            sections: Vec::new(),
            max_scroll: 0.0,
            scroll: 0.0,
            nav_width: UiTokens::NAV_WIDTH,
            brand: brand_row_geometry(UiTokens::NAV_WIDTH),
            top_bar,
            search_rect: Rect::new(
                UiTokens::NAV_WIDTH + 32.0,
                (UiTokens::TOP_BAR_HEIGHT - 32.0) * 0.5,
                360.0,
                32.0,
            ),
        }
    }

    fn add_chrome(&mut self) {
        let chrome = top_chrome_geometry(self.width, self.nav_width);
        self.top_bar = chrome.row;
        self.search_rect = chrome.search;
        self.elements.push(Element {
            id: ElementId::Search,
            kind: ElementKind::Search,
            rect: self.search_rect,
            label: "Find a setting".into(),
            description: "Search WinShort settings by what you want to do".into(),
            scrolls: false,
        });

        let mut y = UiTokens::NAV_FIRST_ITEM_TOP;
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

        self.elements.push(Element {
            id: ElementId::WindowClose,
            kind: ElementKind::ButtonSecondary,
            rect: chrome.close,
            label: "Close".into(),
            description: "Hide the Control Center and keep WinShort running".into(),
            scrolls: false,
        });
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
        for region in &mut self.regions {
            if region.scrolls {
                region.rect = region.rect.translated_y(dy);
            }
        }
        for section in &mut self.sections {
            section.y += dy;
        }
    }
    pub fn focus_order(&self) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|element| !matches!(element.kind, ElementKind::Card | ElementKind::Info))
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
            "Launch at sign-in",
        ),
        (
            ElementId::StartHotkeysEnabled,
            ElementKind::Toggle,
            "Shortcuts enabled",
            "Enable global actions",
        ),
        (
            ElementId::MicHotkey,
            ElementKind::Hotkey,
            "Mute microphone",
            "Toggle mute",
        ),
        (
            ElementId::OutputHotkey,
            ElementKind::Hotkey,
            "Mute speakers",
            "Toggle mute",
        ),
        (
            ElementId::ForegroundHotkey,
            ElementKind::Hotkey,
            "Mute current app",
            "Toggle app audio",
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
            "Input device",
        ),
        (
            ElementId::OutputDevice,
            ElementKind::Value,
            "Speakers",
            "Output device",
        ),
        (
            ElementId::InputAllowlist,
            ElementKind::Value,
            "Microphone cycling",
            "Input devices",
        ),
        (
            ElementId::OutputAllowlist,
            ElementKind::Value,
            "Speaker cycling",
            "Output devices",
        ),
        (
            ElementId::InputRole,
            ElementKind::Value,
            "Input role",
            "Windows role",
        ),
        (
            ElementId::OutputRole,
            ElementKind::Value,
            "Output role",
            "Windows role",
        ),
        (
            ElementId::DisplayProfilesEnabled,
            ElementKind::Toggle,
            "Display profiles",
            "Save arrangements",
        ),
        (
            ElementId::DisplayProfile,
            ElementKind::Value,
            "Display profile",
            "Selected profile",
        ),
        (
            ElementId::DisplayProfileHotkey,
            ElementKind::Hotkey,
            "Profile shortcut",
            "Activate profile",
        ),
        (
            ElementId::DisplayOutputs,
            ElementKind::Value,
            "Which displays",
            "Selected routes",
        ),
        (
            ElementId::DisplayTopology,
            ElementKind::Value,
            "How they work",
            "Topology",
        ),
        (
            ElementId::DisplayRoute,
            ElementKind::Value,
            "Display output",
            "Route",
        ),
        (
            ElementId::EditDisplayRoute,
            ElementKind::Action,
            "Edit output",
            "Advanced values",
        ),
        (
            ElementId::NewDisplayProfile,
            ElementKind::ButtonPrimary,
            "New profile",
            "Create",
        ),
        (
            ElementId::UpdateDisplayProfile,
            ElementKind::Action,
            "Update profile",
            "Capture",
        ),
        (
            ElementId::RenameDisplayProfile,
            ElementKind::Action,
            "Rename profile",
            "Change name",
        ),
        (
            ElementId::DuplicateDisplayProfile,
            ElementKind::Action,
            "Duplicate profile",
            "Copy",
        ),
        (
            ElementId::TestApplyDisplayProfile,
            ElementKind::ButtonPrimary,
            "Test profile",
            "Try safely",
        ),
        (
            ElementId::ApplyDisplayProfile,
            ElementKind::Action,
            "Activate profile",
            "Apply",
        ),
        (
            ElementId::DeleteDisplayProfile,
            ElementKind::ButtonDanger,
            "Delete profile",
            "Remove",
        ),
        (
            ElementId::KeepDisplayChange,
            ElementKind::ButtonPrimary,
            "Keep display setup",
            "Confirm",
        ),
        (
            ElementId::UndoDisplayChange,
            ElementKind::ButtonSecondary,
            "Revert display setup",
            "Restore",
        ),
        (
            ElementId::DesktopsEnabled,
            ElementKind::Toggle,
            "Workspace shortcuts",
            "Enable workspace actions",
        ),
        (
            ElementId::WinNumberEnabled,
            ElementKind::Toggle,
            "Desktop numbers",
            "Switch desktops",
        ),
        (
            ElementId::DesktopNumberModifier,
            ElementKind::Value,
            "Desktop modifier",
            "Modifier",
        ),
        (
            ElementId::MoveDesktopModifier,
            ElementKind::Value,
            "Move and follow",
            "Modifier",
        ),
        (
            ElementId::SilentMoveDesktopModifier,
            ElementKind::Value,
            "Move quietly",
            "Modifier",
        ),
        (
            ElementId::PreviousDesktopHotkey,
            ElementKind::Hotkey,
            "Previous desktop",
            "Go back",
        ),
        (
            ElementId::AssignScratchpadHotkey,
            ElementKind::Hotkey,
            "Move window to Special",
            "Move",
        ),
        (
            ElementId::ToggleScratchpadHotkey,
            ElementKind::Hotkey,
            "Open Special",
            "Open",
        ),
        (
            ElementId::OverlayEnabled,
            ElementKind::Toggle,
            "Show overlay",
            "Status card",
        ),
        (
            ElementId::OverlayAppearance,
            ElementKind::Value,
            "Appearance",
            "System or theme",
        ),
        (
            ElementId::OverlayExternalChanges,
            ElementKind::Toggle,
            "Windows audio changes",
            "Show changes",
        ),
        (
            ElementId::OverlayPosition,
            ElementKind::Value,
            "Position",
            "Overlay location",
        ),
        (
            ElementId::OverlayMonitor,
            ElementKind::Value,
            "Monitor",
            "Overlay monitor",
        ),
        (
            ElementId::OverlayDuration,
            ElementKind::Slider,
            "Duration",
            "How long it stays",
        ),
        (
            ElementId::OverlayOpacity,
            ElementKind::Slider,
            "Opacity",
            "Transparency",
        ),
        (
            ElementId::OverlayScale,
            ElementKind::Slider,
            "Size",
            "Overlay size",
        ),
        (
            ElementId::OverlayPreview,
            ElementKind::Action,
            "Preview",
            "Show overlay",
        ),
        (
            ElementId::DebugLogging,
            ElementKind::Toggle,
            "Debug logging",
            "Temporary",
        ),
        (
            ElementId::OpenConfigFolder,
            ElementKind::Action,
            "Open folder",
            "Configuration",
        ),
        (
            ElementId::ResetSettings,
            ElementKind::ButtonDanger,
            "Reset",
            "Restore defaults",
        ),
        (
            ElementId::Cancel,
            ElementKind::ButtonSecondary,
            "Cancel",
            "Cancel the current action",
        ),
        (
            ElementId::Save,
            ElementKind::ButtonPrimary,
            "Save",
            "Save the current changes",
        ),
    ]
}

fn add_page(layout: &mut SettingsLayout, page: Page, context: &LayoutContext) {
    match page {
        Page::Home => add_home(layout),
        Page::Shortcuts => add_shortcuts(layout, context),
        Page::Audio => add_audio(layout, context),
        Page::Workspaces => add_workspaces(layout, context),
        Page::Displays => add_displays(layout, context),
        Page::Overlay => add_overlay(layout, context),
        Page::System => add_system(layout),
        Page::Advanced => add_advanced(layout),
    }
}

fn add_heading(layout: &mut SettingsLayout, title: &str, description: &str, y: &mut f32) {
    let page_header = layout.sections.is_empty();
    if !page_header {
        *y += UiTokens::SECTION_GAP;
    }
    let height = if page_header {
        UiTokens::PAGE_HEADER_HEIGHT
    } else {
        UiTokens::SECTION_HEADER_HEIGHT
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
fn add_section_content_gap(y: &mut f32) {
    *y += UiTokens::SECTION_CONTENT_GAP;
}

fn add_region(layout: &mut SettingsLayout, kind: RegionKind, y: &mut f32, height: f32) -> Rect {
    let rect = Rect::new(layout.content_column.x, *y, layout.content_column.w, height);
    layout.regions.push(VisualRegion {
        kind,
        rect,
        scrolls: true,
    });
    *y += height + UiTokens::ROW_GAP;
    rect
}

fn add_element(
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

fn add_row(
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
fn add_card(
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

fn add_card_pair(
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

fn add_hotkey_grid(layout: &mut SettingsLayout, y: &mut f32, items: &[(ElementId, &str, &str)]) {
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

fn add_button_grid(
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

fn add_profile_card(
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
}

fn add_audio_mode_group(
    layout: &mut SettingsLayout,
    y: &mut f32,
    kind: crate::ui::presentation::AudioDeviceKind,
    mode: AllowlistMode,
    device_count: usize,
) {
    let row_gap = UiTokens::ROW_GAP;
    let mode_height = 36.0;
    let mode_step = mode_height + row_gap;
    let start = *y;
    for (index, candidate) in [
        AllowlistMode::All,
        AllowlistMode::Selected,
        AllowlistMode::Disabled,
    ]
    .into_iter()
    .enumerate()
    {
        let id = match kind {
            crate::ui::presentation::AudioDeviceKind::Microphone => {
                ElementId::InputCycleMode(index as u8)
            }
            crate::ui::presentation::AudioDeviceKind::Speaker => {
                ElementId::OutputCycleMode(index as u8)
            }
        };
        let selected = candidate == mode;
        add_element(
            layout,
            y,
            id,
            ElementKind::Choice,
            crate::ui::presentation::allowlist_mode_label(candidate, kind),
            if selected {
                "Selected"
            } else {
                "Choose this cycling mode"
            },
            Rect::new(
                layout.content_column.x,
                start + index as f32 * mode_step,
                layout.content_column.w,
                mode_height,
            ),
        );
    }
    *y = start + 3.0 * mode_step;
    if mode == AllowlistMode::Selected && device_count > 0 {
        let column_gap = UiTokens::CARD_COLUMN_GAP;
        let columns = if layout.content_column.w >= 660.0 {
            2
        } else {
            1
        };
        let card_w = if columns == 1 {
            layout.content_column.w
        } else {
            (layout.content_column.w - column_gap) * 0.5
        };
        let device_height = 42.0;
        let device_step = device_height + row_gap;
        let device_start = *y;
        for index in 0..device_count.min(32) {
            let row = index / columns;
            let column = index % columns;
            let id = match kind {
                crate::ui::presentation::AudioDeviceKind::Microphone => {
                    ElementId::InputCycleDevice(index as u8)
                }
                crate::ui::presentation::AudioDeviceKind::Speaker => {
                    ElementId::OutputCycleDevice(index as u8)
                }
            };
            add_element(
                layout,
                y,
                id,
                ElementKind::Checkbox,
                format!("{} option", kind.noun()),
                "Use this device when cycling",
                Rect::new(
                    layout.content_column.x + column as f32 * (card_w + column_gap),
                    device_start + row as f32 * device_step,
                    card_w,
                    device_height,
                ),
            );
        }
        let rows = device_count.min(32).div_ceil(columns);
        *y = device_start + rows as f32 * device_step;
    }
}

fn add_home(layout: &mut SettingsLayout) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Welcome back",
        "What WinShort is doing right now.",
        &mut y,
    );
    add_heading(layout, "Audio", "Your current Windows devices.", &mut y);
    add_section_content_gap(&mut y);
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
    add_section_content_gap(&mut y);
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
    let action_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::HomePreviousDesktop,
        ElementKind::ButtonSecondary,
        "Previous desktop",
        "Return to the last normal desktop",
        Rect::new(
            layout.content_column.x,
            action_y,
            layout.content_column.w,
            UiTokens::ROW_HEIGHT,
        ),
    );
    add_heading(
        layout,
        "Display and shortcuts",
        "The two things you reach for most often.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_card_pair(
        layout,
        &mut y,
        (
            ElementId::HomeDisplayProfile,
            "Display",
            "Saved screen arrangements",
        ),
        (
            ElementId::HomeShortcutHealth,
            "Shortcuts",
            "Active actions and conflicts",
        ),
    );
    let status_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::HomeDiagnostics,
        ElementKind::ButtonSecondary,
        "System status",
        "Details and diagnostics when something needs attention",
        Rect::new(
            layout.content_column.x,
            status_y,
            layout.content_column.w,
            UiTokens::CARD_HEIGHT,
        ),
    );
}

fn add_shortcuts(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Shortcuts",
        "Record the actions you use most. Each keycap is ready to change.",
        &mut y,
    );
    if context.paused {
        add_section_content_gap(&mut y);
        add_region(layout, RegionKind::PauseNotice, &mut y, 68.0);
    }
    add_heading(
        layout,
        "Audio",
        "Control devices and the app in front of you.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_hotkey_grid(
        layout,
        &mut y,
        &[
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
        ],
    );
    add_heading(
        layout,
        "Workspaces",
        "Desktop actions are available when Workspace shortcuts are on.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::DesktopsEnabled,
        ElementKind::Toggle,
        "Workspace shortcuts",
        if context.workspace_enabled {
            "Desktop and Special actions are enabled"
        } else {
            "Turn this on to enable desktop and Special actions"
        },
    );
    if context.workspace_enabled {
        add_hotkey_grid(
            layout,
            &mut y,
            &[
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
            ],
        );
    } else {
        add_region(layout, RegionKind::WorkspaceNotice, &mut y, 70.0);
    }
    add_heading(
        layout,
        "Numbered desktops",
        "Use one modifier with the familiar 1–9 family.",
        &mut y,
    );
    if context.workspace_enabled {
        add_section_content_gap(&mut y);
        add_row(
            layout,
            &mut y,
            ElementId::WinNumberEnabled,
            ElementKind::Toggle,
            "Desktop number shortcuts",
            "Switch to numbered desktops",
        );
        add_row(
            layout,
            &mut y,
            ElementId::DesktopNumberModifier,
            ElementKind::Value,
            "Desktop shortcut modifier",
            "Shown as Win + 1–9 or another modifier family",
        );
    }
    add_heading(
        layout,
        "Display profiles",
        "Give the selected arrangement a shortcut.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_hotkey_grid(
        layout,
        &mut y,
        &[(
            (ElementId::DisplayProfileHotkey),
            "Selected display profile",
            "Activate the selected profile from any app",
        )],
    );
}

fn add_audio(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Audio",
        "Choose Windows default devices and how Next speaker or microphone cycles.",
        &mut y,
    );
    add_heading(
        layout,
        "Speakers",
        "Choose the Windows playback device and cycling mode.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::OutputDevice,
        ElementKind::Value,
        "Windows playback device",
        "Choose the system default speaker",
    );
    add_audio_mode_group(
        layout,
        &mut y,
        crate::ui::presentation::AudioDeviceKind::Speaker,
        context.output_cycle_mode,
        context.output_device_count,
    );
    add_heading(
        layout,
        "Microphones",
        "Choose the Windows recording device and cycling mode.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::InputDevice,
        ElementKind::Value,
        "Windows recording device",
        "Choose the system default microphone",
    );
    add_audio_mode_group(
        layout,
        &mut y,
        crate::ui::presentation::AudioDeviceKind::Microphone,
        context.input_cycle_mode,
        context.input_device_count,
    );
    add_heading(
        layout,
        "Current app audio",
        "WinShort itself is not a target for these actions.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_region(layout, RegionKind::AudioCurrentApp, &mut y, 64.0);
    add_hotkey_grid(
        layout,
        &mut y,
        &[
            (
                ElementId::ForegroundHotkey,
                "Mute current app",
                "Toggle all sessions owned by another app",
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
        ],
    );
}
fn add_workspaces(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Workspaces",
        "Configure desktop and Special Workspace shortcuts.",
        &mut y,
    );
    add_heading(
        layout,
        "Workspace shortcuts",
        "One master switch controls desktop and Special actions.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::DesktopsEnabled,
        ElementKind::Toggle,
        "Workspace shortcuts",
        if context.workspace_enabled {
            "Desktop and Special actions are enabled"
        } else {
            "Turn this on to use the controls below"
        },
    );
    if !context.workspace_enabled {
        add_region(layout, RegionKind::WorkspaceNotice, &mut y, 76.0);
        add_heading(
            layout,
            "Special Workspace",
            "A dedicated place for windows you want nearby but out of the way.",
            &mut y,
        );
        add_hotkey_grid(
            layout,
            &mut y,
            &[
                (
                    ElementId::AssignScratchpadHotkey,
                    "Move window to Special",
                    "Send the current window to Special Workspace",
                ),
                (
                    ElementId::ToggleScratchpadHotkey,
                    "Open / close Special",
                    "Open Special Workspace or return",
                ),
            ],
        );
        return;
    }
    add_heading(
        layout,
        "Numbered desktops",
        "The modifier applies to the nine normal desktop numbers.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::WinNumberEnabled,
        ElementKind::Toggle,
        "Desktop number shortcuts",
        "Switch to desktops 1–9",
    );
    add_row(
        layout,
        &mut y,
        ElementId::DesktopNumberModifier,
        ElementKind::Value,
        "Desktop shortcut modifier",
        "Use this modifier with 1–9",
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
    add_hotkey_grid(
        layout,
        &mut y,
        &[(
            ElementId::PreviousDesktopHotkey,
            "Previous desktop",
            "Return to the last normal desktop",
        )],
    );
    add_heading(
        layout,
        "Special Workspace",
        "A dedicated place for windows you want nearby but out of the way.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_hotkey_grid(
        layout,
        &mut y,
        &[
            (
                ElementId::AssignScratchpadHotkey,
                "Move window to Special",
                "Send the current window to Special Workspace",
            ),
            (
                ElementId::ToggleScratchpadHotkey,
                "Open / close Special",
                "Open Special Workspace or return",
            ),
        ],
    );
}

fn add_display_wizard(layout: &mut SettingsLayout, context: &LayoutContext) {
    let step = context.display_editor_step.expect("display wizard step");
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Display profile editor",
        "Build an arrangement, then test it before keeping it.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_region(layout, RegionKind::DisplayWizardSteps, &mut y, 58.0);
    match step {
        DisplayWizardStep::Displays => {
            add_heading(
                layout,
                "Which screens",
                if context.display_inventory_unknown {
                    "Windows display information is unavailable. These are saved screens; connection status is unknown."
                } else {
                    "Choose which screens this profile should use."
                },
                &mut y,
            );
            add_section_content_gap(&mut y);
            if context.display_output_count == 0 {
                add_region(layout, RegionKind::DisplayWizardSummary, &mut y, 104.0);
            } else {
                let column_gap = UiTokens::CARD_COLUMN_GAP;
                let columns = if layout.content_column.w >= 660.0 {
                    2
                } else {
                    1
                };
                let card_w = if columns == 1 {
                    layout.content_column.w
                } else {
                    (layout.content_column.w - column_gap) * 0.5
                };
                let start = y;
                for index in 0..context.display_output_count.min(32) {
                    let row = index / columns;
                    let column = index % columns;
                    add_element(
                        layout,
                        &mut y,
                        ElementId::DisplayOutputCard(index as u8),
                        ElementKind::Checkbox,
                        "Screen",
                        "Select this screen",
                        Rect::new(
                            layout.content_column.x + column as f32 * (card_w + column_gap),
                            start + row as f32 * (66.0 + UiTokens::ROW_GAP),
                            card_w,
                            66.0,
                        ),
                    );
                }
                let rows = context.display_output_count.min(32).div_ceil(columns);
                y = start + rows as f32 * (66.0 + UiTokens::ROW_GAP);
            }
        }
        DisplayWizardStep::Arrangement => {
            add_heading(
                layout,
                "How they work",
                "Choose a simple arrangement for the selected screens.",
                &mut y,
            );
            add_section_content_gap(&mut y);
            if context.display_route_count <= 1 {
                add_region(layout, RegionKind::DisplayWizardSummary, &mut y, 122.0);
            } else {
                for (index, (label, description)) in [
                    ("Extend", "Show selected screens as separate desktops"),
                    ("Duplicate", "Show the same picture on each selected screen"),
                ]
                .into_iter()
                .enumerate()
                {
                    let choice_y = y;
                    add_element(
                        layout,
                        &mut y,
                        ElementId::DisplayTopologyChoice(index as u8),
                        ElementKind::Choice,
                        label,
                        description,
                        Rect::new(
                            layout.content_column.x,
                            choice_y,
                            layout.content_column.w,
                            170.0,
                        ),
                    );
                }
            }
        }
        DisplayWizardStep::NameAndShortcut => {
            add_heading(
                layout,
                "Name & shortcut",
                "Give this arrangement a name people can recognize.",
                &mut y,
            );
            add_section_content_gap(&mut y);
            add_row(
                layout,
                &mut y,
                ElementId::RenameDisplayProfile,
                ElementKind::Action,
                "Profile name",
                "Choose a short name for this arrangement",
            );
            add_row(
                layout,
                &mut y,
                ElementId::DisplayProfileHotkey,
                ElementKind::Hotkey,
                "Shortcut (optional)",
                "Activate this profile from any app",
            );
        }
        DisplayWizardStep::Review => {
            add_heading(
                layout,
                "Review",
                "Check the summary, then test the display setup safely.",
                &mut y,
            );
            add_section_content_gap(&mut y);
            let summary_rect = add_region(layout, RegionKind::DisplayWizardSummary, &mut y, 178.0);
            add_element(
                layout,
                &mut y,
                ElementId::DisplayWizardSummary,
                ElementKind::Info,
                "Display profile review",
                "Profile, screens, arrangement, shortcut, and readiness",
                summary_rect,
            );
            if context.display_draft_dirty {
                let discard_y = y;
                add_element(
                    layout,
                    &mut y,
                    ElementId::DiscardDisplayEdits,
                    ElementKind::ButtonSecondary,
                    "Discard changes",
                    "Return to the last saved profile",
                    Rect::new(layout.content_column.x, discard_y, 150.0, 36.0),
                );
            }
            let test_y = y;
            add_element(
                layout,
                &mut y,
                ElementId::TestApplyDisplayProfile,
                ElementKind::ButtonPrimary,
                "Test profile",
                if context.display_inventory_unknown {
                    "Status not previewed here; Test revalidates before applying"
                } else {
                    "Try it for 15 seconds before keeping it"
                },
                Rect::new(
                    layout.content_column.x,
                    test_y,
                    layout.content_column.w,
                    UiTokens::ROW_HEIGHT,
                ),
            );
        }
    }
    let nav_y = y;
    if step.previous().is_some() {
        add_element(
            layout,
            &mut y,
            ElementId::DisplayWizardBack,
            ElementKind::ButtonSecondary,
            "Back",
            "Return to the previous step",
            Rect::new(layout.content_column.x, nav_y, 106.0, 36.0),
        );
    }
    if step.next().is_some() {
        add_element(
            layout,
            &mut y,
            ElementId::DisplayWizardNext,
            ElementKind::ButtonPrimary,
            "Next",
            "Continue to the next step",
            Rect::new(layout.content_column.right() - 106.0, nav_y, 106.0, 36.0),
        );
    } else {
        add_element(
            layout,
            &mut y,
            ElementId::DisplayWizardCancel,
            ElementKind::ButtonSecondary,
            "Cancel",
            "Discard this display draft and return to Display Profiles",
            Rect::new(layout.content_column.right() - 106.0, nav_y, 106.0, 36.0),
        );
    }
}

fn add_displays(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    if context.display_profiles_enabled
        && !context.display_rollback_active
        && context.display_editor_step.is_some()
    {
        add_display_wizard(layout, context);
        return;
    }
    add_heading(
        layout,
        "Displays",
        "Save the way your screens work, then test before you keep it.",
        &mut y,
    );
    if !context.display_profiles_enabled {
        add_section_content_gap(&mut y);
        add_row(
            layout,
            &mut y,
            ElementId::DisplayProfilesEnabled,
            ElementKind::Toggle,
            "Display profiles",
            "Turn this on to save monitor arrangements",
        );
        add_region(layout, RegionKind::DisplaySafety, &mut y, 76.0);
        return;
    }
    if context.display_rollback_active {
        add_section_content_gap(&mut y);
        add_region(layout, RegionKind::DisplaySafety, &mut y, 94.0);
        let safety_action_y = y;
        if context.display_keep_available {
            add_element(
                layout,
                &mut y,
                ElementId::KeepDisplayChange,
                ElementKind::ButtonPrimary,
                "Keep this setup",
                "Confirm the tested display arrangement",
                Rect::new(layout.content_column.x, safety_action_y, 150.0, 36.0),
            );
        }
        add_element(
            layout,
            &mut y,
            ElementId::UndoDisplayChange,
            ElementKind::ButtonDanger,
            "Revert",
            "Restore the previous display arrangement",
            Rect::new(
                layout.content_column.x + 160.0,
                safety_action_y,
                120.0,
                36.0,
            ),
        );
        return;
    }
    add_heading(
        layout,
        "Display profiles",
        "Select a profile to activate it or open its editor.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    if context.profile_count == 0 {
        add_card(
            layout,
            &mut y,
            ElementId::NewDisplayProfile,
            "New from current",
            "Create a profile from the current Windows arrangement",
        );
        return;
    }
    let count = context.profile_count.min(32);
    let columns = if layout.content_column.w >= 660.0 {
        2
    } else {
        1
    };
    let profile_start = y;
    for index in 0..count {
        add_profile_card(
            layout,
            &mut y,
            index as u8,
            index,
            if count == 1 { 1 } else { columns },
        );
    }
    let rows = count.div_ceil(columns);
    y = profile_start
        + rows as f32 * UiTokens::PROFILE_CARD_HEIGHT
        + rows.saturating_sub(1) as f32 * UiTokens::ROW_GAP;
    add_heading(
        layout,
        "Manage selected profile",
        "Actions stay with the profile they change.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_button_grid(
        layout,
        &mut y,
        &[
            (
                ElementId::NewDisplayProfile,
                ElementKind::ButtonPrimary,
                "New from current",
                "Create a profile from the current arrangement",
            ),
            (
                ElementId::EditDisplayProfile,
                ElementKind::ButtonSecondary,
                "Edit profile",
                "Open the guided editor",
            ),
            (
                ElementId::UpdateDisplayProfile,
                ElementKind::ButtonSecondary,
                "Replace from current",
                "Update this profile from Windows' arrangement",
            ),
            (
                ElementId::DuplicateDisplayProfile,
                ElementKind::ButtonSecondary,
                "Duplicate",
                "Create a separate editable copy",
            ),
            (
                ElementId::DeleteDisplayProfile,
                ElementKind::ButtonDanger,
                "Delete",
                "Remove the selected profile",
            ),
        ],
    );
}

const PREVIEW_SIDE_INSET: f32 = 18.0;
const PREVIEW_CANVAS_MAX_WIDTH: f32 = 720.0;
const PREVIEW_CANVAS_MIN_HEIGHT: f32 = 118.0;
const PREVIEW_CANVAS_MAX_HEIGHT: f32 = 220.0;
const PREVIEW_TITLE_HEIGHT: f32 = 44.0;
const PREVIEW_BOTTOM_INSET: f32 = 18.0;
const OVERLAY_PLACEMENT_GAP: f32 = 16.0;
const OVERLAY_PLACEMENT_CONTROLS_WIDTH: f32 = 360.0;
const OVERLAY_PLACEMENT_HEADER_HEIGHT: f32 = 44.0;
const OVERLAY_POSITION_GRID_STEP: f32 = 44.0;
const OVERLAY_POSITION_CELL_HEIGHT: f32 = 36.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OverlayPlacementGeometry {
    pub region: Rect,
    pub preview: Rect,
    pub controls: Rect,
}

pub(crate) fn overlay_position_controls_height() -> f32 {
    OVERLAY_PLACEMENT_HEADER_HEIGHT + OVERLAY_POSITION_GRID_STEP * 3.0
}

pub(crate) fn overlay_position_grid_rect(controls: Rect, index: usize) -> Rect {
    let column = index % 3;
    let row = index / 3;
    let cell_width = (controls.w - 16.0).max(3.0) / 3.0;
    Rect::new(
        controls.x + column as f32 * (cell_width + 8.0),
        controls.y + OVERLAY_PLACEMENT_HEADER_HEIGHT + row as f32 * OVERLAY_POSITION_GRID_STEP,
        cell_width,
        OVERLAY_POSITION_CELL_HEIGHT,
    )
}
pub(crate) fn overlay_status_row_rects(row: Rect) -> (Rect, Rect) {
    let gap = UiTokens::CARD_COLUMN_GAP;
    let half = ((row.w - gap).max(2.0)) * 0.5;
    let left = Rect::new(row.x, row.y, half, row.h);
    let right = Rect::new(left.right() + gap, row.y, half, row.h);
    (left, right)
}

pub(crate) fn overlay_placement_geometry(
    origin: Rect,
    aspect: (u32, u32),
) -> OverlayPlacementGeometry {
    let controls_width =
        OVERLAY_PLACEMENT_CONTROLS_WIDTH.min((origin.w - OVERLAY_PLACEMENT_GAP - 240.0).max(1.0));
    let controls = Rect::new(
        origin.right() - controls_width,
        origin.y,
        controls_width,
        overlay_position_controls_height(),
    );
    let preview_width = (controls.x - OVERLAY_PLACEMENT_GAP - origin.x).max(1.0);
    let preview_height = overlay_preview_region_height(preview_width, aspect);
    let region_height = preview_height.max(controls.h);
    OverlayPlacementGeometry {
        region: Rect::new(origin.x, origin.y, origin.w, region_height),
        preview: Rect::new(origin.x, origin.y, preview_width, region_height),
        controls,
    }
}

fn add_overlay_position_elements(layout: &mut SettingsLayout, controls: Rect) {
    let mut end = controls.y;
    for index in 0..9 {
        add_element(
            layout,
            &mut end,
            ElementId::OverlayPositionCell(index),
            ElementKind::Choice,
            "Overlay position",
            "Choose this position",
            overlay_position_grid_rect(controls, index as usize),
        );
    }
}

pub(crate) fn overlay_preview_canvas_size(available_width: f32, aspect: (u32, u32)) -> (f32, f32) {
    let ratio = if aspect.0 == 0 || aspect.1 == 0 {
        16.0 / 9.0
    } else {
        aspect.0 as f32 / aspect.1 as f32
    };
    let available_width = available_width.max(1.0);
    let mut width = available_width.min(PREVIEW_CANVAS_MAX_WIDTH);
    let mut height = width / ratio;
    if height > PREVIEW_CANVAS_MAX_HEIGHT {
        height = PREVIEW_CANVAS_MAX_HEIGHT;
        width = height * ratio;
    }
    if height < PREVIEW_CANVAS_MIN_HEIGHT {
        height = PREVIEW_CANVAS_MIN_HEIGHT;
        width = height * ratio;
        if width > available_width {
            width = available_width;
            height = width / ratio;
        }
    }
    (width, height)
}

pub(crate) fn overlay_preview_region_height(content_width: f32, aspect: (u32, u32)) -> f32 {
    let (_, height) =
        overlay_preview_canvas_size((content_width - PREVIEW_SIDE_INSET * 2.0).max(1.0), aspect);
    PREVIEW_TITLE_HEIGHT + height + PREVIEW_BOTTOM_INSET
}
pub(crate) fn overlay_preview_canvas_rect(preview: Rect, aspect: (u32, u32)) -> Rect {
    let available_width = (preview.w - PREVIEW_SIDE_INSET * 2.0).max(1.0);
    let (width, height) = overlay_preview_canvas_size(available_width, aspect);
    Rect::new(
        preview.x + PREVIEW_SIDE_INSET + (available_width - width) * 0.5,
        preview.y + PREVIEW_TITLE_HEIGHT,
        width,
        height,
    )
}
fn add_overlay(layout: &mut SettingsLayout, context: &LayoutContext) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Overlay",
        "A compact visual cue that never interrupts your work.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    let placement = overlay_placement_geometry(
        Rect::new(layout.content_column.x, y, layout.content_column.w, 0.0),
        context.overlay_preview_aspect,
    );
    add_region(
        layout,
        RegionKind::OverlayPreview,
        &mut y,
        placement.region.h,
    );
    add_overlay_position_elements(layout, placement.controls);
    let status_row = Rect::new(
        layout.content_column.x,
        y,
        layout.content_column.w,
        UiTokens::ROW_HEIGHT,
    );
    let (enabled_rect, monitor_rect) = overlay_status_row_rects(status_row);
    add_element(
        layout,
        &mut y,
        ElementId::OverlayEnabled,
        ElementKind::Toggle,
        "Show status overlay",
        "Show feedback without stealing focus",
        enabled_rect,
    );
    add_element(
        layout,
        &mut y,
        ElementId::OverlayMonitor,
        ElementKind::Value,
        "Monitor",
        "Choose where the status card appears",
        monitor_rect,
    );
    add_row(
        layout,
        &mut y,
        ElementId::OverlayAppearance,
        ElementKind::Value,
        "Overlay style",
        "Choose the status card appearance",
    );
    add_heading(
        layout,
        "Size and timing",
        "Adjust the compact status card with familiar ranges.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    for (id, label, description) in [
        (ElementId::OverlayScale, "Size", "Small, normal, or large"),
        (ElementId::OverlayOpacity, "Opacity", "Low, normal, or high"),
        (
            ElementId::OverlayDuration,
            "Duration",
            "Short, normal, or long",
        ),
    ] {
        let slider_y = y;
        add_element(
            layout,
            &mut y,
            id,
            ElementKind::Slider,
            label,
            description,
            Rect::new(
                layout.content_column.x,
                slider_y,
                layout.content_column.w,
                56.0,
            ),
        );
    }
    let preview_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::OverlayPreview,
        ElementKind::ButtonPrimary,
        "Show on screen",
        "Show the edited overlay without saving it",
        Rect::new(layout.content_column.x, preview_y, 150.0, 36.0),
    );
}

fn add_system(layout: &mut SettingsLayout) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "System",
        "Small choices that shape how WinShort lives on your PC.",
        &mut y,
    );
    add_heading(
        layout,
        "Startup",
        "Decide whether WinShort is ready after sign-in.",
        &mut y,
    );
    add_section_content_gap(&mut y);
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
    add_section_content_gap(&mut y);
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
    add_section_content_gap(&mut y);
    let diagnostics_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::DiagnosticsStatus,
        ElementKind::ButtonSecondary,
        "Diagnostics and support",
        "Inspect status, copy details, or create a sanitized bundle",
        Rect::new(
            layout.content_column.x,
            diagnostics_y,
            layout.content_column.w,
            52.0,
        ),
    );
    let folder_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::OpenConfigFolder,
        ElementKind::ButtonSecondary,
        "Open configuration folder",
        "Open WinShort's local files",
        Rect::new(
            layout.content_column.x,
            folder_y,
            layout.content_column.w,
            52.0,
        ),
    );
    add_heading(
        layout,
        "Reset",
        "Reset is destructive and always asks twice.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    let reset_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::ResetSettings,
        ElementKind::ButtonDanger,
        "Reset WinShort",
        "Restore defaults after a second confirmation",
        Rect::new(
            layout.content_column.x,
            reset_y,
            layout.content_column.w,
            52.0,
        ),
    );
    add_heading(
        layout,
        "About",
        concat!("WinShort · Version ", env!("CARGO_PKG_VERSION")),
        &mut y,
    );
}

fn add_advanced(layout: &mut SettingsLayout) {
    let mut y = layout.content_column.y + 28.0;
    add_heading(
        layout,
        "Advanced",
        "Technical controls for troubleshooting and fine tuning.",
        &mut y,
    );
    add_heading(layout, "Audio", "Windows default-device behavior.", &mut y);
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::InputRole,
        ElementKind::Value,
        "Microphone device role",
        "Role used for mute and status tracking",
    );
    add_row(
        layout,
        &mut y,
        ElementId::OutputRole,
        ElementKind::Value,
        "Speaker device role",
        "Role used for mute and status tracking",
    );
    add_heading(
        layout,
        "Display output editing",
        "Exact values are useful when a profile needs repair.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::DisplayRoute,
        ElementKind::Value,
        "Display output",
        "Select an output from the active display profile",
    );
    let edit_output_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::EditDisplayRoute,
        ElementKind::ButtonSecondary,
        "Edit display output",
        "Position, resolution, refresh, and rotation",
        Rect::new(
            layout.content_column.x,
            edit_output_y,
            layout.content_column.w,
            52.0,
        ),
    );
    add_heading(
        layout,
        "Troubleshooting",
        "Temporary diagnostics only; no setting is persisted here.",
        &mut y,
    );
    add_section_content_gap(&mut y);
    add_row(
        layout,
        &mut y,
        ElementId::DebugLogging,
        ElementKind::Toggle,
        "Temporary debug logging",
        "Add detail until WinShort restarts",
    );
    let diagnostics_y = y;
    add_element(
        layout,
        &mut y,
        ElementId::DiagnosticsStatus,
        ElementKind::ButtonSecondary,
        "Diagnostics and support",
        "Inspect runtime and implementation detail",
        Rect::new(
            layout.content_column.x,
            diagnostics_y,
            layout.content_column.w,
            52.0,
        ),
    );
}

fn add_search_results(layout: &mut SettingsLayout, query: &str) {
    let mut y = layout.content_column.y + 28.0;
    let matches = search(query);
    if matches.is_empty() {
        add_heading(
            layout,
            "No matching settings",
            "Try microphone, desktop, display, or overlay.",
            &mut y,
        );
        add_section_content_gap(&mut y);
        add_region(layout, RegionKind::WorkspaceNotice, &mut y, 76.0);
        return;
    }
    add_heading(
        layout,
        "Find a setting",
        "Choose a result to open the right page.",
        &mut y,
    );
    add_section_content_gap(&mut y);
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
    let mut y = layout.content_column.y + 34.0;
    if step == 1 {
        add_heading(
            layout,
            "Set up WinShort",
            "Choose the defaults you want to use every day.",
            &mut y,
        );
        add_section_content_gap(&mut y);
        add_row(
            layout,
            &mut y,
            ElementId::OutputAllowlist,
            ElementKind::Value,
            "Speaker cycling",
            "Use all speakers or choose a set",
        );
        add_row(
            layout,
            &mut y,
            ElementId::InputAllowlist,
            ElementKind::Value,
            "Microphone cycling",
            "Use all microphones or choose a set",
        );
        add_row(
            layout,
            &mut y,
            ElementId::WinNumberEnabled,
            ElementKind::Toggle,
            "Desktop number shortcuts",
            "Use the familiar 1–9 family",
        );
        let continue_y = y;
        add_element(
            layout,
            &mut y,
            ElementId::OnboardingContinue,
            ElementKind::ButtonPrimary,
            "Continue",
            "Choose shortcuts next",
            Rect::new(layout.content_column.x, continue_y, 120.0, 36.0),
        );
    } else {
        add_heading(
            layout,
            "Your shortcuts are ready",
            "You can change every choice later.",
            &mut y,
        );
        add_section_content_gap(&mut y);
        add_hotkey_grid(
            layout,
            &mut y,
            &[
                (
                    ElementId::MicHotkey,
                    "Mute microphone",
                    "Toggle microphone mute",
                ),
                (
                    ElementId::OutputHotkey,
                    "Mute speakers",
                    "Toggle speaker mute",
                ),
                (
                    ElementId::PreviousDesktopHotkey,
                    "Previous desktop",
                    "Return to the last desktop",
                ),
            ],
        );
        let open_y = y;
        add_element(
            layout,
            &mut y,
            ElementId::OnboardingOpen,
            ElementKind::ButtonPrimary,
            "Open WinShort",
            "Go to the Control Center",
            Rect::new(layout.content_column.x, open_y, 140.0, 36.0),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        brand_row_geometry, overlay_placement_geometry, overlay_position_grid_rect,
        overlay_preview_canvas_rect, overlay_preview_canvas_size, overlay_status_row_rects,
        top_chrome_geometry, ElementId, ElementKind, HotkeySlot, RegionKind, SettingsLayout,
    };
    use crate::ui::navigation::Page;
    use crate::ui::presentation::{AllowlistMode, DisplayWizardStep};

    #[test]
    fn brand_row_centers_icon_and_text_from_shared_row_geometry() {
        let brand = brand_row_geometry(216.0);
        let row_center = brand.row.y + brand.row.h * 0.5;
        assert_eq!(brand.icon.y + brand.icon.h * 0.5, row_center);
        assert_eq!(brand.text.y + brand.text.h * 0.5, row_center);
        assert_eq!(
            brand.icon.y + brand.icon.h * 0.5,
            brand.text.y + brand.text.h * 0.5
        );
        assert_eq!(
            brand.text.x,
            brand.icon.right() + super::UiTokens::BRAND_TEXT_GAP
        );
        assert!(brand.row.h > super::UiTokens::TOP_BAR_HEIGHT);
        let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
        assert_eq!(
            layout
                .element(ElementId::Nav(Page::Home))
                .expect("home navigation")
                .rect
                .y,
            super::UiTokens::NAV_FIRST_ITEM_TOP
        );
    }

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

        let previous = wide
            .element(ElementId::HomePreviousDesktop)
            .expect("previous desktop action");
        assert_eq!(previous.rect.w, wide.content_column.w);
        assert_eq!(previous.rect.h, super::UiTokens::ROW_HEIGHT);
        let status = wide
            .element(ElementId::HomeDiagnostics)
            .expect("home status surface");
        assert_eq!(status.rect.w, wide.content_column.w);
        assert_eq!(status.rect.h, super::UiTokens::CARD_HEIGHT);
    }

    #[test]
    fn compact_shell_keeps_search_clear_of_window_edge() {
        let layout = SettingsLayout::build_shell(760.0, 660.0, 0.0, Page::System, "", 0, None);
        let right_gap = layout.top_bar.right() - layout.search_rect.right();
        assert!(right_gap >= 64.0);
    }

    #[test]
    fn ordinary_content_has_a_page_aware_maximum_width() {
        let layout = SettingsLayout::build_shell(1920.0, 1080.0, 0.0, Page::System, "", 0, None);
        assert!(layout.content_column.w <= 860.0);
        assert!(layout.content_column.right() < layout.content_clip.right());
        let displays =
            SettingsLayout::build_shell(1920.0, 1080.0, 0.0, Page::Displays, "", 3, None);
        assert!(displays.content_column.w > layout.content_column.w);
    }

    #[test]
    fn content_hit_testing_excludes_scrolled_elements_outside_viewport() {
        let layout = SettingsLayout::build_shell(960.0, 660.0, 500.0, Page::Shortcuts, "", 0, None);
        assert!(layout
            .elements
            .iter()
            .filter(|element| element.scrolls)
            .all(|element| { !element.rect.contains(0.0, 0.0) }));
    }

    #[test]
    fn partially_visible_content_intersects_the_viewport() {
        let viewport = super::Rect::new(0.0, 100.0, 400.0, 300.0);

        assert!(viewport.intersects(super::Rect::new(20.0, 80.0, 120.0, 40.0)));
        assert!(viewport.intersects(super::Rect::new(20.0, 390.0, 120.0, 40.0)));
        assert!(!viewport.intersects(super::Rect::new(20.0, 20.0, 120.0, 40.0)));
        assert!(!viewport.intersects(super::Rect::new(20.0, 400.0, 120.0, 40.0)));
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
    fn display_profiles_use_a_responsive_grid() {
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
        assert!(third.rect.y > first.rect.y);
        assert!(first.rect.w > 200.0);

        let narrow = SettingsLayout::build_shell(760.0, 660.0, 0.0, Page::Displays, "", 2, None);
        let narrow_first = narrow
            .element(ElementId::DisplayProfileCard(0))
            .expect("narrow first profile card");
        let narrow_second = narrow
            .element(ElementId::DisplayProfileCard(1))
            .expect("narrow second profile card");
        assert_eq!(narrow_first.rect.x, narrow_second.rect.x);
        assert!(narrow_second.rect.y > narrow_first.rect.y);
    }

    #[test]
    fn display_overview_keeps_expert_route_editing_out_of_normal_flow() {
        let layout = SettingsLayout::build_shell(1200.0, 900.0, 0.0, Page::Displays, "", 1, None);
        assert!(layout.element(ElementId::DisplayProfileCard(0)).is_some());
        assert!(layout.element(ElementId::EditDisplayRoute).is_none());
        assert!(layout.element(ElementId::DisplayRoute).is_none());
        assert!(layout.element(ElementId::EditDisplayProfile).is_some());
    }

    #[test]
    fn display_recovery_layout_has_only_meaningful_keep_or_revert_actions() {
        let context = super::LayoutContext {
            profile_count: 1,
            display_rollback_active: true,
            display_keep_available: true,
            ..Default::default()
        };
        let layout = SettingsLayout::build_shell_with_context(
            1200.0,
            900.0,
            0.0,
            Page::Displays,
            "",
            context,
            None,
        );
        assert!(layout.element(ElementId::KeepDisplayChange).is_some());
        assert!(layout.element(ElementId::UndoDisplayChange).is_some());
        assert!(layout.element(ElementId::TestApplyDisplayProfile).is_none());
        assert!(layout.element(ElementId::DeleteDisplayProfile).is_none());
    }

    #[test]
    fn one_profile_uses_the_available_card_width() {
        let layout = SettingsLayout::build_shell(1920.0, 1080.0, 0.0, Page::Displays, "", 1, None);
        let card = layout
            .element(ElementId::DisplayProfileCard(0))
            .expect("profile card");
        assert_eq!(card.rect.w, layout.content_column.w);
    }
    #[test]
    fn audio_modes_are_mutually_exclusive_and_selected_devices_are_progressive() {
        let context = super::LayoutContext {
            input_cycle_mode: AllowlistMode::Selected,
            input_device_count: 3,
            output_cycle_mode: AllowlistMode::All,
            output_device_count: 2,
            ..Default::default()
        };
        let layout = SettingsLayout::build_shell_with_context(
            1200.0,
            900.0,
            0.0,
            Page::Audio,
            "",
            context,
            None,
        );
        assert!(layout.element(ElementId::InputCycleMode(0)).is_some());
        assert!(layout.element(ElementId::InputCycleMode(1)).is_some());
        assert!(layout.element(ElementId::InputCycleMode(2)).is_some());
        assert!(layout.element(ElementId::InputCycleDevice(2)).is_some());
        assert!(layout.element(ElementId::OutputCycleDevice(0)).is_none());
    }
    #[test]
    fn display_editor_reflows_one_step_without_form_cemetery() {
        for (step, has_routes, has_topology, has_name) in [
            (DisplayWizardStep::Displays, true, false, false),
            (DisplayWizardStep::Arrangement, false, true, false),
            (DisplayWizardStep::NameAndShortcut, false, false, true),
        ] {
            let context = super::LayoutContext {
                display_editor_step: Some(step),
                display_output_count: 2,
                display_route_count: 2,
                ..Default::default()
            };
            let layout = SettingsLayout::build_shell_with_context(
                1200.0,
                900.0,
                0.0,
                Page::Displays,
                "",
                context,
                None,
            );
            assert_eq!(
                layout.element(ElementId::DisplayOutputCard(0)).is_some(),
                has_routes
            );
            assert_eq!(
                layout
                    .element(ElementId::DisplayTopologyChoice(0))
                    .is_some(),
                has_topology
            );
            assert_eq!(
                layout.element(ElementId::RenameDisplayProfile).is_some(),
                has_name
            );
        }
    }

    #[test]
    fn workspaces_settings_has_no_desktop_switcher_surface() {
        let layout = SettingsLayout::build_shell_with_context(
            1200.0,
            900.0,
            0.0,
            Page::Workspaces,
            "",
            super::LayoutContext::default(),
            None,
        );
        assert!(layout.regions.is_empty());
        assert!(layout.element(ElementId::WinNumberEnabled).is_some());
        assert!(layout.element(ElementId::DesktopNumberModifier).is_some());
        assert!(layout.element(ElementId::PreviousDesktopHotkey).is_some());
        assert!(layout
            .sections
            .iter()
            .all(|section| section.title != "Normal desktops"));
    }

    #[test]
    fn sibling_cards_and_rows_use_named_row_gap() {
        let audio = SettingsLayout::build_shell(960.0, 900.0, 0.0, Page::Audio, "", 0, None);
        let speakers = audio
            .sections
            .iter()
            .find(|section| section.title == "Speakers")
            .expect("speakers heading");
        let output = audio.element(ElementId::OutputDevice).expect("output row");
        assert_eq!(
            output.rect.y - (speakers.y + speakers.height),
            super::UiTokens::SECTION_CONTENT_GAP
        );
        let status = audio
            .regions
            .iter()
            .find(|region| region.kind == RegionKind::AudioCurrentApp)
            .expect("current app status");
        let mute = audio
            .element(ElementId::HotkeyCard(HotkeySlot::Foreground))
            .expect("mute current app card");
        let volume_up = audio
            .element(ElementId::HotkeyCard(HotkeySlot::ForegroundVolumeUp))
            .expect("current app volume card");
        assert_eq!(mute.rect.y - status.rect.bottom(), super::UiTokens::ROW_GAP);
        assert_eq!(
            volume_up.rect.y - mute.rect.bottom(),
            super::UiTokens::ROW_GAP
        );

        let home = SettingsLayout::build_shell(960.0, 900.0, 0.0, Page::Home, "", 0, None);
        let desktop = home
            .element(ElementId::HomeCurrentDesktop)
            .expect("current desktop card");
        let special = home.element(ElementId::HomeSpecial).expect("special card");
        let previous = home
            .element(ElementId::HomePreviousDesktop)
            .expect("previous desktop row");
        assert_eq!(desktop.rect.y, special.rect.y);
        assert_eq!(
            previous.rect.y - desktop.rect.bottom(),
            super::UiTokens::ROW_GAP
        );
    }

    #[test]
    fn special_workspace_uses_a_heading_and_consistent_shortcut_cards() {
        let layout = SettingsLayout::build_shell_with_context(
            960.0,
            900.0,
            0.0,
            Page::Workspaces,
            "",
            super::LayoutContext::default(),
            None,
        );
        let special = layout
            .sections
            .iter()
            .find(|section| section.title == "Special Workspace")
            .expect("special workspace heading");
        assert_eq!(
            special.description,
            "A dedicated place for windows you want nearby but out of the way."
        );
        let move_card = layout
            .element(ElementId::HotkeyCard(HotkeySlot::AssignSpecial))
            .expect("move card");
        let toggle_card = layout
            .element(ElementId::HotkeyCard(HotkeySlot::ToggleSpecial))
            .expect("toggle card");
        assert_eq!(
            toggle_card.rect.y - move_card.rect.bottom(),
            super::UiTokens::ROW_GAP
        );
        assert_eq!(toggle_card.rect.x, move_card.rect.x);
        assert_eq!(toggle_card.rect.w, move_card.rect.w);
        assert!(layout.regions.is_empty());
    }
    #[test]
    fn special_shortcut_card_gap_is_stable_at_multiple_scroll_positions() {
        for requested_scroll in [0.0, 120.0, 480.0] {
            let layout = SettingsLayout::build_shell_with_context(
                960.0,
                660.0,
                requested_scroll,
                Page::Workspaces,
                "",
                super::LayoutContext::default(),
                None,
            );
            let move_card = layout
                .element(ElementId::HotkeyCard(HotkeySlot::AssignSpecial))
                .expect("move card");
            let toggle_card = layout
                .element(ElementId::HotkeyCard(HotkeySlot::ToggleSpecial))
                .expect("toggle card");
            assert_eq!(
                toggle_card.rect.y - move_card.rect.bottom(),
                super::UiTokens::ROW_GAP
            );
            assert_eq!(toggle_card.rect.w, move_card.rect.w);
        }
    }

    #[test]
    fn workspace_off_still_exposes_disabled_special_shortcuts() {
        let layout = SettingsLayout::build_shell_with_context(
            960.0,
            900.0,
            0.0,
            Page::Workspaces,
            "",
            super::LayoutContext {
                workspace_enabled: false,
                ..Default::default()
            },
            None,
        );
        assert!(layout
            .regions
            .iter()
            .any(|region| region.kind == RegionKind::WorkspaceNotice));
        assert!(layout
            .sections
            .iter()
            .any(|section| section.title == "Special Workspace"));
        assert!(layout
            .element(ElementId::HotkeyCard(HotkeySlot::AssignSpecial))
            .is_some());
        assert!(layout
            .element(ElementId::HotkeyCard(HotkeySlot::ToggleSpecial))
            .is_some());
    }

    #[test]
    fn display_editor_has_one_step_at_a_time() {
        let context = super::LayoutContext {
            profile_count: 1,
            display_editor_step: Some(DisplayWizardStep::Displays),
            display_output_count: 2,
            display_route_count: 2,
            ..Default::default()
        };
        let layout = SettingsLayout::build_shell_with_context(
            1200.0,
            900.0,
            0.0,
            Page::Displays,
            "",
            context,
            None,
        );
        assert!(layout.element(ElementId::DisplayOutputCard(0)).is_some());
        assert!(layout
            .element(ElementId::DisplayTopologyChoice(0))
            .is_none());
        assert!(layout.element(ElementId::DisplayWizardNext).is_some());
        assert_eq!(
            layout
                .sections
                .first()
                .map(|section| section.title.as_str()),
            Some("Display profile editor")
        );
        assert_eq!(
            layout
                .sections
                .iter()
                .filter(|section| section.title == "Displays")
                .count(),
            0
        );
    }

    #[test]
    fn editor_header_is_the_only_page_header() {
        for step in DisplayWizardStep::ALL {
            let layout = SettingsLayout::build_shell_with_context(
                1200.0,
                900.0,
                0.0,
                Page::Displays,
                "",
                super::LayoutContext {
                    display_editor_step: Some(step),
                    display_output_count: 2,
                    display_route_count: 2,
                    ..Default::default()
                },
                None,
            );
            assert_eq!(
                layout
                    .sections
                    .first()
                    .map(|section| section.title.as_str()),
                Some("Display profile editor")
            );
            assert!(layout
                .sections
                .iter()
                .skip(1)
                .all(|section| !section.page_header));
        }
    }

    #[test]
    fn scrolled_content_hit_testing_stops_at_top_bar_boundary() {
        let layout = SettingsLayout::build_shell(960.0, 660.0, 500.0, Page::Audio, "", 0, None);
        assert_eq!(
            layout.content_clip.y,
            layout.top_bar.bottom() + super::UiTokens::VIEWPORT_TOP_INSET
        );
        assert_eq!(
            layout.hit_test(layout.content_column.x + 8.0, layout.content_clip.y - 1.0),
            None
        );
    }

    #[test]
    fn review_summary_is_accessible_without_joining_keyboard_order() {
        let layout = SettingsLayout::build_shell_with_context(
            1200.0,
            900.0,
            0.0,
            Page::Displays,
            "",
            super::LayoutContext {
                display_editor_step: Some(DisplayWizardStep::Review),
                display_route_count: 2,
                display_draft_dirty: true,
                ..Default::default()
            },
            None,
        );
        assert_eq!(
            layout
                .element(ElementId::DisplayWizardSummary)
                .map(|element| element.kind),
            Some(ElementKind::Info)
        );
        assert!(!layout
            .focus_order()
            .contains(&ElementId::DisplayWizardSummary));
    }

    #[test]
    fn managed_shortcut_cards_include_record_state_and_unassign_controls() {
        let layout = SettingsLayout::build_shell(960.0, 900.0, 0.0, Page::Shortcuts, "", 0, None);
        for (slot, capture) in [
            (HotkeySlot::Microphone, ElementId::MicHotkey),
            (HotkeySlot::CycleOutput, ElementId::CycleOutputHotkey),
            (HotkeySlot::DisplayProfile, ElementId::DisplayProfileHotkey),
        ] {
            let card = layout
                .element(ElementId::HotkeyCard(slot))
                .expect("shortcut card");
            let keycap = layout.element(capture).expect("shortcut keycap");
            let enabled = layout
                .element(ElementId::HotkeyEnabled(slot))
                .expect("shortcut state button");
            let unassign = layout
                .element(ElementId::HotkeyUnassign(slot))
                .expect("shortcut unassign button");
            assert!(card.rect.contains(keycap.rect.x, keycap.rect.y));
            assert!(card.rect.contains(enabled.rect.x, enabled.rect.y));
            assert!(card
                .rect
                .contains(unassign.rect.right(), unassign.rect.bottom()));
            assert!(layout.focus_order().contains(&capture));
            assert!(layout.focus_order().contains(&enabled.id));
            assert!(layout.focus_order().contains(&unassign.id));
        }
    }

    #[test]
    fn overlay_layout_has_no_duplicate_windows_audio_toggle() {
        let layout = SettingsLayout::build_shell(960.0, 900.0, 0.0, Page::Overlay, "", 0, None);
        assert!(layout.element(ElementId::OverlayExternalChanges).is_none());
        assert!(layout.element(ElementId::OverlayPosition).is_none());
        assert!(layout.element(ElementId::OverlayAppearance).is_some());

        let system = SettingsLayout::build_shell(960.0, 900.0, 0.0, Page::System, "", 0, None);
        assert!(system.element(ElementId::OverlayAppearance).is_none());
    }

    #[test]
    fn fixed_overlay_placement_is_always_side_by_side() {
        for width in [960.0, 1200.0, 1920.0] {
            let layout = SettingsLayout::build_shell_with_context(
                width,
                900.0,
                0.0,
                Page::Overlay,
                "",
                super::LayoutContext {
                    overlay_preview_aspect: (16, 9),
                    ..Default::default()
                },
                None,
            );
            let region = layout
                .regions
                .iter()
                .find(|region| region.kind == RegionKind::OverlayPreview)
                .expect("overlay placement");
            let geometry = overlay_placement_geometry(
                super::Rect::new(
                    layout.content_column.x,
                    region.rect.y,
                    layout.content_column.w,
                    0.0,
                ),
                (16, 9),
            );
            assert_eq!(region.rect, geometry.region);
            assert!(geometry.preview.w > 0.0);
            assert_eq!(
                geometry.preview.right() + super::OVERLAY_PLACEMENT_GAP,
                geometry.controls.x
            );
            assert_eq!(geometry.controls.right(), layout.content_column.right());
            assert_eq!(
                layout
                    .element(ElementId::OverlayPositionCell(0))
                    .expect("position cell")
                    .rect,
                overlay_position_grid_rect(geometry.controls, 0)
            );
            let enabled = layout
                .element(ElementId::OverlayEnabled)
                .expect("status toggle")
                .rect;
            let monitor = layout
                .element(ElementId::OverlayMonitor)
                .expect("monitor selector")
                .rect;
            let (expected_enabled, expected_monitor) = overlay_status_row_rects(super::Rect::new(
                layout.content_column.x,
                enabled.y,
                layout.content_column.w,
                enabled.h,
            ));
            assert_eq!(enabled, expected_enabled);
            assert_eq!(monitor, expected_monitor);
            assert_eq!(enabled.w, monitor.w);
            assert_eq!(
                monitor.x,
                enabled.right() + super::UiTokens::CARD_COLUMN_GAP
            );
            assert_eq!(
                layout
                    .sections
                    .iter()
                    .filter(|section| section.title == "Position")
                    .count(),
                0
            );
            let canvas = overlay_preview_canvas_rect(geometry.preview, (16, 9));
            assert!(canvas.x >= geometry.preview.x);
            assert!(canvas.y >= geometry.preview.y);
            assert!(canvas.right() <= geometry.preview.right());
            assert!(canvas.bottom() <= geometry.preview.bottom());
            assert_ne!(canvas, geometry.region);
        }
    }

    #[test]
    fn current_app_audio_has_one_heading_and_compact_status_row() {
        let layout = SettingsLayout::build_shell(1200.0, 900.0, 0.0, Page::Audio, "", 0, None);
        assert_eq!(
            layout
                .sections
                .iter()
                .filter(|section| section.title == "Current app audio")
                .count(),
            1
        );
        let status = layout
            .regions
            .iter()
            .find(|region| region.kind == RegionKind::AudioCurrentApp)
            .expect("current app status");
        assert_eq!(status.rect.h, 64.0);
        assert!(status.rect.h < super::UiTokens::CARD_HEIGHT);
    }

    #[test]
    fn overlay_preview_canvas_preserves_monitor_aspect_ratios() {
        let available = 884.0;
        for aspect in [(16, 9), (16, 10), (21, 9), (9, 16)] {
            let (width, height) = overlay_preview_canvas_size(available, aspect);
            let expected = aspect.0 as f32 / aspect.1 as f32;
            assert!(((width / height) - expected).abs() < 0.001);
        }
        let (wide_width, wide_height) = overlay_preview_canvas_size(available, (21, 9));
        let (portrait_width, portrait_height) = overlay_preview_canvas_size(available, (9, 16));
        assert!(wide_width > wide_height);
        assert!(portrait_height > portrait_width);
    }

    #[test]
    fn fixed_top_chrome_has_one_close_and_aligned_search() {
        let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
        let chrome = top_chrome_geometry(layout.width, layout.nav_width);
        let close = layout
            .element(ElementId::WindowClose)
            .expect("close button");
        assert_eq!(layout.top_bar, chrome.row);
        assert_eq!(layout.search_rect, chrome.search);
        assert_eq!(close.rect, chrome.close);
        assert_eq!(layout.search_rect.y, close.rect.y);
        assert_eq!(layout.search_rect.h, close.rect.h);
        assert!(chrome.caption.w > 0.0);
        assert!(layout.search_rect.right() < close.rect.x);
        assert_eq!(
            layout
                .elements
                .iter()
                .filter(|element| {
                    !element.scrolls && element.kind == ElementKind::ButtonSecondary
                })
                .map(|element| element.id)
                .collect::<Vec<_>>(),
            vec![ElementId::WindowClose]
        );
        assert_eq!(
            layout.content_clip.y,
            chrome.row.bottom() + super::UiTokens::VIEWPORT_TOP_INSET
        );
    }

    #[test]
    fn overlay_position_grid_has_nine_accessible_cells() {
        let layout = SettingsLayout::build_shell(1200.0, 900.0, 0.0, Page::Overlay, "", 0, None);
        assert_eq!(
            layout
                .elements
                .iter()
                .filter(|element| matches!(element.id, ElementId::OverlayPositionCell(_)))
                .count(),
            9
        );
    }

    #[test]
    fn headers_reserve_separate_title_and_description_geometry_at_each_scale() {
        for width in [960.0, 1200.0, 1920.0] {
            for height in [660.0, 900.0, 1200.0] {
                let layout =
                    SettingsLayout::build_shell(width, height, 0.0, Page::Audio, "", 0, None);
                for section in &layout.sections {
                    assert!(section.height >= if section.page_header { 84.0 } else { 64.0 });
                }
            }
        }
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
