from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


def write(path, text):
    (ROOT / path).write_text(text, encoding="utf-8")


def replace_once(text, old, new, label):
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, got {count}")
    return text.replace(old, new, 1)


# ---------------- layout ----------------
path = "src/ui/layout.rs"
text = read(path)
text = replace_once(
    text,
    "    DisplayProfileHotkey,\n    DisplayTopology,\n",
    "    DisplayProfileHotkey,\n    DisplayOutputs,\n    DisplayTopology,\n",
    "layout DisplayOutputs enum",
)
text = replace_once(text, "pub const FOCUS_ORDER: [ElementId; 52]", "pub const FOCUS_ORDER: [ElementId; 53]", "layout focus count")
text = replace_once(
    text,
    "        ElementId::DisplayProfileHotkey,\n        ElementId::DisplayTopology,\n",
    "        ElementId::DisplayProfileHotkey,\n        ElementId::DisplayOutputs,\n        ElementId::DisplayTopology,\n",
    "layout focus DisplayOutputs",
)
text = text.replace('"Display profile engine",\n                    "Store and apply documented DisplayConfig snapshots",', '"Display profiles",\n                    "Switch monitor and TV layouts with saved profiles",')
text = text.replace('"Active display profile",\n                    "Choose the profile used by Test Apply or Apply",', '"Profile",\n                    "Choose the display layout you want to edit or apply",')
text = text.replace('"Profile hotkey",\n                    "Apply the selected confirmed profile from any app",', '"Profile hotkey",\n                    "Apply this tested profile from any app",')
old_hotkey_row = '''                row(
                    ElementId::DisplayProfileHotkey,
                    ElementKind::Hotkey,
                    "Profile hotkey",
                    "Apply this tested profile from any app",
                ),
'''
new_hotkey_row = old_hotkey_row + '''                row(
                    ElementId::DisplayOutputs,
                    ElementKind::Value,
                    "Outputs",
                    "Choose which connected monitor/TV outputs this profile uses",
                ),
'''
text = replace_once(text, old_hotkey_row, new_hotkey_row, "layout outputs row")
text = text.replace('"Topology",\n                    "Extend, Duplicate, Internal, External, or custom",', '"Mode",\n                    "Choose Extend or Duplicate when more than one output is enabled",')
text = text.replace('"Selected route",\n                    "Physical route identity, including adapter and connector IDs",', '"Advanced output",\n                    "Optional per-output resolution, refresh, position, and orientation",')
text = text.replace('"Edit route values",\n                    "Edit position, resolution, refresh rate, and orientation",', '"Advanced output settings",\n                    "Optional fine tuning; normal profile switching does not require this",')
text = text.replace('"New from Current",\n                    "Capture the current topology into a new profile",', '"New profile",\n                    "Start from the current layout, then choose outputs",')
text = text.replace('"Update from Current",\n                    "Explicitly replace the selected profile topology",', '"Capture current layout",\n                    "Replace this profile with the layout Windows is using now",')
text = text.replace('"Test Apply",\n                    "Apply temporarily; Keep or Revert is required",', '"Test configuration",\n                    "Try this layout for 15 seconds before keeping it",')
text = text.replace('"Apply confirmed profile",\n                    "Apply a previously tested and kept profile",', '"Apply profile",\n                    "Switch to this previously tested layout",')
write(path, text)

