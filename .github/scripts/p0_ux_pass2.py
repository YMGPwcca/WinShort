from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


def write(path, text):
    (ROOT / path).write_text(text, encoding="utf-8", newline="\n")


def replace(path, old, new, count=1):
    text = read(path)
    found = text.count(old)
    if found != count:
        raise SystemExit(f"{path}: expected {count} matches, found {found}: {old[:120]!r}")
    write(path, text.replace(old, new))


def insert_before(path, marker, addition):
    text = read(path)
    if text.count(marker) != 1:
        raise SystemExit(f"{path}: marker count {text.count(marker)}: {marker[:100]!r}")
    write(path, text.replace(marker, addition + marker, 1))

# ---------------------------------------------------------------------------
# Config schema v10: disabled hotkeys retain their chord outside active fields.
# Older binaries therefore still see an empty active binding instead of firing
# a shortcut that the user disabled in a newer WinShort.
# ---------------------------------------------------------------------------
replace("src/config/model.rs", "pub const CURRENT_SCHEMA_VERSION: u8 = 9;", "pub const CURRENT_SCHEMA_VERSION: u8 = 10;")

replace(
    "src/config/model.rs",
    '''#[derive(Debug, Clone, PartialEq, Eq)]\npub struct HotkeysCfg {\n    pub toggle_microphone: Option<Hotkey>,\n    pub toggle_output: Option<Hotkey>,\n    pub toggle_foreground_audio: Option<Hotkey>,\n    pub cycle_input_device: Option<Hotkey>,\n    pub cycle_output_device: Option<Hotkey>,\n    pub foreground_volume_up: Option<Hotkey>,\n    pub foreground_volume_down: Option<Hotkey>,\n    pub display_profiles: Vec<DisplayProfileHotkey>,\n}\n''',
    '''#[derive(Debug, Clone, PartialEq, Eq)]\npub struct DisabledHotkey {\n    pub action: String,\n    pub hotkey: Hotkey,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct HotkeysCfg {\n    pub toggle_microphone: Option<Hotkey>,\n    pub toggle_output: Option<Hotkey>,\n    pub toggle_foreground_audio: Option<Hotkey>,\n    pub cycle_input_device: Option<Hotkey>,\n    pub cycle_output_device: Option<Hotkey>,\n    pub foreground_volume_up: Option<Hotkey>,\n    pub foreground_volume_down: Option<Hotkey>,\n    pub display_profiles: Vec<DisplayProfileHotkey>,\n    pub disabled: Vec<DisabledHotkey>,\n}\n\nimpl HotkeysCfg {\n    pub fn disabled_hotkey(&self, action: &str) -> Option<Hotkey> {\n        self.disabled\n            .iter()\n            .find(|binding| binding.action == action)\n            .map(|binding| binding.hotkey)\n    }\n\n    pub fn set_disabled_hotkey(&mut self, action: String, hotkey: Hotkey) {\n        if let Some(binding) = self\n            .disabled\n            .iter_mut()\n            .find(|binding| binding.action == action)\n        {\n            binding.hotkey = hotkey;\n        } else {\n            self.disabled.push(DisabledHotkey { action, hotkey });\n        }\n    }\n\n    pub fn take_disabled_hotkey(&mut self, action: &str) -> Option<Hotkey> {\n        let index = self\n            .disabled\n            .iter()\n            .position(|binding| binding.action == action)?;\n        Some(self.disabled.remove(index).hotkey)\n    }\n\n    pub fn clear_disabled_hotkey(&mut self, action: &str) {\n        self.disabled.retain(|binding| binding.action != action);\n    }\n}\n'''
)

replace(
    "src/config/model.rs",
    "                display_profiles: Vec::new(),\n            },",
    "                display_profiles: Vec::new(),\n                disabled: Vec::new(),\n            },",
)

