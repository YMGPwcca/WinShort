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


def replace_between(text, start, end, replacement, label):
    a = text.find(start)
    if a < 0:
        raise SystemExit(f"{label}: start marker not found")
    b = text.find(end, a)
    if b < 0:
        raise SystemExit(f"{label}: end marker not found")
    return text[:a] + replacement + text[b:]


path = "src/display/mod.rs"
text = read(path)

text = replace_once(
    text,
    "use serde::{Deserialize, Serialize};\n",
    "use serde::{Deserialize, Serialize};\nuse windows::core::PCWSTR;\n",
    "display PCWSTR import",
)
text = replace_once(
    text,
    "    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_0,\n",
    "    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_0,\n    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,\n",
    "display source device import",
)
text = replace_once(
    text,
    "    DISPLAYCONFIG_VIDEO_SIGNAL_INFO_0, QDC_DATABASE_CURRENT, QDC_ONLY_ACTIVE_PATHS,\n    QDC_VIRTUAL_MODE_AWARE, SDC_ALLOW_CHANGES, SDC_APPLY, SDC_SAVE_TO_DATABASE,\n    SDC_USE_SUPPLIED_DISPLAY_CONFIG, SDC_VALIDATE, SDC_VIRTUAL_MODE_AWARE,\n",
    "    DISPLAYCONFIG_VIDEO_SIGNAL_INFO_0, QDC_ALL_PATHS, QDC_DATABASE_CURRENT,\n    QDC_ONLY_ACTIVE_PATHS, QDC_VIRTUAL_MODE_AWARE, SDC_ALLOW_CHANGES,\n    SDC_ALLOW_PATH_ORDER_CHANGES, SDC_APPLY, SDC_SAVE_TO_DATABASE,\n    SDC_TOPOLOGY_SUPPLIED, SDC_USE_SUPPLIED_DISPLAY_CONFIG, SDC_VALIDATE,\n    SDC_VIRTUAL_MODE_AWARE,\n",
    "display topology imports",
)
text = replace_once(
    text,
    "use windows::Win32::Graphics::Gdi::{\n    DISPLAYCONFIG_PATH_CLONE_GROUP_INVALID, DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID,\n    DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID, DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE,\n    DISPLAYCONFIG_PATH_TARGET_MODE_IDX_INVALID,\n};\n",
    "use windows::Win32::Graphics::Gdi::{\n    EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAYCONFIG_PATH_CLONE_GROUP_INVALID,\n    DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID, DISPLAYCONFIG_PATH_MODE_IDX_INVALID,\n    DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID, DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE,\n    DISPLAYCONFIG_PATH_TARGET_MODE_IDX_INVALID,\n};\n",
    "display GDI imports",
)

marker = "/// Stable 16-bit key used only in the packed hotkey message. The full profile\n"
insert = r'''#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayOutput {
    pub route: DisplayRoute,
    pub monitor_name: String,
    pub adapter_name: String,
    pub connector_name: String,
    pub active: bool,
}

impl DisplayOutput {
    pub fn label(&self) -> String {
        let state = if self.active { " · Active now" } else { "" };
        format!(
            "{} — {} · {}{}",
            self.monitor_name, self.adapter_name, self.connector_name, state
        )
    }
}

/// Compare two routes as one physical output target. Source IDs are deliberately
/// not authoritative here: Windows may bind the same connector to a different
/// source when topology changes.
pub fn same_output(left: &DisplayRoute, right: &DisplayRoute) -> bool {
    if left.target_path.is_empty() || right.target_path.is_empty() {
        return false;
    }
    if !left.target_path.eq_ignore_ascii_case(&right.target_path)
        || left.target_id != right.target_id
        || left.output_technology != right.output_technology
    {
        return false;
    }
    if left.target_adapter != 0 && right.target_adapter != 0 {
        return left.target_adapter == right.target_adapter;
    }
    left.source_adapter == 0
        || right.source_adapter == 0
        || left.source_adapter == right.source_adapter
}

pub fn route_has_complete_mode(route: &DisplayRoute) -> bool {
    route.source_width > 0
        && route.source_height > 0
        && route.active_width > 0
        && route.active_height > 0
        && route.refresh_numerator > 0
        && route.refresh_denominator > 0
        && matches!(route.rotation, 1..=4)
}

'''
text = replace_once(text, marker, insert + marker, "display output model")

