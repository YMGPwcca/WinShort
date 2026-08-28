//! Core Audio endpoint wrappers. Unsafe/COM details stay here; callers receive
//! typed state and device identities.

use std::sync::mpsc::Sender;

use windows::core::{GUID, HSTRING, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDevice,
    IMMDeviceEnumerator,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{CoTaskMemFree, CLSCTX_ALL, STGM_READ};

use crate::audio::controller::{AudioCommand, EndpointFlow};
use crate::audio::notifications::EndpointVolumeClient;
use crate::audio::state::{AudioState, DeviceId, OutputState};
use crate::config::model::{DeviceSelection, EndpointRole};
use crate::error::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceLists {
    pub inputs: Vec<DeviceId>,
    pub outputs: Vec<DeviceId>,
    /// Per-flow degradation notes from the last enumeration (#36): a failed
    /// flow yields an empty list plus a warning instead of failing wholesale.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCyclePlan {
    Select(DeviceSelection),
    NoActiveEndpoints,
}

/// Plan the next configured endpoint without consulting friendly names.
///
/// The active inventory order is the cycle order. Endpoint IDs are opaque and
/// remain the only identity used for matching an existing selection.
pub fn plan_device_cycle(current: &DeviceSelection, active: &[DeviceId]) -> DeviceCyclePlan {
    if active.is_empty() {
        return match current {
            DeviceSelection::Default => DeviceCyclePlan::NoActiveEndpoints,
            DeviceSelection::Endpoint(_) => DeviceCyclePlan::Select(DeviceSelection::Default),
        };
    }
    let next = match current {
        DeviceSelection::Default => DeviceSelection::Endpoint(active[0].endpoint.clone()),
        DeviceSelection::Endpoint(id) => {
            match active.iter().position(|device| device.endpoint == *id) {
                Some(index) if index + 1 < active.len() => {
                    DeviceSelection::Endpoint(active[index + 1].endpoint.clone())
                }
                Some(_) | None => DeviceSelection::Default,
            }
        }
    };
    DeviceCyclePlan::Select(next)
}

pub fn device_cycle_result(
    flow: crate::audio::DeviceCycleFlow,
    current: &DeviceSelection,
    active: &[DeviceId],
) -> crate::audio::DeviceCycleResult {
    let previous = current.clone();
    match plan_device_cycle(current, active) {
        DeviceCyclePlan::Select(selection) => {
            let device = match &selection {
                DeviceSelection::Endpoint(id) => {
                    active.iter().find(|device| device.endpoint == *id).cloned()
                }
                DeviceSelection::Default => None,
            };
            crate::audio::DeviceCycleResult::Changed {
                flow,
                previous,
                selection,
                device,
            }
        }
        DeviceCyclePlan::NoActiveEndpoints => {
            crate::audio::DeviceCycleResult::NoDevices { flow, previous }
        }
    }
}

/// Enumerate active endpoints per flow. One flow failing (or one device's
/// properties failing inside a flow) degrades to an empty list + warning
/// rather than losing both flows (#36).
pub fn enumerate_devices(enumerator: &IMMDeviceEnumerator) -> DeviceLists {
    let mut warnings = Vec::new();
    let inputs = match enumerate_flow(enumerator, EndpointFlow::Capture) {
        Ok(devices) => devices,
        Err(e) => {
            warnings.push(format!("capture endpoint enumeration failed: {e}"));
            Vec::new()
        }
    };
    let outputs = match enumerate_flow(enumerator, EndpointFlow::Render) {
        Ok(devices) => devices,
        Err(e) => {
            warnings.push(format!("render endpoint enumeration failed: {e}"));
            Vec::new()
        }
    };
    DeviceLists {
        inputs,
        outputs,
        warnings,
    }
}

fn enumerate_flow(enumerator: &IMMDeviceEnumerator, flow: EndpointFlow) -> Result<Vec<DeviceId>> {
    use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;
    unsafe {
        let collection = enumerator
            .EnumAudioEndpoints(data_flow(flow), DEVICE_STATE_ACTIVE)
            .map_err(|e| Error::win("EnumAudioEndpoints", &e))?;
        let count = collection
            .GetCount()
            .map_err(|e| Error::win("IMMDeviceCollection::GetCount", &e))?;
        let mut devices = Vec::with_capacity(count as usize);
        for index in 0..count {
            let Ok(device) = collection.Item(index) else {
                continue;
            };
            if let Ok(id) = identity(&device, flow) {
                devices.push(id);
            }
        }
        devices.sort_by_key(|d| d.name.to_lowercase());
        Ok(devices)
    }
}
/// PKEY_Device_FriendlyName; defined here to avoid pulling unrelated device APIs.
const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

/// Event context prevents self-originated volume callbacks from being confused
/// with external changes in diagnostics.
pub const AUDIO_EVENT_CONTEXT: GUID = GUID::from_u128(0x7f4f3793_9098_4ad8_9f6e_6a6815af3984);

pub struct EndpointBinding {
    pub identity: DeviceId,
    pub volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

impl EndpointBinding {
    pub fn create(
        enumerator: &IMMDeviceEnumerator,
        flow: EndpointFlow,
        selection: &DeviceSelection,
        role: EndpointRole,
        sender: Sender<AudioCommand>,
    ) -> Result<Self> {
        let device = resolve_device(enumerator, flow, selection, role)?;
        let identity = identity(&device, flow)?;
        let volume: IAudioEndpointVolume = unsafe {
            device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| Error::win("IMMDevice::Activate(IAudioEndpointVolume)", &e))?
        };
        let callback = EndpointVolumeClient::create(sender, flow);
        unsafe {
            volume
                .RegisterControlChangeNotify(&callback)
                .map_err(|e| Error::win("RegisterControlChangeNotify", &e))?;
        }
        Ok(Self {
            identity,
            volume,
            callback,
        })
    }

    pub fn mute(&self) -> Result<bool> {
        unsafe {
            self.volume
                .GetMute()
                .map(|v| v.as_bool())
                .map_err(|e| Error::win("IAudioEndpointVolume::GetMute", &e))
        }
    }

    pub fn volume_percent(&self) -> Result<u8> {
        unsafe {
            self.volume
                .GetMasterVolumeLevelScalar()
                .map(|v| (v.clamp(0.0, 1.0) * 100.0).round() as u8)
                .map_err(|e| Error::win("GetMasterVolumeLevelScalar", &e))
        }
    }

    pub fn set_mute(&self, muted: bool) -> Result<()> {
        unsafe {
            self.volume
                .SetMute(muted, &AUDIO_EVENT_CONTEXT)
                .map_err(|e| Error::win("IAudioEndpointVolume::SetMute", &e))
        }
    }

    pub fn capture_state(&self) -> Result<AudioState> {
        let muted = self.mute()?;
        let volume_pct = self.volume_percent()?;
        Ok(if muted {
            AudioState::Muted { volume_pct }
        } else {
            AudioState::Active { volume_pct }
        })
    }

    pub fn output_state(&self) -> Result<OutputState> {
        Ok(OutputState::Current {
            device: self.identity.clone(),
            muted: self.mute()?,
            volume_pct: self.volume_percent()?,
        })
    }
}

pub(crate) fn resolve_device(
    enumerator: &IMMDeviceEnumerator,
    flow: EndpointFlow,
    selection: &DeviceSelection,
    role: EndpointRole,
) -> Result<IMMDevice> {
    unsafe {
        match selection {
            DeviceSelection::Default => enumerator
                .GetDefaultAudioEndpoint(data_flow(flow), endpoint_role(role))
                .map_err(|e| {
                    Error::os_ctx(
                        "GetDefaultAudioEndpoint",
                        e.code().0 as u32,
                        format!("{:?} / {:?}", flow, role),
                    )
                }),
            DeviceSelection::Endpoint(id) => {
                let wide = HSTRING::from(id);
                enumerator
                    .GetDevice(PCWSTR(wide.as_ptr()))
                    .map_err(|e| Error::win("IMMDeviceEnumerator::GetDevice", &e))
            }
        }
    }
}

pub fn data_flow(flow: EndpointFlow) -> EDataFlow {
    match flow {
        EndpointFlow::Capture => eCapture,
        EndpointFlow::Render => eRender,
    }
}

pub fn endpoint_role(role: EndpointRole) -> ERole {
    match role {
        EndpointRole::Console => eConsole,
        EndpointRole::Multimedia => eMultimedia,
        EndpointRole::Communications => eCommunications,
    }
}

pub(crate) fn identity(device: &IMMDevice, flow: EndpointFlow) -> Result<DeviceId> {
    let endpoint = unsafe {
        let id = device
            .GetId()
            .map_err(|e| Error::win("IMMDevice::GetId", &e))?;
        let result = id
            .to_string()
            .map_err(|e| Error::audio(format!("endpoint id UTF-16: {e}")));
        CoTaskMemFree(Some(id.0.cast()));
        result?
    };
    let name = friendly_name(device).unwrap_or_else(|_| match flow {
        EndpointFlow::Capture => "Microphone".into(),
        EndpointFlow::Render => "Speakers".into(),
    });
    Ok(DeviceId { endpoint, name })
}

/// Owned [`PROPVARIANT`] whose `PropVariantClear` runs on every exit,
/// including early returns and panics (#36).
struct PropVar(PROPVARIANT);

impl Drop for PropVar {
    fn drop(&mut self) {
        unsafe {
            let _ = PropVariantClear(&mut self.0);
        }
    }
}

fn friendly_name(device: &IMMDevice) -> Result<String> {
    unsafe {
        let store = device
            .OpenPropertyStore(STGM_READ)
            .map_err(|e| Error::win("IMMDevice::OpenPropertyStore", &e))?;
        let value = PropVar(
            store
                .GetValue(&PKEY_DEVICE_FRIENDLY_NAME)
                .map_err(|e| Error::win("IPropertyStore::GetValue", &e))?,
        );
        let string = PropVariantToStringAlloc(&value.0)
            .map_err(|e| Error::win("PropVariantToStringAlloc", &e))?;
        let result = string
            .to_string()
            .map_err(|e| Error::audio(format!("friendly name UTF-16: {e}")));
        CoTaskMemFree(Some(string.0.cast()));
        result
    }
}

impl Drop for EndpointBinding {
    // Registration guard (#36): unregistration is tied to object lifetime so
    // no rebuild/shutdown path can forget it.
    fn drop(&mut self) {
        unsafe {
            let _ = self.volume.UnregisterControlChangeNotify(&self.callback);
        }
    }
}

#[cfg(test)]
mod cycle_tests {
    use super::*;

    fn device(endpoint: &str, name: &str) -> DeviceId {
        DeviceId {
            endpoint: endpoint.into(),
            name: name.into(),
        }
    }

    #[test]
    fn default_selects_first_active_endpoint() {
        let active = [
            device("first-id", "Same name"),
            device("second-id", "Same name"),
        ];
        assert_eq!(
            plan_device_cycle(&DeviceSelection::Default, &active),
            DeviceCyclePlan::Select(DeviceSelection::Endpoint("first-id".into()))
        );
    }

    #[test]
    fn endpoint_cycles_and_wraps_to_default() {
        let active = [device("first-id", "First"), device("second-id", "Second")];
        assert_eq!(
            plan_device_cycle(&DeviceSelection::Endpoint("first-id".into()), &active),
            DeviceCyclePlan::Select(DeviceSelection::Endpoint("second-id".into()))
        );
        assert_eq!(
            plan_device_cycle(&DeviceSelection::Endpoint("second-id".into()), &active),
            DeviceCyclePlan::Select(DeviceSelection::Default)
        );
    }

    #[test]
    fn unavailable_endpoint_recovers_to_default() {
        let active = [device("current-id", "Current")];
        assert_eq!(
            plan_device_cycle(&DeviceSelection::Endpoint("missing-id".into()), &active),
            DeviceCyclePlan::Select(DeviceSelection::Default)
        );
    }

    #[test]
    fn no_active_endpoints_is_a_noop() {
        assert_eq!(
            plan_device_cycle(&DeviceSelection::Default, &[]),
            DeviceCyclePlan::NoActiveEndpoints
        );
    }

    #[test]
    fn unavailable_endpoint_recovers_even_when_inventory_is_empty() {
        assert_eq!(
            plan_device_cycle(&DeviceSelection::Endpoint("missing-id".into()), &[]),
            DeviceCyclePlan::Select(DeviceSelection::Default)
        );
    }

    #[test]
    fn duplicate_friendly_names_still_cycle_by_endpoint_id() {
        let active = [device("a", "USB microphone"), device("b", "USB microphone")];
        assert_eq!(
            plan_device_cycle(&DeviceSelection::Endpoint("a".into()), &active),
            DeviceCyclePlan::Select(DeviceSelection::Endpoint("b".into()))
        );
    }
}
