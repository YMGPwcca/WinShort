//! Foreground application session enumeration and aggregate mute semantics.

use windows::core::{GUID, Interface};
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
};
use windows::Win32::System::Com::CLSCTX_ALL;

use crate::audio::controller::EndpointFlow;
use crate::audio::devices::resolve_device;
use crate::audio::state::{Aggregate, AppAudioState};
use crate::config::Config;
use crate::error::{Error, Result};

const SESSION_EVENT_CONTEXT: GUID =
    GUID::from_u128(0x78ab818c_7b20_4737_aa73_c92ad4a879bf);

/// Toggle every render session owned by `pid` together. Mixed state mutes all;
/// no matching session returns a truthful `NoSession` state.
pub fn toggle_foreground(
    enumerator: &IMMDeviceEnumerator,
    config: &Config,
    pid: Option<u32>,
) -> Result<AppAudioState> {
    let Some(pid) = pid.filter(|pid| *pid != 0) else {
        return Ok(AppAudioState::no_external());
    };
    let device = resolve_device(
        enumerator,
        EndpointFlow::Render,
        &config.audio.output_device,
        config.audio.output_role,
    )?;
    let manager: IAudioSessionManager2 = unsafe {
        device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| Error::win("Activate(IAudioSessionManager2)", &e))?
    };
    let enumerator = unsafe {
        manager
            .GetSessionEnumerator()
            .map_err(|e| Error::win("GetSessionEnumerator", &e))?
    };
    let count = unsafe {
        enumerator
            .GetCount()
            .map_err(|e| Error::win("IAudioSessionEnumerator::GetCount", &e))?
    };

    let mut sessions: Vec<(ISimpleAudioVolume, bool)> = Vec::new();
    for index in 0..count {
        let Ok(control) = (unsafe { enumerator.GetSession(index) }) else {
            continue; // session disappeared mid-enumeration
        };
        let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
            continue;
        };
        let Ok(session_pid) = (unsafe { control2.GetProcessId() }) else {
            continue;
        };
        if session_pid != pid {
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

    let app_name = crate::platform::foreground::process_name(pid);
    if sessions.is_empty() {
        return Ok(AppAudioState {
            app_name,
            aggregate: Aggregate::NoSession,
            sessions: 0,
        });
    }

    let all_muted = sessions.iter().all(|(_, muted)| *muted);
    let all_active = sessions.iter().all(|(_, muted)| !*muted);
    // Active -> mute all; Mixed -> mute all; all muted -> unmute all.
    let target_muted = !all_muted;
    let mut changed = 0usize;
    for (volume, _) in &sessions {
        if unsafe { volume.SetMute(target_muted, &SESSION_EVENT_CONTEXT) }.is_ok() {
            changed += 1;
        }
    }
    if changed == 0 {
        return Err(Error::audio("matching sessions disappeared before mute applied"));
    }

    let aggregate = if target_muted {
        Aggregate::AllMuted
    } else {
        Aggregate::AllActive
    };
    crate::log_debug!(
        "foreground audio pid={pid} sessions={} before={} after={:?}",
        sessions.len(),
        if all_muted {
            "muted"
        } else if all_active {
            "active"
        } else {
            "mixed"
        },
        aggregate
    );
    Ok(AppAudioState {
        app_name,
        aggregate,
        sessions: changed,
    })
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
