//! Pages for the control center.

use super::overlay_preview::{overlay_position, overlay_position_label};
use super::state::SettingsUi;
use crate::ui::controls;
use crate::ui::controls::ControlValue;
use crate::ui::layout::{ElementId, ElementKind};
use crate::ui::navigation::Page;
use crate::ui::renderer::Renderer;

impl SettingsUi {
    pub(super) fn draw_page(&self, renderer: &Renderer) {
        self.draw_visual_regions(renderer);
        for element in &self.layout.elements {
            if !element.scrolls || !self.layout.content_clip.intersects(element.rect) {
                continue;
            }
            let interaction = self.interaction(element.id, self.is_disabled(element.id));
            self.draw_element(renderer, element, interaction);
        }
        if self.page == Page::Home && !self.runtime.degraded.is_empty() {
            self.draw_degraded_summary(renderer);
        }
    }

    fn draw_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        match element.id {
            ElementId::HomeSpeaker
            | ElementId::HomeMicrophone
            | ElementId::HomeCurrentDesktop
            | ElementId::HomePreviousDesktop
            | ElementId::HomeSpecial
            | ElementId::HomeDisplayProfile
            | ElementId::HomeShortcutHealth
            | ElementId::HomeDiagnostics => {
                self.draw_home_element(renderer, element, interaction);
            }
            ElementId::DisplayProfileCard(_)
            | ElementId::DisplayOutputCard(_)
            | ElementId::DisplayTopologyChoice(_)
            | ElementId::RenameDisplayProfile
            | ElementId::DisplayWizardBack
            | ElementId::DisplayWizardNext
            | ElementId::DisplayWizardCancel
            | ElementId::EditDisplayProfile
            | ElementId::NewDisplayProfile
            | ElementId::UpdateDisplayProfile
            | ElementId::DuplicateDisplayProfile
            | ElementId::DeleteDisplayProfile
            | ElementId::TestApplyDisplayProfile
            | ElementId::ApplyDisplayProfile
            | ElementId::KeepDisplayChange
            | ElementId::UndoDisplayChange
            | ElementId::DiscardDisplayEdits => {
                self.draw_display_element(renderer, element, interaction);
            }
            ElementId::HotkeyCard(_)
            | ElementId::HotkeyEnabled(_)
            | ElementId::HotkeyUnassign(_)
            | ElementId::MicHotkey
            | ElementId::OutputHotkey
            | ElementId::ForegroundHotkey
            | ElementId::CycleInputHotkey
            | ElementId::CycleOutputHotkey
            | ElementId::ForegroundVolumeUpHotkey
            | ElementId::ForegroundVolumeDownHotkey
            | ElementId::PreviousDesktopHotkey
            | ElementId::AssignScratchpadHotkey
            | ElementId::ToggleScratchpadHotkey
            | ElementId::DisplayProfileHotkey => {
                self.draw_shortcut_element(renderer, element, interaction);
            }
            ElementId::InputDevice
            | ElementId::OutputDevice
            | ElementId::InputCycleMode(_)
            | ElementId::OutputCycleMode(_)
            | ElementId::InputCycleDevice(_)
            | ElementId::OutputCycleDevice(_) => {
                self.draw_audio_element(renderer, element, interaction);
            }
            ElementId::OverlayPositionCell(index) => controls::draw_position_cell(
                renderer,
                element,
                overlay_position_label(index as usize),
                self.draft.overlay.position == overlay_position(index as usize),
                interaction,
            ),
            ElementId::OverlayPreview => {
                self.paint_display_wizard_back(renderer, element, interaction);
            }
            _ if matches!(element.kind, ElementKind::Card | ElementKind::Info) => {}
            _ if element.id.is_shell_chrome() => {}
            _ => controls::draw_row(renderer, element, self.value_for(element.id), interaction),
        }
    }

    fn draw_home_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        match element.id {
            ElementId::HomeSpeaker => self.paint_home_speaker(renderer, element, interaction),
            ElementId::HomeMicrophone => {
                self.paint_home_microphone(renderer, element, interaction);
            }
            ElementId::HomeCurrentDesktop => {
                let value = self.runtime.desktop.current_desktop.map_or_else(
                    || "Desktop status unavailable".into(),
                    |index| format!("Desktop {}", index + 1),
                );
                controls::draw_home_card(
                    renderer,
                    element.rect,
                    "Current desktop",
                    &value,
                    "Normal workspace",
                    "Open",
                    controls::IconKind::Page(Page::Workspaces),
                    interaction,
                );
            }
            ElementId::HomePreviousDesktop => {
                controls::draw_row(renderer, element, self.value_for(element.id), interaction);
            }
            ElementId::HomeSpecial => {
                let (value, detail, action) = self.special_workspace_summary();
                controls::draw_home_card(
                    renderer,
                    element.rect,
                    "Special Desktop",
                    &value,
                    &detail,
                    &action,
                    controls::IconKind::Page(Page::Workspaces),
                    interaction,
                );
            }
            ElementId::HomeDisplayProfile => {
                self.paint_home_display_profile(renderer, element, interaction);
            }
            ElementId::HomeShortcutHealth => {
                let (value, detail, action) = self.shortcut_health_copy();
                controls::draw_home_card(
                    renderer,
                    element.rect,
                    "Shortcuts",
                    &value,
                    &detail,
                    &action,
                    controls::IconKind::Page(Page::Shortcuts),
                    interaction,
                );
            }
            ElementId::HomeDiagnostics => {
                let (title, value, detail) = self.home_diagnostics_copy();
                controls::draw_home_card(
                    renderer,
                    element.rect,
                    &title,
                    &value,
                    &detail,
                    "Open",
                    controls::IconKind::Page(Page::System),
                    interaction,
                );
            }
            _ => {}
        }
    }

    fn draw_display_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        match element.id {
            ElementId::DisplayProfileCard(index) => {
                if let Some(card) = self.profile_card_data(index as usize) {
                    controls::draw_profile_card(renderer, element.rect, &card, interaction);
                }
            }
            ElementId::DisplayOutputCard(index) => {
                if let Some(card) = self.display_output_card_data(index as usize) {
                    controls::draw_display_route_card(renderer, element, &card, interaction);
                }
            }
            ElementId::DisplayTopologyChoice(index) => {
                self.paint_display_topology_choice(renderer, element, interaction, index);
            }
            ElementId::RenameDisplayProfile => {
                let name = self
                    .draft
                    .display_profiles
                    .active()
                    .map(|profile| profile.name.as_str())
                    .unwrap_or("No profile selected");
                controls::draw_profile_name_row(renderer, element, name, interaction);
            }
            _ => self.paint_display_wizard_back(renderer, element, interaction),
        }
    }

    fn draw_shortcut_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        match element.id {
            ElementId::HotkeyCard(slot) => {
                controls::draw_hotkey_card(renderer, element, self.hotkey_enabled(slot));
            }
            ElementId::HotkeyEnabled(_) | ElementId::HotkeyUnassign(_) => {
                let label = match self.value_for(element.id) {
                    ControlValue::Action(value) => value.into_owned(),
                    _ => element.label.clone(),
                };
                controls::draw_button_style(
                    renderer,
                    element.rect,
                    &label,
                    controls::ButtonStyle::Secondary,
                    interaction,
                );
            }
            _ => {
                let value = match self.value_for(element.id) {
                    ControlValue::Text(value) => value.into_owned(),
                    _ => String::new(),
                };
                controls::draw_hotkey_keycap(renderer, element, &value, interaction);
            }
        }
    }

    fn draw_audio_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        match element.id {
            ElementId::InputDevice => {
                let presentation = self
                    .audio_view()
                    .selection(crate::audio::DeviceCycleFlow::Input);
                controls::draw_device_row(renderer, element, &presentation, interaction);
            }
            ElementId::OutputDevice => {
                let presentation = self
                    .audio_view()
                    .selection(crate::audio::DeviceCycleFlow::Output);
                controls::draw_device_row(renderer, element, &presentation, interaction);
            }
            ElementId::InputCycleMode(_) | ElementId::OutputCycleMode(_) => controls::draw_choice(
                renderer,
                element,
                self.choice_selected(element.id),
                interaction,
                true,
            ),
            ElementId::InputCycleDevice(index) => {
                self.paint_input_cycle_device(renderer, element, interaction, index);
            }
            ElementId::OutputCycleDevice(index) => {
                self.paint_output_cycle_device(renderer, element, interaction, index);
            }
            _ => {}
        }
    }
}

