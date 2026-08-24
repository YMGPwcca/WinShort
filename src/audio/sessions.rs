//! Foreground application session enumeration and aggregate mute semantics.

use windows::core::{Interface, GUID};
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
};

use crate::audio::controller::EndpointFlow;
use crate::audio::devices::{data_flow, resolve_device};
use crate::audio::state::{Aggregate, AppAudioState};
use crate::config::Config;
use crate::error::{Error, Result};

const SESSION_EVENT_CONTEXT: GUID = GUID::from_u128(0x78ab818c_7b20_4737_aa73_c92ad4a879bf);

/// Toggle every render session owned by the foreground process together
/// (#18): exact-PID match on the configured endpoint first, then a sweep of
/// all active render endpoints, then a per-process-image-name fallback.
/// Endpoint invalidation triggers exactly one retry.
pub fn toggle_foreground(
    enumerator: &IMMDeviceEnumerator,
    config: &Config,
    pid: Option<u32>,
) -> Result<AppAudioState> {
    let Some(pid) = pid.filter(|pid| *pid != 0) else {
        return Ok(AppAudioState::no_external());
    };
    let app_name = crate::platform::foreground::process_name(pid);
    let mut attempt = 0;
    loop {
        attempt += 1;
        let outcome = toggle_once(enumerator, config, pid, &app_name);
        return match outcome {
            Err(e) if attempt == 1 && Error::is_audio_invalidation(&e) => {
                crate::warn_!(
                    "audio endpoint invalidated during foreground toggle; rebuilding and retrying once"
                );
                continue;
            }
            other => other,
        };
    }
}

/// Read-only variant used by Show Status (#18): always reflects live session
/// state instead of cached overlay data.
pub fn query_foreground(
    enumerator: &IMMDeviceEnumerator,
    config: &Config,
    pid: Option<u32>,
) -> Result<AppAudioState> {
    let Some(pid) = pid.filter(|pid| *pid != 0) else {
        return Ok(AppAudioState::no_external());
    };
    let app_name = crate::platform::foreground::process_name(pid);
    match resolve_sessions(enumerator, config, pid, &app_name)? {
        Resolved::Sessions(sessions) => Ok(state_from_mutes(&sessions, app_name)),
        Resolved::Ambiguous { processes, .. } => Ok(AppAudioState {
            app_name,
            aggregate: Aggregate::Error,
            sessions: 0,
            error: Some(format!(
                "ambiguous target: {} distinct installations share this name",
                processes.len()
            )),
        }),
    }
}

fn toggle_once(
    enumerator: &IMMDeviceEnumerator,
    config: &Config,
    pid: u32,
    app_name: &Option<String>,
) -> Result<AppAudioState> {
    let sessions = match resolve_sessions(enumerator, config, pid, app_name)? {
        Resolved::Sessions(sessions) => sessions,
        Resolved::Ambiguous {
            stem,
            ref processes,
        } => {
            // Fail closed (#46): several installations share this image name;
            // muting all of them could hit unrelated processes.
            crate::warn_!(
                "foreground audio ambiguous ({stem:?}, {} installations): no session muted",
                processes.len()
            );
            return Ok(AppAudioState {
                app_name: app_name.clone(),
                aggregate: Aggregate::Error,
                sessions: 0,
                error: Some(format!(
                    "ambiguous audio target `{stem}`: {} matching installations",
                    processes.len()
                )),
            });
        }
    };
    if sessions.is_empty() {
        return Ok(AppAudioState {
            app_name: app_name.clone(),
            aggregate: Aggregate::NoSession,
            sessions: 0,
            error: None,
        });
    }

    let all_muted = sessions.iter().all(|(_, muted)| *muted);
    // Active -> mute all; Mixed -> mute all; all muted -> unmute all.
    let target_muted = !all_muted;
    let mut failures = Vec::new();
    for (volume, _) in &sessions {
        if let Err(e) = unsafe { volume.SetMute(target_muted, &SESSION_EVENT_CONTEXT) } {
            failures.push(e);
        }
    }
    if failures.len() == sessions.len() {
        return Err(Error::audio(
            "matching sessions disappeared before mute applied",
        ));
    }

    // Re-read actual mute states instead of assuming the write succeeded
    // everywhere (#17c) — partial failure is Mixed truth, never full success.
    let mut muted_now = 0usize;
    for (volume, _) in &sessions {
        match unsafe { volume.GetMute() } {
            Ok(muted) => {
                if muted.as_bool() {
                    muted_now += 1;
                }
            }
            Err(e) => failures.push(e),
        }
    }
    for e in &failures {
        crate::warn_!("foreground audio SetMute/GetMute failed: {e}");
    }
    let aggregate = if muted_now == sessions.len() && failures.is_empty() {
        if target_muted {
            Aggregate::AllMuted
        } else {
            Aggregate::AllActive
        }
    } else if muted_now == 0 && !all_muted {
        Aggregate::AllActive
    } else {
        Aggregate::Mixed
    };
    crate::log_debug!(
        "foreground audio pid={pid} sessions={} before={} after={:?}",
        sessions.len(),
        if all_muted {
            "muted"
        } else {
            "active-or-mixed"
        },
        aggregate
    );
    Ok(AppAudioState {
        app_name: app_name.clone(),
        aggregate,
        sessions: sessions.len(),
        error: None,
    })
}