new_validate = r'''pub fn validate_profile(profile: &DisplayProfile) -> Result<()> {
    if profile.id.trim().is_empty() {
        return Err(Error::config("display profile ID must not be empty"));
    }
    if profile.name.trim().is_empty() {
        return Err(Error::config("display profile name must not be empty"));
    }
    if profile.routes.is_empty() {
        return Err(Error::config(
            "display profile must contain at least one output",
        ));
    }
    if profile.routes.len() > MAX_PROFILE_ROUTES {
        return Err(Error::config(format!(
            "display profile contains more than {MAX_PROFILE_ROUTES} outputs"
        )));
    }
    let mut identities = std::collections::HashSet::new();
    for route in &profile.routes {
        if route.target_path.trim().is_empty() {
            return Err(Error::config("display output identity must not be empty"));
        }
        let identity = format!(
            "{}|{}|{}|{}",
            route.target_path.trim().to_ascii_lowercase(),
            route.target_adapter,
            route.target_id,
            route.output_technology
        );
        if !identities.insert(identity) {
            return Err(Error::config(format!(
                "display profile `{}` contains duplicate outputs",
                profile.name
            )));
        }
        // New/edited profiles may intentionally contain an inactive output.
        // Such a route has identity only until Test Apply lets Windows choose a
        // valid mode and WinShort captures it back. Confirmed profiles remain
        // strict because normal hotkey activation must be deterministic.
        if profile.confirmed && !route_has_complete_mode(route) {
            return Err(Error::config(
                "confirmed display profile contains an unresolved output mode",
            ));
        }
    }
    if profile.confirmed && profile.routes.len() > 1 {
        let distinct_sources = profile
            .routes
            .iter()
            .map(|route| (route.source_adapter, route.source_id))
            .collect::<std::collections::HashSet<_>>()
            .len();
        let distinct_source_modes = profile
            .routes
            .iter()
            .map(|route| {
                (
                    route.source_width,
                    route.source_height,
                    route.source_pixel_format,
                    route.source_position_x,
                    route.source_position_y,
                )
            })
            .collect::<std::collections::HashSet<_>>()
            .len();
        match profile.topology {
            DisplayTopology::Clone if distinct_source_modes != 1 => {
                return Err(Error::config(
                    "duplicate display profile outputs must share one source mode",
                ));
            }
            DisplayTopology::Extend if distinct_sources != profile.routes.len() => {
                return Err(Error::config(
                    "extend display profile outputs must use distinct sources",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

'''
text = replace_between(
    text,
    "pub fn validate_profile(profile: &DisplayProfile) -> Result<()> {",
    "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum ConfirmationState",
    new_validate,
    "validate_profile",
)

