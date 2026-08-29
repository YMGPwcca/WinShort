//! Display topology profiles backed by the documented DisplayConfig API.
//!
//! Profiles store stable target device paths plus the source/target mode data
//! required by `SetDisplayConfig`. Runtime snapshots are kept only long enough
//! to offer a bounded rollback after an explicit apply.

use serde::{Deserialize, Serialize};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig, SetDisplayConfig,
    DISPLAYCONFIG_2DREGION, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_0,
    DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_PIXELFORMAT, DISPLAYCONFIG_RATIONAL,
    DISPLAYCONFIG_ROTATION, DISPLAYCONFIG_SCALING, DISPLAYCONFIG_SCANLINE_ORDERING,
    DISPLAYCONFIG_SOURCE_MODE, DISPLAYCONFIG_TARGET_DEVICE_NAME, DISPLAYCONFIG_TARGET_MODE,
    DISPLAYCONFIG_TOPOLOGY_CLONE, DISPLAYCONFIG_TOPOLOGY_EXTEND, DISPLAYCONFIG_TOPOLOGY_EXTERNAL,
    DISPLAYCONFIG_TOPOLOGY_ID, DISPLAYCONFIG_TOPOLOGY_INTERNAL, DISPLAYCONFIG_VIDEO_SIGNAL_INFO,
    DISPLAYCONFIG_VIDEO_SIGNAL_INFO_0, QDC_DATABASE_CURRENT, QDC_ONLY_ACTIVE_PATHS,
    SDC_ALLOW_CHANGES, SDC_APPLY, SDC_PATH_PERSIST_IF_REQUIRED, SDC_SAVE_TO_DATABASE,
    SDC_USE_SUPPLIED_DISPLAY_CONFIG, SDC_VALIDATE,
};
use windows::Win32::Foundation::{LUID, POINTL};

use crate::error::{Error, Result};

const MAX_QUERY_PATHS: usize = 64;
const MAX_QUERY_MODES: usize = 256;
const QUERY_RETRIES: usize = 3;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const MAX_PROFILE_ROUTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTopology {
    Internal,
    Clone,
    #[default]
    Extend,
    External,
    Custom,
}

impl DisplayTopology {
    pub fn label(self) -> &'static str {
        match self {
            Self::Internal => "Internal",
            Self::Clone => "Duplicate",
            Self::Extend => "Extend",
            Self::External => "External only",
            Self::Custom => "Custom",
        }
    }

    fn from_native(value: DISPLAYCONFIG_TOPOLOGY_ID) -> Self {
        match value {
            DISPLAYCONFIG_TOPOLOGY_INTERNAL => Self::Internal,
            DISPLAYCONFIG_TOPOLOGY_CLONE => Self::Clone,
            DISPLAYCONFIG_TOPOLOGY_EXTEND => Self::Extend,
            DISPLAYCONFIG_TOPOLOGY_EXTERNAL => Self::External,
            _ => Self::Custom,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayProfilesCfg {
    pub enabled: bool,
    pub active_profile: Option<String>,
    pub profiles: Vec<DisplayProfile>,
}

impl Default for DisplayProfilesCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            active_profile: None,
            profiles: Vec::new(),
        }
    }
}

