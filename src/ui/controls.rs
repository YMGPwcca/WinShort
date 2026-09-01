//! Reusable Direct2D controls for the WinShort Control Center.
//!
//! The same rectangles produced by `ui::layout` are used for pointer hit tests,
//! keyboard focus, and UI Automation bounds. Controls keep their states quiet
//! until interaction: hover, pressed, focus, disabled, and selected are all
//! explicit without turning the shell into a wall of chrome.

use std::borrow::Cow;

use crate::ui::layout::{Element, ElementKind, Rect};
use crate::ui::navigation::Page;
use crate::ui::presentation::DeviceSelectionPresentation;
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};
use crate::ui::theme::UiTokens;

const ROW_RADIUS: f32 = UiTokens::CARD_RADIUS - 2.0;
const CONTROL_RADIUS: f32 = UiTokens::CONTROL_RADIUS;
const NAV_RADIUS: f32 = UiTokens::NAV_RADIUS;
const BODY_LEFT: f32 = 18.0;
const CONTROL_WIDTH: f32 = UiTokens::CONTROL_WIDTH;

pub enum ControlValue<'a> {
    Toggle(bool),
    Text(Cow<'a, str>),
    Slider { ratio: f32, label: Cow<'a, str> },
    Action(Cow<'a, str>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Subtle,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Page(Page),
    Speaker,
    Microphone,
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
    if element.kind == ElementKind::Hotkey {
        if let ControlValue::Text(text) = value {
            draw_shortcut_card(r, element, &text, interaction);
        }
        return;
    }
    if element.kind == ElementKind::Choice {
        let selected = matches!(value, ControlValue::Toggle(true));
        draw_choice(r, element, selected, interaction, true);
        return;
    }
    if element.kind == ElementKind::Checkbox {
        let selected = matches!(value, ControlValue::Toggle(true));
        draw_choice(r, element, selected, interaction, false);
        return;
    }
    if element.kind == ElementKind::Slider {
        if let ControlValue::Slider { ratio, label } = value {
            draw_slider_cluster(r, element, ratio.clamp(0.0, 1.0), &label, interaction);
        }
        return;
    }
    if matches!(
        element.kind,
        ElementKind::ButtonPrimary | ElementKind::ButtonSecondary | ElementKind::ButtonDanger
    ) && element.rect.h <= 40.0
    {
        if let ControlValue::Action(label) = value {
            draw_button_style(
                r,
                element.rect.inset(1.0),
                &label,
                button_style(element.kind),
                interaction,
            );
        }
        return;
    }

    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
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
    let stack_top = rect.y + (rect.h - 40.0) * 0.5;
    r.text_clipped(
        &element.label,
        Rect::new(rect.x + BODY_LEFT, stack_top, text_width, 20.0).d2d(),
        TextStyle::BodyStrong,
        label_role,
    );
    r.text_clipped(
        &element.description,
        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, text_width, 18.0).d2d(),
        TextStyle::Caption,
        secondary_role,
    );

    match value {
        ControlValue::Toggle(value) => draw_toggle(r, rect, value, interaction),
        ControlValue::Text(text) => draw_value_box(r, rect, &text, element.kind, interaction),
        ControlValue::Slider { ratio, label } => {
            draw_slider_cluster(r, element, ratio.clamp(0.0, 1.0), &label, interaction)
        }
        ControlValue::Action(label) => draw_button_style(
            r,
            control_rect(rect, 136.0),
            &label,
            button_style(element.kind),
            interaction,
        ),
    }
}

fn button_style(kind: ElementKind) -> ButtonStyle {
    match kind {
        ElementKind::ButtonPrimary => ButtonStyle::Primary,
        ElementKind::ButtonDanger => ButtonStyle::Danger,
        ElementKind::ButtonSecondary => ButtonStyle::Secondary,
        _ => ButtonStyle::Subtle,
    }
}

pub fn draw_button(r: &Renderer, rect: Rect, label: &str, primary: bool, interaction: Interaction) {
    draw_button_style(
        r,
        rect,
        label,
        if primary {
            ButtonStyle::Primary
        } else {
            ButtonStyle::Secondary
        },
        interaction,
    );
}

pub fn draw_button_style(
    r: &Renderer,
    rect: Rect,
    label: &str,
    style: ButtonStyle,
    interaction: Interaction,
) {
    let state = interaction_state(interaction);
    let (bg, border, text) = match style {
        ButtonStyle::Primary => (
            match state {
                InteractionState::Disabled => BrushRole::CardPressed,
                InteractionState::Pressed => BrushRole::AccentPressed,
                InteractionState::Hovered => BrushRole::AccentHover,
                _ => BrushRole::Accent,
            },
            BrushRole::Accent,
            if interaction.disabled {
                BrushRole::TextDisabled
            } else {
                BrushRole::AccentText
            },
        ),
        ButtonStyle::Danger => (
            match state {
                InteractionState::Disabled => BrushRole::CardPressed,
                InteractionState::Hovered => BrushRole::CardHover,
                _ => BrushRole::Card,
            },
            if interaction.disabled {
                BrushRole::Border
            } else {
                BrushRole::Danger
            },
            if interaction.disabled {
                BrushRole::TextDisabled
            } else {
                BrushRole::Danger
            },
        ),
        ButtonStyle::Secondary => (
            match state {
                InteractionState::Disabled => BrushRole::CardPressed,
                InteractionState::Pressed => BrushRole::CardPressed,
                InteractionState::Hovered => BrushRole::ControlHover,
                _ => BrushRole::Card,
            },
            if interaction.disabled {
                BrushRole::Border
            } else {
                BrushRole::BorderStrong
            },
            if interaction.disabled {
                BrushRole::TextDisabled
            } else {
                BrushRole::Text
            },
        ),
        ButtonStyle::Subtle => (
            match state {
                InteractionState::Disabled => BrushRole::CardPressed,
                InteractionState::Pressed => BrushRole::CardPressed,
                InteractionState::Hovered => BrushRole::ControlHover,
                _ => BrushRole::BackgroundSubtle,
            },
            BrushRole::Border,
            if interaction.disabled {
                BrushRole::TextDisabled
            } else {
                BrushRole::TextSecondary
            },
        ),
    };
    r.fill_rounded(rect.d2d(), CONTROL_RADIUS, bg);
    r.stroke_rounded(rect.d2d(), CONTROL_RADIUS, border, 1.0);
    if interaction.focused {
        r.stroke_rounded(
            rect.inset(-3.0).d2d(),
            CONTROL_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
    r.text_clipped(label, rect.d2d(), TextStyle::Button, text);
}
pub(crate) fn titlebar_glyph_bounds(rect: Rect) -> Rect {
    Rect::new(
        rect.x + (rect.w - 12.0) * 0.5,
        rect.y + (rect.h - 12.0) * 0.5,
        12.0,
        12.0,
    )
}

pub fn draw_titlebar_button(
    r: &Renderer,
    element: &Element,
    hovered: bool,
    pressed: bool,
    focused: bool,
    maximized: bool,
) {
    let state = interaction_state(Interaction {
        hovered,
        pressed,
        focused,
        disabled: false,
        hover_t: 0.0,
        state_t: 0.0,
    });
    let close = element.id == crate::ui::layout::ElementId::WindowClose;
    if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        r.fill_rounded(
            element.rect.d2d(),
            4.0,
            if close {
                BrushRole::Danger
            } else {
                BrushRole::ControlHover
            },
        );
    }
    if focused {
        r.stroke_rounded(element.rect.inset(-2.0).d2d(), 6.0, BrushRole::Focus, 1.5);
    }
    let role = if close && matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        BrushRole::AccentText
    } else {
        BrushRole::TextSecondary
    };
    let glyph = titlebar_glyph_bounds(element.rect);
    match element.id {
        crate::ui::layout::ElementId::WindowMinimize => {
            let y = glyph.bottom() - 2.0;
            r.line(glyph.x + 1.0, y, glyph.right() - 1.0, y, role, 1.25);
        }
        crate::ui::layout::ElementId::WindowMaximize => {
            if maximized {
                r.stroke_rounded(
                    Rect::new(glyph.x + 2.0, glyph.y, glyph.w - 2.0, glyph.h - 2.0).d2d(),
                    1.5,
                    role,
                    1.25,
                );
                r.stroke_rounded(
                    Rect::new(glyph.x, glyph.y + 3.0, glyph.w - 2.0, glyph.h - 2.0).d2d(),
                    1.5,
                    role,
                    1.25,
                );
            } else {
                r.stroke_rounded(glyph.d2d(), 1.5, role, 1.25);
            }
        }
        crate::ui::layout::ElementId::WindowClose => {
            r.line(
                glyph.x + 1.5,
                glyph.y + 1.5,
                glyph.right() - 1.5,
                glyph.bottom() - 1.5,
                role,
                1.25,
            );
            r.line(
                glyph.right() - 1.5,
                glyph.y + 1.5,
                glyph.x + 1.5,
                glyph.bottom() - 1.5,
                role,
                1.25,
            );
        }
        _ => {}
    }
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
        // Selection is persistent context, not keyboard focus. Keep the rail
        // quiet and add one accent indicator instead of a saturated block.
        r.fill_rounded(element.rect.d2d(), NAV_RADIUS, BrushRole::BackgroundSubtle);
        r.fill_rounded(
            Rect::new(
                element.rect.x,
                element.rect.y + 8.0,
                3.0,
                element.rect.h - 16.0,
            )
            .d2d(),
            1.5,
            BrushRole::Accent,
        );
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
            BrushRole::Accent
        } else {
            BrushRole::TextSecondary
        },
    );
    r.text_clipped(
        &element.label,
        Rect::new(
            element.rect.x + 44.0,
            element.rect.y,
            element.rect.w - 52.0,
            element.rect.h,
        )
        .d2d(),
        TextStyle::BodyStrong,
        BrushRole::Text,
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

pub(crate) fn section_title_text_rect(rect: Rect) -> Rect {
    Rect::new(rect.x + 12.0, rect.y + 20.0, (rect.w - 12.0).max(1.0), 28.0)
}

pub(crate) fn section_accent_rect(rect: Rect) -> Rect {
    let title = section_title_text_rect(rect);
    let height = 18.0;
    Rect::new(rect.x, title.y + (title.h - height) * 0.5, 3.0, height)
}

pub fn draw_section_header(r: &Renderer, rect: Rect, title: &str, description: &str) {
    // Keep the divider in the inter-section breathing room. The accent is
    // centered from the title's actual text rectangle, not the whole block.
    r.line(
        rect.x,
        rect.y + 8.0,
        rect.right(),
        rect.y + 8.0,
        BrushRole::Border,
        1.0,
    );
    r.fill_rounded(section_accent_rect(rect).d2d(), 1.5, BrushRole::Accent);
    let title_rect = section_title_text_rect(rect);
    r.text_clipped(title, title_rect.d2d(), TextStyle::Section, BrushRole::Text);
    if !description.is_empty() {
        let description_height = r
            .text_height(
                description,
                TextStyle::SectionDescription,
                title_rect.w,
                (rect.h - 54.0).max(20.0),
            )
            .clamp(20.0, (rect.h - 54.0).max(20.0));
        r.text_clipped(
            description,
            Rect::new(
                title_rect.x,
                rect.y + 54.0,
                title_rect.w,
                description_height,
            )
            .d2d(),
            TextStyle::SectionDescription,
            BrushRole::TextSecondary,
        );
    }
}
pub fn draw_page_header(r: &Renderer, rect: Rect, title: &str, description: &str) {
    r.text_clipped(
        title,
        Rect::new(rect.x, rect.y, rect.w, 36.0).d2d(),
        TextStyle::Title,
        BrushRole::Text,
    );
    if !description.is_empty() {
        let description_height = r
            .text_height(
                description,
                TextStyle::Subtitle,
                rect.w,
                (rect.h - 46.0).max(30.0),
            )
            .clamp(22.0, (rect.h - 46.0).max(30.0));
        r.text_clipped(
            description,
            Rect::new(rect.x, rect.y + 46.0, rect.w, description_height).d2d(),
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
    icon: IconKind,
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
    draw_icon_kind(
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
        Rect::new(rect.x + 60.0, rect.y + 13.0, rect.w - 78.0, 22.0).d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        value,
        Rect::new(rect.x + 60.0, rect.y + 37.0, rect.w - 78.0, 25.0).d2d(),
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
        Rect::new(rect.right() - 116.0, rect.y + 66.0, 92.0, 28.0).d2d(),
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
    needs_attention: bool,
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
    let status = if needs_attention {
        "Needs attention"
    } else if confirmed {
        "Ready to activate"
    } else {
        "Test before activating"
    };
    r.text(
        status,
        Rect::new(rect.x + 18.0, rect.y + 74.0, rect.w - 160.0, 18.0).d2d(),
        TextStyle::Caption,
        if needs_attention {
            BrushRole::Warning
        } else if confirmed {
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
    draw_button_style(
        r,
        Rect::new(rect.right() - 136.0, rect.y + 64.0, 116.0, 30.0),
        if selected && confirmed {
            "Activate"
        } else if selected {
            "Review"
        } else {
            "Select"
        },
        if selected && confirmed {
            ButtonStyle::Primary
        } else {
            ButtonStyle::Secondary
        },
        Interaction {
            focused: false,
            ..interaction
        },
    );
    if selected {
        r.stroke_rounded(rect.d2d(), 12.0, BrushRole::Accent, 1.25);
    }
    if interaction.focused {
        r.stroke_rounded(rect.inset(-2.0).d2d(), 14.0, BrushRole::Focus, 1.5);
    }
}

fn draw_icon_kind(r: &Renderer, rect: Rect, icon: IconKind, role: BrushRole) {
    match icon {
        IconKind::Page(page) => draw_page_icon(r, rect, page, role),
        IconKind::Speaker => draw_speaker_icon(r, rect, role),
        IconKind::Microphone => draw_microphone_icon(r, rect, role),
    }
}

pub fn draw_icon(r: &Renderer, rect: Rect, page: Page, role: BrushRole) {
    draw_icon_kind(r, rect, IconKind::Page(page), role);
}
pub fn draw_profile_name_row(
    r: &Renderer,
    element: &Element,
    name: &str,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    let surface = match state {
        InteractionState::Disabled => BrushRole::CardPressed,
        InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::CardHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,
    };
    draw_surface(r, rect, surface, state, ROW_RADIUS);
    r.text_clipped(
        "Profile name",
        Rect::new(rect.x + BODY_LEFT, rect.y + 7.0, rect.w - 190.0, 20.0).d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        name,
        Rect::new(rect.x + BODY_LEFT, rect.y + 29.0, rect.w - 190.0, 20.0).d2d(),
        TextStyle::Value,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
    draw_button_style(
        r,
        Rect::new(rect.right() - 136.0, rect.y + 12.0, 116.0, 34.0),
        "Change",
        ButtonStyle::Secondary,
        Interaction {
            focused: interaction.focused,
            ..interaction
        },
    );
}

pub(crate) fn device_value_rect(row: Rect) -> Rect {
    control_rect(row, CONTROL_WIDTH)
}
pub fn draw_device_row(
    r: &Renderer,
    element: &Element,
    presentation: &DeviceSelectionPresentation,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    let surface = match state {
        InteractionState::Disabled => BrushRole::CardPressed,
        InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::CardHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,
    };
    draw_surface(r, rect, surface, state, ROW_RADIUS);
    let value_rect = device_value_rect(rect);
    let text_width = (value_rect.x - rect.x - BODY_LEFT - 14.0).max(110.0);
    let text_role = if interaction.disabled {
        BrushRole::TextDisabled
    } else {
        BrushRole::Text
    };
    let secondary_role = if interaction.disabled {
        BrushRole::TextDisabled
    } else {
        BrushRole::TextSecondary
    };
    let stack_top = rect.y + (rect.h - 40.0) * 0.5;
    r.text_clipped(
        &element.label,
        Rect::new(rect.x + BODY_LEFT, stack_top, text_width, 20.0).d2d(),
        TextStyle::BodyStrong,
        text_role,
    );
    r.text_clipped(
        &element.description,
        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, text_width, 18.0).d2d(),
        TextStyle::Caption,
        secondary_role,
    );
    let value_state = interaction_state(interaction);
    let value_background = match value_state {
        InteractionState::Disabled | InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::ControlHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::BackgroundSubtle,
    };
    r.fill_rounded(value_rect.d2d(), CONTROL_RADIUS, value_background);
    r.stroke_rounded(
        value_rect.d2d(),
        CONTROL_RADIUS,
        if interaction.focused {
            BrushRole::Focus
        } else {
            BrushRole::Border
        },
        if interaction.focused { 1.5 } else { 1.0 },
    );
    let inner = Rect::new(
        value_rect.x + 10.0,
        value_rect.y + 5.0,
        (value_rect.w - 32.0).max(1.0),
        (value_rect.h - 10.0).max(1.0),
    );
    r.text_clipped(
        &presentation.primary,
        inner.d2d(),
        TextStyle::Value,
        text_role,
    );
    let x = value_rect.right() - 14.0;
    let y = value_rect.y + value_rect.h * 0.5;
    let chevron = if interaction.disabled {
        BrushRole::TextDisabled
    } else if interaction.focused {
        BrushRole::Focus
    } else {
        BrushRole::TextSecondary
    };
    r.line(x - 3.0, y - 2.0, x, y + 1.0, chevron, 1.25);
    r.line(x, y + 1.0, x + 3.0, y - 2.0, chevron, 1.25);
}

pub fn draw_hotkey_card(r: &Renderer, element: &Element, enabled: bool) {
    let rect = element.rect.inset(1.0);
    draw_surface(r, rect, BrushRole::Card, InteractionState::Idle, 10.0);
    let text_width = (rect.w - 222.0).max(120.0);
    let stack_top = rect.y + (rect.h - 40.0) * 0.5;
    r.text_clipped(
        &element.label,
        Rect::new(rect.x + BODY_LEFT, stack_top, text_width, 20.0).d2d(),
        TextStyle::BodyStrong,
        if enabled {
            BrushRole::Text
        } else {
            BrushRole::TextSecondary
        },
    );
    r.text_clipped(
        &element.description,
        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, text_width, 18.0).d2d(),
        TextStyle::Caption,
        BrushRole::TextSecondary,
    );
}

pub fn draw_hotkey_keycap(r: &Renderer, element: &Element, value: &str, interaction: Interaction) {
    let rect = element.rect;
    let state = interaction_state(interaction);
    r.fill_rounded(
        rect.d2d(),
        CONTROL_RADIUS,
        if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
            BrushRole::ControlHover
        } else {
            BrushRole::BackgroundSubtle
        },
    );
    r.stroke_rounded(
        rect.d2d(),
        CONTROL_RADIUS,
        if interaction.focused {
            BrushRole::Focus
        } else {
            BrushRole::BorderStrong
        },
        if interaction.focused { 1.5 } else { 1.0 },
    );
    r.text_clipped(
        value,
        rect.d2d(),
        TextStyle::Button,
        if value.starts_with("Press") {
            BrushRole::Accent
        } else {
            BrushRole::Text
        },
    );
}

pub fn draw_shortcut_card(r: &Renderer, element: &Element, value: &str, interaction: Interaction) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    let surface = match state {
        InteractionState::Disabled => BrushRole::CardPressed,
        InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::CardHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,
    };
    let keycap = value_control_rect(rect, ElementKind::Hotkey);
    draw_surface(r, rect, surface, state, 10.0);
    let text_width = (keycap.x - rect.x - BODY_LEFT - 14.0).max(120.0);
    let keycap_role = if interaction.disabled {
        BrushRole::CardPressed
    } else {
        BrushRole::BackgroundSubtle
    };
    r.fill_rounded(keycap.d2d(), CONTROL_RADIUS, keycap_role);
    r.stroke_rounded(
        keycap.d2d(),
        CONTROL_RADIUS,
        if interaction.focused {
            BrushRole::Focus
        } else {
            BrushRole::BorderStrong
        },
        if interaction.focused { 1.5 } else { 1.0 },
    );
    r.text_clipped(
        &element.label,
        Rect::new(
            rect.x + BODY_LEFT,
            rect.y + (rect.h - 40.0) * 0.5,
            text_width,
            20.0,
        )
        .d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        &element.description,
        Rect::new(
            rect.x + BODY_LEFT,
            rect.y + (rect.h - 40.0) * 0.5 + 22.0,
            text_width,
            18.0,
        )
        .d2d(),
        TextStyle::Caption,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
    r.text_clipped(
        value,
        keycap.d2d(),
        TextStyle::Button,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else if value.starts_with("Press") {
            BrushRole::Accent
        } else {
            BrushRole::Text
        },
    );
    if interaction.focused {
        r.stroke_rounded(
            keycap.inset(-3.0).d2d(),
            CONTROL_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
}

pub fn draw_choice(
    r: &Renderer,
    element: &Element,
    selected: bool,
    interaction: Interaction,
    radio: bool,
) {
    draw_labeled_choice(r, element, &element.label, selected, interaction, radio);
}

pub fn draw_labeled_choice(
    r: &Renderer,
    element: &Element,
    label: &str,
    selected: bool,
    interaction: Interaction,
    radio: bool,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    let surface = match state {
        InteractionState::Disabled => BrushRole::CardPressed,
        InteractionState::Pressed => BrushRole::CardPressed,
        InteractionState::Hovered => BrushRole::CardHover,
        InteractionState::Focused | InteractionState::Idle => BrushRole::BackgroundSubtle,
    };
    r.fill_rounded(rect.d2d(), CONTROL_RADIUS, surface);
    r.stroke_rounded(rect.d2d(), CONTROL_RADIUS, BrushRole::Border, 1.0);
    let center_x = rect.x + 20.0;
    let center_y = rect.y + rect.h * 0.5;
    let stroke = if interaction.disabled {
        BrushRole::TextDisabled
    } else if selected {
        BrushRole::Accent
    } else {
        BrushRole::BorderStrong
    };
    if radio {
        r.ellipse(center_x, center_y, 8.0, 8.0, stroke, false, 1.5);
        if selected {
            r.ellipse(center_x, center_y, 4.0, 4.0, stroke, true, 0.0);
        }
    } else {
        let box_rect = Rect::new(center_x - 8.0, center_y - 8.0, 16.0, 16.0);
        r.stroke_rounded(box_rect.d2d(), 3.0, stroke, 1.5);
        if selected {
            r.line(
                center_x - 4.0,
                center_y,
                center_x - 1.0,
                center_y + 4.0,
                stroke,
                1.8,
            );
            r.line(
                center_x - 1.0,
                center_y + 4.0,
                center_x + 5.0,
                center_y - 4.0,
                stroke,
                1.8,
            );
        }
    }
    r.text_clipped(
        label,
        Rect::new(rect.x + 40.0, rect.y + 5.0, rect.w - 54.0, rect.h - 10.0).d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    if interaction.focused {
        r.stroke_rounded(
            rect.inset(-3.0).d2d(),
            CONTROL_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
}

pub fn draw_display_route_card(
    r: &Renderer,
    element: &Element,
    primary: &str,
    detail: &str,
    selected: bool,
    available: bool,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    let fill = if selected {
        BrushRole::BackgroundSubtle
    } else if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
        BrushRole::CardHover
    } else {
        BrushRole::Card
    };
    r.fill_rounded(rect.d2d(), 10.0, fill);
    r.stroke_rounded(
        rect.d2d(),
        10.0,
        if !available {
            BrushRole::Warning
        } else if selected {
            BrushRole::Accent
        } else {
            BrushRole::Border
        },
        if selected { 1.5 } else { 1.0 },
    );
    let box_rect = Rect::new(rect.x + 16.0, rect.y + 22.0, 18.0, 18.0);
    r.stroke_rounded(
        box_rect.d2d(),
        4.0,
        if available {
            BrushRole::Accent
        } else {
            BrushRole::Warning
        },
        1.5,
    );
    if selected {
        r.line(
            rect.x + 20.0,
            rect.y + 31.0,
            rect.x + 24.0,
            rect.y + 35.0,
            BrushRole::Accent,
            1.8,
        );
        r.line(
            rect.x + 24.0,
            rect.y + 35.0,
            rect.x + 31.0,
            rect.y + 26.0,
            BrushRole::Accent,
            1.8,
        );
    }
    r.text_clipped(
        primary,
        Rect::new(rect.x + 48.0, rect.y + 10.0, rect.w - 64.0, 22.0).d2d(),
        TextStyle::BodyStrong,
        BrushRole::Text,
    );
    r.text_clipped(
        detail,
        Rect::new(rect.x + 48.0, rect.y + 35.0, rect.w - 64.0, 20.0).d2d(),
        TextStyle::Caption,
        if available {
            BrushRole::TextSecondary
        } else {
            BrushRole::Warning
        },
    );
    if interaction.focused {
        r.stroke_rounded(rect.inset(-3.0).d2d(), 13.0, BrushRole::Focus, 1.5);
    }
}

#[allow(clippy::too_many_arguments)] // Diagram content mirrors the topology choice view model.
pub fn draw_topology_choice(
    r: &Renderer,
    element: &Element,
    label: &str,
    description: &str,
    outputs: &[String],
    selected: bool,
    duplicate: bool,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    r.fill_rounded(
        rect.d2d(),
        10.0,
        if selected {
            BrushRole::BackgroundSubtle
        } else if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
            BrushRole::CardHover
        } else {
            BrushRole::Card
        },
    );
    r.stroke_rounded(
        rect.d2d(),
        10.0,
        if selected {
            BrushRole::Accent
        } else {
            BrushRole::Border
        },
        if selected { 1.5 } else { 1.0 },
    );
    let center_x = rect.x + 20.0;
    let center_y = rect.y + 25.0;
    r.ellipse(center_x, center_y, 8.0, 8.0, BrushRole::Accent, false, 1.5);
    if selected {
        r.ellipse(center_x, center_y, 4.0, 4.0, BrushRole::Accent, true, 0.0);
    }
    let diagram_x = rect.x + (rect.w * 0.36).max(190.0);
    let left_width = (diagram_x - rect.x - 56.0).max(120.0);
    r.text_clipped(
        label,
        Rect::new(rect.x + 40.0, rect.y + 10.0, left_width, 22.0).d2d(),
        TextStyle::BodyStrong,
        BrushRole::Text,
    );
    let description_height = r
        .text_height(description, TextStyle::SectionDescription, left_width, 36.0)
        .clamp(16.0, 36.0);
    r.text_clipped(
        description,
        Rect::new(rect.x + 40.0, rect.y + 39.0, left_width, description_height).d2d(),
        TextStyle::SectionDescription,
        BrushRole::TextSecondary,
    );
    let diagram = Rect::new(
        diagram_x,
        rect.y + 14.0,
        (rect.right() - diagram_x - 18.0).max(160.0),
        (rect.h - 28.0).max(100.0),
    );
    r.fill_rounded(diagram.d2d(), 8.0, BrushRole::BackgroundSubtle);
    r.stroke_rounded(diagram.d2d(), 8.0, BrushRole::BorderStrong, 1.0);
    let tile_area = Rect::new(
        diagram.x + 10.0,
        diagram.y + 10.0,
        diagram.w - 20.0,
        diagram.h - 38.0,
    );
    if duplicate {
        r.fill_rounded(tile_area.d2d(), 7.0, BrushRole::Card);
        r.stroke_rounded(tile_area.d2d(), 7.0, BrushRole::BorderStrong, 1.0);
        let names = if outputs.is_empty() {
            "Selected screens".to_string()
        } else {
            outputs
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(" + ")
        };
        r.text_clipped(
            &names,
            Rect::new(
                tile_area.x + 12.0,
                tile_area.y + 13.0,
                tile_area.w - 24.0,
                tile_area.h - 34.0,
            )
            .d2d(),
            TextStyle::BodyStrong,
            BrushRole::Text,
        );
        r.line(
            tile_area.x + 12.0,
            tile_area.bottom() - 22.0,
            tile_area.right() - 12.0,
            tile_area.bottom() - 22.0,
            BrushRole::Border,
            1.0,
        );
        r.text_clipped(
            "Same picture",
            Rect::new(
                diagram.x + 10.0,
                diagram.bottom() - 24.0,
                diagram.w - 20.0,
                16.0,
            )
            .d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    } else {
        let gap = 10.0;
        let tile_w = ((tile_area.w - gap) * 0.5).max(64.0);
        let tile_h = tile_area.h;
        for index in 0..2 {
            let tile = Rect::new(
                tile_area.x + index as f32 * (tile_w + gap),
                tile_area.y,
                tile_w,
                tile_h,
            );
            r.fill_rounded(tile.d2d(), 7.0, BrushRole::Card);
            r.stroke_rounded(tile.d2d(), 7.0, BrushRole::BorderStrong, 1.0);
            let name = outputs
                .get(index)
                .map(String::as_str)
                .unwrap_or("Saved screen");
            r.text_clipped(
                name,
                Rect::new(tile.x + 8.0, tile.y + 12.0, tile.w - 16.0, 28.0).d2d(),
                TextStyle::BodyStrong,
                BrushRole::Text,
            );
            r.line(
                tile.x + 8.0,
                tile.bottom() - 28.0,
                tile.right() - 8.0,
                tile.bottom() - 28.0,
                BrushRole::Border,
                1.0,
            );
            r.text(
                &format!("Screen {}", index + 1),
                Rect::new(tile.x + 8.0, tile.bottom() - 23.0, tile.w - 16.0, 16.0).d2d(),
                TextStyle::Caption,
                BrushRole::TextSecondary,
            );
        }
        r.text_clipped(
            "Separate desktops",
            Rect::new(
                diagram.x + 10.0,
                diagram.bottom() - 24.0,
                diagram.w - 20.0,
                16.0,
            )
            .d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }
    if interaction.focused {
        r.stroke_rounded(rect.inset(-3.0).d2d(), 13.0, BrushRole::Focus, 1.5);
    }
}

pub fn draw_position_cell(
    r: &Renderer,
    element: &Element,
    label: &str,
    selected: bool,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let fill = if selected {
        BrushRole::Accent
    } else if interaction.hovered || interaction.pressed {
        BrushRole::CardHover
    } else {
        BrushRole::BackgroundSubtle
    };
    r.fill_rounded(rect.d2d(), 7.0, fill);
    r.stroke_rounded(
        rect.d2d(),
        7.0,
        if selected {
            BrushRole::Accent
        } else {
            BrushRole::Border
        },
        1.0,
    );
    r.text(
        label,
        rect.d2d(),
        TextStyle::ButtonSmall,
        if selected {
            BrushRole::AccentText
        } else {
            BrushRole::TextSecondary
        },
    );
    if interaction.focused {
        r.stroke_rounded(rect.inset(-3.0).d2d(), 10.0, BrushRole::Focus, 1.5);
    }
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
        // Focus belongs to the actionable child (switch, picker, or button),
        // not to the full setting surface. This prevents nested cyan outlines.
        InteractionState::Focused => {}
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
    r.fill_rounded(
        Rect::new(cx - 10.0, cy - 3.0, 5.0, 6.0).d2d(),
        1.5,
        BrushRole::AccentText,
    );
    r.line(
        cx - 5.0,
        cy - 3.0,
        cx + 2.0,
        cy - 9.0,
        BrushRole::AccentText,
        2.5,
    );
    r.line(
        cx - 5.0,
        cy + 3.0,
        cx + 2.0,
        cy + 9.0,
        BrushRole::AccentText,
        2.5,
    );
    r.line(
        cx + 2.0,
        cy - 9.0,
        cx + 2.0,
        cy + 9.0,
        BrushRole::AccentText,
        2.5,
    );
    r.line(
        cx + 7.0,
        cy - 5.0,
        cx + 10.0,
        cy - 2.0,
        BrushRole::AccentText,
        1.8,
    );
    r.line(
        cx + 10.0,
        cy - 2.0,
        cx + 10.0,
        cy + 2.0,
        BrushRole::AccentText,
        1.8,
    );
    r.line(
        cx + 10.0,
        cy + 2.0,
        cx + 7.0,
        cy + 5.0,
        BrushRole::AccentText,
        1.8,
    );
}

pub(crate) fn scrollbar_thumb_rect(viewport: Rect, scroll: f32, max_scroll: f32) -> Option<Rect> {
    if max_scroll <= 0.0 || viewport.h <= 0.0 {
        return None;
    }
    let total = viewport.h + max_scroll;
    let thumb_h = (viewport.h * viewport.h / total).clamp(40.0, viewport.h - 8.0);
    let travel = (viewport.h - thumb_h - 8.0).max(0.0);
    let ratio = (scroll / max_scroll).clamp(0.0, 1.0);
    Some(Rect::new(
        viewport.right() - 10.0,
        viewport.y + 4.0 + travel * ratio,
        6.0,
        thumb_h,
    ))
}

pub(crate) fn scrollbar_hit_rect(viewport: Rect) -> Rect {
    Rect::new(viewport.right() - 16.0, viewport.y, 16.0, viewport.h)
}

pub(crate) fn scroll_from_scrollbar_pointer(
    viewport: Rect,
    pointer_y: f32,
    pointer_offset: f32,
    max_scroll: f32,
) -> f32 {
    let Some(thumb) = scrollbar_thumb_rect(viewport, 0.0, max_scroll) else {
        return 0.0;
    };
    let travel = (viewport.h - thumb.h - 8.0).max(0.0);
    if travel <= f32::EPSILON {
        return 0.0;
    }
    let top = (pointer_y - pointer_offset).clamp(viewport.y + 4.0, viewport.y + 4.0 + travel);
    ((top - viewport.y - 4.0) / travel * max_scroll).clamp(0.0, max_scroll)
}

pub fn draw_scrollbar(r: &Renderer, viewport: Rect, scroll: f32, max_scroll: f32) {
    let Some(thumb) = scrollbar_thumb_rect(viewport, scroll, max_scroll) else {
        return;
    };
    r.fill_rounded(thumb.d2d(), 3.0, BrushRole::BorderStrong);
}

fn control_rect(row: Rect, width: f32) -> Rect {
    Rect::new(
        row.right() - 18.0 - width,
        row.y + (row.h - 34.0) * 0.5,
        width,
        34.0,
    )
}

pub(crate) fn value_control_rect(row: Rect, _kind: ElementKind) -> Rect {
    control_rect(row, CONTROL_WIDTH)
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

pub(crate) fn slider_track_rect(row: Rect) -> Rect {
    let rect = row.inset(1.0);
    let label_end = (rect.x + rect.w * 0.34).clamp(rect.x + 150.0, rect.x + 230.0);
    let value_width = 74.0;
    let right = rect.right() - 18.0 - value_width;
    Rect::new(
        label_end,
        rect.y + rect.h * 0.5 - 3.0,
        (right - label_end - 14.0).max(100.0),
        6.0,
    )
}

fn draw_slider_cluster(
    r: &Renderer,
    element: &Element,
    ratio: f32,
    label: &str,
    interaction: Interaction,
) {
    let rect = element.rect.inset(1.0);
    let state = interaction_state(Interaction {
        focused: false,
        ..interaction
    });
    draw_surface(
        r,
        rect,
        if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {
            BrushRole::CardHover
        } else {
            BrushRole::Card
        },
        state,
        ROW_RADIUS,
    );
    let stack_top = rect.y + (rect.h - 40.0) * 0.5;
    r.text_clipped(
        &element.label,
        Rect::new(rect.x + BODY_LEFT, stack_top, 150.0, 20.0).d2d(),
        TextStyle::BodyStrong,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    r.text_clipped(
        &element.description,
        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, 150.0, 18.0).d2d(),
        TextStyle::Caption,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::TextSecondary
        },
    );
    let track = slider_track_rect(element.rect);
    r.fill_rounded(
        track.d2d(),
        3.0,
        if interaction.disabled {
            BrushRole::Border
        } else {
            BrushRole::BorderStrong
        },
    );
    let filled = Rect::new(track.x, track.y, track.w * ratio.clamp(0.0, 1.0), track.h);
    if filled.w > 0.0 {
        r.fill_rounded(
            filled.d2d(),
            3.0,
            if interaction.disabled {
                BrushRole::BorderStrong
            } else if matches!(state, InteractionState::Pressed) {
                BrushRole::AccentPressed
            } else {
                BrushRole::Accent
            },
        );
    }
    let knob_x = track.x + track.w * ratio.clamp(0.0, 1.0);
    let radius = if matches!(state, InteractionState::Pressed) {
        8.0
    } else {
        7.0
    };
    r.ellipse(
        knob_x,
        track.y + track.h * 0.5,
        radius,
        radius,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Accent
        },
        true,
        0.0,
    );
    let value_rect = Rect::new(rect.right() - 88.0, rect.y, 70.0, rect.h);
    r.text_clipped(
        label,
        value_rect.d2d(),
        TextStyle::CaptionRight,
        if interaction.disabled {
            BrushRole::TextDisabled
        } else {
            BrushRole::Text
        },
    );
    if interaction.focused {
        r.stroke_rounded(
            rect.inset(-2.0).d2d(),
            ROW_RADIUS + 2.0,
            BrushRole::Focus,
            1.5,
        );
    }
}

fn page_for_nav(id: crate::ui::layout::ElementId) -> Page {
    match id {
        crate::ui::layout::ElementId::Nav(page) => page,
        _ => Page::Home,
    }
}

fn draw_speaker_icon(r: &Renderer, rect: Rect, role: BrushRole) {
    let cy = rect.y + rect.h * 0.5;
    let scale = rect.w.min(rect.h) / 20.0;
    let x = rect.x + 1.0 * scale;
    r.fill_rounded(
        Rect::new(x, cy - 2.5 * scale, 3.5 * scale, 5.0 * scale).d2d(),
        scale,
        role,
    );
    r.line(
        x + 3.0 * scale,
        cy - 2.5 * scale,
        x + 8.0 * scale,
        cy - 6.5 * scale,
        role,
        1.7 * scale,
    );
    r.line(
        x + 3.0 * scale,
        cy + 2.5 * scale,
        x + 8.0 * scale,
        cy + 6.5 * scale,
        role,
        1.7 * scale,
    );
    r.line(
        x + 8.0 * scale,
        cy - 6.5 * scale,
        x + 8.0 * scale,
        cy + 6.5 * scale,
        role,
        1.7 * scale,
    );
    r.line(
        x + 12.0 * scale,
        cy - 4.0 * scale,
        x + 15.0 * scale,
        cy - 1.5 * scale,
        role,
        1.4 * scale,
    );
    r.line(
        x + 15.0 * scale,
        cy - 1.5 * scale,
        x + 15.0 * scale,
        cy + 1.5 * scale,
        role,
        1.4 * scale,
    );
    r.line(
        x + 15.0 * scale,
        cy + 1.5 * scale,
        x + 12.0 * scale,
        cy + 4.0 * scale,
        role,
        1.4 * scale,
    );
}

fn draw_microphone_icon(r: &Renderer, rect: Rect, role: BrushRole) {
    let size = rect.w.min(rect.h);
    let scale = size / 28.0;
    let cx = rect.x + rect.w * 0.5;
    let cy = rect.y + rect.h * 0.5;
    r.stroke_rounded(
        Rect::new(
            cx - 4.0 * scale,
            cy - 10.0 * scale,
            8.0 * scale,
            14.0 * scale,
        )
        .d2d(),
        4.0 * scale,
        role,
        1.7 * scale,
    );
    let stroke = 1.7 * scale;
    for (x1, y1, x2, y2) in [
        (-8.0, 1.0, -8.0, 3.0),
        (-8.0, 3.0, -6.0, 6.0),
        (-6.0, 6.0, 0.0, 8.0),
        (0.0, 8.0, 6.0, 6.0),
        (6.0, 6.0, 8.0, 3.0),
        (8.0, 3.0, 8.0, 1.0),
        (0.0, 8.0, 0.0, 11.0),
        (-4.0, 11.0, 4.0, 11.0),
    ] {
        r.line(
            cx + x1 * scale,
            cy + y1 * scale,
            cx + x2 * scale,
            cy + y2 * scale,
            role,
            stroke,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ShortcutIconGeometry {
    pub outer: Rect,
    pub upper_keys: [Rect; 4],
    pub spacebar: Rect,
}

pub(crate) fn shortcut_icon_geometry(rect: Rect) -> ShortcutIconGeometry {
    let outer = Rect::new(rect.x + 1.5, rect.y + 3.5, rect.w - 3.0, rect.h - 7.0);
    let key_width = (rect.w * 0.11).max(1.5);
    let key_left = rect.x + rect.w * 0.25;
    let key_right = rect.right() - rect.w * 0.25;
    let key_gap = ((key_right - key_left) - key_width * 4.0) / 3.0;
    let upper_y = rect.y + rect.h * 0.43;
    let upper_keys = std::array::from_fn(|index| {
        Rect::new(
            key_left + index as f32 * (key_width + key_gap),
            upper_y,
            key_width,
            1.5,
        )
    });
    let spacebar = Rect::new(
        rect.x + rect.w * 0.30,
        rect.y + rect.h * 0.70,
        rect.w * 0.40,
        1.5,
    );
    ShortcutIconGeometry {
        outer,
        upper_keys,
        spacebar,
    }
}

fn draw_shortcut_icon(r: &Renderer, rect: Rect, role: BrushRole) {
    let geometry = shortcut_icon_geometry(rect);
    r.stroke_rounded(geometry.outer.d2d(), 3.0, role, 1.5);
    for key in geometry.upper_keys {
        r.fill_rounded(key.d2d(), 0.75, role);
    }
    r.fill_rounded(geometry.spacebar.d2d(), 0.75, role);
}

fn draw_audio_icon(r: &Renderer, rect: Rect, role: BrushRole) {
    draw_microphone_icon(r, rect, role);
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
        Page::Shortcuts => draw_shortcut_icon(r, rect, role),
        Page::Audio => draw_audio_icon(r, rect, role),
        Page::Workspaces => {
            r.stroke_rounded(
                Rect::new(rect.x + 3.0, rect.y + 3.0, rect.w - 6.0, 7.0).d2d(),
                2.0,
                role,
                1.5,
            );
            r.stroke_rounded(
                Rect::new(rect.x + 3.0, rect.y + 10.0, rect.w - 6.0, 7.0).d2d(),
                2.0,
                role,
                1.5,
            );
        }
        Page::Displays => {
            r.stroke_rounded(
                Rect::new(rect.x + 3.0, rect.y + 3.0, rect.w - 6.0, rect.h - 9.0).d2d(),
                2.0,
                role,
                1.5,
            );
            r.line(cx, rect.bottom() - 6.0, cx, rect.bottom() - 2.0, role, 1.5);
            r.line(
                rect.x + 6.0,
                rect.bottom() - 2.0,
                rect.right() - 6.0,
                rect.bottom() - 2.0,
                role,
                1.5,
            );
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
        device_value_rect, interaction_state, scroll_from_scrollbar_pointer, scrollbar_hit_rect,
        scrollbar_thumb_rect, section_accent_rect, section_title_text_rect, shortcut_icon_geometry,
        titlebar_glyph_bounds, value_control_rect, value_text_rect, Interaction, InteractionState,
        CONTROL_WIDTH, VALUE_TEXT_PADDING,
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
    fn value_control_geometry_is_right_aligned_and_shared_across_kinds() {
        let row = Rect::new(24.0, 100.0, 560.0, 58.0);
        let value = value_control_rect(row, ElementKind::Value);
        let hotkey = value_control_rect(row, ElementKind::Hotkey);
        assert_eq!(value.w, CONTROL_WIDTH);
        assert_eq!(hotkey.w, CONTROL_WIDTH);
        assert_eq!(value.x, hotkey.x);
        assert_eq!(value.right(), row.right() - 18.0);
    }

    #[test]
    fn device_value_geometry_uses_the_shared_control_column() {
        let wide = Rect::new(24.0, 100.0, 800.0, 58.0);
        let narrow = Rect::new(24.0, 100.0, 330.0, 58.0);
        assert_eq!(device_value_rect(wide).w, CONTROL_WIDTH);
        assert_eq!(device_value_rect(narrow).w, CONTROL_WIDTH);
        assert_eq!(device_value_rect(wide).right(), wide.right() - 18.0);
        assert_eq!(device_value_rect(narrow).right(), narrow.right() - 18.0);
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
    #[test]
    fn scrollbar_drag_maps_thumb_centers_to_scroll_extremes() {
        let viewport = Rect::new(200.0, 100.0, 400.0, 300.0);
        let max_scroll = 600.0;
        let start = scrollbar_thumb_rect(viewport, 0.0, max_scroll).expect("start thumb");
        let end = scrollbar_thumb_rect(viewport, max_scroll, max_scroll).expect("end thumb");
        assert!(end.y > start.y);

        let offset = start.h * 0.5;
        let start_scroll =
            scroll_from_scrollbar_pointer(viewport, start.y + offset, offset, max_scroll);
        let end_scroll =
            scroll_from_scrollbar_pointer(viewport, end.y + end.h * 0.5, offset, max_scroll);
        assert!(start_scroll.abs() < f32::EPSILON);
        assert!((end_scroll - max_scroll).abs() < f32::EPSILON);

        let hit = scrollbar_hit_rect(viewport);
        assert!(hit.contains(viewport.right() - 8.0, viewport.y + 20.0));
        assert!(!hit.contains(viewport.right() - 20.0, viewport.y + 20.0));
    }
    #[test]
    fn shortcut_icon_geometry_has_keyboard_rows_and_spacebar() {
        let geometry = shortcut_icon_geometry(Rect::new(0.0, 0.0, 20.0, 20.0));
        assert!(geometry.outer.w > 14.0);
        assert!(geometry.outer.h > 8.0);
        assert!(geometry
            .upper_keys
            .iter()
            .all(|key| key.w > key.h && key.w >= 1.5));
        assert!(geometry.spacebar.w > geometry.upper_keys[0].w * 2.0);
        assert!(geometry.spacebar.y > geometry.upper_keys[0].y);
    }
    #[test]
    fn titlebar_glyph_bounds_are_compact_inside_full_hit_target() {
        let button = Rect::new(900.0, 2.0, 44.0, 28.0);
        let glyph = titlebar_glyph_bounds(button);
        assert_eq!(glyph.w, 12.0);
        assert_eq!(glyph.h, 12.0);
        assert!(glyph.x > button.x);
        assert!(glyph.right() < button.right());
        assert!(glyph.y > button.y);
        assert!(glyph.bottom() < button.bottom());
    }
    #[test]
    fn section_accent_is_centered_on_title_text_geometry() {
        let heading = Rect::new(40.0, 120.0, 620.0, 88.0);
        let title = section_title_text_rect(heading);
        let accent = section_accent_rect(heading);
        assert_eq!(accent.x, heading.x);
        assert_eq!(accent.y + accent.h * 0.5, title.y + title.h * 0.5);
        assert!(heading.y + 8.0 < title.y);
    }
}