# ---------------- picker ----------------
path = "src/ui/picker.rs"
text = read(path)
text = replace_once(
    text,
    "    DisplayProfile,\n    DisplayTopology,\n",
    "    DisplayProfile,\n    DisplayOutputs,\n    DisplayTopology,\n",
    "picker DisplayOutputs kind",
)
text = replace_once(
    text,
    "        matches!(self, Self::InputAllowlist | Self::OutputAllowlist)\n",
    "        matches!(\n            self,\n            Self::InputAllowlist | Self::OutputAllowlist | Self::DisplayOutputs\n        )\n",
    "picker multiselect kinds",
)
text = replace_once(
    text,
    "    DisplayProfile(Option<String>),\n    DisplayTopology(DisplayTopology),\n",
    "    DisplayProfile(Option<String>),\n    DisplayOutput(crate::display::DisplayRoute),\n    DisplayOutputs(Vec<crate::display::DisplayRoute>),\n    DisplayTopology(DisplayTopology),\n",
    "picker output values",
)
# Only audio allowlists have the two special rows.
text = replace_once(
    text,
    "fn normalize_allowlist_selection(list: HWND) {\n",
    "fn normalize_multi_selection(kind: PickerKind, list: HWND) {\n    if !matches!(kind, PickerKind::InputAllowlist | PickerKind::OutputAllowlist) {\n        return;\n    }\n",
    "picker normalize function",
)
old_mouse = '''            let multi_select = unsafe { win::state_cell::<PickerUi>(parent) }
                .is_some_and(|cell| cell.borrow().kind.is_multi_select());
            if multi_select {
                normalize_allowlist_selection(hwnd);
            } else {
                commit_selected(parent, hwnd);
            }
'''
new_mouse = '''            let kind = unsafe { win::state_cell::<PickerUi>(parent) }
                .map(|cell| cell.borrow().kind);
            if kind.is_some_and(PickerKind::is_multi_select) {
                normalize_multi_selection(kind.expect("picker kind"), hwnd);
            } else {
                commit_selected(parent, hwnd);
            }
'''
text = replace_once(text, old_mouse, new_mouse, "picker mouse multiselect")
old_command = '''                let (list, multi_select) = {
                    let ui = cell.borrow();
                    (ui.list, ui.kind.is_multi_select())
                };
                if source == list && notification == LBN_DBLCLK && !multi_select {
                    commit_selected(hwnd, source);
                    LRESULT(0)
                } else if source == list && notification == LBN_SELCHANGE {
                    if multi_select {
                        normalize_allowlist_selection(source);
                    }
'''
new_command = '''                let (list, kind) = {
                    let ui = cell.borrow();
                    (ui.list, ui.kind)
                };
                if source == list && notification == LBN_DBLCLK && !kind.is_multi_select() {
                    commit_selected(hwnd, source);
                    LRESULT(0)
                } else if source == list && notification == LBN_SELCHANGE {
                    if kind.is_multi_select() {
                        normalize_multi_selection(kind, source);
                    }
'''
text = replace_once(text, old_command, new_command, "picker command multiselect")
# Add DisplayOutputs aggregation before the existing audio allowlist aggregation.
needle = '''    if kind.is_multi_select() {
        let count = unsafe {
'''
replacement = '''    if kind.is_multi_select() {
        let count = unsafe {
'''
# We replace the endpoint aggregation tail inside selected_value with a kind-aware branch.
old_tail = '''        let ui = cell.borrow();
        let mut use_all = false;
        let mut endpoints = Vec::new();
        for index in indices {
            let Some(choice) = ui.choices.get(index as usize) else {
                continue;
            };
            match &choice.value {
                PickerValue::Allowlist(None) => use_all = true,
                PickerValue::Allowlist(Some(values)) => endpoints.extend(values.iter().cloned()),
                _ => {}
            }
        }
        let allowlist = if !endpoints.is_empty() {
            Some(endpoints)
        } else if use_all {
            None
        } else {
            Some(Vec::new())
        };
        return Some((kind, PickerValue::Allowlist(allowlist)));
'''
new_tail = '''        let ui = cell.borrow();
        if kind == PickerKind::DisplayOutputs {
            let outputs = indices
                .into_iter()
                .filter_map(|index| ui.choices.get(index as usize))
                .filter_map(|choice| match &choice.value {
                    PickerValue::DisplayOutput(route) => Some(route.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            return Some((kind, PickerValue::DisplayOutputs(outputs)));
        }
        let mut use_all = false;
        let mut endpoints = Vec::new();
        for index in indices {
            let Some(choice) = ui.choices.get(index as usize) else {
                continue;
            };
            match &choice.value {
                PickerValue::Allowlist(None) => use_all = true,
                PickerValue::Allowlist(Some(values)) => endpoints.extend(values.iter().cloned()),
                _ => {}
            }
        }
        let allowlist = if !endpoints.is_empty() {
            Some(endpoints)
        } else if use_all {
            None
        } else {
            Some(Vec::new())
        };
        return Some((kind, PickerValue::Allowlist(allowlist)));
'''
text = replace_once(text, old_tail, new_tail, "picker selected display outputs")
# Show a checkbox for every multi-select row instead of relying on highlight alone.
text = replace_once(
    text,
    "    hovered: bool,\n    font: HFONT,\n) -> LRESULT {\n",
    "    hovered: bool,\n    font: HFONT,\n    multi_select: bool,\n) -> LRESULT {\n",
    "picker draw signature",
)
old_text_rect = '''    let mut text_rect = item.rcItem;
    text_rect.left += 12;
    text_rect.right -= 12;
'''
new_text_rect = '''    let mut text_rect = item.rcItem;
    text_rect.left += if multi_select { 34 } else { 12 };
    text_rect.right -= 12;
    if multi_select {
        let mut box_rect = item.rcItem;
        box_rect.left += 10;
        box_rect.right = box_rect.left + 14;
        box_rect.top += (box_rect.bottom - box_rect.top - 14) / 2;
        box_rect.bottom = box_rect.top + 14;
        let _ = unsafe { FrameRect(item.hDC, &box_rect, border) };
        if selected {
            let mut mark = box_rect;
            mark.left += 3;
            mark.top += 3;
            mark.right -= 3;
            mark.bottom -= 3;
            let _ = unsafe { FillRect(item.hDC, &mark, border) };
        }
    }
'''
text = replace_once(text, old_text_rect, new_text_rect, "picker checkbox draw")
old_draw_call = '''                let (list, hovered, label, font) = {
                    let ui = cell.borrow();
                    (
                        ui.list,
                        ui.hovered_index == Some(item.itemID as usize),
                        ui.choices
                            .get(item.itemID as usize)
                            .map(|choice| choice.label.clone()),
                        ui.font,
                    )
                };
'''
new_draw_call = '''                let (list, hovered, label, font, multi_select) = {
                    let ui = cell.borrow();
                    (
                        ui.list,
                        ui.hovered_index == Some(item.itemID as usize),
                        ui.choices
                            .get(item.itemID as usize)
                            .map(|choice| choice.label.clone()),
                        ui.font,
                        ui.kind.is_multi_select(),
                    )
                };
'''
text = replace_once(text, old_draw_call, new_draw_call, "picker draw state")
text = replace_once(
    text,
    "                    draw_picker_item(item, &value, hovered, font)\n",
    "                    draw_picker_item(item, &value, hovered, font, multi_select)\n",
    "picker draw call",
)
write(path, text)

