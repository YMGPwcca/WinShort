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
use crate::config::model::EndpointRole;
use crate::config::{ConfigHandle, ConfigRevisionStamp, ConfigSnapshot};
use crate::error::{Error, Result};
use crate::event::AppEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointFlow {
    Capture,
    Render,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DefaultDeviceChange {
    flow: EndpointFlow,
    role: EndpointRole,
    endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingDefaultSwitch {
    flow: EndpointFlow,
    endpoint: String,
    observed_roles: u8,
}

fn default_role_bit(role: EndpointRole) -> u8 {
    match role {
        EndpointRole::Console => 0b001,
        EndpointRole::Multimedia => 0b010,
        EndpointRole::Communications => 0b100,
    }
}

/// Refresh work extracted from one worker backlog. External refreshes and
/// config changes are tracked independently because an already-consumed
/// ConfigChanged must not erase a real external refresh.
#[derive(Debug, Default, PartialEq, Eq)]
struct PendingRebuild {
    external_refresh: bool,
    config_revision: Option<u64>,
    default_changes: Vec<DefaultDeviceChange>,
}

impl PendingRebuild {
    fn external() -> Self {
        Self {
            external_refresh: true,
            ..Self::default()
        }
    }

    fn config(stamp: ConfigRevisionStamp) -> Self {
        Self {
            config_revision: Some(stamp.revision),
            ..Self::default()
        }
    }

    fn is_empty(&self) -> bool {
        !self.external_refresh && self.config_revision.is_none() && self.default_changes.is_empty()
    }
}

/// Decide and consume one pending rebuild against the worker's revision.
///
/// The live revision stamp is authoritative: a config command's stamp is only
/// a wake/synchronization hint and cannot override a newer live publication.
/// A newer live revision consumes the revision and performs one rebuild. Once
/// the revision is already consumed, only an external refresh can request a
/// rebuild; a stale ConfigChanged alone is a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingRebuildDecision {
    None,
    Rebuild(crate::event::AudioEventOrigin),
}

fn decide_pending_rebuild(
    live_stamp: ConfigRevisionStamp,
    consumed_revision: u64,
    pending: PendingRebuild,
) -> (u64, PendingRebuildDecision) {
    if live_stamp.revision != consumed_revision {
        (
            live_stamp.revision,
            PendingRebuildDecision::Rebuild(config_event_origin(live_stamp.origin)),
        )
    } else if pending.external_refresh {
        (
            consumed_revision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::External),
        )
    } else {
        (consumed_revision, PendingRebuildDecision::None)
    }
}

/// A decided rebuild owns the exact config snapshot used for every endpoint
/// flow in that rebuild.
#[derive(Debug, Clone)]
struct RebuildPlan {
    snapshot: ConfigSnapshot,
    origin: crate::event::AudioEventOrigin,
}

