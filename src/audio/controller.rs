//! Core Audio worker. Owns its MTA COM apartment, enumerator, endpoint volume
//! interfaces, and callbacks. Raw COM pointers never leave this thread.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use windows::Win32::Media::Audio::{
    IMMDeviceEnumerator, IMMNotificationClient, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

use crate::audio::devices::EndpointBinding;
use crate::audio::notifications::DeviceNotificationClient;
use crate::audio::state::{
    AppVolumeState, AudioRuntimeSnapshot, AudioState, DeviceCycleFlow, DeviceCycleResult,
    OutputState,
};
use crate::config::ConfigHandle;
use crate::error::{Error, Result};
use crate::event::AppEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointFlow {
    Capture,
    Render,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioCommand {
    ToggleMicrophone(u64),
    ToggleOutput(u64),
    ToggleForeground {
        pid: Option<u32>,
        request_id: u64,
    },
    CycleDevice {
        flow: DeviceCycleFlow,
        request_id: u64,
    },
    AdjustForegroundVolume {
        pid: Option<u32>,
        adjustment: crate::audio::sessions::VolumeAdjustment,
        request_id: u64,
    },
    RefreshEndpoint(EndpointFlow),
    RefreshAll,
    ConfigChanged {
        origin: crate::event::ConfigCommitOrigin,
    },
    /// Live read-only foreground resolver for Show Status (#18).
    QueryForeground {
        pid: Option<u32>,
        request_id: u64,
    },
    Shutdown,
}

pub struct AudioService {
    sender: Sender<AudioCommand>,
    devices: Arc<std::sync::RwLock<crate::audio::devices::DeviceLists>>,
    runtime: Arc<std::sync::RwLock<AudioRuntimeSnapshot>>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl AudioService {
    pub fn start(
        main_hwnd: windows::Win32::Foundation::HWND,
        config: Arc<ConfigHandle>,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let hwnd_raw = main_hwnd.0 as isize;
        let worker_sender = sender.clone();
        let devices = Arc::new(std::sync::RwLock::new(
            crate::audio::devices::DeviceLists::default(),
        ));
        let runtime = Arc::new(std::sync::RwLock::new(AudioRuntimeSnapshot::default()));
        let worker_devices = Arc::clone(&devices);
        let worker_runtime = Arc::clone(&runtime);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("winshort-audio".into())
            .spawn(move || {
                audio_thread(
                    hwnd_raw,
                    config,
                    worker_sender,
                    worker_devices,
                    worker_runtime,
                    receiver,
                    ready_tx,
                )
            })
            .map_err(|e| Error::internal(format!("spawn audio thread: {e}")))?;
        if ready_rx
            .recv_timeout(std::time::Duration::from_secs(8))
            .map_err(|_| Error::audio("audio startup timed out"))
            .and_then(|r| r)
            .is_err()
        {
            // No orphan worker (#24): tell it to stop, then join.
            let _ = sender.send(AudioCommand::Shutdown);
            let _ = join.join();
            return Err(Error::audio("audio startup timed out"));
        }
        Ok(Self {
            sender,
            devices,
            runtime,
            join: Some(join),
        })
    }

    pub fn send(&self, command: AudioCommand) {
        let _ = self.sender.send(command);
    }

    pub fn devices(&self) -> crate::audio::devices::DeviceLists {
        self.devices.read().expect("audio device list").clone()
    }

    pub fn runtime_snapshot(&self) -> AudioRuntimeSnapshot {
        self.runtime.read().expect("audio runtime snapshot").clone()
    }

    pub fn shutdown(&mut self) {
        let _ = self.sender.send(AudioCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for AudioService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct AudioController {
    main_hwnd_raw: isize,
    config: Arc<ConfigHandle>,
    config_revision: u64,
    sender: Sender<AudioCommand>,
    devices: Arc<std::sync::RwLock<crate::audio::devices::DeviceLists>>,
    runtime: Arc<std::sync::RwLock<AudioRuntimeSnapshot>>,
    enumerator: IMMDeviceEnumerator,
    device_callback: IMMNotificationClient,
    capture: Option<EndpointBinding>,
    render: Option<EndpointBinding>,
    capture_error: Option<String>,
    render_error: Option<String>,
}

impl AudioController {
    fn create(
        main_hwnd_raw: isize,
        config: Arc<ConfigHandle>,
        sender: Sender<AudioCommand>,
        devices: Arc<std::sync::RwLock<crate::audio::devices::DeviceLists>>,
        runtime: Arc<std::sync::RwLock<AudioRuntimeSnapshot>>,
    ) -> Result<Self> {
        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| Error::win("CoCreateInstance(MMDeviceEnumerator)", &e))?
        };
        let device_callback = DeviceNotificationClient::create(sender.clone());
        unsafe {
            enumerator
                .RegisterEndpointNotificationCallback(&device_callback)
                .map_err(|e| Error::win("RegisterEndpointNotificationCallback", &e))?;
        }
        let revision = config.revision();
        let mut controller = Self {
            main_hwnd_raw,
            config,
            config_revision: revision,
            sender,
            devices,
            runtime,
            enumerator,
            device_callback,
            capture: None,
            render: None,
            capture_error: None,
            render_error: None,
        };
        controller.rebuild_all(true, crate::event::AudioEventOrigin::Initial);
        Ok(controller)
    }

    fn handle(&mut self, command: AudioCommand) -> bool {
        match command {
            // ConfigChanged carries the revision bump with it — rebuild here
            // and consume the revision so one Save/device cycle triggers
            // exactly ONE rebuild (#17a).
            AudioCommand::ConfigChanged { origin } => {
                let revision = self.config.revision();
                if revision != self.config_revision {
                    self.config_revision = revision;
                    self.rebuild_all(false, config_event_origin(origin));
                } else {
                    crate::log_debug!("config change already consumed at revision {revision}");
                }
            }
            other => return self.handle_other(other),
        }
        true
    }

    fn handle_other(&mut self, command: AudioCommand) -> bool {
        self.refresh_config_if_needed();
        match command {
            AudioCommand::ToggleMicrophone(request_id) => self.toggle(
                EndpointFlow::Capture,
                crate::event::AudioEventOrigin::WinShortAction(request_id),
            ),
            AudioCommand::ToggleOutput(request_id) => self.toggle(
                EndpointFlow::Render,
                crate::event::AudioEventOrigin::WinShortAction(request_id),
            ),
            AudioCommand::ToggleForeground { pid, request_id } => {
                let config = self.config.get();
                let state =
                    crate::audio::sessions::toggle_foreground(&self.enumerator, &config, pid)
                        .unwrap_or_else(|e| crate::audio::AppAudioState {
                            app_name: None,
                            aggregate: crate::audio::Aggregate::Error,
                            sessions: 0,
                            error: Some(e.to_string()),
                        });
                self.post(AppEvent::ForegroundAudioChanged {
                    state,
                    origin: crate::event::AudioEventOrigin::WinShortAction(request_id),
                });
            }
            AudioCommand::CycleDevice { flow, request_id } => {
                let result = self.cycle_device(flow);
                self.post(AppEvent::DeviceCycleResolved { request_id, result });
            }
            AudioCommand::AdjustForegroundVolume {
                pid,
                adjustment,
                request_id,
            } => {
                let config = self.config.get();
                let state = crate::audio::sessions::adjust_foreground_volume(
                    &self.enumerator,
                    &config,
                    pid,
                    adjustment,
                )
                .unwrap_or_else(|error| AppVolumeState::error(None, error.to_string()));
                self.post(AppEvent::ForegroundVolumeChanged {
                    state,
                    origin: crate::event::AudioEventOrigin::WinShortAction(request_id),
                });
            }
            AudioCommand::RefreshEndpoint(flow) => {
                crate::log_debug!("audio {:?} endpoint notification", flow);
                self.publish(flow, crate::event::AudioEventOrigin::External);
            }
            // ConfigChanged is consumed by handle() above; refresh_config_if_needed
            // covers any residual revision drift.
            AudioCommand::QueryForeground { pid, request_id } => {
                let config = self.config.get();
                let state =
                    crate::audio::sessions::query_foreground(&self.enumerator, &config, pid)
                        .unwrap_or_else(|e| crate::audio::AppAudioState {
                            app_name: None,
                            aggregate: crate::audio::Aggregate::Error,
                            sessions: 0,
                            error: Some(e.to_string()),
                        });
                self.post(AppEvent::ForegroundAudioChanged {
                    state,
                    origin: crate::event::AudioEventOrigin::StatusRequest(request_id),
                });
            }
            AudioCommand::RefreshAll => {
                self.rebuild_all(false, crate::event::AudioEventOrigin::External)
            }
            AudioCommand::ConfigChanged { .. } => {}
            AudioCommand::Shutdown => return false,
        }
        true
    }

    fn cycle_device(&self, flow: DeviceCycleFlow) -> DeviceCycleResult {
        let config = self.config.get();
        let previous = match flow {
            DeviceCycleFlow::Input => config.audio.input_device.clone(),
            DeviceCycleFlow::Output => config.audio.output_device.clone(),
        };
        let devices = self.devices.read().expect("audio device list");
        let active = match flow {
            DeviceCycleFlow::Input => &devices.inputs,
            DeviceCycleFlow::Output => &devices.outputs,
        };
        crate::audio::devices::device_cycle_result(flow, &previous, active)
    }

    fn refresh_config_if_needed(&mut self) {
        let revision = self.config.revision();
        if revision != self.config_revision {
            self.config_revision = revision;
            self.rebuild_all(false, crate::event::AudioEventOrigin::External);
        }
    }

    fn rebuild_all(&mut self, startup: bool, origin: crate::event::AudioEventOrigin) {
        let old_render = self.render.as_ref().map(|e| e.identity.clone());
        self.rebuild(EndpointFlow::Capture);
        self.rebuild(EndpointFlow::Render);
        // Partial-failure tolerant enumeration (#36): a failed flow yields
        // warnings, not the loss of both lists. Poisoned lock recovery: the
        // guarded DeviceLists is always structurally valid.
        let lists = crate::audio::devices::enumerate_devices(&self.enumerator);
        for warning in &lists.warnings {
            crate::warn_!("audio {warning}");
        }
        match self.devices.write() {
            Ok(mut shared) => *shared = lists,
            Err(poisoned) => *poisoned.into_inner() = lists,
        }

        if !startup {
            if let (Some(old), Some(new)) =
                (old_render, self.render.as_ref().map(|e| e.identity.clone()))
            {
                if matches!(origin, crate::event::AudioEventOrigin::External)
                    && old.endpoint != new.endpoint
                {
                    self.post(AppEvent::DefaultOutputChanged(new));
                }
            }
            self.post(AppEvent::DevicesChanged);
        }
        self.publish(EndpointFlow::Capture, origin);
        self.publish(EndpointFlow::Render, origin);
    }

    fn rebuild(&mut self, flow: EndpointFlow) {
        match flow {
            EndpointFlow::Capture => {
                let _ = self.capture.take();
                self.capture_error = None;
            }
            EndpointFlow::Render => {
                let _ = self.render.take();
                self.render_error = None;
            }
        }
        let config = self.config.get();
        let (selection, role) = match flow {
            EndpointFlow::Capture => (&config.audio.input_device, config.audio.input_role),
            EndpointFlow::Render => (&config.audio.output_device, config.audio.output_role),
        };
        match EndpointBinding::create(&self.enumerator, flow, selection, role, self.sender.clone())
        {
            Ok(endpoint) => {
                crate::info!("audio {:?} endpoint: {}", flow, endpoint.identity.name);
                match flow {
                    EndpointFlow::Capture => self.capture = Some(endpoint),
                    EndpointFlow::Render => self.render = Some(endpoint),
                }
            }
            Err(error) => {
                crate::warn_!("audio {:?} unavailable: {error}", flow);
                match flow {
                    EndpointFlow::Capture => self.capture_error = Some(error.to_string()),
                    EndpointFlow::Render => self.render_error = Some(error.to_string()),
                }
            }
        }
        self.publish_runtime();
    }

    fn publish_runtime(&self) {
        let snapshot = AudioRuntimeSnapshot {
            capture: self
                .capture
                .as_ref()
                .map(|endpoint| endpoint.identity.clone()),
            render: self
                .render
                .as_ref()
                .map(|endpoint| endpoint.identity.clone()),
            capture_error: self.capture_error.clone(),
            render_error: self.render_error.clone(),
        };
        match self.runtime.write() {
            Ok(mut current) => *current = snapshot,
            Err(poisoned) => *poisoned.into_inner() = snapshot,
        }
    }

    fn toggle(&mut self, flow: EndpointFlow, origin: crate::event::AudioEventOrigin) {
        let missing = match flow {
            EndpointFlow::Capture => self.capture.is_none(),
            EndpointFlow::Render => self.render.is_none(),
        };
        if missing {
            self.rebuild(flow);
        }
        let endpoint = match flow {
            EndpointFlow::Capture => self.capture.as_ref(),
            EndpointFlow::Render => self.render.as_ref(),
        };
        let result = endpoint
            .ok_or_else(|| Error::audio("endpoint unavailable"))
            .and_then(|endpoint| endpoint.mute().and_then(|muted| endpoint.set_mute(!muted)));
        if let Err(e) = result {
            crate::warn_!("audio toggle {:?} failed: {e}", flow);
        }
        self.publish(flow, origin);
    }

    fn publish(&self, flow: EndpointFlow, origin: crate::event::AudioEventOrigin) {
        match flow {
            EndpointFlow::Capture => {
                let state = self
                    .capture
                    .as_ref()
                    .ok_or_else(|| Error::audio("microphone unavailable"))
                    .and_then(EndpointBinding::capture_state)
                    .unwrap_or_else(|e| AudioState::Unavailable {
                        reason: e.to_string(),
                    });
                self.post(AppEvent::MicrophoneStateChanged { state, origin });
            }
            EndpointFlow::Render => {
                let state = self
                    .render
                    .as_ref()
                    .ok_or_else(|| Error::audio("output unavailable"))
                    .and_then(EndpointBinding::output_state)
                    .unwrap_or_else(|e| OutputState::Unavailable {
                        reason: e.to_string(),
                    });
                self.post(AppEvent::OutputStateChanged { state, origin });
            }
        }
    }

    fn post(&self, event: AppEvent) {
        let hwnd = windows::Win32::Foundation::HWND(self.main_hwnd_raw as *mut _);
        unsafe {
            let _ = crate::event::post_event(hwnd, event);
        }
    }

    fn shutdown(&mut self) {
        // Endpoint bindings unregister via Drop (#36).
        self.capture.take();
        self.render.take();
        unsafe {
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.device_callback);
        }
    }
}

/// Keep existing Settings-save notifications compatible while suppressing
/// state-card duplicates for device-cycle actions.
fn config_event_origin(origin: crate::event::ConfigCommitOrigin) -> crate::event::AudioEventOrigin {
    match origin {
        crate::event::ConfigCommitOrigin::Settings => crate::event::AudioEventOrigin::External,
        crate::event::ConfigCommitOrigin::DeviceCycle => {
            crate::event::AudioEventOrigin::Config(origin)
        }
    }
}

fn audio_thread(
    hwnd_raw: isize,
    config: Arc<ConfigHandle>,
    sender: Sender<AudioCommand>,
    devices: Arc<std::sync::RwLock<crate::audio::devices::DeviceLists>>,
    runtime: Arc<std::sync::RwLock<AudioRuntimeSnapshot>>,
    receiver: Receiver<AudioCommand>,
    ready: mpsc::SyncSender<std::result::Result<(), Error>>,
) {
    // COM apartment owned by a guard (#24): CoUninitialize runs on every exit.
    let com = crate::platform::com::ComApartment::init_mta();
    if !com.ok() {
        let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
        let _ = ready.send(Err(Error::os("CoInitializeEx(audio)", code)));
        return;
    }

    let mut controller = match AudioController::create(hwnd_raw, config, sender, devices, runtime) {
        Ok(controller) => controller,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    crate::info!("audio controller ready");

    // Coalescing worker loop (#19): after each handled command, drain the
    // backlog and collapse refresh bursts into a single rebuild. Toggle-style
    // commands still execute individually.
    loop {
        let command = match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(command) => command,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if !controller.handle(command) {
            break;
        }
        let mut backlog: Vec<AudioCommand> = Vec::new();
        while let Ok(next) = receiver.try_recv() {
            backlog.push(next);
        }
        let (rebuild_origin, rest) = coalesce_backlog(backlog);
        let mut stop = false;
        for pending in rest {
            // Shutdown terminates the worker like a direct handle() call.
            if !controller.handle(pending) {
                stop = true;
                break;
            }
        }
        if stop {
            break;
        }
        if let Some(origin) = rebuild_origin {
            controller.rebuild_all(false, origin);
        }
    }
    controller.shutdown();
    drop(com);
    crate::info!("audio controller stopped");
}

/// Collapse a command backlog (#19): any number of refresh-style commands
/// becomes a single rebuild request; all other commands pass through in
/// order. A device-cycle config origin wins over a concurrent external
/// refresh so user-triggered changes do not produce duplicate state overlays.
fn coalesce_backlog(
    backlog: Vec<AudioCommand>,
) -> (Option<crate::event::AudioEventOrigin>, Vec<AudioCommand>) {
    let mut rebuild_origin = None;
    let mut rest = Vec::new();
    for cmd in backlog {
        match cmd {
            AudioCommand::RefreshAll => {
                if rebuild_origin.is_none() {
                    rebuild_origin = Some(crate::event::AudioEventOrigin::External);
                }
            }
            AudioCommand::ConfigChanged { origin } => {
                rebuild_origin = Some(config_event_origin(origin));
            }
            other => rest.push(other),
        }
    }
    (rebuild_origin, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_burst_collapses_to_single_rebuild() {
        // #19: five rapid RefreshAll commands must produce exactly one rebuild.
        let backlog = vec![
            AudioCommand::RefreshAll,
            AudioCommand::RefreshEndpoint(EndpointFlow::Capture),
            AudioCommand::RefreshAll,
            AudioCommand::ToggleOutput(1),
            AudioCommand::RefreshAll,
            AudioCommand::ConfigChanged {
                origin: crate::event::ConfigCommitOrigin::Settings,
            },
            AudioCommand::Shutdown,
        ];
        let (rebuild, rest) = coalesce_backlog(backlog);
        assert_eq!(rebuild, Some(crate::event::AudioEventOrigin::External));
        assert_eq!(
            rest,
            vec![
                AudioCommand::RefreshEndpoint(EndpointFlow::Capture),
                AudioCommand::ToggleOutput(1),
                AudioCommand::Shutdown,
            ],
            "non-refresh commands pass through in order"
        );
    }

    #[test]
    fn empty_backlog_needs_no_rebuild() {
        assert!(coalesce_backlog(Vec::new()).0.is_none());
    }
    #[test]
    fn device_cycle_origin_suppresses_concurrent_external_refresh() {
        let (origin, rest) = coalesce_backlog(vec![
            AudioCommand::RefreshAll,
            AudioCommand::ConfigChanged {
                origin: crate::event::ConfigCommitOrigin::DeviceCycle,
            },
        ]);
        assert_eq!(
            origin,
            Some(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        );
        assert!(rest.is_empty());
    }
}