# ---------------- settings ----------------
path = "src/ui/settings.rs"
text = read(path)
text = replace_once(
    text,
    "    selected_display_route: usize,\n    display_rollback_active: bool,\n",
    "    selected_display_route: usize,\n    display_outputs: Vec<crate::display::DisplayOutput>,\n    display_rollback_active: bool,\n",
    "settings outputs field",
)
text = replace_once(
    text,
    "            selected_display_route: 0,\n            display_rollback_active: false,\n",
    "            selected_display_route: 0,\n            display_outputs: Vec::new(),\n            display_rollback_active: false,\n",
    "settings outputs init",
)
# Replace the raw route label helper and add friendly output/cache helpers.
start = text.index("    fn selected_display_route_label(&self) -> String {\n")
end = text.index("    fn selected_display_route_edit_value(&self) -> Option<String> {\n", start)
replacement = r'''    fn refresh_display_outputs(&mut self) {
        match crate::display::output_inventory() {
            Ok(outputs) => self.display_outputs = outputs,
            Err(error) => {
                crate::warn_!("display output inventory unavailable: {error}");
                self.display_outputs.clear();
            }
        }
    }

    fn output_label(&self, route: &crate::display::DisplayRoute) -> String {
        self.display_outputs
            .iter()
            .find(|output| crate::display::same_output(&output.route, route))
            .map(crate::display::DisplayOutput::label)
            .unwrap_or_else(|| "Configured output — currently unavailable".into())
    }

    fn display_outputs_label(&self) -> String {
        let Some(profile) = self.draft.display_profiles.active() else {
            return "No profile selected".into();
        };
        match profile.routes.as_slice() {
            [] => "No outputs selected".into(),
            [route] => self.output_label(route),
            routes => format!("{} outputs selected", routes.len()),
        }
    }

    fn selected_display_route_label(&self) -> String {
        let Some((_, route)) = self.selected_display_route() else {
            return "No output selected".into();
        };
        self.output_label(route)
    }

'''
text = text[:start] + replacement + text[end:]
# value_for: add Outputs and make one-output mode human-friendly.
text = replace_once(
    text,
    "            ElementId::DisplayProfileHotkey => self.hotkey_value(id, self.active_profile_hotkey()),\n            ElementId::DisplayTopology => ControlValue::Text(Cow::Owned(\n",
    "            ElementId::DisplayProfileHotkey => self.hotkey_value(id, self.active_profile_hotkey()),\n            ElementId::DisplayOutputs => {\n                ControlValue::Text(Cow::Owned(self.display_outputs_label()))\n            }\n            ElementId::DisplayTopology => ControlValue::Text(Cow::Owned(\n",
    "settings value DisplayOutputs",
)
old_topology_value = '''                self.draft
                    .display_profiles
                    .active()
                    .map(|profile| profile.topology.label().to_string())
                    .unwrap_or_else(|| "No profile selected".into()),
'''
new_topology_value = '''                self.draft
                    .display_profiles
                    .active()
                    .map(|profile| {
                        if profile.routes.len() <= 1 {
                            "Single output".to_string()
                        } else {
                            profile.topology.label().to_string()
                        }
                    })
                    .unwrap_or_else(|| "No profile selected".into()),
'''
text = replace_once(text, old_topology_value, new_topology_value, "settings topology value")
text = text.replace('ElementId::NewDisplayProfile => ControlValue::Action(Cow::Borrowed("Capture")),', 'ElementId::NewDisplayProfile => ControlValue::Action(Cow::Borrowed("Create")),')
text = text.replace('ElementId::UpdateDisplayProfile => ControlValue::Action(Cow::Borrowed("Update")),', 'ElementId::UpdateDisplayProfile => ControlValue::Action(Cow::Borrowed("Capture")),')
text = text.replace('ElementId::TestApplyDisplayProfile => ControlValue::Action(Cow::Borrowed("Test")),', 'ElementId::TestApplyDisplayProfile => ControlValue::Action(Cow::Borrowed("Test")),')
# Disable Mode for a single output; Outputs follows the other editable profile rows.
text = replace_once(
    text,
    "            ElementId::DisplayProfileHotkey\n            | ElementId::DisplayTopology\n            | ElementId::DisplayRoute\n",
    "            ElementId::DisplayProfileHotkey\n            | ElementId::DisplayOutputs\n            | ElementId::DisplayRoute\n",
    "settings editable outputs disable group",
)
insert_mode_disable = '''            ElementId::DisplayTopology => {
                !self.draft.display_profiles.enabled
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.len() <= 1)
                    || self.display_rollback_active
            }
'''
marker = "            ElementId::NewDisplayProfile => {\n"
text = replace_once(text, marker, insert_mode_disable + marker, "settings topology disable")
# Activate Outputs picker.
text = replace_once(
    text,
    "            ElementId::DisplayTopology => {\n                post_main(crate::event::AppEvent::OpenSettingsPicker(\n                    PickerKind::DisplayTopology,\n                ));\n            }\n",
    "            ElementId::DisplayOutputs => {\n                post_main(crate::event::AppEvent::OpenSettingsPicker(\n                    PickerKind::DisplayOutputs,\n                ));\n            }\n            ElementId::DisplayTopology => {\n                post_main(crate::event::AppEvent::OpenSettingsPicker(\n                    PickerKind::DisplayTopology,\n                ));\n            }\n",
    "settings activate outputs",
)
# apply_picker DisplayOutputs before topology.
needle = '''            (PickerKind::DisplayTopology, PickerValue::DisplayTopology(topology)) => {
'''
outputs_arm = r'''            (PickerKind::DisplayOutputs, PickerValue::DisplayOutputs(routes)) => {
                if routes.is_empty() {
                    self.validation = vec![Violation {
                        field: "display_profiles.outputs".into(),
                        message: "Select at least one output for this profile".into(),
                    }];
                    return;
                }
                if let Some(profile) = self
                    .draft
                    .display_profiles
                    .active_profile
                    .as_deref()
                    .and_then(|id| {
                        self.draft
                            .display_profiles
                            .profiles
                            .iter_mut()
                            .find(|profile| profile.id.eq_ignore_ascii_case(id))
                    })
                {
                    let mut merged = Vec::with_capacity(routes.len());
                    for selected in routes {
                        if let Some(existing) = profile
                            .routes
                            .iter()
                            .find(|route| crate::display::same_output(route, &selected))
                        {
                            merged.push(existing.clone());
                        } else {
                            merged.push(selected);
                        }
                    }
                    profile.routes = merged;
                    profile.confirmed = false;
                    if profile.routes.len() <= 1 {
                        profile.topology = crate::display::DisplayTopology::Custom;
                    } else if !matches!(
                        profile.topology,
                        crate::display::DisplayTopology::Extend
                            | crate::display::DisplayTopology::Clone
                    ) {
                        profile.topology = crate::display::DisplayTopology::Extend;
                    }
                    self.selected_display_route = self
                        .selected_display_route
                        .min(profile.routes.len().saturating_sub(1));
                }
            }
'''
text = replace_once(text, needle, outputs_arm + needle, "settings apply outputs")
# Refresh inventory whenever Settings is shown.
old_show = '''            if !ui.dirty() {
                ui.replace_draft((*crate::app::config()).clone());
            }
            ui.validation.clear();
'''
new_show = '''            if !ui.dirty() {
                ui.replace_draft((*crate::app::config()).clone());
            }
            ui.refresh_display_outputs();
            ui.validation.clear();
'''
text = replace_once(text, old_show, new_show, "settings show inventory")
# Open picker: refresh inventory, include it in choices.
old_open_head = '''        self.cancel_picker();
        let (draft, control_rect, dpi, selected_display_route) = {
            let ui = cell.borrow();
'''
new_open_head = '''        self.cancel_picker();
        if matches!(kind, PickerKind::DisplayOutputs | PickerKind::DisplayRoute) {
            cell.borrow_mut().refresh_display_outputs();
        }
        let (draft, display_outputs, control_rect, dpi, selected_display_route) = {
            let ui = cell.borrow();
'''
text = replace_once(text, old_open_head, new_open_head, "settings open picker inventory")
old_open_tuple = '''            (
                ui.draft.clone(),
                controls::value_control_rect(element.rect, element.kind),
                ui.dpi,
                ui.selected_display_route,
            )
'''
new_open_tuple = '''            (
                ui.draft.clone(),
                ui.display_outputs.clone(),
                controls::value_control_rect(element.rect, element.kind),
                ui.dpi,
                ui.selected_display_route,
            )
'''
text = replace_once(text, old_open_tuple, new_open_tuple, "settings open picker tuple")
text = replace_once(
    text,
    "            picker_choices(kind, &draft, &devices, &monitors, selected_display_route);\n",
    "            picker_choices(\n                kind,\n                &draft,\n                &devices,\n                &monitors,\n                &display_outputs,\n                selected_display_route,\n            );\n",
    "settings picker_choices call",
)
# picker element
text = replace_once(
    text,
    "        PickerKind::DisplayProfile => ElementId::DisplayProfile,\n        PickerKind::DisplayTopology => ElementId::DisplayTopology,\n",
    "        PickerKind::DisplayProfile => ElementId::DisplayProfile,\n        PickerKind::DisplayOutputs => ElementId::DisplayOutputs,\n        PickerKind::DisplayTopology => ElementId::DisplayTopology,\n",
    "settings picker element outputs",
)
# picker_choices signature
text = replace_once(
    text,
    "    monitors: &[crate::platform::monitor::MonitorGeometry],\n    selected_display_route: usize,\n",
    "    monitors: &[crate::platform::monitor::MonitorGeometry],\n    display_outputs: &[crate::display::DisplayOutput],\n    selected_display_route: usize,\n",
    "settings picker_choices signature",
)
# DisplayOutputs choices and simplify topology choices.
old_topology_choices = '''        PickerKind::DisplayTopology => {
            choices.extend(
                crate::display::DisplayTopology::ALL
                    .into_iter()
                    .map(|topology| PickerChoice {
                        label: topology.label().into(),
                        value: PickerValue::DisplayTopology(topology),
                    }),
            );
        }
'''
new_topology_choices = '''        PickerKind::DisplayOutputs => {
            choices.extend(display_outputs.iter().map(|output| PickerChoice {
                label: output.label(),
                value: PickerValue::DisplayOutput(output.route.clone()),
            }));
        }
        PickerKind::DisplayTopology => {
            for topology in [
                crate::display::DisplayTopology::Extend,
                crate::display::DisplayTopology::Clone,
            ] {
                choices.push(PickerChoice {
                    label: topology.label().into(),
                    value: PickerValue::DisplayTopology(topology),
                });
            }
        }
'''
text = replace_once(text, old_topology_choices, new_topology_choices, "settings output/topology choices")
old_route_choices = '''        PickerKind::DisplayRoute => {
            if let Some(profile) = draft.display_profiles.active() {
                choices.extend(profile.routes.iter().enumerate().map(|(index, route)| {
                    PickerChoice {
                        label: format!(
                            "Route {} — GPU {:016X} → {:016X}, source {}, target {} · {}",
                            index + 1,
                            route.source_adapter,
                            route.target_adapter,
                            route.source_id,
                            route.target_id,
                            route.target_path
                        ),
                        value: PickerValue::DisplayRoute(index),
                    }
                }));
            }
        }
'''
new_route_choices = '''        PickerKind::DisplayRoute => {
            if let Some(profile) = draft.display_profiles.active() {
                choices.extend(profile.routes.iter().enumerate().map(|(index, route)| {
                    let label = display_outputs
                        .iter()
                        .find(|output| crate::display::same_output(&output.route, route))
                        .map(crate::display::DisplayOutput::label)
                        .unwrap_or_else(|| format!("Output {} — currently unavailable", index + 1));
                    PickerChoice {
                        label,
                        value: PickerValue::DisplayRoute(index),
                    }
                }));
            }
        }
'''
text = replace_once(text, old_route_choices, new_route_choices, "settings friendly route choices")
# current index: DisplayOutputs is multi select.
text = replace_once(
    text,
    "        PickerKind::InputAllowlist | PickerKind::OutputAllowlist => 0,\n",
    "        PickerKind::InputAllowlist\n        | PickerKind::OutputAllowlist\n        | PickerKind::DisplayOutputs => 0,\n",
    "settings multi current index",
)
# selection indices DisplayOutputs.
old_selection_head = '''    let configured = match kind {
        PickerKind::InputAllowlist => draft.audio.cycle_input_allowlist.as_deref(),
        PickerKind::OutputAllowlist => draft.audio.cycle_output_allowlist.as_deref(),
        _ => return Vec::new(),
    };
'''
new_selection_head = '''    if kind == PickerKind::DisplayOutputs {
        let Some(profile) = draft.display_profiles.active() else {
            return Vec::new();
        };
        return choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| match &choice.value {
                PickerValue::DisplayOutput(route)
                    if profile
                        .routes
                        .iter()
                        .any(|configured| crate::display::same_output(configured, route)) =>
                {
                    Some(index)
                }
                _ => None,
            })
            .collect();
    }
    let configured = match kind {
        PickerKind::InputAllowlist => draft.audio.cycle_input_allowlist.as_deref(),
        PickerKind::OutputAllowlist => draft.audio.cycle_output_allowlist.as_deref(),
        _ => return Vec::new(),
    };
'''
text = replace_once(text, old_selection_head, new_selection_head, "settings output selection indices")
# current_picker_value exhaustive arm
text = replace_once(
    text,
    "        PickerKind::DisplayProfile => {\n            PickerValue::DisplayProfile(draft.display_profiles.active_profile.clone())\n        }\n        PickerKind::DisplayTopology => PickerValue::DisplayTopology(\n",
    "        PickerKind::DisplayProfile => {\n            PickerValue::DisplayProfile(draft.display_profiles.active_profile.clone())\n        }\n        PickerKind::DisplayOutputs => PickerValue::DisplayOutputs(Vec::new()),\n        PickerKind::DisplayTopology => PickerValue::DisplayTopology(\n",
    "settings current DisplayOutputs",
)
# Tests calling picker_choices now need an empty display inventory argument.
text = re.sub(
    r"picker_choices\((PickerKind::[A-Za-z]+), (&config|&draft), (&devices), (&\[\]), 0\)",
    r"picker_choices(\1, \2, \3, \4, &[], 0)",
    text,
)
# Add a focused pure UX test before the interaction test module ends.
test_marker = '''    #[test]
    fn value_for_uses_cached_devices_without_reacquiring_app() {
'''
ux_test = r'''    #[test]
    fn display_outputs_value_hides_raw_displayconfig_identity() {
        let mut ui = empty_settings_ui();
        let route = crate::display::DisplayRoute {
            target_path: r"\\?\DISPLAY#MONITOR-A".into(),
            target_adapter: 1,
            target_id: 2,
            output_technology: 5,
            ..Default::default()
        };
        ui.display_outputs = vec![crate::display::DisplayOutput {
            route: route.clone(),
            monitor_name: "Desk Monitor".into(),
            adapter_name: "AMD Radeon Graphics".into(),
            connector_name: "HDMI".into(),
            active: true,
        }];
        ui.draft.display_profiles.profiles = vec![crate::display::DisplayProfile {
            id: "ai".into(),
            name: "AI".into(),
            topology: crate::display::DisplayTopology::Custom,
            confirmed: false,
            routes: vec![route],
        }];
        ui.draft.display_profiles.active_profile = Some("ai".into());
        match ui.value_for(ElementId::DisplayOutputs) {
            ControlValue::Text(value) => {
                assert!(value.contains("Desk Monitor"));
                assert!(value.contains("AMD Radeon Graphics"));
                assert!(!value.contains("DISPLAY#"));
            }
            _ => panic!("unexpected control value variant"),
        }
    }

'''
text = replace_once(text, test_marker, ux_test + test_marker, "settings UX test")
write(path, text)

print("display UI refactor applied")