new_apply = r'''pub fn apply_profile(
    profile: &mut DisplayProfile,
    persist: bool,
) -> std::result::Result<DisplayRollback, DisplayApplyError> {
    validate_profile(profile).map_err(DisplayApplyError::plain)?;
    let before = query_state().map_err(DisplayApplyError::plain)?;
    let current_routes = capture_routes(&before).map_err(DisplayApplyError::plain)?;
    let all = query_all_paths().map_err(DisplayApplyError::plain)?;
    let topology_paths =
        build_topology_paths(profile, &all.paths).map_err(DisplayApplyError::plain)?;
    let strategy = validate_topology_paths(&topology_paths).map_err(DisplayApplyError::plain)?;
    if let Err(error) = apply_topology_paths(&topology_paths, strategy) {
        return Err(DisplayApplyError::after_restore_failure(
            error,
            &before,
            &current_routes,
        ));
    }

    let topology_state = match query_state() {
        Ok(state) => state,
        Err(error) => {
            return Err(DisplayApplyError::after_restore_failure(
                error,
                &before,
                &current_routes,
            ));
        }
    };
    let topology_routes = match capture_routes(&topology_state) {
        Ok(routes) => routes,
        Err(error) => {
            return Err(DisplayApplyError::after_restore_failure(
                error,
                &before,
                &current_routes,
            ));
        }
    };
    let mut resolved = match resolve_profile_outputs(profile, &topology_routes) {
        Ok(profile) => profile,
        Err(error) => {
            return Err(DisplayApplyError::after_restore_failure(
                error,
                &before,
                &current_routes,
            ));
        }
    };
    let original_confirmed = resolved.confirmed;
    resolved.confirmed = true;
    if let Err(error) = validate_profile(&resolved) {
        return Err(DisplayApplyError::after_restore_failure(
            error,
            &before,
            &current_routes,
        ));
    }
    resolved.confirmed = original_confirmed;

    let (paths, modes) = match resolve_profile(&resolved, &topology_state, &topology_routes) {
        Ok(values) => values,
        Err(error) => {
            return Err(DisplayApplyError::after_restore_failure(
                error,
                &before,
                &current_routes,
            ));
        }
    };
    if let Err(error) = set_display_config(&paths, &modes, false, false) {
        return Err(DisplayApplyError::after_restore_failure(
            error,
            &before,
            &current_routes,
        ));
    }
    if let Err(error) = set_display_config(&paths, &modes, true, persist) {
        return Err(DisplayApplyError::after_restore_failure(
            error,
            &before,
            &current_routes,
        ));
    }

    let after = match query_state() {
        Ok(state) => state,
        Err(error) => {
            return Err(DisplayApplyError::after_restore_failure(
                error,
                &before,
                &current_routes,
            ));
        }
    };
    let after_routes = match capture_routes(&after) {
        Ok(routes) => routes,
        Err(error) => {
            return Err(DisplayApplyError::after_restore_failure(
                error,
                &before,
                &current_routes,
            ));
        }
    };
    let final_profile = match resolve_profile_outputs(&resolved, &after_routes) {
        Ok(profile) => profile,
        Err(error) => {
            return Err(DisplayApplyError::after_restore_failure(
                error,
                &before,
                &current_routes,
            ));
        }
    };
    let verified = final_profile.routes.len() == resolved.routes.len()
        && resolved.routes.iter().zip(&final_profile.routes).all(|(expected, actual)| {
            profile_route_matches(resolved.topology, expected, actual)
        });
    if !verified {
        return Err(DisplayApplyError::after_restore_failure(
            Error::config("display profile verification failed"),
            &before,
            &current_routes,
        ));
    }
    profile.routes = final_profile.routes;
    Ok(DisplayRollback {
        state: before,
        routes: current_routes,
    })
}
'''
text = replace_between(
    text,
    "pub fn apply_profile(\n",
    "pub fn rollback(rollback: &DisplayRollback) -> Result<()> {",
    new_apply,
    "apply_profile",
)