fn state_from_mutes(
    sessions: &[(windows::Win32::Media::Audio::ISimpleAudioVolume, bool)],
    app_name: Option<String>,
) -> AppAudioState {
    if sessions.is_empty() {
        return AppAudioState {
            app_name,
            aggregate: Aggregate::NoSession,
            sessions: 0,
            error: None,
        };
    }
    let muted_count = sessions.iter().filter(|(_, m)| *m).count();
    let aggregate = if muted_count == sessions.len() {
        Aggregate::AllMuted
    } else if muted_count == 0 {
        Aggregate::AllActive
    } else {
        Aggregate::Mixed
    };
    AppAudioState {
        app_name,
        aggregate,
        sessions: sessions.len(),
        error: None,
    }
}

/// Resolution ladder (#18): configured endpoint + exact PID; then every
/// active render endpoint + exact PID; then image-name match across all
/// endpoints (multi-process apps).
/// One fallback candidate: a process owning at least one render session (#46).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackCandidate {
    pub pid: u32,
    /// Lowercased image stem (always discoverable via process_name).
    pub stem: String,
    /// Full executable path when discoverable — the GROUPING identity (#46):
    /// same path => same installation; missing path => pid-private group.
    pub image_path: Option<String>,
}

/// Outcome of ambiguous-aware fallback selection (#46).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackSelection {
    /// Distinct process set to operate on (possibly empty = no candidates).
    Pids(Vec<u32>),
    /// More than one distinct installation matches — refuse to choose.
    Ambiguous {
        stem: String,
        processes: Vec<String>,
    },
}

/// Fail-closed fallback selection (#46).
///
/// Groups stem-matching candidates by FULL executable path (case-insensitive
/// on Windows). Candidates without a discoverable path each form their own
/// group so they can never merge with an unrelated process. One group =>
/// its pids; multiple groups => `Ambiguous` (caller must not mutate);
/// zero => empty selection.
fn select_fallback_pids(stem: &str, candidates: &[FallbackCandidate]) -> FallbackSelection {
    let want = stem.to_lowercase();
    // Group key: full path when known; otherwise a pid-private sentinel.
    let mut groups: Vec<(String, Vec<u32>)> = Vec::new();
    for cand in candidates {
        if cand.stem != want {
            continue;
        }
        let key = cand
            .image_path
            .as_ref()
            .map(|p| p.to_lowercase())
            .unwrap_or_else(|| format!("\0pid:{}", cand.pid));
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, pids)) => pids.push(cand.pid),
            None => groups.push((key, vec![cand.pid])),
        }
    }
    if groups.is_empty() {
        return FallbackSelection::Pids(Vec::new());
    }
    if groups.len() > 1 {
        return FallbackSelection::Ambiguous {
            stem: want,
            processes: groups.iter().map(|(k, _)| k.clone()).collect(),
        };
    }
    FallbackSelection::Pids(std::mem::take(&mut groups[0].1))
}