impl SettingsUi {
    fn paint_home_speaker(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        {
            let detail = match &self.runtime.output {
                crate::audio::OutputState::Current { muted: true, .. } => "Muted".into(),
                crate::audio::OutputState::Current { volume_pct, .. } => {
                    format!("{volume_pct}% volume")
                }
                crate::audio::OutputState::Unavailable { .. } => {
                    "Windows Audio is not available".into()
                }
            };
            controls::draw_home_card(
                renderer,
                element.rect,
                "Speakers",
                &self.audio_view().current_output_name(),
                &detail,
                "Choose",
                controls::IconKind::Speaker,
                interaction,
            );
        }
    }

    fn paint_home_microphone(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        {
            let detail = match &self.runtime.microphone {
                crate::audio::AudioState::Muted { volume_pct } => {
                    format!("Muted · {volume_pct}% input volume")
                }
                crate::audio::AudioState::Active { volume_pct } => {
                    format!("{volume_pct}% input volume")
                }
                crate::audio::AudioState::Unavailable { .. } => {
                    "Windows Audio is not available".into()
                }
            };
            controls::draw_home_card(
                renderer,
                element.rect,
                "Microphone",
                &self.audio_view().current_input_name(),
                &detail,
                "Choose",
                controls::IconKind::Microphone,
                interaction,
            );
        }
    }