helper_marker = "fn source_mode(\n"
helpers = r'''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopologyApplyStrategy {
    PersistenceDatabase,
    BestMode,
}

fn resolve_profile_outputs(
    requested: &DisplayProfile,
    actual_routes: &[DisplayRoute],
) -> Result<DisplayProfile> {
    let mut routes = Vec::with_capacity(requested.routes.len());
    let mut used = std::collections::HashSet::new();
    for desired in &requested.routes {
        let index = resolve_output_route_index(desired, actual_routes)?;
        if !used.insert(index) {
            return Err(Error::config(
                "two configured outputs resolved to the same active display",
            ));
        }
        let mut actual = actual_routes
            .get(index)
            .cloned()
            .ok_or_else(|| Error::config("resolved display output index is invalid"))?;
        if route_has_complete_mode(desired) {
            copy_requested_mode(desired, &mut actual);
        }
        routes.push(actual);
    }
    Ok(DisplayProfile {
        id: requested.id.clone(),
        name: requested.name.clone(),
        topology: requested.topology,
        confirmed: requested.confirmed,
        routes,
    })
}

fn resolve_output_route_index(route: &DisplayRoute, available: &[DisplayRoute]) -> Result<usize> {
    let mut candidates = available
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.target_path.eq_ignore_ascii_case(&route.target_path)
                && candidate.target_id == route.target_id
                && candidate.output_technology == route.output_technology
        })
        .collect::<Vec<_>>();
    if candidates.len() == 1 {
        return Ok(candidates[0].0);
    }
    candidates.sort_by_key(|(_, candidate)| {
        std::cmp::Reverse(output_match_score(route, candidate))
    });
    match candidates.as_slice() {
        [] => resolve_route_index(route, available),
        [best, second, ..] if output_match_score(route, best.1) == output_match_score(route, second.1) => {
            Err(Error::config(format!(
                "display output `{}` resolves ambiguously",
                route.target_path
            )))
        }
        [best, ..] => Ok(best.0),
    }
}

fn output_match_score(route: &DisplayRoute, candidate: &DisplayRoute) -> u32 {
    u32::from(route.target_adapter != 0 && route.target_adapter == candidate.target_adapter) * 8
        + u32::from(route.source_adapter != 0 && route.source_adapter == candidate.source_adapter) * 4
        + u32::from(route.source_id == candidate.source_id) * 2
        + u32::from(route.target_id == candidate.target_id)
}

fn copy_requested_mode(source: &DisplayRoute, target: &mut DisplayRoute) {
    target.rotation = source.rotation;
    target.scaling = source.scaling;
    target.refresh_numerator = source.refresh_numerator;
    target.refresh_denominator = source.refresh_denominator;
    target.scanline_ordering = source.scanline_ordering;
    target.source_width = source.source_width;
    target.source_height = source.source_height;
    target.source_pixel_format = source.source_pixel_format;
    target.source_position_x = source.source_position_x;
    target.source_position_y = source.source_position_y;
    target.active_width = source.active_width;
    target.active_height = source.active_height;
    target.total_width = source.total_width.max(source.active_width);
    target.total_height = source.total_height.max(source.active_height);
}

fn build_topology_paths(
    profile: &DisplayProfile,
    available: &[DISPLAYCONFIG_PATH_INFO],
) -> Result<Vec<DISPLAYCONFIG_PATH_INFO>> {
    let mut choices = Vec::with_capacity(profile.routes.len());
    for route in &profile.routes {
        let mut candidates = available
            .iter()
            .copied()
            .filter(|path| path.targetInfo.targetAvailable.as_bool())
            .filter_map(|path| {
                let target_path = target_device_path(&path).ok()?;
                target_path
                    .eq_ignore_ascii_case(&route.target_path)
                    .then_some(path)
            })
            .filter(|path| {
                path.targetInfo.id == route.target_id
                    && (route.output_technology == 0
                        || path.targetInfo.outputTechnology.0 == route.output_technology)
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|path| std::cmp::Reverse(path_match_score(route, path)));
        if candidates.is_empty() {
            return Err(Error::config(format!(
                "display output `{}` is unavailable",
                route.target_path
            )));
        }
        choices.push(candidates);
    }

    let selected = match profile.topology {
        DisplayTopology::Clone if choices.len() > 1 => choose_clone_paths(&choices)?,
        DisplayTopology::Extend if choices.len() > 1 => choose_distinct_source_paths(&choices)?,
        _ if choices.len() > 1 => choose_distinct_source_paths(&choices)
            .unwrap_or_else(|_| choices.iter().map(|set| set[0]).collect()),
        _ => choices.iter().map(|set| set[0]).collect(),
    };
    Ok(selected
        .into_iter()
        .enumerate()
        .map(|(index, path)| prepare_topology_path(
            path,
            (profile.topology == DisplayTopology::Clone).then_some(0u16),
            index,
        ))
        .collect())
}

fn path_match_score(route: &DisplayRoute, path: &DISPLAYCONFIG_PATH_INFO) -> u32 {
    let source_adapter = luid_to_u64(path.sourceInfo.adapterId);
    let target_adapter = luid_to_u64(path.targetInfo.adapterId);
    u32::from(route.target_adapter != 0 && route.target_adapter == target_adapter) * 8
        + u32::from(route.source_adapter != 0 && route.source_adapter == source_adapter) * 4
        + u32::from(route.source_id == path.sourceInfo.id) * 2
        + u32::from(route.target_id == path.targetInfo.id)
}

fn source_key(path: &DISPLAYCONFIG_PATH_INFO) -> (u64, u32) {
    (luid_to_u64(path.sourceInfo.adapterId), path.sourceInfo.id)
}

fn choose_distinct_source_paths(
    choices: &[Vec<DISPLAYCONFIG_PATH_INFO>],
) -> Result<Vec<DISPLAYCONFIG_PATH_INFO>> {
    fn visit(
        choices: &[Vec<DISPLAYCONFIG_PATH_INFO>],
        index: usize,
        used: &mut std::collections::HashSet<(u64, u32)>,
        selected: &mut Vec<DISPLAYCONFIG_PATH_INFO>,
    ) -> bool {
        if index == choices.len() {
            return true;
        }
        for path in &choices[index] {
            let key = source_key(path);
            if !used.insert(key) {
                continue;
            }
            selected.push(*path);
            if visit(choices, index + 1, used, selected) {
                return true;
            }
            selected.pop();
            used.remove(&key);
        }
        false
    }

    let mut used = std::collections::HashSet::new();
    let mut selected = Vec::with_capacity(choices.len());
    if visit(choices, 0, &mut used, &mut selected) {
        Ok(selected)
    } else {
        Err(Error::config(
            "Windows exposes no distinct display sources for this Extend layout",
        ))
    }
}

fn choose_clone_paths(choices: &[Vec<DISPLAYCONFIG_PATH_INFO>]) -> Result<Vec<DISPLAYCONFIG_PATH_INFO>> {
    // Windows 10+ can express a virtual clone group even when paths have
    // different source identifiers. Prefer that when every chosen path is
    // virtual-mode aware; SetDisplayConfig remains the final authority.
    let preferred = choices.iter().map(|set| set[0]).collect::<Vec<_>>();
    if preferred
        .iter()
        .all(|path| path.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE != 0)
    {
        return Ok(preferred);
    }
    for first in &choices[0] {
        let key = source_key(first);
        let mut result = vec![*first];
        let mut valid = true;
        for set in &choices[1..] {
            if let Some(path) = set.iter().find(|candidate| source_key(candidate) == key) {
                result.push(*path);
            } else {
                valid = false;
                break;
            }
        }
        if valid {
            return Ok(result);
        }
    }
    Err(Error::config(
        "Windows exposes no compatible source for this Duplicate layout",
    ))
}

fn prepare_topology_path(
    mut path: DISPLAYCONFIG_PATH_INFO,
    clone_group: Option<u16>,
    _index: usize,
) -> DISPLAYCONFIG_PATH_INFO {
    // DISPLAYCONFIG_PATH_ACTIVE is 0x1. Keep capability bits supplied by
    // QueryDisplayConfig and mark only the selected paths active.
    path.flags |= 0x1;
    if path.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE != 0 {
        let clone = u32::from(clone_group.unwrap_or(DISPLAYCONFIG_PATH_CLONE_GROUP_INVALID as u16));
        path.sourceInfo.Anonymous =
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_SOURCE_INFO_0 {
                Anonymous: windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_SOURCE_INFO_0_0 {
                    _bitfield: clone | (DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID << 16),
                },
            };
        path.targetInfo.Anonymous =
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_TARGET_INFO_0 {
                Anonymous: windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_TARGET_INFO_0_0 {
                    _bitfield: DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID
                        | (DISPLAYCONFIG_PATH_TARGET_MODE_IDX_INVALID << 16),
                },
            };
    } else {
        path.sourceInfo.Anonymous =
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_SOURCE_INFO_0 {
                modeInfoIdx: DISPLAYCONFIG_PATH_MODE_IDX_INVALID,
            };
        path.targetInfo.Anonymous =
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_TARGET_INFO_0 {
                modeInfoIdx: DISPLAYCONFIG_PATH_MODE_IDX_INVALID,
            };
    }
    path
}

fn validate_topology_paths(paths: &[DISPLAYCONFIG_PATH_INFO]) -> Result<TopologyApplyStrategy> {
    match set_topology_paths(paths, false, TopologyApplyStrategy::PersistenceDatabase) {
        Ok(()) => Ok(TopologyApplyStrategy::PersistenceDatabase),
        Err(database_error) => match set_topology_paths(paths, false, TopologyApplyStrategy::BestMode) {
            Ok(()) => Ok(TopologyApplyStrategy::BestMode),
            Err(best_mode_error) => Err(Error::config(format!(
                "display topology is unsupported: database mode failed ({database_error}); best-mode validation failed ({best_mode_error})"
            ))),
        },
    }
}

fn apply_topology_paths(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    strategy: TopologyApplyStrategy,
) -> Result<()> {
    set_topology_paths(paths, true, strategy)
}

fn set_topology_paths(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    apply: bool,
    strategy: TopologyApplyStrategy,
) -> Result<()> {
    let mut flags = SDC_ALLOW_CHANGES | SDC_VIRTUAL_MODE_AWARE;
    flags |= if apply { SDC_APPLY } else { SDC_VALIDATE };
    match strategy {
        TopologyApplyStrategy::PersistenceDatabase => {
            flags |= SDC_TOPOLOGY_SUPPLIED | SDC_ALLOW_PATH_ORDER_CHANGES;
        }
        TopologyApplyStrategy::BestMode => {
            flags |= SDC_USE_SUPPLIED_DISPLAY_CONFIG;
        }
    }
    let status = unsafe { SetDisplayConfig(Some(paths), None, flags) };
    if status != 0 {
        return Err(Error::os("SetDisplayConfig(topology)", status as u32));
    }
    Ok(())
}

'''
text = replace_once(text, helper_marker, helpers + helper_marker, "topology helpers")