insert_before(
    "src/config/model.rs",
    "#[derive(Serialize, Deserialize)]\npub struct HotkeysToml {",
    '''#[derive(Serialize, Deserialize)]\npub struct DisabledHotkeyToml {\n    pub action: String,\n    pub hotkey: String,\n}\n\n'''
)
replace(
    "src/config/model.rs",
    "    #[serde(default)]\n    pub display_profiles: Vec<DisplayProfileHotkeyToml>,\n}",
    "    #[serde(default)]\n    pub display_profiles: Vec<DisplayProfileHotkeyToml>,\n    #[serde(default)]\n    pub disabled: Vec<DisabledHotkeyToml>,\n}",
)
replace(
    "src/config/model.rs",
    "            display_profiles: Vec::new(),\n        }\n    }\n}\n\n#[derive(Serialize, Deserialize)]\npub struct DesktopRuleToml",
    "            display_profiles: Vec::new(),\n            disabled: Vec::new(),\n        }\n    }\n}\n\n#[derive(Serialize, Deserialize)]\npub struct DesktopRuleToml",
)
replace(
    "src/config/model.rs",
    '''                display_profiles: self\n                    .hotkeys\n                    .display_profiles\n                    .iter()\n                    .map(|binding| DisplayProfileHotkeyToml {\n                        profile_id: binding.profile_id.clone(),\n                        hotkey: binding.hotkey.to_string(),\n                    })\n                    .collect(),\n            },\n''',
    '''                display_profiles: self\n                    .hotkeys\n                    .display_profiles\n                    .iter()\n                    .map(|binding| DisplayProfileHotkeyToml {\n                        profile_id: binding.profile_id.clone(),\n                        hotkey: binding.hotkey.to_string(),\n                    })\n                    .collect(),\n                disabled: self\n                    .hotkeys\n                    .disabled\n                    .iter()\n                    .map(|binding| DisabledHotkeyToml {\n                        action: binding.action.clone(),\n                        hotkey: binding.hotkey.to_string(),\n                    })\n                    .collect(),\n            },\n'''
)

insert_before(
    "src/config/model.rs",
    "        c.virtual_desktops.enabled = t.virtual_desktops.enabled;",
    '''        for (index, binding) in t.hotkeys.disabled.iter().enumerate() {\n            let action = binding.action.trim();\n            if action.is_empty() {\n                warnings.push(format!(\n                    "hotkeys.disabled[{index}].action: must not be empty"\n                ));\n                continue;\n            }\n            match Hotkey::parse(&binding.hotkey) {\n                Ok(hotkey) => c.hotkeys.set_disabled_hotkey(action.to_string(), hotkey),\n                Err(error) => warnings.push(format!(\n                    "hotkeys.disabled[{index}].hotkey: {error}"\n                )),\n            }\n        }\n\n'''
)
replace(
    "src/config/model.rs",
    '''            "foreground_volume_down",\n            "display_profiles",\n        ]),\n''',
    '''            "foreground_volume_down",\n            "display_profiles",\n            "disabled",\n        ]),\n'''
)

# ---------------------------------------------------------------------------
# Layout: give every keybind a passive card plus three truthful child controls:
# keycap, Enable/Disable, Unassign. One column at normal Control Center width.
# ---------------------------------------------------------------------------
insert_before(
    "src/ui/layout.rs",
    "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\npub enum ElementId {",
    '''#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\npub enum HotkeySlot {\n    Microphone,\n    Output,\n    Foreground,\n    CycleInput,\n    CycleOutput,\n    ForegroundVolumeUp,\n    ForegroundVolumeDown,\n    PreviousDesktop,\n    AssignSpecial,\n    ToggleSpecial,\n    DisplayProfile,\n}\n\nimpl HotkeySlot {\n    pub fn from_capture_id(id: ElementId) -> Option<Self> {\n        Some(match id {\n            ElementId::MicHotkey => Self::Microphone,\n            ElementId::OutputHotkey => Self::Output,\n            ElementId::ForegroundHotkey => Self::Foreground,\n            ElementId::CycleInputHotkey => Self::CycleInput,\n            ElementId::CycleOutputHotkey => Self::CycleOutput,\n            ElementId::ForegroundVolumeUpHotkey => Self::ForegroundVolumeUp,\n            ElementId::ForegroundVolumeDownHotkey => Self::ForegroundVolumeDown,\n            ElementId::PreviousDesktopHotkey => Self::PreviousDesktop,\n            ElementId::AssignScratchpadHotkey => Self::AssignSpecial,\n            ElementId::ToggleScratchpadHotkey => Self::ToggleSpecial,\n            ElementId::DisplayProfileHotkey => Self::DisplayProfile,\n            _ => return None,\n        })\n    }\n}\n\n'''
)
replace(
    "src/ui/layout.rs",
    "    StartHotkeysEnabled,\n    MicHotkey,",
    "    StartHotkeysEnabled,\n    HotkeyCard(HotkeySlot),\n    HotkeyEnabled(HotkeySlot),\n    HotkeyUnassign(HotkeySlot),\n    MicHotkey,",
)