fn plan_pending_rebuild(
    snapshot: ConfigSnapshot,
    consumed_revision: u64,
    pending: PendingRebuild,
) -> (u64, Option<RebuildPlan>) {
    let (revision, decision) = decide_pending_rebuild(snapshot.stamp, consumed_revision, pending);
    let plan = match decision {
        PendingRebuildDecision::None => None,
        PendingRebuildDecision::Rebuild(origin) => Some(RebuildPlan { snapshot, origin }),
    };
    (revision, plan)
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
    SetDefaultDevice {
        flow: DeviceCycleFlow,
        endpoint: String,
        request_id: u64,
    },
    AdjustForegroundVolume {
        pid: Option<u32>,
        adjustment: crate::audio::sessions::VolumeAdjustment,
        request_id: u64,
    },
    RefreshEndpoint(EndpointFlow),
    RefreshAll,
    DefaultDeviceChanged {
        flow: EndpointFlow,
        role: EndpointRole,
        endpoint: String,
    },
    ConfigChanged {
        stamp: ConfigRevisionStamp,
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
    pending_default_switches: Vec<PendingDefaultSwitch>,
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
        let initial = config.snapshot();
        let revision = initial.stamp.revision;
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
            pending_default_switches: Vec::new(),
        };
        controller.rebuild_all(true, initial.value, crate::event::AudioEventOrigin::Initial);
        Ok(controller)
    }

    fn handle(&mut self, command: AudioCommand) -> bool {
        match command {
            // ConfigChanged carries the publication stamp as a wake signal;
            // the central policy rereads the authoritative live stamp.
            AudioCommand::ConfigChanged { stamp } => {
                self.apply_pending_rebuild(PendingRebuild::config(stamp));
            }
            other => return self.handle_other(other),
        }
        true
    }

    fn handle_other(&mut self, command: AudioCommand) -> bool {
        if matches!(&command, AudioCommand::RefreshAll) {
            // A direct RefreshAll is itself the pending external work. Let
            // the central policy combine it with any config revision drift.
            self.apply_pending_rebuild(PendingRebuild::external());
            return true;
        }
        self.refresh_config_if_needed();
        match command {
            AudioCommand::ToggleMicrophone(_) => self.toggle(EndpointFlow::Capture),
            AudioCommand::ToggleOutput(_) => self.toggle(EndpointFlow::Render),
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
            AudioCommand::SetDefaultDevice {
                flow,
                endpoint,
                request_id,
            } => {
                let result = self.set_default_device(flow, &endpoint);
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
            AudioCommand::DefaultDeviceChanged {
                flow,
                role,
                endpoint,
            } => {
                let mut pending = PendingRebuild::default();
                pending.default_changes.push(DefaultDeviceChange {
                    flow,
                    role,
                    endpoint,
                });
                self.apply_pending_rebuild(pending);
            }
            AudioCommand::RefreshEndpoint(flow) => {
                crate::log_debug!("audio {:?} endpoint notification", flow);
                self.publish(flow);
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
            AudioCommand::RefreshAll => self.apply_pending_rebuild(PendingRebuild::external()),
            AudioCommand::ConfigChanged { .. } => {}
            AudioCommand::Shutdown => return false,
        }
        true
    }

    fn apply_pending_rebuild(&mut self, pending: PendingRebuild) {
        if pending.is_empty() {
            return;
        }
        let snapshot = self.config.snapshot();
        self.apply_pending_rebuild_snapshot(snapshot, pending);
    }

    fn apply_pending_rebuild_snapshot(
        &mut self,
        snapshot: ConfigSnapshot,
        pending: PendingRebuild,
    ) {
        let mut pending = pending;
        self.filter_default_changes(&mut pending);
        let (revision, plan) = plan_pending_rebuild(snapshot, self.config_revision, pending);
        self.config_revision = revision;
        if let Some(plan) = plan {
            self.rebuild_all(false, plan.snapshot.value, plan.origin);
        }
    }

    fn filter_default_changes(&mut self, pending: &mut PendingRebuild) {
        if pending.external_refresh {
            self.pending_default_switches.clear();
        }
        let changes = std::mem::take(&mut pending.default_changes);
        for change in changes {
            if !self.is_pending_default_change(&change) {
                pending.external_refresh = true;
            }
        }
    }

    fn is_pending_default_change(&mut self, change: &DefaultDeviceChange) -> bool {
        Self::consume_pending_default_change(&mut self.pending_default_switches, change)
    }

    fn consume_pending_default_change(
        pending: &mut Vec<PendingDefaultSwitch>,
        change: &DefaultDeviceChange,
    ) -> bool {
        let Some(index) = pending.iter().position(|expected| {
            expected.flow == change.flow && expected.endpoint == change.endpoint
        }) else {
            pending.retain(|expected| expected.flow != change.flow);
            return false;
        };
        let complete = {
            let expected = &mut pending[index];
            expected.observed_roles |= default_role_bit(change.role);
            expected.observed_roles == 0b111
        };
        if complete {
            pending.remove(index);
        }
        true
    }

    fn set_default_device(&mut self, flow: DeviceCycleFlow, endpoint: &str) -> DeviceCycleResult {
        let active = {
            let devices = self.devices.read().expect("audio device list");
            match flow {
                DeviceCycleFlow::Input => devices.inputs.clone(),
                DeviceCycleFlow::Output => devices.outputs.clone(),
            }
        };
        let endpoint_flow = match flow {
            DeviceCycleFlow::Input => EndpointFlow::Capture,
            DeviceCycleFlow::Output => EndpointFlow::Render,
        };
        let previous =
            crate::audio::devices::current_default_device(&self.enumerator, endpoint_flow).ok();
        let Some(device) = active
            .into_iter()
            .find(|device| device.endpoint.as_str() == endpoint)
        else {
            return DeviceCycleResult::Failed {
                flow,
                previous,
                target: None,
                error: "selected endpoint is no longer active".into(),
            };
        };
        if let Err(error) =
            crate::audio::devices::set_system_default(&self.enumerator, endpoint_flow, &device)
        {
            return DeviceCycleResult::Failed {
                flow,
                previous,
                target: Some(device),
                error: error.to_string(),
            };
        }
        self.pending_default_switches.push(PendingDefaultSwitch {
            flow: endpoint_flow,
            endpoint: device.endpoint.clone(),
            observed_roles: 0,
        });
        let snapshot = self.config.snapshot();
        self.rebuild_all(
            false,
            snapshot.value,
            crate::event::AudioEventOrigin::Config(crate::event::ConfigCommitOrigin::DeviceCycle),
        );
        DeviceCycleResult::Changed {
            flow,
            previous,
            device,
        }
    }

    fn cycle_device(&mut self, flow: DeviceCycleFlow) -> DeviceCycleResult {
        let active = {
            let devices = self.devices.read().expect("audio device list");
            match flow {
                DeviceCycleFlow::Input => devices.inputs.clone(),
                DeviceCycleFlow::Output => devices.outputs.clone(),
            }
        };
        let endpoint_flow = match flow {
            DeviceCycleFlow::Input => EndpointFlow::Capture,
            DeviceCycleFlow::Output => EndpointFlow::Render,
        };
        let current_default =
            crate::audio::devices::current_default_device(&self.enumerator, endpoint_flow);
        let previous = current_default.as_ref().ok().cloned();
        let config_snapshot = self.config.snapshot();
        let allowlist = match flow {
            DeviceCycleFlow::Input => config_snapshot.value.audio.cycle_input_allowlist.as_deref(),
            DeviceCycleFlow::Output => config_snapshot
                .value
                .audio
                .cycle_output_allowlist
                .as_deref(),
        };
        match crate::audio::devices::device_cycle_result_with_allowlist(
            flow, previous, &active, allowlist,
        ) {
            DeviceCycleResult::NoDevices { flow, previous } => {
                DeviceCycleResult::NoDevices { flow, previous }
            }
            DeviceCycleResult::Changed {
                flow,
                previous,
                device,
            } => {
                if let Err(error) = crate::audio::devices::set_system_default(
                    &self.enumerator,
                    endpoint_flow,
                    &device,
                ) {
                    return DeviceCycleResult::Failed {
                        flow,
                        previous,
                        target: Some(device),
                        error: error.to_string(),
                    };
                }
                self.pending_default_switches.push(PendingDefaultSwitch {
                    flow: endpoint_flow,
                    endpoint: device.endpoint.clone(),
                    observed_roles: 0,
                });
                let snapshot = self.config.snapshot();
                self.rebuild_all(
                    false,
                    snapshot.value,
                    crate::event::AudioEventOrigin::Config(
                        crate::event::ConfigCommitOrigin::DeviceCycle,
                    ),
                );
                DeviceCycleResult::Changed {
                    flow,
                    previous,
                    device,
                }
            }
            DeviceCycleResult::Failed {
                flow,
                previous,
                target,
                error,
            } => DeviceCycleResult::Failed {
                flow,
                previous,
                target,
                error,
            },
        }
    }

    fn refresh_config_if_needed(&mut self) {
        let snapshot = self.config.snapshot();
        if snapshot.stamp.revision != self.config_revision {
            let pending = PendingRebuild::config(snapshot.stamp);
            self.apply_pending_rebuild_snapshot(snapshot, pending);
        }
    }

    fn rebuild_all(
        &mut self,
        startup: bool,
        config: Arc<crate::config::Config>,
        origin: crate::event::AudioEventOrigin,
    ) {
        let old_render = self.render.as_ref().map(|e| e.identity.clone());
        self.rebuild(EndpointFlow::Capture, config.as_ref());
        self.rebuild(EndpointFlow::Render, config.as_ref());
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
        self.publish(EndpointFlow::Capture);
        self.publish(EndpointFlow::Render);
    }

    fn rebuild(&mut self, flow: EndpointFlow, config: &crate::config::Config) {
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

    fn toggle(&mut self, flow: EndpointFlow) {
        let missing = match flow {
            EndpointFlow::Capture => self.capture.is_none(),
            EndpointFlow::Render => self.render.is_none(),
        };
        if missing {
            let snapshot = self.config.snapshot();
            self.rebuild(flow, snapshot.value.as_ref());
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
        self.publish(flow);
    }

    fn publish(&self, flow: EndpointFlow) {
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
                self.post(AppEvent::MicrophoneStateChanged { state });
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
                self.post(AppEvent::OutputStateChanged { state });
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
        let (pending_rebuild, rest) = coalesce_backlog(backlog);
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
        controller.apply_pending_rebuild(pending_rebuild);
    }
    controller.shutdown();
    drop(com);
    crate::info!("audio controller stopped");
}

/// Describe a backlog's collapsible refresh work while preserving every
/// non-refresh command's position and payload.
fn coalesce_backlog(backlog: Vec<AudioCommand>) -> (PendingRebuild, Vec<AudioCommand>) {
    let mut pending = PendingRebuild::default();
    let mut rest = Vec::new();
    for cmd in backlog {
        match cmd {
            AudioCommand::RefreshAll => pending.external_refresh = true,
            AudioCommand::ConfigChanged { stamp } => pending.config_revision = Some(stamp.revision),
            AudioCommand::DefaultDeviceChanged {
                flow,
                role,
                endpoint,
            } => pending.default_changes.push(DefaultDeviceChange {
                flow,
                role,
                endpoint,
            }),
            other => rest.push(other),
        }
    }
    (pending, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(revision: u64, origin: crate::event::ConfigCommitOrigin) -> ConfigRevisionStamp {
        ConfigRevisionStamp { revision, origin }
    }

    fn config_changed(revision: u64, origin: crate::event::ConfigCommitOrigin) -> AudioCommand {
        AudioCommand::ConfigChanged {
            stamp: stamp(revision, origin),
        }
    }

    fn default_changed(flow: EndpointFlow, role: EndpointRole, endpoint: &str) -> AudioCommand {
        AudioCommand::DefaultDeviceChanged {
            flow,
            role,
            endpoint: endpoint.into(),
        }
    }

    #[test]
    fn default_device_notifications_coalesce_as_refresh_work() {
        let (pending, rest) = coalesce_backlog(vec![
            default_changed(EndpointFlow::Render, EndpointRole::Console, "first"),
            default_changed(EndpointFlow::Render, EndpointRole::Communications, "second"),
        ]);
        assert!(rest.is_empty());
        assert_eq!(pending.default_changes.len(), 2);
        assert_eq!(pending.default_changes[0].endpoint, "first");
        assert_eq!(pending.default_changes[1].endpoint, "second");
    }

    #[test]
    fn own_default_notifications_are_consumed_for_all_three_roles() {
        let mut pending = vec![PendingDefaultSwitch {
            flow: EndpointFlow::Render,
            endpoint: "render-endpoint".into(),
            observed_roles: 0,
        }];
        for role in [
            EndpointRole::Console,
            EndpointRole::Multimedia,
            EndpointRole::Communications,
        ] {
            assert!(AudioController::consume_pending_default_change(
                &mut pending,
                &DefaultDeviceChange {
                    flow: EndpointFlow::Render,
                    role,
                    endpoint: "render-endpoint".into(),
                },
            ));
        }
        assert!(pending.is_empty());
    }

    fn snapshot(
        revision: u64,
        origin: crate::event::ConfigCommitOrigin,
        marker: u32,
    ) -> ConfigSnapshot {
        let mut config = crate::config::Config::default();
        config.overlay.duration_ms = marker;
        ConfigSnapshot {
            value: std::sync::Arc::new(config),
            stamp: stamp(revision, origin),
        }
    }

    #[test]
    fn rebuild_plan_owns_one_config_snapshot_and_origin() {
        let snapshot = snapshot(6, crate::event::ConfigCommitOrigin::DeviceCycle, 600);
        let (consumed, plan) =
            plan_pending_rebuild(snapshot.clone(), 5, PendingRebuild::config(snapshot.stamp));
        let plan = plan.expect("new revision plans one rebuild");
        assert_eq!(consumed, 6);
        assert_eq!(plan.snapshot.stamp, snapshot.stamp);
        assert_eq!(plan.snapshot.value.overlay.duration_ms, 600);
        assert_eq!(
            plan.origin,
            crate::event::AudioEventOrigin::Config(crate::event::ConfigCommitOrigin::DeviceCycle)
        );
    }

    #[test]
    fn newer_publication_does_not_mutate_existing_rebuild_plan() {
        let handle = crate::config::ConfigHandle::new(crate::config::Config::default());
        let mut rev6_config = crate::config::Config::default();
        rev6_config.overlay.duration_ms = 600;
        handle
            .replace_after_save(
                rev6_config,
                crate::event::ConfigCommitOrigin::Settings,
                |_| Ok(()),
            )
            .unwrap();
        let rev6 = handle.snapshot();
        let (consumed6, plan6) =
            plan_pending_rebuild(rev6.clone(), 1, PendingRebuild::config(rev6.stamp));
        let plan6 = plan6.expect("rev6 needs one rebuild");

        let mut rev7_config = crate::config::Config::default();
        rev7_config.overlay.duration_ms = 700;
        handle
            .replace_after_save(
                rev7_config,
                crate::event::ConfigCommitOrigin::DeviceCycle,
                |_| Ok(()),
            )
            .unwrap();
        let rev7 = handle.snapshot();

        assert_eq!(consumed6, 2);
        assert_eq!(plan6.snapshot.stamp, rev6.stamp);
        assert_eq!(plan6.snapshot.value.overlay.duration_ms, 600);
        assert_eq!(plan6.origin, crate::event::AudioEventOrigin::External);

        let (consumed7, plan7) =
            plan_pending_rebuild(rev7.clone(), consumed6, PendingRebuild::config(rev7.stamp));
        let plan7 = plan7.expect("rev7 needs one later rebuild");
        assert_eq!(consumed7, 3);
        assert_eq!(plan7.snapshot.stamp, rev7.stamp);
        assert_eq!(plan7.snapshot.value.overlay.duration_ms, 700);
        assert_eq!(
            plan7.origin,
            crate::event::AudioEventOrigin::Config(crate::event::ConfigCommitOrigin::DeviceCycle)
        );
    }

    #[test]
    fn capture_and_render_share_the_planned_config_arc() {
        let snapshot = snapshot(6, crate::event::ConfigCommitOrigin::Settings, 600);
        let (_, plan) = plan_pending_rebuild(
            snapshot,
            5,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::Settings)),
        );
        let plan = plan.expect("new revision plans one rebuild");
        let capture_config = std::sync::Arc::clone(&plan.snapshot.value);
        let render_config = std::sync::Arc::clone(&plan.snapshot.value);
        assert!(std::sync::Arc::ptr_eq(&capture_config, &render_config));
        assert_eq!(capture_config.overlay.duration_ms, 600);
        assert_eq!(render_config.overlay.duration_ms, 600);
    }

    #[test]
    fn settings_plan_then_device_cycle_publication_rebuilds_both_revisions_coherently() {
        let rev6 = snapshot(6, crate::event::ConfigCommitOrigin::Settings, 600);
        let (consumed6, plan6) =
            plan_pending_rebuild(rev6.clone(), 5, PendingRebuild::config(rev6.stamp));
        let plan6 = plan6.expect("rev6 needs one rebuild");
        assert_eq!(consumed6, 6);
        assert_eq!(plan6.snapshot.value.overlay.duration_ms, 600);
        assert_eq!(plan6.origin, crate::event::AudioEventOrigin::External);

        let rev7 = snapshot(7, crate::event::ConfigCommitOrigin::DeviceCycle, 700);
        let (consumed7, plan7) =
            plan_pending_rebuild(rev7.clone(), consumed6, PendingRebuild::config(rev7.stamp));
        let plan7 = plan7.expect("rev7 needs one rebuild");
        assert_eq!(consumed7, 7);
        assert_eq!(plan7.snapshot.value.overlay.duration_ms, 700);
        assert_eq!(
            plan7.origin,
            crate::event::AudioEventOrigin::Config(crate::event::ConfigCommitOrigin::DeviceCycle)
        );
    }

    #[test]
    fn device_cycle_plan_then_settings_publication_rebuilds_both_revisions_coherently() {
        let rev6 = snapshot(6, crate::event::ConfigCommitOrigin::DeviceCycle, 600);
        let (consumed6, plan6) =
            plan_pending_rebuild(rev6.clone(), 5, PendingRebuild::config(rev6.stamp));
        let plan6 = plan6.expect("rev6 needs one rebuild");
        assert_eq!(consumed6, 6);
        assert_eq!(plan6.snapshot.value.overlay.duration_ms, 600);
        assert_eq!(
            plan6.origin,
            crate::event::AudioEventOrigin::Config(crate::event::ConfigCommitOrigin::DeviceCycle)
        );

        let rev7 = snapshot(7, crate::event::ConfigCommitOrigin::Settings, 700);
        let (consumed7, plan7) =
            plan_pending_rebuild(rev7.clone(), consumed6, PendingRebuild::config(rev7.stamp));
        let plan7 = plan7.expect("rev7 needs one rebuild");
        assert_eq!(consumed7, 7);
        assert_eq!(plan7.snapshot.value.overlay.duration_ms, 700);
        assert_eq!(plan7.origin, crate::event::AudioEventOrigin::External);
    }

    #[test]
    fn refresh_burst_collapses_to_single_rebuild() {
        let backlog = vec![
            AudioCommand::RefreshAll,
            AudioCommand::RefreshEndpoint(EndpointFlow::Capture),
            AudioCommand::RefreshAll,
            AudioCommand::ToggleOutput(1),
            AudioCommand::RefreshAll,
            config_changed(7, crate::event::ConfigCommitOrigin::Settings),
            AudioCommand::Shutdown,
        ];
        let (pending, rest) = coalesce_backlog(backlog);
        assert!(pending.external_refresh);
        assert_eq!(pending.config_revision, Some(7));
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
    fn device_cycle_live_revision_observed_before_config_changed_rebuilds_quietly() {
        let pending =
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle));
        let (consumed, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            5,
            pending,
        );
        assert_eq!(consumed, 6);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        );
    }

    #[test]
    fn delayed_device_cycle_config_changed_for_consumed_revision_is_noop() {
        let (consumed, first) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            5,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle)),
        );
        assert!(matches!(
            first,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        ));
        let (same_revision, delayed) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            consumed,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle)),
        );
        assert_eq!(same_revision, 6);
        assert_eq!(delayed, PendingRebuildDecision::None);
    }

    #[test]
    fn settings_live_revision_observed_before_config_changed_uses_external_origin() {
        let (consumed, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::Settings),
            5,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::Settings)),
        );
        assert_eq!(consumed, 6);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::External)
        );
    }

    #[test]
    fn rapid_device_cycle_then_settings_uses_latest_live_origin() {
        let (pending, rest) = coalesce_backlog(vec![
            config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            config_changed(7, crate::event::ConfigCommitOrigin::Settings),
        ]);
        assert!(rest.is_empty());
        assert_eq!(pending.config_revision, Some(7));
        let (consumed, decision) = decide_pending_rebuild(
            stamp(7, crate::event::ConfigCommitOrigin::Settings),
            5,
            pending,
        );
        assert_eq!(consumed, 7);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::External)
        );

        let (_, stale) = decide_pending_rebuild(
            stamp(7, crate::event::ConfigCommitOrigin::Settings),
            consumed,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle)),
        );
        assert_eq!(stale, PendingRebuildDecision::None);
    }

    #[test]
    fn rapid_settings_then_device_cycle_uses_latest_live_origin() {
        let (pending, _) = coalesce_backlog(vec![
            config_changed(6, crate::event::ConfigCommitOrigin::Settings),
            config_changed(7, crate::event::ConfigCommitOrigin::DeviceCycle),
        ]);
        let (consumed, decision) = decide_pending_rebuild(
            stamp(7, crate::event::ConfigCommitOrigin::DeviceCycle),
            5,
            pending,
        );
        assert_eq!(consumed, 7);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        );
    }

    #[test]
    fn stale_config_changed_plus_real_refresh_is_one_external_rebuild() {
        let (pending, rest) = coalesce_backlog(vec![
            config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            AudioCommand::RefreshAll,
        ]);
        assert!(rest.is_empty());
        let (consumed, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            6,
            pending,
        );
        assert_eq!(consumed, 6);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::External)
        );
    }

    #[test]
    fn new_device_cycle_revision_plus_real_refresh_is_one_quiet_rebuild() {
        let (pending, rest) = coalesce_backlog(vec![
            AudioCommand::RefreshAll,
            config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
        ]);
        assert!(rest.is_empty());
        let (consumed, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            5,
            pending,
        );
        assert_eq!(consumed, 6);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        );
    }

    #[test]
    fn preflight_consumption_makes_later_config_changed_noop() {
        let (pending, rest) = coalesce_backlog(vec![
            AudioCommand::ToggleOutput(1),
            config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
        ]);
        assert_eq!(rest, vec![AudioCommand::ToggleOutput(1)]);

        let (consumed, preflight) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            5,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle)),
        );
        assert_eq!(
            preflight,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        );
        let (same_revision, delayed) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            consumed,
            pending,
        );
        assert_eq!(same_revision, 6);
        assert_eq!(delayed, PendingRebuildDecision::None);
    }

    #[test]
    fn stale_config_command_origin_cannot_describe_newer_live_revision() {
        let (revision, decision) = decide_pending_rebuild(
            stamp(7, crate::event::ConfigCommitOrigin::DeviceCycle),
            6,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::Settings)),
        );
        assert_eq!(revision, 7);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        );
    }

    #[test]
    fn stale_config_changed_alone_does_not_rebuild() {
        let (_, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            6,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle)),
        );
        assert_eq!(decision, PendingRebuildDecision::None);
    }

    #[test]
    fn pending_config_change_consumes_revision_and_rebuilds_once() {
        let (revision, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::Settings),
            5,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::Settings)),
        );
        assert_eq!(revision, 6);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::External)
        );
    }

    #[test]
    fn device_cycle_origin_suppresses_concurrent_external_refresh() {
        let (pending, rest) = coalesce_backlog(vec![
            AudioCommand::RefreshAll,
            config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
        ]);
        assert!(rest.is_empty());
        let (revision, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            5,
            pending,
        );
        assert_eq!(revision, 6);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        );
    }

    #[test]
    fn stale_device_cycle_config_and_real_refresh_use_one_external_rebuild() {
        let (pending, rest) = coalesce_backlog(vec![
            config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            AudioCommand::RefreshAll,
        ]);
        assert!(rest.is_empty());
        let (revision, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            6,
            pending,
        );
        assert_eq!(revision, 6);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::External)
        );
    }

    #[test]
    fn stale_config_after_revision_drift_consumed_before_final_batch_is_noop() {
        let (pending, rest) = coalesce_backlog(vec![
            AudioCommand::ToggleOutput(1),
            config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
        ]);
        assert_eq!(rest, vec![AudioCommand::ToggleOutput(1)]);

        let (consumed, first_decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            5,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle)),
        );
        assert_eq!(
            first_decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        );
        let (same_revision, final_decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            consumed,
            pending,
        );
        assert_eq!(same_revision, 6);
        assert_eq!(final_decision, PendingRebuildDecision::None);
    }

    #[test]
    fn coalesced_config_consumption_prevents_later_revision_drift_rebuild() {
        let (consumed, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            5,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle)),
        );
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                crate::event::ConfigCommitOrigin::DeviceCycle
            ))
        );
        let (same_revision, later_decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            consumed,
            PendingRebuild::config(stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle)),
        );
        assert_eq!(same_revision, 6);
        assert_eq!(later_decision, PendingRebuildDecision::None);
    }

    #[test]
    fn latest_config_origin_wins_within_one_batch() {
        let (pending, _) = coalesce_backlog(vec![
            config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            config_changed(7, crate::event::ConfigCommitOrigin::Settings),
        ]);
        assert_eq!(pending.config_revision, Some(7));
        let (_, decision) = decide_pending_rebuild(
            stamp(7, crate::event::ConfigCommitOrigin::Settings),
            5,
            pending,
        );
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::External)
        );
    }

    #[test]
    fn refresh_order_does_not_override_pending_config_origin() {
        for backlog in [
            vec![
                AudioCommand::RefreshAll,
                config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            ],
            vec![
                config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
                AudioCommand::RefreshAll,
            ],
        ] {
            let (pending, _) = coalesce_backlog(backlog);
            let (_, decision) = decide_pending_rebuild(
                stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
                5,
                pending,
            );
            assert_eq!(
                decision,
                PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::Config(
                    crate::event::ConfigCommitOrigin::DeviceCycle
                ))
            );
        }

        let (pending, _) = coalesce_backlog(vec![
            AudioCommand::RefreshAll,
            config_changed(6, crate::event::ConfigCommitOrigin::DeviceCycle),
        ]);
        let (_, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::DeviceCycle),
            6,
            pending,
        );
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::External)
        );
    }

    #[test]
    fn direct_external_refresh_combines_new_config_drift_once() {
        let (revision, decision) = decide_pending_rebuild(
            stamp(6, crate::event::ConfigCommitOrigin::Settings),
            5,
            PendingRebuild::external(),
        );
        assert_eq!(revision, 6);
        assert_eq!(
            decision,
            PendingRebuildDecision::Rebuild(crate::event::AudioEventOrigin::External)
        );
    }

    #[test]
    fn empty_backlog_needs_no_rebuild() {
        assert!(coalesce_backlog(Vec::new()).0.is_empty());
    }
}