# Device/inventory helpers are inserted before the existing target_device_path.
old_target = r'''fn target_device_path(path: &DISPLAYCONFIG_PATH_INFO) -> Result<String> {
    let mut request = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        ..Default::default()
    };
    let status = unsafe {
        DisplayConfigGetDeviceInfo((&mut request as *mut DISPLAYCONFIG_TARGET_DEVICE_NAME).cast())
    };
    if status != 0 {
        return Err(Error::os(
            "DisplayConfigGetDeviceInfo(GET_TARGET_NAME)",
            status as u32,
        ));
    }
    utf16_string(&request.monitorDevicePath)
        .ok_or_else(|| Error::config("active display target has no stable device path"))
}
'''
new_target = r'''fn target_device_name(path: &DISPLAYCONFIG_PATH_INFO) -> Result<DISPLAYCONFIG_TARGET_DEVICE_NAME> {
    let mut request = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        ..Default::default()
    };
    let status = unsafe {
        DisplayConfigGetDeviceInfo((&mut request as *mut DISPLAYCONFIG_TARGET_DEVICE_NAME).cast())
    };
    if status != 0 {
        return Err(Error::os(
            "DisplayConfigGetDeviceInfo(GET_TARGET_NAME)",
            status as u32,
        ));
    }
    Ok(request)
}

fn target_device_path(path: &DISPLAYCONFIG_PATH_INFO) -> Result<String> {
    let request = target_device_name(path)?;
    utf16_string(&request.monitorDevicePath)
        .ok_or_else(|| Error::config("display target has no stable device path"))
}

fn monitor_friendly_name(path: &DISPLAYCONFIG_PATH_INFO) -> String {
    target_device_name(path)
        .ok()
        .and_then(|request| utf16_string(&request.monitorFriendlyDeviceName))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Monitor".into())
}

fn adapter_friendly_name(path: &DISPLAYCONFIG_PATH_INFO) -> String {
    let mut request = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        },
        ..Default::default()
    };
    let status = unsafe {
        DisplayConfigGetDeviceInfo((&mut request as *mut DISPLAYCONFIG_SOURCE_DEVICE_NAME).cast())
    };
    if status == 0 {
        let mut device = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        if unsafe {
            EnumDisplayDevicesW(
                PCWSTR(request.viewGdiDeviceName.as_ptr()),
                0,
                &mut device,
                0,
            )
            .as_bool()
        } {
            if let Some(name) = utf16_string(&device.DeviceString) {
                if !name.trim().is_empty() {
                    return name;
                }
            }
        }
        if let Some(name) = utf16_string(&request.viewGdiDeviceName) {
            return name;
        }
    }
    "Display adapter".into()
}

fn connector_name(output_technology: i32) -> String {
    match output_technology as u32 {
        4 => "DVI".into(),
        5 => "HDMI".into(),
        6 => "LVDS".into(),
        10 | 11 => "DisplayPort".into(),
        0x8000_0000 => "Internal".into(),
        value => format!("Output {value}"),
    }
}

fn identity_route(path: &DISPLAYCONFIG_PATH_INFO, target_path: String) -> DisplayRoute {
    DisplayRoute {
        target_path,
        source_adapter: luid_to_u64(path.sourceInfo.adapterId),
        source_id: path.sourceInfo.id,
        target_adapter: luid_to_u64(path.targetInfo.adapterId),
        target_id: path.targetInfo.id,
        path_flags: path.flags,
        source_status_flags: path.sourceInfo.statusFlags,
        target_status_flags: path.targetInfo.statusFlags,
        target_available: path.targetInfo.targetAvailable.as_bool(),
        output_technology: path.targetInfo.outputTechnology.0,
        rotation: path.targetInfo.rotation.0,
        scaling: path.targetInfo.scaling.0,
        refresh_numerator: path.targetInfo.refreshRate.Numerator,
        refresh_denominator: path.targetInfo.refreshRate.Denominator,
        scanline_ordering: path.targetInfo.scanLineOrdering.0,
        ..Default::default()
    }
}

pub fn output_inventory() -> Result<Vec<DisplayOutput>> {
    let active_state = query_state()?;
    let active_routes = capture_routes(&active_state)?;
    let all = query_all_paths()?;
    let mut outputs: Vec<DisplayOutput> = Vec::new();
    for path in &all.paths {
        if !path.targetInfo.targetAvailable.as_bool() {
            continue;
        }
        let target_path = match target_device_path(path) {
            Ok(path) => path,
            Err(error) => {
                crate::warn_!("display output skipped because identity is unavailable: {error}");
                continue;
            }
        };
        let identity = identity_route(path, target_path);
        let active_route = active_routes
            .iter()
            .find(|route| same_output(route, &identity))
            .cloned();
        let output = DisplayOutput {
            route: active_route.clone().unwrap_or(identity),
            monitor_name: monitor_friendly_name(path),
            adapter_name: adapter_friendly_name(path),
            connector_name: connector_name(path.targetInfo.outputTechnology.0),
            active: active_route.is_some(),
        };
        if let Some(existing) = outputs
            .iter_mut()
            .find(|existing| same_output(&existing.route, &output.route))
        {
            if output.active && !existing.active {
                *existing = output;
            }
        } else {
            outputs.push(output);
        }
    }
    outputs.sort_by_key(|output| output.label().to_ascii_lowercase());
    Ok(outputs)
}
'''
text = replace_once(text, old_target, new_target, "target device helpers")

