//! Pages for the control center.

use super::overlay_preview::{overlay_position, overlay_position_label};
use super::state::SettingsUi;
use crate::ui::controls;
use crate::ui::controls::ControlValue;
use crate::ui::layout::{
    AudioElement, DisplayElement, ElementDomain, ElementKind, HomeElement, OverlayElement,
    ShellElement, ShortcutElement, SystemElement, WorkspaceElement,
};
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
        match element.id.domain() {
            ElementDomain::Shell(domain) => {
                self.draw_shell_element(renderer, element, interaction, domain)
            }
            ElementDomain::Home(domain) => {
                self.draw_home_element(renderer, element, interaction, domain)
            }
            ElementDomain::Audio(domain) => {
                self.draw_audio_element(renderer, element, interaction, domain)
            }
            ElementDomain::Displays(domain) => {
                self.draw_display_element(renderer, element, interaction, domain)
            }
            ElementDomain::Shortcuts(domain) => {
                self.draw_shortcut_element(renderer, element, interaction, domain)
            }
            ElementDomain::Workspaces(domain) => {
                self.draw_workspace_element(renderer, element, interaction, domain)
            }
            ElementDomain::Overlay(domain) => {
                self.draw_overlay_element(renderer, element, interaction, domain)
            }
            ElementDomain::System(domain) => {
                self.draw_system_element(renderer, element, interaction, domain)
            }
        }
    }

    fn draw_standard_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        match element.kind {
            ElementKind::Card | ElementKind::Info => {}
            _ => controls::draw_row(renderer, element, self.value_for(element.id), interaction),
        }
    }

    fn draw_shell_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        domain: ShellElement,
    ) {
        match domain {
            ShellElement::Search
            | ShellElement::Nav(_)
            | ShellElement::SearchResult(_)
            | ShellElement::WindowClose => {}
            ShellElement::OnboardingContinue | ShellElement::OnboardingOpen => {
                self.draw_standard_element(renderer, element, interaction)
            }
        }
    }

    fn draw_home_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        domain: HomeElement,
    ) {
        match domain {
            HomeElement::Speaker => self.paint_home_speaker(renderer, element, interaction),
            HomeElement::Microphone => self.paint_home_microphone(renderer, element, interaction),
            HomeElement::CurrentDesktop => {
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
            HomeElement::PreviousDesktop => {
                self.draw_standard_element(renderer, element, interaction);
            }
            HomeElement::Special => {
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
            HomeElement::DisplayProfile => {
                self.paint_home_display_profile(renderer, element, interaction);
            }
            HomeElement::ShortcutHealth => {
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
            HomeElement::Diagnostics => {
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
        }
    }

    fn draw_audio_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        domain: AudioElement,
    ) {
        match domain {
            AudioElement::InputDevice => {
                let presentation = self
                    .audio_view()
                    .selection(crate::audio::DeviceCycleFlow::Input);
                controls::draw_device_row(renderer, element, &presentation, interaction);
            }
            AudioElement::OutputDevice => {
                let presentation = self
                    .audio_view()
                    .selection(crate::audio::DeviceCycleFlow::Output);
                controls::draw_device_row(renderer, element, &presentation, interaction);
            }
            AudioElement::InputCycleMode(_) | AudioElement::OutputCycleMode(_) => {
                controls::draw_choice(
                    renderer,
                    element,
                    self.choice_selected(element.id),
                    interaction,
                    true,
                );
            }
            AudioElement::InputCycleDevice(index) => {
                self.paint_input_cycle_device(renderer, element, interaction, index);
            }
            AudioElement::OutputCycleDevice(index) => {
                self.paint_output_cycle_device(renderer, element, interaction, index);
            }
            AudioElement::InputAllowlist
            | AudioElement::OutputAllowlist
            | AudioElement::InputRole
            | AudioElement::OutputRole => {
                self.draw_standard_element(renderer, element, interaction);
            }
        }
    }

    fn draw_display_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        domain: DisplayElement,
    ) {
        match domain {
            DisplayElement::ProfileCard(index) => {
                if let Some(card) = self.profile_card_data(index as usize) {
                    controls::draw_profile_card(renderer, element.rect, &card, interaction);
                }
            }
            DisplayElement::OutputCard(index) => {
                if let Some(card) = self.display_output_card_data(index as usize) {
                    controls::draw_display_route_card(renderer, element, &card, interaction);
                }
            }
            DisplayElement::TopologyChoice(index) => {
                self.paint_display_topology_choice(renderer, element, interaction, index);
            }
            DisplayElement::RenameProfile => {
                let name = self
                    .draft
                    .display_profiles
                    .active()
                    .map(|profile| profile.name.as_str())
                    .unwrap_or("No profile selected");
                controls::draw_profile_name_row(renderer, element, name, interaction);
            }
            DisplayElement::NewProfile | DisplayElement::TestApply => self.paint_action_element(
                renderer,
                element,
                interaction,
                controls::ButtonStyle::Primary,
                true,
            ),
            DisplayElement::WizardNext | DisplayElement::KeepChange => self.paint_action_element(
                renderer,
                element,
                interaction,
                controls::ButtonStyle::Primary,
                false,
            ),
            DisplayElement::DeleteProfile | DisplayElement::DiscardEdits => self
                .paint_action_element(
                    renderer,
                    element,
                    interaction,
                    controls::ButtonStyle::Danger,
                    false,
                ),
            DisplayElement::WizardBack
            | DisplayElement::WizardCancel
            | DisplayElement::EditProfile
            | DisplayElement::UpdateProfile
            | DisplayElement::DuplicateProfile
            | DisplayElement::UndoChange => self.paint_action_element(
                renderer,
                element,
                interaction,
                controls::ButtonStyle::Secondary,
                false,
            ),
            DisplayElement::WizardSummary
            | DisplayElement::ProfilesEnabled
            | DisplayElement::Profile
            | DisplayElement::Outputs
            | DisplayElement::Topology
            | DisplayElement::Route
            | DisplayElement::EditRoute => {
                self.draw_standard_element(renderer, element, interaction);
            }
        }
    }

    fn draw_shortcut_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        domain: ShortcutElement,
    ) {
        match domain {
            ShortcutElement::Card(slot) => {
                controls::draw_hotkey_card(renderer, element, self.hotkey_enabled(slot));
            }
            ShortcutElement::Enabled(_) | ShortcutElement::Unassign(_) => {
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
            ShortcutElement::Capture(_) => {
                let value = match self.value_for(element.id) {
                    ControlValue::Text(value) => value.into_owned(),
                    _ => String::new(),
                };
                controls::draw_hotkey_keycap(renderer, element, &value, interaction);
            }
        }
    }

    fn draw_workspace_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        domain: WorkspaceElement,
    ) {
        match domain {
            WorkspaceElement::Enabled
            | WorkspaceElement::WinNumberEnabled
            | WorkspaceElement::DesktopNumberModifier
            | WorkspaceElement::MoveDesktopModifier
            | WorkspaceElement::SilentMoveDesktopModifier => {
                self.draw_standard_element(renderer, element, interaction);
            }
        }
    }

    fn draw_overlay_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        domain: OverlayElement,
    ) {
        match domain {
            OverlayElement::PositionCell(index) => controls::draw_position_cell(
                renderer,
                element,
                overlay_position_label(index as usize),
                self.draft.overlay.position == overlay_position(index as usize),
                interaction,
            ),
            OverlayElement::Preview => self.paint_action_element(
                renderer,
                element,
                interaction,
                controls::ButtonStyle::Primary,
                false,
            ),
            OverlayElement::Enabled
            | OverlayElement::Microphone
            | OverlayElement::Speaker
            | OverlayElement::CurrentAppAudio
            | OverlayElement::Workspace
            | OverlayElement::DisplayProfile
            | OverlayElement::Appearance
            | OverlayElement::Position
            | OverlayElement::Monitor
            | OverlayElement::Duration
            | OverlayElement::Blur
            | OverlayElement::Scale => {
                self.draw_standard_element(renderer, element, interaction);
            }
        }
    }

    fn draw_system_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        domain: SystemElement,
    ) {
        match domain {
            SystemElement::StartWithWindows
            | SystemElement::StartHotkeysEnabled
            | SystemElement::DebugLogging
            | SystemElement::DiagnosticsStatus
            | SystemElement::OpenConfigFolder
            | SystemElement::ResetSettings => {
                self.draw_standard_element(renderer, element, interaction);
            }
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

    fn paint_home_microphone(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
        let detail = match &self.runtime.microphone {
            crate::audio::AudioState::Muted { volume_pct } => {
                format!("Muted · {volume_pct}% input volume")
            }
            crate::audio::AudioState::Active { volume_pct } => {
                format!("{volume_pct}% input volume")
            }
            crate::audio::AudioState::Unavailable { .. } => "Windows Audio is not available".into(),
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

    fn paint_home_display_profile(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
    ) {
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

    fn paint_display_topology_choice(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        index: u8,
    ) {
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

    fn paint_input_cycle_device(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        index: u8,
    ) {
        let selected = self
            .audio_view()
            .cycle_device_selected(crate::audio::DeviceCycleFlow::Input, index as usize);
        let label = self
            .audio_view()
            .cycle_device_label(crate::audio::DeviceCycleFlow::Input, index as usize);
        controls::draw_labeled_choice(renderer, element, &label, selected, interaction, false);
    }

    fn paint_output_cycle_device(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        index: u8,
    ) {
        let selected = self
            .audio_view()
            .cycle_device_selected(crate::audio::DeviceCycleFlow::Output, index as usize);
        let label = self
            .audio_view()
            .cycle_device_label(crate::audio::DeviceCycleFlow::Output, index as usize);
        controls::draw_labeled_choice(renderer, element, &label, selected, interaction, false);
    }

    fn paint_action_element(
        &self,
        renderer: &Renderer,
        element: &crate::ui::layout::Element,
        interaction: controls::Interaction,
        style: controls::ButtonStyle,
        use_row_when_tall: bool,
    ) {
        if use_row_when_tall && element.rect.h > 40.0 {
            controls::draw_row(renderer, element, self.value_for(element.id), interaction);
            return;
        }

        let label = match self.value_for(element.id) {
            ControlValue::Action(value) => value.into_owned(),
            _ => element.label.clone(),
        };
        controls::draw_button_style(renderer, element.rect, &label, style, interaction);
    }
}
