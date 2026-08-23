//! Core Audio worker. Owns its MTA COM apartment, enumerator, endpoint volume
//! interfaces, and callbacks. Raw COM pointers never leave this thread.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use windows::Win32::Media::Audio::{IMMDeviceEnumerator, IMMNotificationClient, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

use crate::audio::devices::EndpointBinding;
use crate::audio::notifications::DeviceNotificationClient;
use crate::audio::state::{AudioState, OutputState};
use crate::config::ConfigHandle;
use crate::error::{Error, Result};
use crate::event::AppEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointFlow {
    Capture,
    Render,
}

#[derive(Debug)]
pub enum AudioCommand {
    ToggleMicrophone,
    ToggleOutput,
    ToggleForeground(Option<u32>),
    RefreshEndpoint(EndpointFlow),
    RefreshAll,
    ConfigChanged,
    Shutdown,
}

pub struct AudioService {
    sender: Sender<AudioCommand>,
    devices: Arc<std::sync::RwLock<crate::audio::devices::DeviceLists>>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl AudioService {
    pub fn start(
        main_hwnd: windows::Win32::Foundation::HWND,
        config: Arc<ConfigHandle>,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let worker_sender = sender.clone();
        let devices = Arc::new(std::sync::RwLock::new(
            crate::audio::devices::DeviceLists::default(),
        ));
        let worker_devices = Arc::clone(&devices);
        let hwnd_raw = main_hwnd.0 as isize;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("winshort-audio".into())
            .spawn(move || {
                audio_thread(
                    hwnd_raw,
                    config,
                    worker_sender,
                    worker_devices,
                    receiver,
                    ready_tx,
                )
            })
            .map_err(|e| Error::internal(format!("spawn audio thread: {e}")))?;
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(8))
            .map_err(|_| Error::audio("audio startup timed out"))??;
        Ok(Self { sender, devices, join: Some(join) })
    }

    pub fn send(&self, command: AudioCommand) {
        let _ = self.sender.send(command);
    }

    pub fn devices(&self) -> crate::audio::devices::DeviceLists {
        self.devices.read().expect("audio device list").clone()
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
    enumerator: IMMDeviceEnumerator,
    device_callback: IMMNotificationClient,
    capture: Option<EndpointBinding>,
    render: Option<EndpointBinding>,
}

impl AudioController {
    fn create(
        main_hwnd_raw: isize,
        config: Arc<ConfigHandle>,
        sender: Sender<AudioCommand>,
        devices: Arc<std::sync::RwLock<crate::audio::devices::DeviceLists>>,
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
            enumerator,
            device_callback,
            capture: None,
            render: None,
        };
        controller.rebuild_all(true);
        Ok(controller)
    }

    fn handle(&mut self, command: AudioCommand) -> bool {
        self.refresh_config_if_needed();
        match command {
            AudioCommand::ToggleMicrophone => self.toggle(EndpointFlow::Capture),
            AudioCommand::ToggleOutput => self.toggle(EndpointFlow::Render),
            AudioCommand::ToggleForeground(pid) => {
                let config = self.config.get();
                let state = crate::audio::sessions::toggle_foreground(
                    &self.enumerator,
                    &config,
                    pid,
                )
                .unwrap_or_else(|e| crate::audio::AppAudioState {
                    app_name: None,
                    aggregate: crate::audio::Aggregate::NoSession,
                    sessions: 0,
                });
                self.post(AppEvent::ForegroundAudioChanged(state));
            }
            AudioCommand::RefreshEndpoint(flow) => {
                crate::log_debug!("audio {:?} endpoint notification", flow);
                self.publish(flow);
            }
            AudioCommand::RefreshAll | AudioCommand::ConfigChanged => self.rebuild_all(false),
            AudioCommand::Shutdown => return false,
        }
        true
    }
    fn refresh_config_if_needed(&mut self) {
        let revision = self.config.revision();
        if revision != self.config_revision {
            self.config_revision = revision;
            self.rebuild_all(false);
        }
    }

    fn rebuild_all(&mut self, startup: bool) {
        let old_render = self.render.as_ref().map(|e| e.identity.clone());
        self.rebuild(EndpointFlow::Capture);
        self.rebuild(EndpointFlow::Render);
        if let Ok(lists) = crate::audio::devices::enumerate_devices(&self.enumerator) {
            if let Ok(mut shared) = self.devices.write() {
                *shared = lists;
            }
        }

        if !startup {
            if let (Some(old), Some(new)) = (old_render, self.render.as_ref().map(|e| e.identity.clone())) {
                if old.endpoint != new.endpoint {
                    self.post(AppEvent::OutputStateChanged(OutputState::Changed { new }));
                }
            }
            self.post(AppEvent::DevicesChanged);
        }
        self.publish(EndpointFlow::Capture);
        self.publish(EndpointFlow::Render);
    }

    fn rebuild(&mut self, flow: EndpointFlow) {
        let slot = match flow {
            EndpointFlow::Capture => &mut self.capture,
            EndpointFlow::Render => &mut self.render,
        };
        if let Some(old) = slot.take() {
            old.unregister();
        }
        let config = self.config.get();
        let (selection, role) = match flow {
            EndpointFlow::Capture => (&config.audio.input_device, config.audio.input_role),
            EndpointFlow::Render => (&config.audio.output_device, config.audio.output_role),
        };
        match EndpointBinding::create(
            &self.enumerator,
            flow,
            selection,
            role,
            self.sender.clone(),
        ) {
            Ok(endpoint) => {
                crate::info!("audio {:?} endpoint: {}", flow, endpoint.identity.name);
                *slot = Some(endpoint);
            }
            Err(e) => {
                crate::warn_!("audio {:?} unavailable: {e}", flow);
                *slot = None;
            }
        }
    }

    fn toggle(&mut self, flow: EndpointFlow) {
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
                    .unwrap_or_else(|e| AudioState::Unavailable { reason: e.to_string() });
                self.post(AppEvent::MicrophoneStateChanged(state));
            }
            EndpointFlow::Render => {
                let state = self
                    .render
                    .as_ref()
                    .ok_or_else(|| Error::audio("output unavailable"))
                    .and_then(EndpointBinding::output_state)
                    .unwrap_or_else(|e| OutputState::Unavailable { reason: e.to_string() });
                self.post(AppEvent::OutputStateChanged(state));
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
        if let Some(endpoint) = self.capture.take() {
            endpoint.unregister();
        }
        if let Some(endpoint) = self.render.take() {
            endpoint.unregister();
        }
        unsafe {
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.device_callback);
        }
    }
}

fn audio_thread(
    hwnd_raw: isize,
    config: Arc<ConfigHandle>,
    sender: Sender<AudioCommand>,
    devices: Arc<std::sync::RwLock<crate::audio::devices::DeviceLists>>,
    receiver: Receiver<AudioCommand>,
    ready: mpsc::SyncSender<std::result::Result<(), Error>>,
) {
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if hr.0 < 0 {
        let _ = ready.send(Err(Error::os("CoInitializeEx(audio)", hr.0 as u32)));
        return;
    }

    let mut controller = match AudioController::create(hwnd_raw, config, sender, devices) {
        Ok(controller) => controller,
        Err(e) => {
            let _ = ready.send(Err(e));
            unsafe { CoUninitialize(); }
            return;
        }
    };
    let _ = ready.send(Ok(()));
    crate::info!("audio controller ready");

    while let Ok(command) = receiver.recv() {
        if !controller.handle(command) {
            break;
        }
    }
    controller.shutdown();
    unsafe { CoUninitialize(); }
    crate::info!("audio controller stopped");
}