start = '''fn add_hotkey_grid(layout: &mut SettingsLayout, y: &mut f32, items: &[(ElementId, &str, &str)]) {\n'''
end = '''fn add_button_grid(\n'''
text = read("src/ui/layout.rs")
a = text.index(start)
b = text.index(end, a)
new = '''fn add_managed_hotkey(\n    layout: &mut SettingsLayout,\n    card: Rect,\n    slot: HotkeySlot,\n    capture_id: ElementId,\n    label: &str,\n    description: &str,\n) {\n    layout.elements.push(Element {\n        id: ElementId::HotkeyCard(slot),\n        kind: ElementKind::Card,\n        rect: card,\n        label: label.into(),\n        description: description.into(),\n        scrolls: true,\n    });\n    let control_x = card.right() - 188.0;\n    layout.elements.push(Element {\n        id: capture_id,\n        kind: ElementKind::Hotkey,\n        rect: Rect::new(control_x, card.y + 10.0, 172.0, 32.0),\n        label: format!("{label} shortcut"),\n        description: "Record a new shortcut".into(),\n        scrolls: true,\n    });\n    layout.elements.push(Element {\n        id: ElementId::HotkeyEnabled(slot),\n        kind: ElementKind::ButtonSecondary,\n        rect: Rect::new(control_x, card.y + 50.0, 82.0, 28.0),\n        label: "Shortcut state".into(),\n        description: format!("Enable or disable {label}"),\n        scrolls: true,\n    });\n    layout.elements.push(Element {\n        id: ElementId::HotkeyUnassign(slot),\n        kind: ElementKind::ButtonSecondary,\n        rect: Rect::new(control_x + 90.0, card.y + 50.0, 82.0, 28.0),\n        label: "Unassign".into(),\n        description: format!("Remove the shortcut for {label}"),\n        scrolls: true,\n    });\n}\n\nfn add_hotkey_grid(layout: &mut SettingsLayout, y: &mut f32, items: &[(ElementId, &str, &str)]) {\n    let gap = UiTokens::CARD_GAP;\n    let columns = if layout.content_column.w >= 980.0 { 2 } else { 1 };\n    let card_w = if columns == 1 {\n        layout.content_column.w\n    } else {\n        (layout.content_column.w - gap) * 0.5\n    };\n    let row_h = 88.0;\n    let start = *y;\n    for (index, (capture_id, label, description)) in items.iter().copied().enumerate() {\n        let Some(slot) = HotkeySlot::from_capture_id(capture_id) else {\n            continue;\n        };\n        let row = index / columns;\n        let column = index % columns;\n        let card = Rect::new(\n            layout.content_column.x + column as f32 * (card_w + gap),\n            start + row as f32 * (row_h + gap),\n            card_w,\n            row_h,\n        );\n        add_managed_hotkey(layout, card, slot, capture_id, label, description);\n    }\n    if !items.is_empty() {\n        *y = start + items.len().div_ceil(columns) as f32 * (row_h + gap);\n    }\n}\n\n'''
write("src/ui/layout.rs", text[:a] + new + text[b:])

# Convert the standalone workspace and display-profile hotkey rows to managed cards.
replace(
    "src/ui/layout.rs",
    '''    add_row(\n        layout,\n        &mut y,\n        ElementId::DisplayProfileHotkey,\n        ElementKind::Hotkey,\n        "Selected display profile",\n        "Activate the selected profile from any app",\n    );\n''',
    '''    add_hotkey_grid(\n        layout,\n        &mut y,\n        &[((ElementId::DisplayProfileHotkey), "Selected display profile", "Activate the selected profile from any app")],\n    );\n'''
)
replace(
    "src/ui/layout.rs",
    '''    add_row(\n        layout,\n        &mut y,\n        ElementId::PreviousDesktopHotkey,\n        ElementKind::Hotkey,\n        "Previous desktop",\n        "Return to the last normal desktop",\n    );\n''',
    '''    add_hotkey_grid(\n        layout,\n        &mut y,\n        &[(ElementId::PreviousDesktopHotkey, "Previous desktop", "Return to the last normal desktop")],\n    );\n'''
)