impl DisplayProfilesCfg {
    pub fn active(&self) -> Option<&DisplayProfile> {
        self.active_profile.as_deref().and_then(|id| {
            self.profiles
                .iter()
                .find(|profile| profile.id.eq_ignore_ascii_case(id))
        })
    }
    pub fn upsert(&mut self, profile: DisplayProfile) {
        let id = profile.id.clone();
        if let Some(existing) = self
            .profiles
            .iter_mut()
            .find(|existing| existing.id.eq_ignore_ascii_case(&id))
        {
            *existing = profile;
        } else {
            self.profiles.push(profile);
        }
        self.active_profile = Some(id);
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.profiles.len();
        self.profiles
            .retain(|profile| !profile.id.eq_ignore_ascii_case(id));
        if self
            .active_profile
            .as_deref()
            .is_some_and(|active| active.eq_ignore_ascii_case(id))
        {
            self.active_profile = self.profiles.first().map(|profile| profile.id.clone());
        }
        before != self.profiles.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DisplayProfile {
    pub id: String,
    pub name: String,
    pub topology: DisplayTopology,
    pub routes: Vec<DisplayRoute>,
}

/// One source-to-target DisplayConfig path and its active source/target modes.
/// The target device path is the stable route identity; adapter LUIDs are
/// retained as a fallback and are refreshed from the live topology on apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DisplayRoute {
    pub target_path: String,
    pub source_adapter: u64,
    pub source_id: u32,
    pub target_adapter: u64,
    pub target_id: u32,
    pub path_flags: u32,
    pub source_status_flags: u32,
    pub target_status_flags: u32,
    pub target_available: bool,
    pub output_technology: i32,
    pub rotation: i32,
    pub scaling: i32,
    pub refresh_numerator: u32,
    pub refresh_denominator: u32,
    pub scanline_ordering: i32,
    pub source_width: u32,
    pub source_height: u32,
    pub source_pixel_format: i32,
    pub source_position_x: i32,
    pub source_position_y: i32,
    pub pixel_rate: u64,
    pub hsync_numerator: u32,
    pub hsync_denominator: u32,
    pub vsync_numerator: u32,
    pub vsync_denominator: u32,
    pub active_width: u32,
    pub active_height: u32,
    pub total_width: u32,
    pub total_height: u32,
    pub video_standard: u32,
}

/// Resolve one persisted route against a live route inventory.
///
/// Stable target paths are preferred and permit adapter-LUID changes across
/// boots. A path/ID collision is ambiguous and is refused instead of choosing
/// an arbitrary output.
pub fn resolve_route_index(route: &DisplayRoute, available: &[DisplayRoute]) -> Result<usize> {
    let mut matches = available.iter().enumerate().filter(|(_, candidate)| {
        !route.target_path.is_empty()
            && candidate
                .target_path
                .eq_ignore_ascii_case(&route.target_path)
    });
    let first = matches.next().map(|(index, _)| index);
    let second = matches.next().map(|(index, _)| index);
    if let Some(index) = first {
        if second.is_some() {
            return Err(Error::config(format!(
                "display route `{}` resolves ambiguously",
                route.target_path
            )));
        }
        return Ok(index);
    }

    let mut fallback = available.iter().enumerate().filter(|(_, candidate)| {
        candidate.source_adapter == route.source_adapter
            && candidate.source_id == route.source_id
            && candidate.target_adapter == route.target_adapter
            && candidate.target_id == route.target_id
    });
    let Some(index) = fallback.next().map(|(index, _)| index) else {
        return Err(Error::config(format!(
            "display route `{}` is unavailable",
            route.target_path
        )));
    };
    if fallback.next().is_some() {
        return Err(Error::config(format!(
            "display route `{}` resolves ambiguously",
            route.target_path
        )));
    }
    Ok(index)
}

pub fn validate_profile(profile: &DisplayProfile) -> Result<()> {
    if profile.id.trim().is_empty() {
        return Err(Error::config("display profile ID must not be empty"));
    }
    if profile.name.trim().is_empty() {
        return Err(Error::config("display profile name must not be empty"));
    }
    if profile.routes.is_empty() {
        return Err(Error::config(
            "display profile must contain at least one route",
        ));
    }
    if profile.routes.len() > MAX_PROFILE_ROUTES {
        return Err(Error::config(format!(
            "display profile contains more than {MAX_PROFILE_ROUTES} routes"
        )));
    }
    let mut identities = std::collections::HashSet::new();
    for route in &profile.routes {
        if route.target_path.trim().is_empty() {
            return Err(Error::config("display route target path must not be empty"));
        }
        let identity = route.target_path.trim().to_ascii_lowercase();
        if !identities.insert(identity) {
            return Err(Error::config(format!(
                "display profile `{}` contains duplicate routes",
                profile.name
            )));
        }
    }
    Ok(())
}

#[derive(Clone)]
struct DisplayState {
    topology: DISPLAYCONFIG_TOPOLOGY_ID,
    paths: Vec<DISPLAYCONFIG_PATH_INFO>,
    modes: Vec<DISPLAYCONFIG_MODE_INFO>,
}

/// Runtime rollback token returned by a successful explicit profile apply.
pub struct DisplayRollback {
    state: DisplayState,
}

pub fn capture_current_profile(id: &str, name: &str) -> Result<DisplayProfile> {
    if id.trim().is_empty() || name.trim().is_empty() {
        return Err(Error::config(
            "display profile ID and name must not be empty",
        ));
    }
    let state = query_state()?;
    let routes = capture_routes(&state)?;
    if routes.is_empty() {
        return Err(Error::config(
            "current display topology has no active routes",
        ));
    }
    Ok(DisplayProfile {
        id: id.into(),
        name: name.into(),
        topology: DisplayTopology::from_native(state.topology),
        routes,
    })
}

/// Apply a profile after validating and resolving every route. The returned
/// rollback token remains valid until the caller's safety window expires.
pub fn apply_profile(profile: &DisplayProfile) -> Result<DisplayRollback> {
    validate_profile(profile)?;
    let before = query_state()?;
    let current_routes = capture_routes(&before)?;
    let (paths, modes) = resolve_profile(profile, &before, &current_routes)?;
    set_display_config(&paths, &modes, false)?;
    set_display_config(&paths, &modes, true)?;
    let after = match query_state() {
        Ok(state) => state,
        Err(error) => {
            let _ = set_display_config(&before.paths, &before.modes, true);
            return Err(error);
        }
    };
    let after_routes = match capture_routes(&after) {
        Ok(routes) => routes,
        Err(error) => {
            let _ = set_display_config(&before.paths, &before.modes, true);
            return Err(error);
        }
    };
    for route in &profile.routes {
        if resolve_route_index(route, &after_routes).is_err() {
            let _ = set_display_config(&before.paths, &before.modes, true);
            return Err(Error::config(
                "display profile verification failed; previous topology restored",
            ));
        }
    }
    Ok(DisplayRollback { state: before })
}

pub fn rollback(rollback: &DisplayRollback) -> Result<()> {
    set_display_config(&rollback.state.paths, &rollback.state.modes, true)
}

fn resolve_profile(
    profile: &DisplayProfile,
    state: &DisplayState,
    current_routes: &[DisplayRoute],
) -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>)> {
    let mut paths = Vec::with_capacity(profile.routes.len());
    let mut modes = Vec::with_capacity(profile.routes.len() * 2);
    for route in &profile.routes {
        let route_index = resolve_route_index(route, current_routes)?;
        let current = state
            .paths
            .get(route_index)
            .ok_or_else(|| Error::config("display route index is outside the live topology"))?;
        let source_mode_index = modes.len() as u32;
        modes.push(source_mode(current, route, source_mode_index));
        let target_mode_index = modes.len() as u32;
        modes.push(target_mode(current, route, target_mode_index));

        let mut path = *current;
        path.flags = route.path_flags;
        path.sourceInfo.statusFlags = route.source_status_flags;
        path.targetInfo.statusFlags = route.target_status_flags;
        path.targetInfo.outputTechnology =
            windows::Win32::Devices::Display::DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY(
                route.output_technology,
            );
        path.targetInfo.rotation = DISPLAYCONFIG_ROTATION(route.rotation);
        path.targetInfo.scaling = DISPLAYCONFIG_SCALING(route.scaling);
        path.targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
            Numerator: route.refresh_numerator,
            Denominator: route.refresh_denominator,
        };
        path.targetInfo.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING(route.scanline_ordering);
        path.targetInfo.targetAvailable = route.target_available.into();
        path.sourceInfo.Anonymous =
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_SOURCE_INFO_0 {
                modeInfoIdx: source_mode_index,
            };
        path.targetInfo.Anonymous =
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_TARGET_INFO_0 {
                modeInfoIdx: target_mode_index,
            };
        paths.push(path);
    }
    Ok((paths, modes))
}