    fn paint_home_display_profile(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        {
            let (name, detail) = self.display_summary();
            let compact_detail = detail
                .split_once(" · ")
                .map_or(detail.as_str(), |(summary, _)| summary);
            controls::draw_home_card(
                renderer,
                element.rect,
                "Display",
                &name,
                compact_detail,
                if self.draft.display_profiles.profiles.is_empty() {
                    "Set up"
                } else {
                    "Open"
                },
                controls::IconKind::Page(Page::Displays),
                interaction,
            );
        }
    }

    fn paint_display_topology_choice(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        index: u8,
    ) {
        {
            let topology = if index == 0 {
                crate::display::DisplayTopology::Extend
            } else {
                crate::display::DisplayTopology::Clone
            };
            let selected = self
                .draft
                .display_profiles
                .active()
                .is_some_and(|profile| profile.topology == topology);
            let output_names = self.selected_display_names();
            controls::draw_topology_choice(
                renderer,
                element,
                topology.label(),
                &element.description,
                &output_names,
                selected,
                topology == crate::display::DisplayTopology::Clone,
                interaction,
            );
        }
    }

    fn paint_input_cycle_device(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        index: u8,
    ) {
        {
            let selected = self
                .audio_view()
                .cycle_device_selected(crate::audio::DeviceCycleFlow::Input, index as usize);
            let label = self
                .audio_view()
                .cycle_device_label(crate::audio::DeviceCycleFlow::Input, index as usize);
            controls::draw_labeled_choice(renderer, element, &label, selected, interaction, false);
        }
    }

    fn paint_output_cycle_device(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        index: u8,
    ) {
        {
            let selected = self
                .audio_view()
                .cycle_device_selected(crate::audio::DeviceCycleFlow::Output, index as usize);
            let label = self
                .audio_view()
                .cycle_device_label(crate::audio::DeviceCycleFlow::Output, index as usize);
            controls::draw_labeled_choice(renderer, element, &label, selected, interaction, false);
        }
    }

    fn paint_display_wizard_back(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        {
            if (element.id == ElementId::NewDisplayProfile
                || element.id == ElementId::TestApplyDisplayProfile)
                && element.rect.h > 40.0
            {
                controls::draw_row(renderer, element, self.value_for(element.id), interaction);
            } else {
                let label = match self.value_for(element.id) {
                    ControlValue::Action(value) => value.into_owned(),
                    _ => element.label.clone(),
                };
                controls::draw_button_style(
                    renderer,
                    element.rect,
                    &label,
                    if element.id == ElementId::DeleteDisplayProfile
                        || element.id == ElementId::DiscardDisplayEdits
                    {
                        controls::ButtonStyle::Danger
                    } else if matches!(
                        element.id,
                        ElementId::NewDisplayProfile
                            | ElementId::TestApplyDisplayProfile
                            | ElementId::KeepDisplayChange
                            | ElementId::OverlayPreview
                            | ElementId::DisplayWizardNext
                    ) {
                        controls::ButtonStyle::Primary
                    } else {
                        controls::ButtonStyle::Secondary
                    },
                    interaction,
                );
            }
        }
    }
}