# ---------------------------------------------------------------------------
# Drawing helpers for managed hotkey cards and keycaps.
# ---------------------------------------------------------------------------
insert_before(
    "src/ui/controls.rs",
    "pub fn draw_shortcut_card(r: &Renderer, element: &Element, value: &str, interaction: Interaction) {",
    '''pub fn draw_hotkey_card(r: &Renderer, element: &Element, enabled: bool) {\n    let rect = element.rect.inset(1.0);\n    draw_surface(r, rect, BrushRole::Card, InteractionState::Idle, 10.0);\n    let text_width = (rect.w - 222.0).max(120.0);\n    let stack_top = rect.y + (rect.h - 40.0) * 0.5;\n    r.text_clipped(\n        &element.label,\n        Rect::new(rect.x + BODY_LEFT, stack_top, text_width, 20.0).d2d(),\n        TextStyle::BodyStrong,\n        if enabled { BrushRole::Text } else { BrushRole::TextSecondary },\n    );\n    r.text_clipped(\n        &element.description,\n        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, text_width, 18.0).d2d(),\n        TextStyle::Caption,\n        BrushRole::TextSecondary,\n    );\n}\n\npub fn draw_hotkey_keycap(r: &Renderer, element: &Element, value: &str, interaction: Interaction) {\n    let rect = element.rect.inset(1.0);\n    let state = interaction_state(interaction);\n    r.fill_rounded(\n        rect.d2d(),\n        CONTROL_RADIUS,\n        if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {\n            BrushRole::ControlHover\n        } else {\n            BrushRole::BackgroundSubtle\n        },\n    );\n    r.stroke_rounded(\n        rect.d2d(),\n        CONTROL_RADIUS,\n        if interaction.focused { BrushRole::Focus } else { BrushRole::BorderStrong },\n        if interaction.focused { 1.5 } else { 1.0 },\n    );\n    r.text_clipped(\n        value,\n        rect.d2d(),\n        TextStyle::Button,\n        if value.starts_with("Press") { BrushRole::Accent } else { BrushRole::Text },\n    );\n}\n\n'''
)

# ---------------------------------------------------------------------------
# Control Center state semantics for active/disabled/unassigned shortcuts.
# ---------------------------------------------------------------------------
replace(
    "src/ui/control_center.rs",
    "    ElementId, ElementKind, LayoutContext, Rect as UiRect, RegionKind, SettingsLayout,\n",
    "    ElementId, ElementKind, HotkeySlot, LayoutContext, Rect as UiRect, RegionKind, SettingsLayout,\n",
)

# Draw passive card, capture keycap, and management buttons as separate elements.
insert_marker = '''                ElementId::DesktopStripItem(index) => {\n'''
insert = '''                ElementId::HotkeyCard(slot) => {\n                    controls::draw_hotkey_card(renderer, element, self.hotkey_enabled(slot));\n                }\n                ElementId::HotkeyEnabled(_) | ElementId::HotkeyUnassign(_) => {\n                    let label = match self.value_for(element.id) {\n                        ControlValue::Action(value) => value.into_owned(),\n                        _ => element.label.clone(),\n                    };\n                    controls::draw_button_style(\n                        renderer,\n                        element.rect,\n                        &label,\n                        controls::ButtonStyle::Secondary,\n                        interaction,\n                    );\n                }\n                ElementId::MicHotkey\n                | ElementId::OutputHotkey\n                | ElementId::ForegroundHotkey\n                | ElementId::CycleInputHotkey\n                | ElementId::CycleOutputHotkey\n                | ElementId::ForegroundVolumeUpHotkey\n                | ElementId::ForegroundVolumeDownHotkey\n                | ElementId::PreviousDesktopHotkey\n                | ElementId::AssignScratchpadHotkey\n                | ElementId::ToggleScratchpadHotkey\n                | ElementId::DisplayProfileHotkey => {\n                    let value = match self.value_for(element.id) {\n                        ControlValue::Text(value) => value.into_owned(),\n                        _ => String::new(),\n                    };\n                    controls::draw_hotkey_keycap(renderer, element, &value, interaction);\n                }\n'''
insert_before("src/ui/control_center.rs", insert_marker, insert)