query_marker = "fn query_current_topology() -> Result<DISPLAYCONFIG_TOPOLOGY_ID> {\n"
query_all = r'''fn query_all_paths() -> Result<DisplayState> {
    for _ in 0..QUERY_RETRIES {
        let flags = QDC_ALL_PATHS | QDC_VIRTUAL_MODE_AWARE;
        let mut path_count = 0u32;
        let mut mode_count = 0u32;
        let status = unsafe { GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count) };
        if status.0 != 0 {
            return Err(Error::os("GetDisplayConfigBufferSizes(all paths)", status.0));
        }
        if path_count as usize > MAX_QUERY_PATHS || mode_count as usize > MAX_QUERY_MODES {
            return Err(Error::config("display route inventory exceeds safety bounds"));
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count.max(1) as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count.max(1) as usize];
        let status = unsafe {
            QueryDisplayConfig(
                flags,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if status.0 == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if status.0 != 0 {
            return Err(Error::os("QueryDisplayConfig(all paths)", status.0));
        }
        paths.truncate(path_count as usize);
        modes.truncate(mode_count as usize);
        return Ok(DisplayState {
            topology: DISPLAYCONFIG_TOPOLOGY_ID::default(),
            paths,
            modes,
        });
    }
    Err(Error::os(
        "QueryDisplayConfig(all paths)",
        ERROR_INSUFFICIENT_BUFFER,
    ))
}

'''
text = replace_once(text, query_marker, query_all + query_marker, "query all paths")

