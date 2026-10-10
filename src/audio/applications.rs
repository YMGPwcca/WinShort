//! Read-only inventory of muted executables across active render endpoints.
use crate::audio::state::{Aggregate, AppAudioState, ApplicationAudioInfo};
use crate::error::{Error, Result};
use std::collections::BTreeMap;
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eRender, AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, ISimpleAudioVolume, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::CLSCTX_ALL;

#[cfg(test)]
pub(super) fn muted_applications(
    enumerator: &IMMDeviceEnumerator,
) -> Result<Vec<ApplicationAudioInfo>> {
    Ok(application_inventory(enumerator)?
        .into_iter()
        .filter(|app| app.state.aggregate == Aggregate::AllMuted)
        .collect())
}

pub(super) fn application_inventory(
    enumerator: &IMMDeviceEnumerator,
) -> Result<Vec<ApplicationAudioInfo>> {
    let devices = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
        .map_err(|e| Error::win("EnumMutedApplicationEndpoints", &e))?;
    let count = unsafe { devices.GetCount() }
        .map_err(|e| Error::win("CountMutedApplicationEndpoints", &e))?;
    let mut apps = BTreeMap::new();
    let mut identities = BTreeMap::new();
    for index in 0..count {
        let device = unsafe { devices.Item(index) }
            .map_err(|e| Error::win("MutedApplicationEndpoint", &e))?;
        let manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None) }
            .map_err(|e| Error::win("MutedApplicationSessions", &e))?;
        read_sessions(&manager, &mut identities, &mut apps)?;
    }
    Ok(apps.into_values().collect())
}

fn read_sessions(
    manager: &IAudioSessionManager2,
    identities: &mut BTreeMap<u32, ApplicationAudioInfo>,
    apps: &mut BTreeMap<String, ApplicationAudioInfo>,
) -> Result<()> {
    let sessions = unsafe { manager.GetSessionEnumerator() }
        .map_err(|e| Error::win("MutedApplicationEnumerator", &e))?;
    let count = unsafe { sessions.GetCount() }
        .map_err(|e| Error::win("MutedApplicationSessionCount", &e))?;
    for index in 0..count {
        let Ok(control) = (unsafe { sessions.GetSession(index) }) else {
            continue;
        };
        if unsafe { control.GetState() }.ok() == Some(AudioSessionStateExpired) {
            continue;
        }
        let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
            continue;
        };
        let pid = unsafe { control2.GetProcessId() }.unwrap_or(0);
        if pid == 0 || crate::platform::foreground::process_is_running(pid) == Some(false) {
            continue;
        }
        let volume: ISimpleAudioVolume = control
            .cast()
            .map_err(|e| Error::win("MutedApplicationVolume", &e))?;
        let muted = unsafe { volume.GetMute() }
            .map_err(|e| Error::win("ReadApplicationMute", &e))?
            .as_bool();
        let info = identities.entry(pid).or_insert_with(|| process_info(pid));
        add_session(apps, info.clone(), muted);
    }
    Ok(())
}

fn process_info(pid: u32) -> ApplicationAudioInfo {
    ApplicationAudioInfo::from_process(
        pid,
        AppAudioState {
            app_name: crate::platform::foreground::process_name(pid),
            aggregate: Aggregate::AllMuted,
            sessions: 0,
            error: None,
        },
    )
}

fn add_session(
    apps: &mut BTreeMap<String, ApplicationAudioInfo>,
    info: ApplicationAudioInfo,
    muted: bool,
) {
    let app = apps.entry(info.identity.clone()).or_insert(info);
    app.state.aggregate = if app.state.sessions == 0 {
        if muted {
            Aggregate::AllMuted
        } else {
            Aggregate::AllActive
        }
    } else {
        match (app.state.aggregate, muted) {
            (Aggregate::AllMuted, true) => Aggregate::AllMuted,
            (Aggregate::AllActive, false) => Aggregate::AllActive,
            _ => Aggregate::Mixed,
        }
    };
    app.state.sessions += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn info(pid: u32, path: &str) -> ApplicationAudioInfo {
        ApplicationAudioInfo::new(
            pid,
            Some(path.into()),
            AppAudioState {
                app_name: Some("Player".into()),
                aggregate: Aggregate::AllMuted,
                sessions: 0,
                error: None,
            },
        )
    }
    #[test]
    #[ignore = "read-only comparison of live grouped inventory and foreground mute scope"]
    fn native_compare_program_mute_scopes() {
        let _com = crate::platform::com::ComApartment::init_mta();
        let enumerator: IMMDeviceEnumerator = unsafe {
            windows::Win32::System::Com::CoCreateInstance(
                &windows::Win32::Media::Audio::MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            )
        }
        .unwrap();
        let path = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("WinShort/config.toml");
        let wire: crate::config::model::ConfigToml =
            toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let (config, _) = crate::config::Config::from_toml(&wire);
        for app in application_inventory(&enumerator).unwrap() {
            if ["zen", "spotify"].iter().any(|name| {
                app.state
                    .app_name
                    .as_deref()
                    .unwrap_or_default()
                    .eq_ignore_ascii_case(name)
            }) {
                println!(
                    "Grouped {:?}: {:?}, sessions={}, representative pid={}",
                    app.state.app_name, app.state.aggregate, app.state.sessions, app.pid
                );
            }
        }
        for pid in std::env::var("WINSHORT_AUDIO_PROBE_PIDS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.parse::<u32>().ok())
        {
            let state =
                crate::audio::sessions::query_foreground(&enumerator, &config, Some(pid)).unwrap();
            println!(
                "Scoped pid={pid} {:?}: {:?}, sessions={}",
                state.app_name, state.aggregate, state.sessions
            );
        }
    }

    #[test]
    #[ignore = "reads live Core Audio sessions without mutating audio or configuration"]
    fn native_read_only_inventory_is_executable_keyed() {
        let _com = crate::platform::com::ComApartment::init_mta();
        let enumerator: IMMDeviceEnumerator = unsafe {
            windows::Win32::System::Com::CoCreateInstance(
                &windows::Win32::Media::Audio::MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            )
        }
        .unwrap();
        let apps = muted_applications(&enumerator).unwrap();
        let identities = apps
            .iter()
            .map(|app| &app.identity)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(identities.len(), apps.len());
        assert!(apps
            .iter()
            .all(|app| app.state.aggregate == Aggregate::AllMuted && app.state.sessions > 0));
        println!("Read-only inventory: {} muted executables", apps.len());
    }

    #[test]
    fn paths_group_same_installation_but_keep_same_name_installations_separate() {
        let mut apps = BTreeMap::new();
        add_session(&mut apps, info(1, "C:/A/player.exe"), true);
        add_session(&mut apps, info(2, "c:/a/PLAYER.exe"), true);
        add_session(&mut apps, info(3, "C:/B/player.exe"), true);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps["c:\\a\\player.exe"].state.sessions, 2);
        add_session(&mut apps, info(2, "C:/A/player.exe"), false);
        assert_eq!(apps["c:\\a\\player.exe"].state.aggregate, Aggregate::Mixed);
        assert_eq!(
            apps["c:\\b\\player.exe"].state.aggregate,
            Aggregate::AllMuted
        );
    }
}
