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
    QDC_VIRTUAL_MODE_AWARE, SDC_ALLOW_CHANGES, SDC_APPLY, SDC_SAVE_TO_DATABASE,
    SDC_USE_SUPPLIED_DISPLAY_CONFIG, SDC_VALIDATE, SDC_VIRTUAL_MODE_AWARE,
};
use windows::Win32::Foundation::{LUID, POINTL};
use windows::Win32::Graphics::Gdi::{
    DISPLAYCONFIG_PATH_CLONE_GROUP_INVALID, DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID,
    DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID, DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE,
    DISPLAYCONFIG_PATH_TARGET_MODE_IDX_INVALID,
};

use crate::error::{Error, Result};

const MAX_QUERY_PATHS: usize = 64;
const MAX_QUERY_MODES: usize = 256;
const QUERY_RETRIES: usize = 3;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const MAX_PROFILE_ROUTES: usize = 32;
pub const MAX_PROFILES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTopology {
    Internal,
    Clone,
    #[default]
    Extend,
    External,
    #[serde(other)]
    Custom,
}

impl DisplayTopology {
    pub const ALL: [Self; 5] = [
        Self::Internal,
        Self::Clone,
        Self::Extend,
        Self::External,
        Self::Custom,
    ];

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
    pub fn upsert(&mut self, profile: DisplayProfile) -> bool {
        let id = profile.id.clone();
        if let Some(existing) = self
            .profiles
            .iter_mut()
            .find(|existing| existing.id.eq_ignore_ascii_case(&id))
        {
            *existing = profile;
        } else if self.profiles.len() >= MAX_PROFILES {
            return false;
        } else {
            self.profiles.push(profile);
        }
        self.active_profile = Some(id);
        true
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
    /// Only a successful Test Apply followed by Keep sets this true.
    pub confirmed: bool,
    pub routes: Vec<DisplayRoute>,
}

/// One source-to-target DisplayConfig path and its active source/target modes.
/// The target device path plus source/target IDs identify the connector route;
/// adapter LUIDs disambiguate same-path collisions and provide a fallback.
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
/// Stable 16-bit key used only in the packed hotkey message. The full profile
/// ID remains persisted and is resolved after dispatch; validation rejects key
/// collisions so delayed messages cannot select a different profile.
pub fn profile_id_key(id: &str) -> u16 {
    let mut hash = 0x811C_9DC5u32;
    for byte in id.bytes() {
        hash ^= u32::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(16_777_619);
    }
    let key = (hash as u16) ^ ((hash >> 16) as u16);
    if key == 0 {
        1
    } else {
        key
    }
}

/// Resolve one persisted route against a live route inventory.
///
/// Stable target paths plus source/target IDs are preferred and permit adapter
/// LUID changes across boots. Saved adapter LUIDs only disambiguate otherwise
/// identical connector identities; unresolved collisions are refused.
pub fn resolve_route_index(route: &DisplayRoute, available: &[DisplayRoute]) -> Result<usize> {
    let mut matches = available.iter().enumerate().filter(|(_, candidate)| {
        !route.target_path.is_empty()
            && candidate
                .target_path
                .eq_ignore_ascii_case(&route.target_path)
            && candidate.source_id == route.source_id
            && candidate.target_id == route.target_id
    });
    let Some(index) = matches.next().map(|(index, _)| index) else {
        return resolve_route_by_adapter_ids(route, available);
    };
    if matches.next().is_none() {
        return Ok(index);
    }

    // The same panel can be reachable through two GPUs while exposing the
    // same source/target IDs. When the saved LUIDs are known, use them only
    // to disambiguate that stable-path collision; otherwise fail closed.
    if route.source_adapter != 0 || route.target_adapter != 0 {
        let mut exact = available.iter().enumerate().filter(|(_, candidate)| {
            !route.target_path.is_empty()
                && candidate
                    .target_path
                    .eq_ignore_ascii_case(&route.target_path)
                && candidate.source_id == route.source_id
                && candidate.target_id == route.target_id
                && candidate.source_adapter == route.source_adapter
                && candidate.target_adapter == route.target_adapter
        });
        if let Some(exact_index) = exact.next().map(|(index, _)| index) {
            if exact.next().is_none() {
                return Ok(exact_index);
            }
        }
    }
    Err(Error::config(format!(
        "display route `{}` resolves ambiguously",
        route.target_path
    )))
}

fn resolve_route_by_adapter_ids(route: &DisplayRoute, available: &[DisplayRoute]) -> Result<usize> {
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
        let identity = format!(
            "{}|{}|{}|{}|{}",
            route.target_path.trim().to_ascii_lowercase(),
            route.source_adapter,
            route.source_id,
            route.target_adapter,
            route.target_id
        );
        if !identities.insert(identity) {
            return Err(Error::config(format!(
                "display profile `{}` contains duplicate routes",
                profile.name
            )));
        }
        if route.source_width == 0
            || route.source_height == 0
            || route.active_width == 0
            || route.active_height == 0
        {
            return Err(Error::config("display route resolution must be positive"));
        }
        if route.refresh_numerator == 0 || route.refresh_denominator == 0 {
            return Err(Error::config("display route refresh rate must be positive"));
        }
        if !matches!(route.rotation, 1..=4) {
            return Err(Error::config("display route rotation must be 1..=4"));
        }
    }
    if profile.routes.len() > 1 {
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
                    "duplicate display profile routes must share one source mode",
                ));
            }
            DisplayTopology::Extend if distinct_sources != profile.routes.len() => {
                return Err(Error::config(
                    "extend display profile routes must use distinct sources",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationState {
    Pending,
    RollbackRequested,
    Confirmed,
    Reverted,
    RollbackFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationIntent {
    Keep,
    Revert,
    Timeout,
    RollbackSucceeded,
    RollbackFailed,
}

/// Pure state transition used by the main-thread rollback timer and tests.
/// Timeout is deliberately a rollback request, never an acceptance path.
pub fn transition_confirmation(
    state: ConfirmationState,
    intent: ConfirmationIntent,
) -> ConfirmationState {
    match (state, intent) {
        (ConfirmationState::Pending, ConfirmationIntent::Keep) => ConfirmationState::Confirmed,
        (ConfirmationState::Pending, ConfirmationIntent::Revert)
        | (ConfirmationState::Pending, ConfirmationIntent::Timeout)
        | (ConfirmationState::RollbackFailed, ConfirmationIntent::Revert)
        | (ConfirmationState::RollbackFailed, ConfirmationIntent::Timeout) => {
            ConfirmationState::RollbackRequested
        }
        (ConfirmationState::RollbackRequested, ConfirmationIntent::RollbackSucceeded) => {
            ConfirmationState::Reverted
        }
        (ConfirmationState::RollbackRequested, ConfirmationIntent::RollbackFailed) => {
            ConfirmationState::RollbackFailed
        }
        (state, _) => state,
    }
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
    routes: Vec<DisplayRoute>,
}
/// Failure from profile activation. If restoring the pre-apply state also
/// failed, `rollback` retains the captured state for an explicit retry.
pub struct DisplayApplyError {
    pub error: Error,
    pub rollback: Option<Box<DisplayRollback>>,
}

impl DisplayApplyError {
    fn plain(error: Error) -> Self {
        Self {
            error,
            rollback: None,
        }
    }

    fn after_restore_failure(
        primary: Error,
        state: &DisplayState,
        routes: &[DisplayRoute],
    ) -> Self {
        match restore_state(state, routes) {
            Ok(()) => Self::plain(primary),
            Err(restore_error) => Self {
                error: Error::config(format!(
                    "{primary}; previous display topology restore failed: {restore_error}"
                )),
                rollback: Some(Box::new(DisplayRollback {
                    state: state.clone(),
                    routes: routes.to_vec(),
                })),
            },
        }
    }
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
        confirmed: false,
        routes,
    })
}

/// Apply a profile after validating and resolving every route. `persist`
/// controls whether this activation is saved to the Windows display database;
/// a Test Apply passes `false` so only Keep makes it durable. If recovery of
/// a failed activation also fails, the error retains a retryable rollback token.
pub fn apply_profile(
    profile: &mut DisplayProfile,
    persist: bool,
) -> std::result::Result<DisplayRollback, DisplayApplyError> {
    validate_profile(profile).map_err(DisplayApplyError::plain)?;
    let before = query_state().map_err(DisplayApplyError::plain)?;
    let current_routes = capture_routes(&before).map_err(DisplayApplyError::plain)?;
    let (paths, modes) =
        resolve_profile(profile, &before, &current_routes).map_err(DisplayApplyError::plain)?;
    set_display_config(&paths, &modes, false, false).map_err(DisplayApplyError::plain)?;
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
    let verified = after_routes.len() == profile.routes.len()
        && profile.routes.iter().all(|route| {
            let Ok(index) = resolve_route_index(route, &after_routes) else {
                return false;
            };
            after_routes
                .get(index)
                .is_some_and(|actual| profile_route_matches(profile.topology, route, actual))
        });
    if !verified {
        return Err(DisplayApplyError::after_restore_failure(
            Error::config("display profile verification failed"),
            &before,
            &current_routes,
        ));
    }
    for route in &mut profile.routes {
        if let Ok(index) = resolve_route_index(route, &after_routes) {
            if let Some(actual) = after_routes.get(index) {
                *route = actual.clone();
            }
        }
    }
    Ok(DisplayRollback {
        state: before,
        routes: current_routes,
    })
}
pub fn rollback(rollback: &DisplayRollback) -> Result<()> {
    restore_state(&rollback.state, &rollback.routes)
}

/// Persist the currently active topology after a successful temporary test.
/// The active paths and modes are queried again so Keep never saves stale
/// caller-owned buffers.
pub fn persist_current() -> Result<()> {
    let state = query_state()?;
    let routes = capture_routes(&state)?;
    if routes.is_empty() {
        return Err(Error::config(
            "current display topology has no active routes",
        ));
    }
    restore_state(&state, &routes)
}

fn restore_state(state: &DisplayState, expected_routes: &[DisplayRoute]) -> Result<()> {
    set_display_config(&state.paths, &state.modes, true, true)?;
    let current = query_state()?;
    let actual_routes = capture_routes(&current)?;
    if actual_routes.len() != expected_routes.len() {
        return Err(Error::config(format!(
            "rollback restored {} routes; expected {}",
            actual_routes.len(),
            expected_routes.len()
        )));
    }
    for expected in expected_routes {
        let index = resolve_route_index(expected, &actual_routes)
            .map_err(|_| Error::config("rollback route verification failed"))?;
        let actual = actual_routes
            .get(index)
            .ok_or_else(|| Error::config("rollback route index is invalid"))?;
        if !route_state_matches(expected, actual) {
            return Err(Error::config("rollback mode verification failed"));
        }
    }
    Ok(())
}

/// Compare the user-visible topology and mode fields. Drivers may normalize
/// opaque path/status and signal-timing fields while retaining the requested
/// route, position, dimensions, rotation, scaling, and refresh.
fn route_state_matches(expected: &DisplayRoute, actual: &DisplayRoute) -> bool {
    route_mode_matches(expected, actual, true)
}

fn profile_route_matches(
    topology: DisplayTopology,
    expected: &DisplayRoute,
    actual: &DisplayRoute,
) -> bool {
    // Clone targets can expose a different physical active signal after
    // Windows picks a common source mode; the shared desktop/source shape and
    // connector mode still must match.
    route_mode_matches(expected, actual, topology != DisplayTopology::Clone)
}

fn route_mode_matches(
    expected: &DisplayRoute,
    actual: &DisplayRoute,
    include_active_dimensions: bool,
) -> bool {
    expected
        .target_path
        .eq_ignore_ascii_case(&actual.target_path)
        && expected.output_technology == actual.output_technology
        && expected.rotation == actual.rotation
        && expected.scaling == actual.scaling
        && expected.refresh_numerator == actual.refresh_numerator
        && expected.refresh_denominator == actual.refresh_denominator
        && expected.source_width == actual.source_width
        && expected.source_height == actual.source_height
        && expected.source_position_x == actual.source_position_x
        && expected.source_position_y == actual.source_position_y
        && (!include_active_dimensions
            || (expected.active_width == actual.active_width
                && expected.active_height == actual.active_height))
}

fn resolve_profile(
    profile: &DisplayProfile,
    state: &DisplayState,
    current_routes: &[DisplayRoute],
) -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>)> {
    let mut paths = Vec::with_capacity(profile.routes.len());
    let mut modes = Vec::with_capacity(profile.routes.len() * 2);
    let mut source_modes = std::collections::HashMap::new();
    let mut target_modes = std::collections::HashMap::new();
    for route in &profile.routes {
        let route_index = resolve_route_index(route, current_routes)?;
        let current = state
            .paths
            .get(route_index)
            .ok_or_else(|| Error::config("display route index is outside the live topology"))?;

        // Clone topologies can reference one source from several targets. The
        // DisplayConfig contract permits each source/target mode only once;
        // every path sharing an identifier must point at that one mode entry.
        let source_key = (
            luid_to_u64(current.sourceInfo.adapterId),
            current.sourceInfo.id,
        );
        let source_values = (
            route.source_width,
            route.source_height,
            route.source_pixel_format,
            route.source_position_x,
            route.source_position_y,
        );
        let source_mode_index = if let Some((index, previous)) = source_modes.get(&source_key) {
            if *previous != source_values {
                return Err(Error::config(
                    "display profile contains conflicting modes for a shared source",
                ));
            }
            *index
        } else {
            let index = modes.len() as u32;
            modes.push(source_mode(current, route, index));
            source_modes.insert(source_key, (index, source_values));
            index
        };

        let target_key = (
            luid_to_u64(current.targetInfo.adapterId),
            current.targetInfo.id,
        );
        let target_values = (
            route.pixel_rate,
            route.hsync_numerator,
            route.hsync_denominator,
            route.vsync_numerator,
            route.vsync_denominator,
            route.active_width,
            route.active_height,
            route.total_width,
            route.total_height,
            route.video_standard,
            route.scanline_ordering,
        );
        let target_mode_index = if let Some((index, previous)) = target_modes.get(&target_key) {
            if *previous != target_values {
                return Err(Error::config(
                    "display profile contains conflicting modes for a shared target",
                ));
            }
            *index
        } else {
            let index = modes.len() as u32;
            modes.push(target_mode(current, route, index));
            target_modes.insert(target_key, (index, target_values));
            index
        };

        let mut path = *current;
        let virtual_mode = current.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE != 0;
        path.flags = (route.path_flags & !DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE)
            | (current.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE);
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
        path.sourceInfo.Anonymous = if virtual_mode {
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_SOURCE_INFO_0 {
                Anonymous: windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_SOURCE_INFO_0_0 {
                    _bitfield: DISPLAYCONFIG_PATH_CLONE_GROUP_INVALID | (source_mode_index << 16),
                },
            }
        } else {
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_SOURCE_INFO_0 {
                modeInfoIdx: source_mode_index,
            }
        };
        path.targetInfo.Anonymous = if virtual_mode {
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_TARGET_INFO_0 {
                Anonymous: windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_TARGET_INFO_0_0 {
                    _bitfield: DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID
                        | (target_mode_index << 16),
                },
            }
        } else {
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_TARGET_INFO_0 {
                modeInfoIdx: target_mode_index,
            }
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
fn mode_indices(path: &DISPLAYCONFIG_PATH_INFO) -> Result<(usize, usize)> {
    let (source, target) = if path.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE != 0 {
        let source_bits = unsafe { path.sourceInfo.Anonymous.Anonymous._bitfield };
        let target_bits = unsafe { path.targetInfo.Anonymous.Anonymous._bitfield };
        (
            (source_bits >> 16) & u32::from(u16::MAX),
            (target_bits >> 16) & u32::from(u16::MAX),
        )
    } else {
        (unsafe { path.sourceInfo.Anonymous.modeInfoIdx }, unsafe {
            path.targetInfo.Anonymous.modeInfoIdx
        })
    };
    if source == DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID || source == u32::MAX {
        return Err(Error::config("display source mode index is unavailable"));
    }
    if target == DISPLAYCONFIG_PATH_TARGET_MODE_IDX_INVALID || target == u32::MAX {
        return Err(Error::config("display target mode index is unavailable"));
    }
    Ok((source as usize, target as usize))
}

fn capture_routes(state: &DisplayState) -> Result<Vec<DisplayRoute>> {
    let mut routes = Vec::with_capacity(state.paths.len());
    for path in &state.paths {
        let (source_index, target_index) = mode_indices(path)?;
        let source = state.modes.get(source_index).ok_or_else(|| {
            Error::config(format!(
                "display source mode index {source_index} is invalid for {} modes",
                state.modes.len()
            ))
        })?;
        let target = state.modes.get(target_index).ok_or_else(|| {
            Error::config(format!(
                "display target mode index {target_index} is invalid for {} modes",
                state.modes.len()
            ))
        })?;
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
    let flags = QDC_DATABASE_CURRENT | QDC_VIRTUAL_MODE_AWARE;
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
        let flags = QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE;
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
    save_to_database: bool,
) -> Result<()> {
    let mut flags = SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES | SDC_VIRTUAL_MODE_AWARE;
    if apply {
        flags |= SDC_APPLY;
        if save_to_database {
            flags |= SDC_SAVE_TO_DATABASE;
        }
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
            source_width: 1920,
            source_height: 1080,
            active_width: 1920,
            active_height: 1080,
            refresh_numerator: 60,
            refresh_denominator: 1,
            rotation: 1,
            ..Default::default()
        }
    }
    #[test]
    fn same_monitor_on_two_connectors_resolves_as_two_distinct_routes() {
        let igpu = route("DISPLAY\\MONITOR-A", 1, 10);
        let dgpu = route("DISPLAY\\MONITOR-A", 2, 20);
        let available = [igpu.clone(), dgpu.clone()];
        assert_eq!(resolve_route_index(&igpu, &available).unwrap(), 0);
        assert_eq!(resolve_route_index(&dgpu, &available).unwrap(), 1);
    }
    #[test]
    fn same_monitor_with_reused_connector_ids_uses_saved_adapter_luids() {
        let igpu = route("DISPLAY\\MONITOR-A", 0, 0);
        let dgpu = DisplayRoute {
            source_adapter: 3,
            target_adapter: 4,
            ..route("DISPLAY\\MONITOR-A", 0, 0)
        };
        let available = [igpu.clone(), dgpu.clone()];
        assert_eq!(resolve_route_index(&igpu, &available).unwrap(), 0);
        assert_eq!(resolve_route_index(&dgpu, &available).unwrap(), 1);
    }
    #[test]
    fn virtual_mode_paths_read_mode_indices_from_upper_bitfields() {
        let mut path = DISPLAYCONFIG_PATH_INFO {
            flags: DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE,
            ..Default::default()
        };
        path.sourceInfo.Anonymous =
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_SOURCE_INFO_0 {
                Anonymous: windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_SOURCE_INFO_0_0 {
                    _bitfield: (3 << 16) | 7,
                },
            };
        path.targetInfo.Anonymous =
            windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_TARGET_INFO_0 {
                Anonymous: windows::Win32::Devices::Display::DISPLAYCONFIG_PATH_TARGET_INFO_0_0 {
                    _bitfield: DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID | (4 << 16),
                },
            };
        assert_eq!(mode_indices(&path).unwrap(), (3, 4));
    }
    #[test]
    fn profile_id_key_is_case_insensitive_and_nonzero() {
        assert_eq!(profile_id_key("Gaming"), profile_id_key("gaming"));
        assert_ne!(profile_id_key("Gaming"), profile_id_key("AI"));
        assert_ne!(profile_id_key(""), 0);
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
            confirmed: false,
            routes: vec![route("A", 0, 0), route("a", 0, 0)],
        };
        assert!(validate_profile(&profile).is_err());
        profile.routes = vec![route("A", 0, 0)];
        assert!(validate_profile(&profile).is_ok());
    }
    #[test]
    fn profile_validation_rejects_malformed_mode_values() {
        let mut profile = DisplayProfile {
            id: "work".into(),
            name: "Work".into(),
            topology: DisplayTopology::Extend,
            confirmed: false,
            routes: vec![route("A", 0, 0)],
        };
        profile.routes[0].active_width = 0;
        assert!(validate_profile(&profile).is_err());
        profile.routes[0].active_width = 1920;
        profile.routes[0].refresh_denominator = 0;
        assert!(validate_profile(&profile).is_err());
        profile.routes[0].refresh_denominator = 1;
        profile.routes[0].rotation = 0;
        assert!(validate_profile(&profile).is_err());
    }
    #[test]
    fn topology_validation_requires_real_clone_or_extend_source_shape() {
        let mut clone = DisplayProfile {
            id: "clone".into(),
            name: "Clone".into(),
            topology: DisplayTopology::Clone,
            confirmed: false,
            routes: vec![route("A", 1, 10), route("B", 2, 20)],
        };
        assert!(validate_profile(&clone).is_ok());
        clone.routes[1].source_width = 1280;
        assert!(validate_profile(&clone).is_err());
        clone.routes[1].source_width = 1920;

        clone.topology = DisplayTopology::Extend;
        assert!(validate_profile(&clone).is_ok());
        clone.routes[1].source_id = 1;
        assert!(validate_profile(&clone).is_err());
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
            confirmed: false,
        });
        profiles.upsert(DisplayProfile {
            id: "WORK".into(),
            name: "Work updated".into(),
            topology: DisplayTopology::Clone,
            routes: vec![route("B", 0, 0)],
            confirmed: false,
        });
        assert_eq!(profiles.profiles.len(), 1);
        assert_eq!(profiles.active().unwrap().name, "Work updated");
        assert!(profiles.remove("work"));
        assert!(profiles.profiles.is_empty());
        assert!(profiles.active_profile.is_none());
    }
    #[test]
    fn confirmation_timeout_requests_rollback_instead_of_accepting() {
        let state =
            transition_confirmation(ConfirmationState::Pending, ConfirmationIntent::Timeout);
        assert_eq!(state, ConfirmationState::RollbackRequested);
        assert_ne!(state, ConfirmationState::Confirmed);
        assert_eq!(
            transition_confirmation(state, ConfirmationIntent::RollbackSucceeded),
            ConfirmationState::Reverted
        );
    }

    #[test]
    fn keep_confirms_only_before_rollback_and_failed_recovery_stays_visible() {
        assert_eq!(
            transition_confirmation(ConfirmationState::Pending, ConfirmationIntent::Keep),
            ConfirmationState::Confirmed
        );
        let failed = transition_confirmation(
            ConfirmationState::RollbackRequested,
            ConfirmationIntent::RollbackFailed,
        );
        assert_eq!(failed, ConfirmationState::RollbackFailed);
        assert_eq!(
            transition_confirmation(failed, ConfirmationIntent::Keep),
            ConfirmationState::RollbackFailed
        );
        assert_eq!(
            transition_confirmation(failed, ConfirmationIntent::Revert),
            ConfirmationState::RollbackRequested
        );
    }
    #[test]
    fn failed_timeout_remains_retryable_and_never_becomes_confirmed() {
        let failed = transition_confirmation(
            transition_confirmation(ConfirmationState::Pending, ConfirmationIntent::Timeout),
            ConfirmationIntent::RollbackFailed,
        );
        assert_eq!(failed, ConfirmationState::RollbackFailed);
        assert_eq!(
            transition_confirmation(failed, ConfirmationIntent::Timeout),
            ConfirmationState::RollbackRequested
        );
        assert_eq!(
            transition_confirmation(failed, ConfirmationIntent::Keep),
            ConfirmationState::RollbackFailed
        );
    }
}
