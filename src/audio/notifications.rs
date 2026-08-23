//! Core Audio callbacks. They enqueue compact worker commands only; no UI or
//! endpoint manipulation occurs inside COM callback methods (spec §25).

use std::sync::mpsc::Sender;

use windows::core::{implement, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole, IMMNotificationClient,
    IMMNotificationClient_Impl,
};

use crate::audio::controller::{AudioCommand, EndpointFlow};

#[implement(IMMNotificationClient)]
pub struct DeviceNotificationClient {
    sender: Sender<AudioCommand>,
}

impl DeviceNotificationClient {
    pub fn create(sender: Sender<AudioCommand>) -> IMMNotificationClient {
        Self { sender }.into()
    }
}

impl IMMNotificationClient_Impl for DeviceNotificationClient_Impl {
    fn OnDeviceStateChanged(
        &self,
        _device_id: &PCWSTR,
        _new_state: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        let _ = self.sender.send(AudioCommand::RefreshAll);
        Ok(())
    }

    fn OnDeviceAdded(&self, _device_id: &PCWSTR) -> windows::core::Result<()> {
        let _ = self.sender.send(AudioCommand::RefreshAll);
        Ok(())
    }

    fn OnDeviceRemoved(&self, _device_id: &PCWSTR) -> windows::core::Result<()> {
        let _ = self.sender.send(AudioCommand::RefreshAll);
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        _flow: EDataFlow,
        _role: ERole,
        _default_device_id: &PCWSTR,
    ) -> windows::core::Result<()> {
        let _ = self.sender.send(AudioCommand::RefreshAll);
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        _device_id: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        let _ = self.sender.send(AudioCommand::RefreshAll);
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
pub struct EndpointVolumeClient {
    sender: Sender<AudioCommand>,
    flow: EndpointFlow,
}

impl EndpointVolumeClient {
    pub fn create(
        sender: Sender<AudioCommand>,
        flow: EndpointFlow,
    ) -> IAudioEndpointVolumeCallback {
        Self { sender, flow }.into()
    }
}

impl IAudioEndpointVolumeCallback_Impl for EndpointVolumeClient_Impl {
    fn OnNotify(
        &self,
        _notification: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
    ) -> windows::core::Result<()> {
        let _ = self.sender.send(AudioCommand::RefreshEndpoint(self.flow));
        Ok(())
    }
}