fn source_mode(
    path: &DISPLAYCONFIG_PATH_INFO,
    route: &DisplayRoute,
    _mode_index: u32,
) -> DISPLAYCONFIG_MODE_INFO {
    DISPLAYCONFIG_MODE_INFO {
        infoType: DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
        id: path.sourceInfo.id,
        adapterId: path.sourceInfo.adapterId,
        Anonymous: DISPLAYCONFIG_MODE_INFO_0 {
            sourceMode: DISPLAYCONFIG_SOURCE_MODE {
                width: route.source_width,
                height: route.source_height,
                pixelFormat: DISPLAYCONFIG_PIXELFORMAT(route.source_pixel_format),
                position: POINTL {
                    x: route.source_position_x,
                    y: route.source_position_y,
                },
            },
        },
    }
}

fn target_mode(
    path: &DISPLAYCONFIG_PATH_INFO,
    route: &DisplayRoute,
    _mode_index: u32,
) -> DISPLAYCONFIG_MODE_INFO {
    DISPLAYCONFIG_MODE_INFO {
        infoType: DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
        id: path.targetInfo.id,
        adapterId: path.targetInfo.adapterId,
        Anonymous: DISPLAYCONFIG_MODE_INFO_0 {
            targetMode: DISPLAYCONFIG_TARGET_MODE {
                targetVideoSignalInfo: DISPLAYCONFIG_VIDEO_SIGNAL_INFO {
                    pixelRate: route.pixel_rate,
                    hSyncFreq: DISPLAYCONFIG_RATIONAL {
                        Numerator: route.hsync_numerator,
                        Denominator: route.hsync_denominator,
                    },
                    vSyncFreq: DISPLAYCONFIG_RATIONAL {
                        Numerator: route.vsync_numerator,
                        Denominator: route.vsync_denominator,
                    },
                    activeSize: DISPLAYCONFIG_2DREGION {
                        cx: route.active_width,
                        cy: route.active_height,
                    },
                    totalSize: DISPLAYCONFIG_2DREGION {
                        cx: route.total_width,
                        cy: route.total_height,
                    },
                    Anonymous: DISPLAYCONFIG_VIDEO_SIGNAL_INFO_0 {
                        videoStandard: route.video_standard,
                    },
                    scanLineOrdering: DISPLAYCONFIG_SCANLINE_ORDERING(route.scanline_ordering),
                },
            },
        },
    }
}

