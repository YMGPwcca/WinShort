//! Onboarding for the layout.

use super::builder::{add_element, add_heading, add_hotkey_grid, add_row, add_section_content_gap};
use super::geometry::Rect;
use super::model::{ElementId, ElementKind};
use super::shell::SettingsLayout;

pub(super) fn add_onboarding(layout: &mut SettingsLayout, step: u8) {
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
