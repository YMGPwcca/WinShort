//! Small deterministic layout engine for the native settings surface.
//! Layout is recomputed on resize/DPI/config changes, never in a polling loop.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;

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
    pub const FOCUS_ORDER: [ElementId; 33] = [
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
        ElementId::InputRole,
        ElementId::OutputRole,
        ElementId::DesktopsEnabled,
        ElementId::WinNumberEnabled,
        ElementId::DesktopNumberModifier,
        ElementId::MoveDesktopModifier,
        ElementId::SilentMoveDesktopModifier,
        ElementId::PreviousDesktopHotkey,
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
}

#[derive(Debug, Clone)]
pub struct Element {
    pub id: ElementId,
    pub kind: ElementKind,
    /// Rect in viewport logical coordinates after scroll is applied.
    pub rect: Rect,
    pub label: &'static str,
    pub description: &'static str,
    pub scrolls: bool,
}

#[derive(Debug, Clone)]
pub struct SectionLabel {
    pub title: &'static str,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct SettingsLayout {
    pub width: f32,
    #[allow(dead_code)] // design-space record; metrics derived from width/rows
    pub height: f32,
    pub content_clip: Rect,
    pub footer: Rect,
    pub elements: Vec<Element>,
    pub sections: Vec<SectionLabel>,
    pub max_scroll: f32,
    pub scroll: f32,
}