/// Result of session resolution — `Ambiguous` must never be mutated (#46).
enum Resolved {
    Sessions(Vec<SessionTuple>),
    Ambiguous {
        stem: String,
        processes: Vec<String>,
    },
}

fn resolve_sessions(
    enumerator: &IMMDeviceEnumerator,
    config: &Config,
    pid: u32,
    app_name: &Option<String>,
) -> Result<Resolved> {
    let device = resolve_device(
        enumerator,
        EndpointFlow::Render,
        &config.audio.output_device,
        config.audio.output_role,
    )?;
    let sessions = collect_on_manager(device, &[pid])?;
    if !sessions.is_empty() {
        return Ok(Resolved::Sessions(sessions));
    }
    let swept = collect_all_render_endpoints(enumerator, &[pid])?;
    if !swept.is_empty() {
        return Ok(Resolved::Sessions(swept));
    }

    // Name fallback with fail-closed disambiguation (#46): group candidates by
    // FULL executable path; a single group is served, several are refused.
    let lowered = app_name.as_ref().map(|n| n.to_lowercase());
    let Some(stem) = lowered else {
        return Ok(Resolved::Sessions(Vec::new()));
    };
    // One cheap enumeration pass to learn which pids own sessions.
    let owner_pids = collect_all_render_pids(enumerator)?;
    let candidates: Vec<FallbackCandidate> = owner_pids
        .into_iter()
        .map(|owner| FallbackCandidate {
            pid: owner,
            stem: crate::platform::foreground::process_name(owner)
                .map(|n| n.to_lowercase())
                .unwrap_or_default(),
            image_path: crate::platform::foreground::process_image_path(owner),
        })
        .collect();
    match select_fallback_pids(&stem, &candidates) {
        FallbackSelection::Pids(pids) if pids.is_empty() => Ok(Resolved::Sessions(Vec::new())),
        FallbackSelection::Pids(pids) => Ok(Resolved::Sessions(collect_all_render_endpoints(
            enumerator, &pids,
        )?)),
        FallbackSelection::Ambiguous { processes, .. } => {
            crate::warn_!(
                "foreground audio fallback ambiguous: {} distinct installations match `{stem:?}`; refusing to mute",
                processes.len()
            );
            Ok(Resolved::Ambiguous { stem, processes })
        }
    }
}

/// PIDs owning at least one render session, across all active endpoints.
fn collect_all_render_pids(enumerator: &IMMDeviceEnumerator) -> Result<Vec<u32>> {
    use windows::Win32::Media::Audio::{IAudioSessionManager2, DEVICE_STATE_ACTIVE};
    use windows::Win32::System::Com::CLSCTX_ALL;
    let collection = unsafe {
        enumerator
            .EnumAudioEndpoints(data_flow(EndpointFlow::Render), DEVICE_STATE_ACTIVE)
            .map_err(|e| Error::win("EnumAudioEndpoints", &e))?
    };
    let count = unsafe {
        collection
            .GetCount()
            .map_err(|e| Error::win("IMMDeviceCollection::GetCount", &e))?
    };
    let mut pids = Vec::new();
    for index in 0..count {
        let Ok(device) = (unsafe { collection.Item(index) }) else {
            continue;
        };
        let manager: Result<IAudioSessionManager2> = unsafe {
            device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| Error::win("Activate(IAudioSessionManager2)", &e))
        };
        let Ok(manager) = manager else { continue };
        let Ok(session_enumerator) = (unsafe {
            manager
                .GetSessionEnumerator()
                .map_err(|e| Error::win("GetSessionEnumerator", &e))
        }) else {
            continue;
        };
        let ses_count = unsafe { session_enumerator.GetCount() }.unwrap_or(0);
        for i in 0..ses_count {
            let Ok(control) = (unsafe { session_enumerator.GetSession(i) }) else {
                continue;
            };
            let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
                continue;
            };
            if let Ok(session_pid) = unsafe { control2.GetProcessId() } {
                if session_pid != 0 && !pids.contains(&session_pid) {
                    pids.push(session_pid);
                }
            }
        }
    }
    Ok(pids)
}

