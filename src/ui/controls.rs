//! Reusable Direct2D controls for the WinShort Control Center.
//!
//! The same rectangles produced by `ui::layout` are used for pointer hit tests,
//! keyboard focus, and UI Automation bounds. Controls keep their states quiet
//! until interaction: hover, pressed, focus, disabled, and selected are all
//! explicit without turning the shell into a wall of chrome.

use std::borrow::Cow;

use crate::ui::layout::{Element, ElementKind, Rect};
use crate::ui::navigation::Page;
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};
use crate::ui::theme::UiTokens;

const ROW_RADIUS: f32 = UiTokens::CARD_RADIUS - 2.0;
const CONTROL_RADIUS: f32 = UiTokens::CONTROL_RADIUS;
const NAV_RADIUS: f32 = UiTokens::NAV_RADIUS;
const BODY_LEFT: f32 = 18.0;
const VALUE_WIDTH: f32 = UiTokens::VALUE_WIDTH;
const HOTKEY_WIDTH: f32 = UiTokens::HOTKEY_WIDTH;

pub enum ControlValue<'a> {
    Toggle(bool),
    Text(Cow<'a, str>),
    Slider { ratio: f32, label: Cow<'a, str> },
    Action(Cow<'a, str>),
}

#[derive(Debug, Clone, Copy)]
pub struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
    /// 0→1 hover transition sampled from Motion.
    pub hover_t: f32,
    /// 0→1 toggle state transition sampled from Motion.
    pub state_t: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InteractionState {
    Disabled,
    Pressed,
    Focused,
    Hovered,
    Idle,
}

pub(crate) fn interaction_state(interaction: Interaction) -> InteractionState {
    if interaction.disabled {
        InteractionState::Disabled
    } else if interaction.pressed {
        InteractionState::Pressed
    } else if interaction.focused {
        InteractionState::Focused
    } else if interaction.hovered || interaction.hover_t > 0.02 {
        InteractionState::Hovered
    } else {
        InteractionState::Idle
    }
}

pub fn draw_row(
    r: &Renderer,
    element: &Element,
    value: ControlValue<'_>,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(interaction);
    let surface = match state {
        InteractionState::Disabled => BrushRole::CardPressed,
        InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::CardHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,
    };
    draw_surface(r, rect, surface, state, ROW_RADIUS);

    let label_role = if interaction.disabled {
        BrushRole::TextDisabled
    } else {
        BrushRole::Text
    };
    let secondary_role = if interaction.disabled {
        BrushRole::TextDisabled
    } else {
        BrushRole::TextSecondary
    };
    let text_width = (rect.w - 260.0).max(110.0);
    r.text_clipped(
        &element.label,
        Rect::new(rect.x + BODY_LEFT, rect.y + 8.0, text_width, 22.0).d2d(),
        TextStyle::BodyStrong,
        label_role,
    );
    r.text_clipped(
        &element.description,
        Rect::new(rect.x + BODY_LEFT, rect.y + 31.0, text_width, 18.0).d2d(),
        TextStyle::Caption,
        secondary_role,
    );

    match value {
        ControlValue::Toggle(value) => draw_toggle(r, rect, value, interaction),
        ControlValue::Text(text) => draw_value_box(r, rect, &text, element.kind, interaction),
        ControlValue::Slider { ratio, label } => {
            draw_slider(r, rect, ratio.clamp(0.0, 1.0), &label, interaction)
        }
        ControlValue::Action(label) => draw_action(r, rect, &label, interaction),
    }
}

pub fn draw_button(r: &Renderer, rect: Rect, label: &str, primary: bool, interaction: Interaction) {
    let state = interaction_state(interaction);
    let bg = match (primary, state) {
        (_, InteractionState::Disabled) => BrushRole::CardPressed,
        (true, InteractionState::Pressed) => BrushRole::AccentPressed,
        (true, InteractionState::Hovered) => BrushRole::AccentHover,
        (true, _) => BrushRole::Accent,
        (false, InteractionState::Pressed) => BrushRole::CardPressed,
        (false, InteractionState::Hovered) => BrushRole::ControlHover,
        (false, _) => BrushRole::Card,
    };
    let text = if interaction.disabled {
        BrushRole::TextDisabled
    } else if primary {
        BrushRole::AccentText
    } else {
        BrushRole::Text
    };
    r.fill_rounded(rect.d2d(), CONTROL_RADIUS, bg);
    if !interaction.disabled {
        let border = if primary {
            match state {
                InteractionState::Pressed => BrushRole::AccentPressed,
                InteractionState::Hovered => BrushRole::AccentHover,
                _ => BrushRole::Accent,
            }
        } else {
            match state {
                InteractionState::Pressed => BrushRole::AccentPressed,
                InteractionState::Hovered => BrushRole::BorderStrong,
                _ => BrushRole::Border,
            }
        };
        r.stroke_rounded(rect.d2d(), CONTROL_RADIUS, border, 1.0);
    }
    if interaction.focused {
        r.stroke_rounded(
            rect.inset(-3.0).d2d(),
            CONTROL_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
    r.text(label, rect.d2d(), TextStyle::Button, text);
}

pub fn draw_nav_item(
    r: &Renderer,
    element: &Element,
    page: Page,
    hovered: bool,
    pressed: bool,
    focused: bool,
) {
    let selected =
        matches!(element.id, crate::ui::layout::ElementId::Nav(candidate) if candidate == page);
    let state = interaction_state(Interaction {
        hovered,
        pressed,
        focused,
        disabled: false,
        hover_t: 0.0,
        state_t: if selected { 1.0 } else { 0.0 },
    });
    if selected {
        r.fill_rounded(element.rect.d2d(), NAV_RADIUS, BrushRole::Accent);
    } else if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        r.fill_rounded(element.rect.d2d(), NAV_RADIUS, BrushRole::CardHover);
    }
    if focused {
        r.stroke_rounded(
            element.rect.inset(-2.0).d2d(),
            NAV_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
    draw_page_icon(
        r,
        Rect::new(element.rect.x + 12.0, element.rect.y + 10.0, 20.0, 20.0),
        page_for_nav(element.id),
        if selected {
            BrushRole::AccentText
        } else {
            BrushRole::TextSecondary
        },
    );
    r.text(
        &element.label,
        Rect::new(
            element.rect.x + 44.0,
            element.rect.y,
            element.rect.w - 52.0,
            element.rect.h,
        )
        .d2d(),
        TextStyle::BodyStrong,
        if selected {
            BrushRole::AccentText
        } else {
            BrushRole::Text
        },
    );
}

pub fn draw_search_box(r: &Renderer, rect: Rect, text: &str, focused: bool, hovered: bool) {
    let state = interaction_state(Interaction {
        hovered,
        pressed: false,
        focused,
        disabled: false,
        hover_t: 0.0,
        state_t: 0.0,
    });
    let bg = if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        BrushRole::CardHover
    } else {
        BrushRole::Card
    };
    r.fill_rounded(rect.d2d(), 9.0, bg);
    r.stroke_rounded(
        rect.d2d(),
        9.0,
        if focused {
            BrushRole::Focus
        } else {
            BrushRole::Border
        },
        if focused { 1.5 } else { 1.0 },
    );
    let icon_role = if focused {
        BrushRole::Accent
    } else {
        BrushRole::TextSecondary
    };
    r.ellipse(
        rect.x + 18.0,
        rect.y + 17.0,
        5.5,
        5.5,
        icon_role,
        false,
        1.5,
    );
    r.line(
        rect.x + 22.0,
        rect.y + 21.0,
        rect.x + 26.0,
        rect.y + 25.0,
        icon_role,
        1.5,
    );
    let shown = if text.is_empty() {
        "Find a setting"
    } else {
        text
    };
    r.text_clipped(
        shown,
        Rect::new(rect.x + 36.0, rect.y + 2.0, rect.w - 48.0, rect.h - 4.0).d2d(),
        TextStyle::Body,
        if text.is_empty() {
            BrushRole::TextSecondary
        } else {
            BrushRole::Text
        },
    );
}

pub fn draw_section_header(r: &Renderer, rect: Rect, title: &str, description: &str) {
    r.text(title, rect.d2d(), TextStyle::Section, BrushRole::Text);
    if !description.is_empty() {
        r.text_clipped(
            description,
            Rect::new(rect.x, rect.y + 24.0, rect.w, 20.0).d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }
}
pub fn draw_page_header(r: &Renderer, rect: Rect, title: &str, description: &str) {
    r.text(title, rect.d2d(), TextStyle::Title, BrushRole::Text);
    if !description.is_empty() {
        r.text_clipped(
            description,
            Rect::new(rect.x, rect.y + 36.0, rect.w, 22.0).d2d(),
            TextStyle::Subtitle,
            BrushRole::TextSecondary,
        );
    }
}

#[allow(clippy::too_many_arguments)] // Drawing a card keeps its content explicit at call sites.
pub fn draw_home_card(
    r: &Renderer,
    rect: Rect,
    title: &str,
    value: &str,
    detail: &str,
    action: &str,
    icon: Page,
    interaction: Interaction,
) {
    let state = interaction_state(interaction);
    let surface = match state {
        InteractionState::Disabled => BrushRole::CardPressed,
        InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::CardHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,
    };
    draw_surface(r, rect.inset(1.0), surface, state, 12.0);
    draw_page_icon(
        r,
        Rect::new(rect.x + 18.0, rect.y + 18.0, 28.0, 28.0),
        icon,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Accent
        },
    );
    r.text_clipped(
        title,
        Rect::new(rect.x + 60.0, rect.y + 13.0, rect.w - 190.0, 22.0).d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        value,
        Rect::new(rect.x + 60.0, rect.y + 37.0, rect.w - 190.0, 25.0).d2d(),
        TextStyle::Section,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        detail,
        Rect::new(rect.x + 60.0, rect.y + 67.0, rect.w - 190.0, 18.0).d2d(),
        TextStyle::Caption,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
    r.text(
        action,
        Rect::new(rect.right() - 116.0, rect.y + 38.0, 92.0, 28.0).d2d(),
        TextStyle::ButtonSmall,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Accent
        },
    );
    if interaction.focused {
        r.stroke_rounded(rect.inset(-2.0).d2d(), 14.0, BrushRole::Focus, 1.5);
    }
}

#[allow(clippy::too_many_arguments)] // Card fields map directly to the profile view model.
pub fn draw_profile_card(
    r: &Renderer,
    rect: Rect,
    name: &str,
    display_summary: &str,
    shortcut: &str,
    confirmed: bool,
    selected: bool,
    interaction: Interaction,
) {
    let state = interaction_state(interaction);
    let surface = if selected {
        BrushRole::BackgroundSubtle
    } else {
        match state {
            InteractionState::Hovered => BrushRole::CardHover,
            InteractionState::Pressed => BrushRole::CardPressed,
            _ => BrushRole::Card,
        }
    };
    draw_surface(r, rect.inset(1.0), surface, state, 12.0);
    r.text_clipped(
        name,
        Rect::new(
            rect.x + 18.0,
            rect.y + 16.0,
            (rect.w - 170.0).max(120.0),
            26.0,
        )
        .d2d(),
        TextStyle::Section,
        BrushRole::Text,
    );
    r.text_clipped(
        display_summary,
        Rect::new(
            rect.x + 18.0,
            rect.y + 49.0,
            (rect.w - 170.0).max(120.0),
            20.0,
        )
        .d2d(),
        TextStyle::Body,
        BrushRole::TextSecondary,
    );
    let status = if confirmed {
        "Ready to activate"
    } else {
        "Test before activating"
    };
    r.text(
        status,
        Rect::new(rect.x + 18.0, rect.y + 74.0, rect.w - 160.0, 18.0).d2d(),
        TextStyle::Caption,
        if confirmed {
            BrushRole::Success
        } else {
            BrushRole::Warning
        },
    );
    r.text_clipped(
        shortcut,
        Rect::new(rect.right() - 136.0, rect.y + 18.0, 116.0, 20.0).d2d(),
        TextStyle::CaptionRight,
        BrushRole::TextSecondary,
    );
    r.text(
        if confirmed { "Activate" } else { "Edit" },
        Rect::new(rect.right() - 136.0, rect.y + 66.0, 116.0, 26.0).d2d(),
        TextStyle::ButtonSmall,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Accent
        },
    );
    if selected {
        r.stroke_rounded(rect.d2d(), 12.0, BrushRole::Accent, 1.25);
    }
    if interaction.focused {
        r.stroke_rounded(rect.inset(-2.0).d2d(), 14.0, BrushRole::Focus, 1.5);
    }
}

pub fn draw_icon(r: &Renderer, rect: Rect, page: Page, role: BrushRole) {
    draw_page_icon(r, rect, page, role);
}

fn draw_surface(
    r: &Renderer,
    rect: Rect,
    surface: BrushRole,
    state: InteractionState,
    radius: f32,
) {
    r.fill_rounded(rect.translated_y(2.0).d2d(), radius, BrushRole::Shadow);
    r.fill_rounded(rect.d2d(), radius, surface);
    match state {
        InteractionState::Focused => {
            r.stroke_rounded(rect.inset(2.0).d2d(), radius - 2.0, BrushRole::Focus, 1.5)
        }
        InteractionState::Hovered | InteractionState::Pressed => {
            r.stroke_rounded(rect.d2d(), radius, BrushRole::BorderStrong, 1.0)
        }
        InteractionState::Disabled | InteractionState::Idle => {}
    }
}

pub fn draw_app_mark(r: &Renderer, rect: Rect) {
    r.fill_rounded(rect.d2d(), 9.0, BrushRole::Accent);
    let cx = rect.x + rect.w * 0.5;
    let cy = rect.y + rect.h * 0.5;
    r.line(
        cx - 7.0,
        cy + 5.0,
        cx - 7.0,
        cy - 3.0,
        BrushRole::AccentText,
        2.0,
    );
    r.line(cx, cy + 5.0, cx, cy - 7.0, BrushRole::AccentText, 2.0);
    r.line(
        cx + 7.0,
        cy + 5.0,
        cx + 7.0,
        cy + 1.0,
        BrushRole::AccentText,
        2.0,
    );
}

pub fn draw_scrollbar(r: &Renderer, viewport: Rect, scroll: f32, max_scroll: f32) {
    if max_scroll <= 0.0 {
        return;
    }
    let total = viewport.h + max_scroll;
    let thumb_h = (viewport.h * viewport.h / total).max(36.0);
    let travel = viewport.h - thumb_h - 8.0;
    let y = viewport.y + 4.0 + travel * (scroll / max_scroll);
    r.fill_rounded(
        Rect::new(viewport.right() - 7.0, y, 3.0, thumb_h).d2d(),
        1.5,
        BrushRole::BorderStrong,
    );
}

fn control_rect(row: Rect, width: f32) -> Rect {
    Rect::new(
        row.right() - 18.0 - width,
        row.y + (row.h - 34.0) * 0.5,
        width,
        34.0,
    )
}

pub(crate) fn value_control_rect(row: Rect, kind: ElementKind) -> Rect {
    let width = match kind {
        ElementKind::Hotkey => HOTKEY_WIDTH,
        ElementKind::Value => VALUE_WIDTH,
        _ => VALUE_WIDTH,
    };
    control_rect(row, width)
}

const VALUE_TEXT_PADDING: f32 = 10.0;
const VALUE_CHEVRON_RESERVE: f32 = 30.0;

pub(crate) fn value_text_rect(row: Rect, kind: ElementKind) -> Rect {
    let control = value_control_rect(row, kind);
    let left = control.x + VALUE_TEXT_PADDING;
    let top = control.y + VALUE_TEXT_PADDING;
    let right_padding = if kind == ElementKind::Value {
        VALUE_CHEVRON_RESERVE
    } else {
        VALUE_TEXT_PADDING
    };
    let right = (control.right() - right_padding).max(left);
    let bottom = (control.bottom() - VALUE_TEXT_PADDING).max(top);
    Rect::new(left, top, right - left, bottom - top)
}

fn draw_toggle(r: &Renderer, row: Rect, value: bool, interaction: Interaction) {
    let rect = control_rect(row, 48.0);
    let state = interaction_state(interaction);
    let track = match state {
        InteractionState::Disabled => BrushRole::Border,
        InteractionState::Pressed => {
            if value {
                BrushRole::AccentPressed
            } else {
                BrushRole::CardPressed
            }
        }
        InteractionState::Hovered => {
            if value {
                BrushRole::AccentHover
            } else {
                BrushRole::ControlHover
            }
        }
        InteractionState::Focused | InteractionState::Idle => {
            if value {
                BrushRole::Accent
            } else {
                BrushRole::Border
            }
        }
    };
    r.fill_rounded(rect.d2d(), 17.0, track);
    let outline = match state {
        InteractionState::Focused => BrushRole::Focus,
        InteractionState::Pressed => BrushRole::AccentPressed,
        InteractionState::Hovered => BrushRole::BorderStrong,
        InteractionState::Disabled | InteractionState::Idle => BrushRole::BorderStrong,
    };
    r.stroke_rounded(
        rect.d2d(),
        17.0,
        outline,
        if matches!(state, InteractionState::Focused) {
            1.5
        } else {
            1.0
        },
    );
    let t = interaction.state_t.clamp(0.0, 1.0);
    let knob_x = rect.x + 12.0 + (rect.w - 24.0) * t;
    r.ellipse(
        knob_x,
        rect.y + rect.h * 0.5,
        7.0,
        7.0,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else if value {
            BrushRole::AccentText
        } else {
            BrushRole::TextSecondary
        },
        true,
        0.0,
    );
}

fn draw_value_box(
    r: &Renderer,
    row: Rect,
    text: &str,
    kind: ElementKind,
    interaction: Interaction,
) {
    let rect = value_control_rect(row, kind);
    let state = interaction_state(interaction);
    let bg = match state {
        InteractionState::Disabled | InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::ControlHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::BackgroundSubtle,
    };
    let border = match state {
        InteractionState::Disabled => BrushRole::Border,
        InteractionState::Pressed => BrushRole::AccentPressed,
        InteractionState::Focused => BrushRole::Focus,
        InteractionState::Hovered => BrushRole::BorderStrong,
        InteractionState::Idle => BrushRole::Border,
    };
    r.fill_rounded(rect.d2d(), CONTROL_RADIUS, bg);
    r.stroke_rounded(
        rect.d2d(),
        CONTROL_RADIUS,
        border,
        if matches!(state, InteractionState::Focused) {
            1.75
        } else {
            1.0
        },
    );
    let role = if interaction.disabled {
        BrushRole::TextDisabled
    } else if matches!(kind, ElementKind::Hotkey) && text.starts_with("Press") {
        BrushRole::Accent
    } else {
        BrushRole::Text
    };
    r.text_clipped(
        text,
        value_text_rect(row, kind).d2d(),
        TextStyle::Value,
        role,
    );
    if matches!(kind, ElementKind::Value) {
        let x = rect.right() - 15.0;
        let y = rect.y + rect.h * 0.5;
        let chevron = match state {
            InteractionState::Disabled => BrushRole::TextDisabled,
            InteractionState::Focused => BrushRole::Focus,
            InteractionState::Pressed | InteractionState::Hovered => BrushRole::Text,
            InteractionState::Idle => BrushRole::TextSecondary,
        };
        r.line(x - 3.0, y - 2.0, x, y + 1.0, chevron, 1.25);
        r.line(x, y + 1.0, x + 3.0, y - 2.0, chevron, 1.25);
    }
}

fn draw_slider(r: &Renderer, row: Rect, ratio: f32, label: &str, interaction: Interaction) {
    let rect = control_rect(row, VALUE_WIDTH);
    let state = interaction_state(interaction);
    let label_w = 52.0;
    let track = Rect::new(rect.x, rect.y + 15.0, rect.w - label_w - 12.0, 4.0);
    r.fill_rounded(
        track.d2d(),
        2.0,
        if interaction.disabled {
            BrushRole::Border
        } else {
            BrushRole::BorderStrong
        },
    );
    let filled = Rect::new(track.x, track.y, track.w * ratio, track.h);
    if filled.w > 0.0 {
        r.fill_rounded(
            filled.d2d(),
            2.0,
            if interaction.disabled {
                BrushRole::BorderStrong
            } else if matches!(state, InteractionState::Pressed) {
                BrushRole::AccentPressed
            } else {
                BrushRole::Accent
            },
        );
    }
    let knob_x = track.x + track.w * ratio;
    r.ellipse(
        knob_x,
        track.y + 2.0,
        if matches!(state, InteractionState::Pressed) {
            7.0
        } else {
            6.0
        },
        if matches!(state, InteractionState::Pressed) {
            7.0
        } else {
            6.0
        },
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Accent
        },
        true,
        0.0,
    );
    r.text(
        label,
        Rect::new(rect.right() - label_w, rect.y, label_w, rect.h).d2d(),
        TextStyle::CaptionRight,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
}

fn draw_action(r: &Renderer, row: Rect, label: &str, interaction: Interaction) {
    draw_button(r, control_rect(row, 136.0), label, false, interaction);
}

fn page_for_nav(id: crate::ui::layout::ElementId) -> Page {
    match id {
        crate::ui::layout::ElementId::Nav(page) => page,
        _ => Page::Home,
    }
}

fn draw_page_icon(r: &Renderer, rect: Rect, page: Page, role: BrushRole) {
    let cx = rect.x + rect.w * 0.5;
    let cy = rect.y + rect.h * 0.5;
    match page {
        Page::Home => {
            r.line(rect.x + 4.0, cy, cx, rect.y + 4.0, role, 1.7);
            r.line(cx, rect.y + 4.0, rect.right() - 4.0, cy, role, 1.7);
            r.line(
                rect.x + 7.0,
                cy - 1.0,
                rect.x + 7.0,
                rect.bottom() - 4.0,
                role,
                1.7,
            );
            r.line(
                rect.right() - 7.0,
                cy - 1.0,
                rect.right() - 7.0,
                rect.bottom() - 4.0,
                role,
                1.7,
            );
        }
        Page::Shortcuts => {
            r.line(rect.x + 4.0, cy, rect.right() - 4.0, cy, role, 1.8);
            r.line(rect.x + 8.0, cy - 5.0, rect.x + 8.0, cy + 5.0, role, 1.8);
            r.line(
                rect.right() - 8.0,
                cy - 5.0,
                rect.right() - 8.0,
                cy + 5.0,
                role,
                1.8,
            );
        }
        Page::Audio => {
            r.line(rect.x + 4.0, cy - 3.0, rect.x + 9.0, cy - 3.0, role, 1.8);
            r.line(
                rect.x + 9.0,
                cy - 3.0,
                rect.x + 14.0,
                rect.y + 5.0,
                role,
                1.8,
            );
            r.line(
                rect.x + 14.0,
                rect.y + 5.0,
                rect.x + 14.0,
                rect.bottom() - 5.0,
                role,
                1.8,
            );
            r.line(
                rect.x + 14.0,
                rect.bottom() - 5.0,
                rect.x + 9.0,
                cy + 3.0,
                role,
                1.8,
            );
            r.line(rect.x + 9.0, cy + 3.0, rect.x + 4.0, cy + 3.0, role, 1.8);
            r.line(
                rect.right() - 5.0,
                cy - 4.0,
                rect.right() - 2.0,
                cy - 1.0,
                role,
                1.4,
            );
            r.line(
                rect.right() - 2.0,
                cy - 1.0,
                rect.right() - 5.0,
                cy + 2.0,
                role,
                1.4,
            );
        }
        Page::Workspaces | Page::Displays => {
            r.stroke_rounded(
                Rect::new(rect.x + 3.0, rect.y + 4.0, rect.w - 6.0, rect.h - 8.0).d2d(),
                2.0,
                role,
                1.5,
            );
            r.line(cx, rect.y + 5.0, cx, rect.bottom() - 5.0, role, 1.1);
            r.line(rect.x + 4.0, cy, rect.right() - 4.0, cy, role, 1.1);
        }
        Page::Overlay => {
            r.stroke_rounded(
                Rect::new(rect.x + 3.0, rect.y + 4.0, rect.w - 6.0, rect.h - 8.0).d2d(),
                3.0,
                role,
                1.5,
            );
            r.line(
                rect.x + 7.0,
                rect.bottom() - 6.0,
                rect.right() - 7.0,
                rect.bottom() - 6.0,
                role,
                1.5,
            );
        }
        Page::System | Page::Advanced => {
            r.ellipse(cx, cy, 7.0, 7.0, role, false, 1.5);
            r.ellipse(cx, cy, 2.0, 2.0, role, true, 0.0);
            for (dx, dy) in [(0.0, -9.0), (0.0, 9.0), (-9.0, 0.0), (9.0, 0.0)] {
                r.line(cx + dx * 0.8, cy + dy * 0.8, cx + dx, cy + dy, role, 1.4);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        interaction_state, value_control_rect, value_text_rect, Interaction, InteractionState,
        VALUE_TEXT_PADDING,
    };
    use crate::ui::layout::{ElementKind, Rect};

    fn interaction() -> Interaction {
        Interaction {
            hovered: false,
            pressed: false,
            focused: false,
            disabled: false,
            hover_t: 0.0,
            state_t: 0.0,
        }
    }

    #[test]
    fn interaction_state_has_explicit_precedence() {
        let mut value = interaction();
        assert_eq!(interaction_state(value), InteractionState::Idle);
        value.hovered = true;
        assert_eq!(interaction_state(value), InteractionState::Hovered);
        value.focused = true;
        assert_eq!(interaction_state(value), InteractionState::Focused);
        value.pressed = true;
        assert_eq!(interaction_state(value), InteractionState::Pressed);
        value.disabled = true;
        assert_eq!(interaction_state(value), InteractionState::Disabled);
    }

    #[test]
    fn value_control_geometry_is_right_aligned_and_kind_aware() {
        let row = Rect::new(24.0, 100.0, 560.0, 58.0);
        let value = value_control_rect(row, ElementKind::Value);
        let hotkey = value_control_rect(row, ElementKind::Hotkey);
        assert_eq!(value.w, 206.0);
        assert_eq!(hotkey.w, 184.0);
        assert!(value.x > row.x);
        assert_eq!(value.right(), row.right() - 18.0);
    }

    #[test]
    fn value_text_geometry_reserves_chevron_and_stays_positive() {
        let row = Rect::new(24.0, 100.0, 560.0, 58.0);
        let value = value_control_rect(row, ElementKind::Value);
        let text = value_text_rect(row, ElementKind::Value);
        assert!(text.w > 0.0);
        assert!(text.h > 0.0);
        assert!(text.x > value.x);
        assert!(text.right() < value.right() - 12.0);
        let hotkey = value_control_rect(row, ElementKind::Hotkey);
        let hotkey_text = value_text_rect(row, ElementKind::Hotkey);
        assert_eq!(hotkey_text.right(), hotkey.right() - VALUE_TEXT_PADDING);
    }
}