fn capture_routes(state: &DisplayState) -> Result<Vec<DisplayRoute>> {
    let mut routes = Vec::with_capacity(state.paths.len());
    for path in &state.paths {
        let source_index = unsafe { path.sourceInfo.Anonymous.modeInfoIdx } as usize;
        let target_index = unsafe { path.targetInfo.Anonymous.modeInfoIdx } as usize;
        let source = state
            .modes
            .get(source_index)
            .ok_or_else(|| Error::config("display source mode index is invalid"))?;
        let target = state
            .modes
            .get(target_index)
            .ok_or_else(|| Error::config("display target mode index is invalid"))?;
        if source.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
            || target.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_TARGET
        {
            return Err(Error::config("display path mode types are inconsistent"));
        }
        let source_mode = unsafe { source.Anonymous.sourceMode };
        let target_mode = unsafe { target.Anonymous.targetMode };
        let signal = target_mode.targetVideoSignalInfo;
        let video_standard = unsafe { signal.Anonymous.videoStandard };
        routes.push(DisplayRoute {
            target_path: target_device_path(path)?,
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
            source_width: source_mode.width,
            source_height: source_mode.height,
            source_pixel_format: source_mode.pixelFormat.0,
            source_position_x: source_mode.position.x,
            source_position_y: source_mode.position.y,
            pixel_rate: signal.pixelRate,
            hsync_numerator: signal.hSyncFreq.Numerator,
            hsync_denominator: signal.hSyncFreq.Denominator,
            vsync_numerator: signal.vSyncFreq.Numerator,
            vsync_denominator: signal.vSyncFreq.Denominator,
            active_width: signal.activeSize.cx,
            active_height: signal.activeSize.cy,
            total_width: signal.totalSize.cx,
            total_height: signal.totalSize.cy,
            video_standard,
        });
    }
    Ok(routes)
}

fn target_device_path(path: &DISPLAYCONFIG_PATH_INFO) -> Result<String> {
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

fn utf16_string(value: &[u16]) -> Option<String> {
    let end = value.iter().position(|character| *character == 0)?;
    (end > 0).then(|| String::from_utf16_lossy(&value[..end]))
}

fn query_current_topology() -> Result<DISPLAYCONFIG_TOPOLOGY_ID> {
    let flags = QDC_DATABASE_CURRENT;
    let mut path_count = 0u32;
    let mut mode_count = 0u32;
    let status = unsafe { GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count) };
    if status.0 != 0 {
        return Err(Error::os("GetDisplayConfigBufferSizes(database)", status.0));
    }
    if path_count as usize > MAX_QUERY_PATHS || mode_count as usize > MAX_QUERY_MODES {
        return Err(Error::config(
            "database display topology exceeds safety bounds",
        ));
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count.max(1) as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count.max(1) as usize];
    let mut topology = DISPLAYCONFIG_TOPOLOGY_ID::default();
    let status = unsafe {
        QueryDisplayConfig(
            flags,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            Some(&mut topology),
        )
    };
    if status.0 != 0 {
        return Err(Error::os("QueryDisplayConfig(database)", status.0));
    }
    Ok(topology)
}