type SessionTuple = (windows::Win32::Media::Audio::ISimpleAudioVolume, bool);

fn collect_from_enumerated(
    session_enumerator: &windows::Win32::Media::Audio::IAudioSessionEnumerator,
    pid_filter: &[u32],
) -> Vec<SessionTuple> {
    let count = unsafe { session_enumerator.GetCount() }.unwrap_or(0);
    let mut sessions = Vec::new();
    for index in 0..count {
        let Ok(control) = (unsafe { session_enumerator.GetSession(index) }) else {
            continue; // session disappeared mid-enumeration
        };
        let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
            continue;
        };
        let Ok(session_pid) = (unsafe { control2.GetProcessId() }) else {
            continue;
        };
        if !pid_filter.is_empty() && !pid_filter.contains(&session_pid) {
            continue;
        }
        let Ok(volume) = control.cast::<ISimpleAudioVolume>() else {
            continue;
        };
        let Ok(muted) = (unsafe { volume.GetMute() }) else {
            continue;
        };
        sessions.push((volume, muted.as_bool()));
    }
    sessions
}

fn collect_on_manager(
    device: windows::Win32::Media::Audio::IMMDevice,
    pid_filter: &[u32],
) -> Result<Vec<SessionTuple>> {
    use windows::Win32::System::Com::CLSCTX_ALL;
    let manager: IAudioSessionManager2 = unsafe {
        device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| Error::win("Activate(IAudioSessionManager2)", &e))?
    };
    let session_enumerator = unsafe {
        manager
            .GetSessionEnumerator()
            .map_err(|e| Error::win("GetSessionEnumerator", &e))?
    };
    Ok(collect_from_enumerated(&session_enumerator, pid_filter))
}

fn collect_all_render_endpoints(
    enumerator: &IMMDeviceEnumerator,
    pid_filter: &[u32],
) -> Result<Vec<SessionTuple>> {
    use windows::Win32::Media::Audio::{IAudioSessionManager2, DEVICE_STATE_ACTIVE};
    use windows::Win32::System::Com::CLSCTX_ALL;
    let collection = unsafe {
        enumerator
            .EnumAudioEndpoints(data_flow(EndpointFlow::Render), DEVICE_STATE_ACTIVE)
            .map_err(|e| Error::win("EnumAudioEndpoints", &e))?
    };
    let count = unsafe {
        collection
            .GetCount()
            .map_err(|e| Error::win("IMMDeviceCollection::GetCount", &e))?
    };
    let mut sessions = Vec::new();
    for index in 0..count {
        let Ok(device) = (unsafe { collection.Item(index) }) else {
            continue;
        };
        let manager: Result<IAudioSessionManager2> = unsafe {
            device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| Error::win("Activate(IAudioSessionManager2)", &e))
        };
        let Ok(manager) = manager else { continue };
        let Ok(session_enumerator) = (unsafe {
            manager
                .GetSessionEnumerator()
                .map_err(|e| Error::win("GetSessionEnumerator", &e))
        }) else {
            continue;
        };
        sessions.extend(collect_from_enumerated(&session_enumerator, pid_filter));
    }
    Ok(sessions)
}

#[cfg(test)]
mod fallback_selection_tests {
    use super::*;

    fn cand(pid: u32, path: Option<&str>) -> FallbackCandidate {
        FallbackCandidate {
            pid,
            stem: std::path::Path::new(path.unwrap_or("unknown"))
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
            image_path: path.map(Into::into),
        }
    }

    fn named(pid: u32, stem: &str) -> FallbackCandidate {
        FallbackCandidate {
            pid,
            stem: stem.into(),
            image_path: None,
        }
    }

    #[test]
    fn unique_fallback_candidate_is_selected() {
        // D: single candidate sharing the foreground stem.
        let out = select_fallback_pids("player", &[cand(101, Some("C:\\PortableA\\player.exe"))]);
        assert_eq!(out, FallbackSelection::Pids(vec![101]));
    }