# Add backend tests before the current first test in the display test module.
test_marker = "    #[test]\n    fn same_monitor_on_two_connectors_resolves_as_two_distinct_routes() {\n"
backend_tests = r'''    #[test]
    fn unresolved_draft_route_is_valid_until_confirmed() {
        let mut profile = DisplayProfile {
            id: "draft".into(),
            name: "Draft".into(),
            topology: DisplayTopology::Custom,
            confirmed: false,
            routes: vec![DisplayRoute {
                target_path: "DISPLAY\\MONITOR-A".into(),
                target_id: 1,
                output_technology: 5,
                ..Default::default()
            }],
        };
        assert!(validate_profile(&profile).is_ok());
        profile.confirmed = true;
        assert!(validate_profile(&profile).is_err());
    }

    #[test]
    fn same_output_keeps_same_panel_routes_on_different_gpus_distinct() {
        let igpu = DisplayRoute {
            target_path: "DISPLAY\\MONITOR-A".into(),
            target_adapter: 10,
            target_id: 1,
            output_technology: 5,
            ..Default::default()
        };
        let dgpu = DisplayRoute {
            target_adapter: 20,
            output_technology: 10,
            ..igpu.clone()
        };
        assert!(!same_output(&igpu, &dgpu));
        assert!(same_output(&igpu, &igpu));
    }

'''
text = replace_once(text, test_marker, backend_tests + test_marker, "display backend tests")
write(path, text)