# value_for uses configured chord even while disabled, plus explicit state/action buttons.
old_values = '''            ElementId::MicHotkey => self.hotkey_value(id, self.draft.hotkeys.toggle_microphone),\n            ElementId::OutputHotkey => self.hotkey_value(id, self.draft.hotkeys.toggle_output),\n            ElementId::ForegroundHotkey => {\n                self.hotkey_value(id, self.draft.hotkeys.toggle_foreground_audio)\n            }\n            ElementId::CycleInputHotkey => {\n                self.hotkey_value(id, self.draft.hotkeys.cycle_input_device)\n            }\n            ElementId::CycleOutputHotkey => {\n                self.hotkey_value(id, self.draft.hotkeys.cycle_output_device)\n            }\n            ElementId::ForegroundVolumeUpHotkey => {\n                self.hotkey_value(id, self.draft.hotkeys.foreground_volume_up)\n            }\n            ElementId::ForegroundVolumeDownHotkey => {\n                self.hotkey_value(id, self.draft.hotkeys.foreground_volume_down)\n            }\n            ElementId::PreviousDesktopHotkey => {\n                self.hotkey_value(id, self.draft.virtual_desktops.previous_desktop)\n            }\n            ElementId::AssignScratchpadHotkey => {\n                self.hotkey_value(id, self.draft.virtual_desktops.scratchpad_assign)\n            }\n            ElementId::ToggleScratchpadHotkey => {\n                self.hotkey_value(id, self.draft.virtual_desktops.scratchpad_toggle)\n            }\n'''
new_values = '''            ElementId::HotkeyCard(_) => ControlValue::Action(Cow::Borrowed("")),\n            ElementId::HotkeyEnabled(slot) => ControlValue::Action(Cow::Borrowed(\n                if self.hotkey_enabled(slot) { "Disable" } else { "Enable" },\n            )),\n            ElementId::HotkeyUnassign(_) => ControlValue::Action(Cow::Borrowed("Unassign")),\n            ElementId::MicHotkey\n            | ElementId::OutputHotkey\n            | ElementId::ForegroundHotkey\n            | ElementId::CycleInputHotkey\n            | ElementId::CycleOutputHotkey\n            | ElementId::ForegroundVolumeUpHotkey\n            | ElementId::ForegroundVolumeDownHotkey\n            | ElementId::PreviousDesktopHotkey\n            | ElementId::AssignScratchpadHotkey\n            | ElementId::ToggleScratchpadHotkey => {\n                let slot = HotkeySlot::from_capture_id(id).expect("hotkey slot");\n                self.hotkey_value(id, self.configured_hotkey(slot))\n            }\n'''
replace("src/ui/control_center.rs", old_values, new_values)
replace(
    "src/ui/control_center.rs",
    "            ElementId::DisplayProfileHotkey => self.hotkey_value(id, self.active_profile_hotkey()),",
    "            ElementId::DisplayProfileHotkey => self.hotkey_value(id, self.configured_hotkey(HotkeySlot::DisplayProfile)),",
)