    #[test]
    fn multi_pid_same_installation_groups_together() {
        // B (Chrome/Electron-style): main + helper share ONE full path; both
        // pids must be selected as a single group.
        let out = select_fallback_pids(
            "chrome",
            &[
                cand(10, Some("C:\\Apps\\chrome.exe")),
                cand(11, Some("C:\\Apps\\chrome.exe")),
                cand(12, None), // unknown-path pid must NOT join the group
            ],
        );
        assert_eq!(out, FallbackSelection::Pids(vec![10, 11]));
    }

    #[test]
    fn two_unrelated_same_basename_installs_are_ambiguous() {
        // C/E: PortableA vs PortableB — same stem, different full paths.
        let out = select_fallback_pids(
            "player",
            &[
                cand(20, Some("C:\\PortableA\\player.exe")),
                cand(21, Some("C:\\PortableB\\player.exe")),
                cand(22, Some("C:\\PortableB\\PLAYER.EXE")), // case-insensitive
            ],
        );
        match out {
            FallbackSelection::Ambiguous { processes, .. } => {
                assert_eq!(processes.len(), 2, "{processes:?}");
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn unknown_path_candidate_stays_selectable_when_alone() {
        // Path discovery failed but the stem matches: still usable alone,
        // and it can never silently merge with another group.
        let out = select_fallback_pids("player", &[named(30, "player")]);
        assert_eq!(out, FallbackSelection::Pids(vec![30]));

        // ...but an unknown-path pid never merges into a known-path group.
        let out = select_fallback_pids(
            "player",
            &[named(31, "player"), cand(32, Some("C:\\A\\player.exe"))],
        );
        match out {
            FallbackSelection::Ambiguous { .. } => {}
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn empty_candidates_yield_empty_selection() {
        // F: every session disappeared during resolution.
        assert_eq!(
            select_fallback_pids("player", &[]),
            FallbackSelection::Pids(Vec::new())
        );
    }

    #[test]
    fn different_stem_is_never_matched() {
        let out = select_fallback_pids("player", &[cand(40, Some("C:\\x\\notplayer.exe"))]);
        assert_eq!(out, FallbackSelection::Pids(Vec::new()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_pid_is_explicit_no_external_state() {
        // This branch needs no COM and protects the self/no-foreground edge.
        // The enumerator is intentionally not constructible in this pure test,
        // so verify through the data constructor used by the branch.
        let state = AppAudioState::no_external();
        assert_eq!(state.aggregate, Aggregate::NoExternalApp);
        assert_eq!(state.sessions, 0);
    }
}

#[cfg(test)]
mod fallback_group_props {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashMap;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(128))]

        /// #46/#37 invariant over generated groups: the selected set contains
        /// only pids from ONE distinct full-path group; ambiguous selections
        /// are refused rather than silently merged.
        #[test]
        fn fallback_groups_never_merge_distinct_paths(
            paths in prop::collection::vec("[a-z]{1,8}/player[0-9]{0,2}\\.exe", 1..4),
            pids in prop::collection::vec(any::<u32>(), 1..8),
        ) {
            use crate::audio::sessions::{FallbackCandidate, select_fallback_pids};
            let stem = "player";
            let candidates: Vec<_> = pids
                .iter()
                .enumerate()
                .map(|(i, &pid)| {
                    let path = &paths[i % paths.len()];
                    FallbackCandidate {
                        pid,
                        stem: stem.into(),
                        image_path: Some(path.clone()),
                    }
                })
                .collect();
            match select_fallback_pids(stem, &candidates) {
                crate::audio::sessions::FallbackSelection::Pids(selected) => {
                    // Every selected pid's path must be identical.
                    let by_pid: HashMap<u32, String> = candidates
                        .iter()
                        .map(|c| (c.pid, c.image_path.clone().unwrap()))
                        .collect();
                    let mut distinct_paths: Vec<String> = selected
                        .iter()
                        .map(|&pid| by_pid[&pid].clone())
                        .collect();
                    distinct_paths.sort();
                    distinct_paths.dedup();
                    prop_assert!(
                        distinct_paths.len() == 1 || selected.is_empty(),
                        "selected pids span multiple installations: {distinct_paths:?}"
                    );
                }
                other => {}
            }
        }
    }
}