impl SettingsLayout {
    pub fn build(width: f32, height: f32, requested_scroll: f32) -> Self {
        let margin = 24.0;
        let content_top = 18.0;
        let footer_h = 70.0;
        let content_bottom = (height - footer_h).max(content_top + 120.0);
        let content_clip = Rect::new(0.0, content_top, width, content_bottom - content_top);
        let footer = Rect::new(0.0, content_bottom, width, height - content_bottom);
        let card_x = margin;
        let card_w = (width - margin * 2.0).max(320.0);

        let mut raw_elements = Vec::new();
        let mut raw_sections = Vec::new();
        let mut y = 20.0;

        // Product header.
        y += 66.0;

        add_section(&mut raw_sections, &mut y, "General");
        add_card_rows(
            &mut raw_elements,
            &mut y,
            card_x,
            card_w,
            &[
                row(
                    ElementId::StartWithWindows,
                    ElementKind::Toggle,
                    "Start with Windows",
                    "Launch quietly after sign-in",
                ),
                row(
                    ElementId::StartHotkeysEnabled,
                    ElementKind::Toggle,
                    "Start hotkeys enabled",
                    "Use tray Suspend when you need a pause",
                ),
            ],
        );

        add_section(&mut raw_sections, &mut y, "Hotkeys");
        add_card_rows(
            &mut raw_elements,
            &mut y,
            card_x,
            card_w,
            &[
                row(
                    ElementId::MicHotkey,
                    ElementKind::Hotkey,
                    "Microphone",
                    "Toggle input mute",
                ),
                row(
                    ElementId::OutputHotkey,
                    ElementKind::Hotkey,
                    "Output",
                    "Toggle speaker mute",
                ),
                row(
                    ElementId::ForegroundHotkey,
                    ElementKind::Hotkey,
                    "Current app",
                    "Mute all sessions owned by the foreground app",
                ),
                row(
                    ElementId::CycleInputHotkey,
                    ElementKind::Hotkey,
                    "Cycle input device",
                    "Switch WinShort to the next input endpoint",
                ),
                row(
                    ElementId::CycleOutputHotkey,
                    ElementKind::Hotkey,
                    "Cycle output device",
                    "Switch WinShort to the next output endpoint",
                ),
                row(
                    ElementId::ForegroundVolumeUpHotkey,
                    ElementKind::Hotkey,
                    "App volume up",
                    "Raise foreground app volume by 5%",
                ),
                row(
                    ElementId::ForegroundVolumeDownHotkey,
                    ElementKind::Hotkey,
                    "App volume down",
                    "Lower foreground app volume by 5%",
                ),
            ],
        );

        add_section(&mut raw_sections, &mut y, "Audio");
        add_card_rows(
            &mut raw_elements,
            &mut y,
            card_x,
            card_w,
            &[
                row(
                    ElementId::InputDevice,
                    ElementKind::Value,
                    "Input device",
                    "Follow the default capture endpoint",
                ),
                row(
                    ElementId::OutputDevice,
                    ElementKind::Value,
                    "Output device",
                    "Follow the default render endpoint",
                ),
                row(
                    ElementId::InputRole,
                    ElementKind::Value,
                    "Input role",
                    "Only applies when following the Windows default device",
                ),
                row(
                    ElementId::OutputRole,
                    ElementKind::Value,
                    "Output role",
                    "Only applies when following the Windows default device",
                ),
            ],
        );

        add_section(&mut raw_sections, &mut y, "Virtual Desktops");
        add_card_rows(
            &mut raw_elements,
            &mut y,
            card_x,
            card_w,
            &[
                row(
                    ElementId::DesktopsEnabled,
                    ElementKind::Toggle,
                    "Desktop engine",
                    "Use native Shell COM when compatible",
                ),
                row(
                    ElementId::WinNumberEnabled,
                    ElementKind::Toggle,
                    "Win + number switching",
                    "Replace taskbar shortcuts with Desktop 1–9",
                ),
                row(
                    ElementId::DesktopNumberModifier,
                    ElementKind::Value,
                    "Desktop number chord",
                    "Modifier family for Desktop 1–9",
                ),
                row(
                    ElementId::MoveDesktopModifier,
                    ElementKind::Value,
                    "Move and follow chord",
                    "Move the foreground window to Desktop 1–9",
                ),
                row(
                    ElementId::SilentMoveDesktopModifier,
                    ElementKind::Value,
                    "Silent move chord",
                    "Move without switching away",
                ),
                row(
                    ElementId::PreviousDesktopHotkey,
                    ElementKind::Hotkey,
                    "Previous desktop",
                    "Return to the previously active desktop",
                ),
            ],
        );

        add_section(&mut raw_sections, &mut y, "Overlay");
        add_card_rows(
            &mut raw_elements,
            &mut y,
            card_x,
            card_w,
            &[
                row(
                    ElementId::OverlayEnabled,
                    ElementKind::Toggle,
                    "Show status overlay",
                    "Never steals focus or receives clicks",
                ),
                row(
                    ElementId::OverlayAppearance,
                    ElementKind::Value,
                    "Appearance",
                    "Follow System, Dark, or Light",
                ),
                row(
                    ElementId::OverlayExternalChanges,
                    ElementKind::Toggle,
                    "Show external audio changes",
                    "Show overlays for changes made outside WinShort",
                ),
                row(
                    ElementId::OverlayPosition,
                    ElementKind::Value,
                    "Position",
                    "Respect the active monitor work area",
                ),
                row(
                    ElementId::OverlayMonitor,
                    ElementKind::Value,
                    "Monitor",
                    "Choose where status appears",
                ),
                row(
                    ElementId::OverlayDuration,
                    ElementKind::Slider,
                    "Duration",
                    "How long the settled state remains",
                ),
                row(
                    ElementId::OverlayOpacity,
                    ElementKind::Slider,
                    "Opacity",
                    "HUD surface visibility",
                ),
                row(
                    ElementId::OverlayScale,
                    ElementKind::Slider,
                    "Scale",
                    "Size independent of monitor DPI",
                ),
                row(
                    ElementId::OverlayPreview,
                    ElementKind::Action,
                    "Preview overlay",
                    "Show a representative microphone state",
                ),
            ],
        );
        add_section(&mut raw_sections, &mut y, "Advanced");
        add_card_rows(
            &mut raw_elements,
            &mut y,
            card_x,
            card_w,
            &[
                row(
                    ElementId::DebugLogging,
                    ElementKind::Toggle,
                    "Debug logging",
                    "Extra troubleshooting detail until WinShort restarts",
                ),
                row(
                    ElementId::DiagnosticsStatus,
                    ElementKind::Action,
                    "Diagnostics & support",
                    "System status, logs, and a sanitized support bundle",
                ),
                row(
                    ElementId::OpenConfigFolder,
                    ElementKind::Action,
                    "Open config folder",
                    "Configuration and diagnostic logs",
                ),
                row(
                    ElementId::ResetSettings,
                    ElementKind::Action,
                    "Reset settings",
                    "Restore defaults in the draft; Save to apply",
                ),
            ],
        );

        y += 24.0;
        let content_height = y;
        let visible_h = content_clip.h;
        let max_scroll = (content_height - visible_h).max(0.0);
        let scroll = requested_scroll.clamp(0.0, max_scroll);
        let dy = -scroll;

        let elements = raw_elements
            .into_iter()
            .map(|mut e| {
                e.rect = e.rect.translated_y(dy);
                e
            })
            .collect();
        let sections = raw_sections
            .into_iter()
            .map(|mut s| {
                s.y += dy;
                s
            })
            .collect();

        // Footer controls stay fixed.
        let button_y = footer.y + 16.0;
        let button_h = 36.0;
        let save_w = 88.0;
        let cancel_w = 88.0;
        let save_x = width - margin - save_w;
        let cancel_x = save_x - 10.0 - cancel_w;
        let mut elements: Vec<Element> = elements;
        elements.push(Element {
            id: ElementId::Cancel,
            kind: ElementKind::ButtonSecondary,
            rect: Rect::new(cancel_x, button_y, cancel_w, button_h),
            label: "Cancel",
            description: "Discard draft changes",
            scrolls: false,
        });
        elements.push(Element {
            id: ElementId::Save,
            kind: ElementKind::ButtonPrimary,
            rect: Rect::new(save_x, button_y, save_w, button_h),
            label: "Save",
            description: "Validate and apply immediately",
            scrolls: false,
        });

        Self {
            width,
            height,
            content_clip,
            footer,
            elements,
            sections,
            max_scroll,
            scroll,
        }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<ElementId> {
        self.elements
            .iter()
            .rev()
            .find(|e| e.rect.contains(x, y) && (!e.scrolls || self.content_clip.contains(x, y)))
            .map(|e| e.id)
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }
}

#[derive(Clone, Copy)]
struct RowSpec {
    id: ElementId,
    kind: ElementKind,
    label: &'static str,
    description: &'static str,
}

const fn row(
    id: ElementId,
    kind: ElementKind,
    label: &'static str,
    description: &'static str,
) -> RowSpec {
    RowSpec {
        id,
        kind,
        label,
        description,
    }
}

fn add_section(sections: &mut Vec<SectionLabel>, y: &mut f32, title: &'static str) {
    *y += 18.0;
    sections.push(SectionLabel { title, y: *y });
    *y += 30.0;
}

fn add_card_rows(elements: &mut Vec<Element>, y: &mut f32, x: f32, width: f32, rows: &[RowSpec]) {
    const ROW_H: f32 = 64.0;
    for spec in rows {
        elements.push(Element {
            id: spec.id,
            kind: spec.kind,
            rect: Rect::new(x, *y, width, ROW_H),
            label: spec.label,
            description: spec.description,
            scrolls: true,
        });
        *y += ROW_H;
    }
}
