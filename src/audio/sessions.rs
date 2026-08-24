//! Foreground application session enumeration and aggregate mute semantics.

use windows::core::{GUID, Interface};
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
};
use windows::Win32::System::Com::CLSCTX_ALL;

use crate::audio::controller::EndpointFlow;
use crate::audio::devices::{data_flow, resolve_device};
use crate::audio::state::{Aggregate, AppAudioState};
use crate::config::Config;
use crate::error::{Error, Result};

const SESSION_EVENT_CONTEXT: GUID =
    GUID::from_u128(0x78ab818c_7b20_4737_aa73_c92ad4a879bf);

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
    let sessions = resolve_sessions(enumerator, config, pid, &app_name)?;
    Ok(state_from_mutes(&sessions, app_name))
}

fn toggle_once(
    enumerator: &IMMDeviceEnumerator,
    config: &Config,
    pid: u32,
    app_name: &Option<String>,
) -> Result<AppAudioState> {
    let mut sessions = resolve_sessions(enumerator, config, pid, app_name)?;
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
        return Err(Error::audio("matching sessions disappeared before mute applied"));
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
        if all_muted { "muted" } else { "active-or-mixed" },
        aggregate
    );
    Ok(AppAudioState {
        app_name: app_name.clone(),
        aggregate,
        sessions: sessions.len(),
        error: None,
    })
}

fn state_from_mutes(sessions: &[(windows::Win32::Media::Audio::ISimpleAudioVolume, bool)], app_name: Option<String>) -> AppAudioState {
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
    AppAudioState { app_name, aggregate, sessions: sessions.len(), error: None }
}

/// Resolution ladder (#18): configured endpoint + exact PID; then every
/// active render endpoint + exact PID; then image-name match across all
/// endpoints (multi-process apps).
fn resolve_sessions(
    enumerator: &IMMDeviceEnumerator,
    config: &Config,
    pid: u32,
    app_name: &Option<String>,
) -> Result<Vec<(windows::Win32::Media::Audio::ISimpleAudioVolume, bool)>> {
    let device = resolve_device(
        enumerator,
        EndpointFlow::Render,
        &config.audio.output_device,
        config.audio.output_role,
    )?;
    let sessions = collect_on_manager(device, Some(pid), None)?;
    if !sessions.is_empty() {
        return Ok(sessions);
    }
    let swept = collect_all_render_endpoints(enumerator, Some(pid), None)?;
    if !swept.is_empty() {
        return Ok(swept);
    }
    let lowered = app_name.as_ref().map(|n| n.to_lowercase());
    match lowered {
        Some(name) => collect_all_render_endpoints(enumerator, None, Some(&name)),
        None => Ok(Vec::new()),
    }
}

type SessionTuple = (windows::Win32::Media::Audio::ISimpleAudioVolume, bool);

fn collect_from_enumerated(
    session_enumerator: &windows::Win32::Media::Audio::IAudioSessionEnumerator,
    pid_filter: Option<u32>,
    name_filter: Option<&str>,
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
        if let Some(want) = pid_filter {
            if session_pid != want {
                continue;
            }
        }
        if let Some(name) = name_filter {
            match crate::platform::foreground::process_name(session_pid) {
                Some(owner) if owner.to_lowercase() == name => {}
                _ => continue,
            }
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
    pid_filter: Option<u32>,
    name_filter: Option<&str>,
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
    Ok(collect_from_enumerated(&session_enumerator, pid_filter, name_filter))
}

fn collect_all_render_endpoints(
    enumerator: &IMMDeviceEnumerator,
    pid_filter: Option<u32>,
    name_filter: Option<&str>,
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
        let Ok(session_enumerator) =
            (unsafe {
                manager
                    .GetSessionEnumerator()
                    .map_err(|e| Error::win("GetSessionEnumerator", &e))
            })
        else {
            continue;
        };
        sessions.extend(collect_from_enumerated(&session_enumerator, pid_filter, name_filter));
    }
    Ok(sessions)
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