# Hotkey helper methods inserted after hotkey_value.
marker = '''    fn choice_selected(&self, id: ElementId) -> bool {\n'''
helpers = '''    fn hotkey_action(&self, slot: HotkeySlot) -> Option<String> {\n        Some(match slot {\n            HotkeySlot::Microphone => "toggle_microphone".into(),\n            HotkeySlot::Output => "toggle_output".into(),\n            HotkeySlot::Foreground => "toggle_foreground_audio".into(),\n            HotkeySlot::CycleInput => "cycle_input_device".into(),\n            HotkeySlot::CycleOutput => "cycle_output_device".into(),\n            HotkeySlot::ForegroundVolumeUp => "foreground_volume_up".into(),\n            HotkeySlot::ForegroundVolumeDown => "foreground_volume_down".into(),\n            HotkeySlot::PreviousDesktop => "previous_desktop".into(),\n            HotkeySlot::AssignSpecial => "scratchpad_assign".into(),\n            HotkeySlot::ToggleSpecial => "scratchpad_toggle".into(),\n            HotkeySlot::DisplayProfile => format!(\n                "display_profile:{}",\n                self.draft.display_profiles.active()?.id\n            ),\n        })\n    }\n\n    fn active_hotkey(&self, slot: HotkeySlot) -> Option<Hotkey> {\n        match slot {\n            HotkeySlot::Microphone => self.draft.hotkeys.toggle_microphone,\n            HotkeySlot::Output => self.draft.hotkeys.toggle_output,\n            HotkeySlot::Foreground => self.draft.hotkeys.toggle_foreground_audio,\n            HotkeySlot::CycleInput => self.draft.hotkeys.cycle_input_device,\n            HotkeySlot::CycleOutput => self.draft.hotkeys.cycle_output_device,\n            HotkeySlot::ForegroundVolumeUp => self.draft.hotkeys.foreground_volume_up,\n            HotkeySlot::ForegroundVolumeDown => self.draft.hotkeys.foreground_volume_down,\n            HotkeySlot::PreviousDesktop => self.draft.virtual_desktops.previous_desktop,\n            HotkeySlot::AssignSpecial => self.draft.virtual_desktops.scratchpad_assign,\n            HotkeySlot::ToggleSpecial => self.draft.virtual_desktops.scratchpad_toggle,\n            HotkeySlot::DisplayProfile => self.active_profile_hotkey(),\n        }\n    }\n\n    fn configured_hotkey(&self, slot: HotkeySlot) -> Option<Hotkey> {\n        self.active_hotkey(slot).or_else(|| {\n            self.hotkey_action(slot)\n                .as_deref()\n                .and_then(|action| self.draft.hotkeys.disabled_hotkey(action))\n        })\n    }\n\n    fn hotkey_enabled(&self, slot: HotkeySlot) -> bool {\n        self.active_hotkey(slot).is_some()\n    }\n\n    fn set_active_hotkey(&mut self, slot: HotkeySlot, hotkey: Option<Hotkey>) {\n        match slot {\n            HotkeySlot::Microphone => self.draft.hotkeys.toggle_microphone = hotkey,\n            HotkeySlot::Output => self.draft.hotkeys.toggle_output = hotkey,\n            HotkeySlot::Foreground => self.draft.hotkeys.toggle_foreground_audio = hotkey,\n            HotkeySlot::CycleInput => self.draft.hotkeys.cycle_input_device = hotkey,\n            HotkeySlot::CycleOutput => self.draft.hotkeys.cycle_output_device = hotkey,\n            HotkeySlot::ForegroundVolumeUp => self.draft.hotkeys.foreground_volume_up = hotkey,\n            HotkeySlot::ForegroundVolumeDown => self.draft.hotkeys.foreground_volume_down = hotkey,\n            HotkeySlot::PreviousDesktop => self.draft.virtual_desktops.previous_desktop = hotkey,\n            HotkeySlot::AssignSpecial => self.draft.virtual_desktops.scratchpad_assign = hotkey,\n            HotkeySlot::ToggleSpecial => self.draft.virtual_desktops.scratchpad_toggle = hotkey,\n            HotkeySlot::DisplayProfile => self.set_active_profile_hotkey(hotkey),\n        }\n    }\n\n    fn set_recorded_hotkey(&mut self, slot: HotkeySlot, hotkey: Hotkey) {\n        let Some(action) = self.hotkey_action(slot) else {\n            return;\n        };\n        if self.active_hotkey(slot).is_none() && self.draft.hotkeys.disabled_hotkey(&action).is_some() {\n            self.draft.hotkeys.set_disabled_hotkey(action, hotkey);\n        } else {\n            self.set_active_hotkey(slot, Some(hotkey));\n            self.draft.hotkeys.clear_disabled_hotkey(&action);\n        }\n    }\n\n    fn toggle_hotkey_enabled(&mut self, hwnd: HWND, slot: HotkeySlot) {\n        let Some(action) = self.hotkey_action(slot) else {\n            return;\n        };\n        let before = self.draft.clone();\n        if let Some(hotkey) = self.active_hotkey(slot) {\n            self.set_active_hotkey(slot, None);\n            self.draft.hotkeys.set_disabled_hotkey(action, hotkey);\n        } else if let Some(hotkey) = self.draft.hotkeys.take_disabled_hotkey(&action) {\n            self.set_active_hotkey(slot, Some(hotkey));\n            let violations = crate::config::validate(&self.draft);\n            if !violations.is_empty() {\n                self.replace_draft(before);\n                self.validation = violations;\n                return;\n            }\n        } else {\n            return;\n        }\n        self.commit_local_change(hwnd, before);\n    }\n\n    fn unassign_hotkey(&mut self, hwnd: HWND, slot: HotkeySlot) {\n        let Some(action) = self.hotkey_action(slot) else {\n            return;\n        };\n        let before = self.draft.clone();\n        self.set_active_hotkey(slot, None);\n        self.draft.hotkeys.clear_disabled_hotkey(&action);\n        if self.draft != before {\n            self.commit_local_change(hwnd, before);\n        }\n    }\n\n'''
insert_before("src/ui/control_center.rs", marker, helpers)