fn query_state() -> Result<DisplayState> {
    for _ in 0..QUERY_RETRIES {
        let flags = QDC_ONLY_ACTIVE_PATHS;
        let mut path_count = 0u32;
        let mut mode_count = 0u32;
        let status =
            unsafe { GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count) };
        if status.0 != 0 {
            return Err(Error::os("GetDisplayConfigBufferSizes", status.0));
        }
        if path_count == 0 {
            return Err(Error::config("Windows reported no active display paths"));
        }
        if path_count as usize > MAX_QUERY_PATHS || mode_count as usize > MAX_QUERY_MODES {
            return Err(Error::config(
                "active display topology exceeds safety bounds",
            ));
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
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
            return Err(Error::os("QueryDisplayConfig", status.0));
        }
        paths.truncate(path_count as usize);
        modes.truncate(mode_count as usize);
        let topology = match query_current_topology() {
            Ok(topology) => topology,
            Err(error) => {
                crate::warn_!("current display topology kind unavailable: {error}");
                DISPLAYCONFIG_TOPOLOGY_ID::default()
            }
        };
        return Ok(DisplayState {
            topology,
            paths,
            modes,
        });
    }
    Err(Error::os("QueryDisplayConfig", ERROR_INSUFFICIENT_BUFFER))
}

fn set_display_config(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    modes: &[DISPLAYCONFIG_MODE_INFO],
    apply: bool,
) -> Result<()> {
    let mut flags =
        SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES | SDC_PATH_PERSIST_IF_REQUIRED;
    if apply {
        flags |= SDC_APPLY | SDC_SAVE_TO_DATABASE;
    } else {
        flags |= SDC_VALIDATE;
    }
    let status = unsafe { SetDisplayConfig(Some(paths), Some(modes), flags) };
    if status != 0 {
        return Err(Error::os("SetDisplayConfig", status as u32));
    }
    Ok(())
}

fn luid_to_u64(value: LUID) -> u64 {
    ((value.HighPart as i64 as u64) << 32) | u64::from(value.LowPart)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(path: &str, source_id: u32, target_id: u32) -> DisplayRoute {
        DisplayRoute {
            target_path: path.into(),
            source_id,
            target_id,
            source_adapter: 1,
            target_adapter: 2,
            ..Default::default()
        }
    }

    #[test]
    fn route_identity_prefers_stable_target_path_over_adapter_luid() {
        let saved = route("DISPLAY\\MONITOR-A", 0, 0);
        let live = route("display/monitor-a", 0, 0);
        assert_eq!(resolve_route_index(&saved, &[live]).unwrap(), 0);
    }

    #[test]
    fn ambiguous_route_identity_is_refused() {
        let saved = route("DISPLAY\\MONITOR-A", 0, 0);
        let live = [
            route("display/monitor-a", 0, 0),
            route("display/monitor-a", 0, 0),
        ];
        assert!(resolve_route_index(&saved, &live).is_err());
    }

    #[test]
    fn unavailable_route_is_refused() {
        let saved = route("DISPLAY\\MISSING", 0, 0);
        assert!(resolve_route_index(&saved, &[route("DISPLAY\\OTHER", 1, 2)]).is_err());
    }

    #[test]
    fn profile_validation_rejects_empty_and_duplicate_routes() {
        let mut profile = DisplayProfile {
            id: "work".into(),
            name: "Work".into(),
            topology: DisplayTopology::Extend,
            routes: vec![route("A", 0, 0), route("a", 0, 0)],
        };
        assert!(validate_profile(&profile).is_err());
        profile.routes = vec![route("A", 0, 0)];
        assert!(validate_profile(&profile).is_ok());
    }

    #[test]
    fn profile_store_defaults_to_no_selection() {
        let profiles = DisplayProfilesCfg::default();
        assert!(profiles.enabled);
        assert!(profiles.active().is_none());
    }
    #[test]
    fn profile_store_upsert_replace_and_remove_is_case_insensitive() {
        let mut profiles = DisplayProfilesCfg::default();
        profiles.upsert(DisplayProfile {
            id: "work".into(),
            name: "Work".into(),
            topology: DisplayTopology::Extend,
            routes: vec![route("A", 0, 0)],
        });
        profiles.upsert(DisplayProfile {
            id: "WORK".into(),
            name: "Work updated".into(),
            topology: DisplayTopology::Clone,
            routes: vec![route("B", 0, 0)],
        });
        assert_eq!(profiles.profiles.len(), 1);
        assert_eq!(profiles.active().unwrap().name, "Work updated");
        assert!(profiles.remove("work"));
        assert!(profiles.profiles.is_empty());
        assert!(profiles.active_profile.is_none());
    }
}