# Config validation: unresolved modes are allowed only for unconfirmed drafts.
path = "src/config/validate.rs"
text = read(path)
old = r'''            if route.source_width == 0
                || route.source_height == 0
                || route.active_width == 0
                || route.active_height == 0
            {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].resolution"),
                    "display route resolution must be positive",
                ));
            }
            if route.refresh_numerator == 0 || route.refresh_denominator == 0 {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].refresh"),
                    "display route refresh rate must be positive",
                ));
            }
            if !matches!(route.rotation, 1..=4) {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].rotation"),
                    "display route rotation must be 1..=4",
                ));
            }
'''
new = r'''            if profile.confirmed && !crate::display::route_has_complete_mode(route) {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].mode"),
                    "confirmed display output must have a resolved mode",
                ));
            }
'''
text = replace_once(text, old, new, "config display mode validation")
text = replace_once(
    text,
    "        if profile.routes.len() > 1 {\n",
    "        if profile.confirmed && profile.routes.len() > 1 {\n",
    "config topology strictness",
)
write(path, text)

# Config repair: keep identity-only routes for unconfirmed profiles.
path = "src/config/model.rs"
text = read(path)
old = r'''                        !route.target_path.trim().is_empty()
                            && route.source_width > 0
                            && route.source_height > 0
                            && route.active_width > 0
                            && route.active_height > 0
                            && route.refresh_numerator > 0
                            && route.refresh_denominator > 0
                            && matches!(route.rotation, 1..=4)
                            && seen_routes.insert(format!(
                                "{}|{}|{}|{}|{}",
                                route.target_path.trim().to_ascii_lowercase(),
                                route.source_adapter,
                                route.source_id,
                                route.target_adapter,
                                route.target_id
                            ))
'''
new = r'''                        !route.target_path.trim().is_empty()
                            && (!profile.confirmed
                                || crate::display::route_has_complete_mode(route))
                            && seen_routes.insert(format!(
                                "{}|{}|{}|{}",
                                route.target_path.trim().to_ascii_lowercase(),
                                route.target_adapter,
                                route.target_id,
                                route.output_technology
                            ))
'''
text = replace_once(text, old, new, "config repair display draft modes")
write(path, text)

print("display backend refactor applied")