# Management buttons disable when nothing is assigned; workspace/profile gating still applies.
replace(
    "src/ui/control_center.rs",
    '''    fn is_disabled(&self, id: ElementId) -> bool {\n        match id {\n''',
    '''    fn is_disabled(&self, id: ElementId) -> bool {\n        match id {\n            ElementId::HotkeyEnabled(slot) | ElementId::HotkeyUnassign(slot) => {\n                self.configured_hotkey(slot).is_none()\n                    || (matches!(slot, HotkeySlot::PreviousDesktop | HotkeySlot::AssignSpecial | HotkeySlot::ToggleSpecial)\n                        && !self.draft.virtual_desktops.enabled)\n                    || (slot == HotkeySlot::DisplayProfile\n                        && self.draft.display_profiles.active().is_none())\n            }\n            ElementId::HotkeyCard(_) => false,\n'''
)

# Activation routes the two management buttons explicitly.
insert_before(
    "src/ui/control_center.rs",
    '''            ElementId::MicHotkey\n            | ElementId::OutputHotkey\n''',
    '''            ElementId::HotkeyCard(_) => {}\n            ElementId::HotkeyEnabled(slot) => self.toggle_hotkey_enabled(hwnd, slot),\n            ElementId::HotkeyUnassign(slot) => self.unassign_hotkey(hwnd, slot),\n'''
)

# Recording writes through the new slot helper, preserving disabled state.
old_record = '''        match id {\n            ElementId::MicHotkey => self.draft.hotkeys.toggle_microphone = Some(hk),\n            ElementId::OutputHotkey => self.draft.hotkeys.toggle_output = Some(hk),\n            ElementId::ForegroundHotkey => self.draft.hotkeys.toggle_foreground_audio = Some(hk),\n            ElementId::CycleInputHotkey => self.draft.hotkeys.cycle_input_device = Some(hk),\n            ElementId::CycleOutputHotkey => self.draft.hotkeys.cycle_output_device = Some(hk),\n            ElementId::ForegroundVolumeUpHotkey => self.draft.hotkeys.foreground_volume_up = Some(hk),\n            ElementId::ForegroundVolumeDownHotkey => {\n                self.draft.hotkeys.foreground_volume_down = Some(hk)\n            }\n            ElementId::PreviousDesktopHotkey => self.draft.virtual_desktops.previous_desktop = Some(hk),\n            ElementId::AssignScratchpadHotkey => self.draft.virtual_desktops.scratchpad_assign = Some(hk),\n            ElementId::ToggleScratchpadHotkey => self.draft.virtual_desktops.scratchpad_toggle = Some(hk),\n            ElementId::DisplayProfileHotkey => self.set_active_profile_hotkey(Some(hk)),\n            _ => {}\n        }\n'''
new_record = '''        if let Some(slot) = HotkeySlot::from_capture_id(id) {\n            self.set_recorded_hotkey(slot, hk);\n        }\n'''
replace("src/ui/control_center.rs", old_record, new_record, count=2)

# ---------------------------------------------------------------------------
# Docs: schema and UX contract.
# ---------------------------------------------------------------------------
replace("docs/CONFIG_SCHEMA.md", "schema_version = 9", "schema_version = 10")
replace(
    "docs/CONFIG_SCHEMA.md",
    '''foreground_volume_down = ""\n''',
    '''foreground_volume_down = ""\n\n# Disabled shortcuts keep their chord here while the active field stays empty.\n# This lets each shortcut be re-enabled without losing the user's assignment.\n[[hotkeys.disabled]]\naction = "cycle_output_device"\nhotkey = "Alt+Win+F2"\n'''
)
replace(
    "docs/UI_DESIGN.md",
    "Selecting a shortcut enters the existing global capture mode. Captured chords are",
    "Each shortcut card exposes its keycap, an Enable/Disable action, and an explicit Unassign action. Selecting the keycap enters the existing global capture mode. Disabled shortcuts keep their chord so they can be re-enabled without recording again. Captured chords are",
)

print("P0 UX pass 2 patch applied")
