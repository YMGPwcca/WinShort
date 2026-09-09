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
use crate::audio::state::{AudioState, DeviceCycleFlow, DeviceCycleResult, DeviceId, OutputState};
use crate::config::model::{DeviceSelection, EndpointRole};
use crate::error::{Error, Result};

/// The three Windows default roles are always handled together for a cycle.
pub const ALL_ENDPOINT_ROLES: [EndpointRole; 3] = [
    EndpointRole::Console,
    EndpointRole::Multimedia,
    EndpointRole::Communications,
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DefaultDevices {
    pub console: Option<DeviceId>,
    pub multimedia: Option<DeviceId>,
    pub communications: Option<DeviceId>,
}

impl DefaultDevices {
    pub fn for_role(&self, role: EndpointRole) -> Option<&DeviceId> {
        match role {
            EndpointRole::Console => self.console.as_ref(),
            EndpointRole::Multimedia => self.multimedia.as_ref(),
            EndpointRole::Communications => self.communications.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceLists {
    pub inputs: Vec<DeviceId>,
    pub outputs: Vec<DeviceId>,
    /// Current Windows defaults, retained as metadata rather than pseudo
    /// entries in the selectable endpoint lists.
    pub input_defaults: DefaultDevices,
    pub output_defaults: DefaultDevices,
    /// Per-flow degradation notes from the last enumeration (#36): a failed
    /// flow yields an empty list plus a warning instead of losing both flows.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCyclePlan {
    Select(DeviceId),
    NoActiveEndpoints,
}

pub(crate) fn default_selection_result(
    flow: DeviceCycleFlow,
    previous: Option<DeviceId>,
    target: DeviceId,
) -> DeviceCycleResult {
    if previous
        .as_ref()
        .is_some_and(|current| current.endpoint == target.endpoint)
    {
        DeviceCycleResult::AlreadySelected {
            flow,
            device: target,
        }
    } else {
        DeviceCycleResult::Changed {
            flow,
            previous,
            device: target,
        }
    }
}

/// Plan the next real Windows endpoint from the current system default.
///
/// The active inventory order is the cycle order. Endpoint IDs are opaque and
/// remain the only identity used for matching; there is no synthetic default
/// entry or internal target in this ring.
#[cfg(test)]
pub fn plan_device_cycle(current: Option<&DeviceId>, active: &[DeviceId]) -> DeviceCyclePlan {
    plan_device_cycle_with_allowlist(current, active, None)
}

/// Plan the next endpoint after applying an optional endpoint-ID allowlist.
///
/// `None` preserves the historical all-active behavior. `Some(&[])` is an
/// explicit deny-all policy and yields `NoActiveEndpoints`.
pub fn plan_device_cycle_with_allowlist(
    current: Option<&DeviceId>,
    active: &[DeviceId],
    allowlist: Option<&[String]>,
) -> DeviceCyclePlan {
    let allowed =
        |device: &DeviceId| allowlist.is_none_or(|ids| ids.iter().any(|id| id == &device.endpoint));
    let allowed_count = active.iter().filter(|device| allowed(device)).count();
    if allowed_count == 0 {
        return DeviceCyclePlan::NoActiveEndpoints;
    }
    let current_index = current.and_then(|current| {
        active
            .iter()
            .filter(|device| allowed(device))
            .position(|device| device.endpoint == current.endpoint)
    });
    let next = current_index
        .map(|index| (index + 1) % allowed_count)
        .unwrap_or(0);
    DeviceCyclePlan::Select(
        active
            .iter()
            .filter(|device| allowed(device))
            .nth(next)
            .expect("allowlist count matches filtered inventory")
            .clone(),
    )
}

#[cfg(test)]
pub fn device_cycle_result(
    flow: crate::audio::DeviceCycleFlow,
    previous: Option<DeviceId>,
    active: &[DeviceId],
) -> crate::audio::DeviceCycleResult {
    device_cycle_result_with_allowlist(flow, previous, active, None)
}

pub fn device_cycle_result_with_allowlist(
    flow: crate::audio::DeviceCycleFlow,
    previous: Option<DeviceId>,
    active: &[DeviceId],
    allowlist: Option<&[String]>,
) -> crate::audio::DeviceCycleResult {
    match plan_device_cycle_with_allowlist(previous.as_ref(), active, allowlist) {
        DeviceCyclePlan::Select(device) => crate::audio::DeviceCycleResult::Changed {
            flow,
            previous,
            device,
        },
        DeviceCyclePlan::NoActiveEndpoints => {
            crate::audio::DeviceCycleResult::NoDevices { flow, previous }
        }
    }
}

/// Enumerate active endpoints and the current Windows default per role.
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
    let input_defaults = enumerate_defaults(enumerator, EndpointFlow::Capture, &mut warnings);
    let output_defaults = enumerate_defaults(enumerator, EndpointFlow::Render, &mut warnings);
    DeviceLists {
        inputs,
        outputs,
        input_defaults,
        output_defaults,
        warnings,
    }
}

fn enumerate_defaults(
    enumerator: &IMMDeviceEnumerator,
    flow: EndpointFlow,
    warnings: &mut Vec<String>,
) -> DefaultDevices {
    let mut defaults = DefaultDevices::default();
    for role in ALL_ENDPOINT_ROLES {
        match default_device(enumerator, flow, role) {
            Ok(device) => match role {
                EndpointRole::Console => defaults.console = Some(device),
                EndpointRole::Multimedia => defaults.multimedia = Some(device),
                EndpointRole::Communications => defaults.communications = Some(device),
            },
            Err(error) => warnings.push(format!(
                "{flow:?} {role:?} default enumeration failed: {error}"
            )),
        }
    }
    defaults
}

/// Return the current console default, which is the canonical cycle cursor.
pub(crate) fn current_default_device(
    enumerator: &IMMDeviceEnumerator,
    flow: EndpointFlow,
) -> Result<DeviceId> {
    default_device(enumerator, flow, EndpointRole::Console)
}

fn default_device(
    enumerator: &IMMDeviceEnumerator,
    flow: EndpointFlow,
    role: EndpointRole,
) -> Result<DeviceId> {
    let device = unsafe {
        enumerator
            .GetDefaultAudioEndpoint(data_flow(flow), endpoint_role(role))
            .map_err(|e| {
                Error::os_ctx(
                    "GetDefaultAudioEndpoint",
                    e.code().0 as u32,
                    format!("{:?} / {:?}", flow, role),
                )
            })?
    };
    identity(&device, flow)
}

/// Set one real endpoint as the Windows default for every applicable role.
///
/// If a later role fails, previously changed roles are restored when their
/// prior defaults were readable. The caller must treat any error as a failed
/// switch and must not publish success.
pub(crate) fn set_system_default(
    enumerator: &IMMDeviceEnumerator,
    flow: EndpointFlow,
    target: &DeviceId,
) -> Result<()> {
    let policy = super::policy::PolicyConfig::create()?;
    let previous: [Option<String>; 3] =
        ALL_ENDPOINT_ROLES.map(|role| match default_device(enumerator, flow, role) {
            Ok(device) => Some(device.endpoint),
            Err(error) => {
                crate::warn_!(
                    "audio {:?} {:?} prior default unavailable: {error}",
                    flow,
                    role
                );
                None
            }
        });

    for (index, role) in ALL_ENDPOINT_ROLES.iter().copied().enumerate() {
        if let Err(error) = policy.set_default_endpoint(&target.endpoint, endpoint_role(role)) {
            crate::warn_!("audio {:?} {:?} default switch failed: {error}", flow, role);
            for rollback_index in (0..index).rev() {
                if let Some(endpoint) = previous[rollback_index].as_deref() {
                    if let Err(rollback_error) = policy.set_default_endpoint(
                        endpoint,
                        endpoint_role(ALL_ENDPOINT_ROLES[rollback_index]),
                    ) {
                        crate::warn_!(
                            "audio {:?} {:?} default rollback failed: {rollback_error}",
                            flow,
                            ALL_ENDPOINT_ROLES[rollback_index]
                        );
                    }
                }
            }
            return Err(error);
        }
    }
    Ok(())
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
                match enumerator.GetDevice(PCWSTR(wide.as_ptr())) {
                    Ok(device) => Ok(device),
                    Err(error) => {
                        crate::warn_!(
                            "audio {:?} configured endpoint unavailable; using system default: {error}",
                            flow
                        );
                        enumerator
                            .GetDefaultAudioEndpoint(data_flow(flow), endpoint_role(role))
                            .map_err(|fallback| {
                                Error::os_ctx(
                                    "GetDefaultAudioEndpoint",
                                    fallback.code().0 as u32,
                                    format!("{:?} / {:?}", flow, role),
                                )
                            })
                    }
                }
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
    fn missing_system_default_selects_first_active_endpoint() {
        let active = [
            device("first-id", "Same name"),
            device("second-id", "Same name"),
        ];
        assert_eq!(
            plan_device_cycle(None, &active),
            DeviceCyclePlan::Select(active[0].clone())
        );
    }

    #[test]
    fn endpoint_cycles_and_wraps_to_first_real_endpoint() {
        let active = [device("first-id", "First"), device("second-id", "Second")];
        assert_eq!(
            plan_device_cycle(Some(&active[0]), &active),
            DeviceCyclePlan::Select(active[1].clone())
        );
        assert_eq!(
            plan_device_cycle(Some(&active[1]), &active),
            DeviceCyclePlan::Select(active[0].clone())
        );
    }
    #[test]
    fn allowlist_filters_cycle_order_and_wraps() {
        let active = [
            device("first-id", "First"),
            device("second-id", "Second"),
            device("third-id", "Third"),
        ];
        let allowlist = vec!["third-id".into(), "first-id".into()];
        assert_eq!(
            plan_device_cycle_with_allowlist(None, &active, Some(&allowlist)),
            DeviceCyclePlan::Select(active[0].clone())
        );
        assert_eq!(
            plan_device_cycle_with_allowlist(Some(&active[0]), &active, Some(&allowlist)),
            DeviceCyclePlan::Select(active[2].clone())
        );
        assert_eq!(
            plan_device_cycle_with_allowlist(Some(&active[1]), &active, Some(&allowlist)),
            DeviceCyclePlan::Select(active[0].clone())
        );
    }

    #[test]
    fn explicit_empty_allowlist_has_no_cycle_targets() {
        let active = [device("first-id", "First")];
        let allowlist: Vec<String> = Vec::new();
        assert_eq!(
            plan_device_cycle_with_allowlist(None, &active, Some(&allowlist)),
            DeviceCyclePlan::NoActiveEndpoints
        );
    }
    #[test]
    fn offline_allowlisted_endpoint_returns_after_inventory_reconnect() {
        let gone = device("gone-id", "Gone");
        let stable = device("stable-id", "Stable");
        let allowlist = vec!["gone-id".into(), "stable-id".into()];
        assert_eq!(
            plan_device_cycle_with_allowlist(
                Some(&gone),
                std::slice::from_ref(&stable),
                Some(&allowlist)
            ),
            DeviceCyclePlan::Select(stable.clone())
        );
        assert_eq!(
            plan_device_cycle_with_allowlist(
                Some(&stable),
                &[gone.clone(), stable.clone()],
                Some(&allowlist)
            ),
            DeviceCyclePlan::Select(gone)
        );
    }

    #[test]
    fn cycle_result_reports_real_previous_and_target_endpoints() {
        let active = [device("first-id", "First"), device("second-id", "Second")];
        let result = device_cycle_result(
            crate::audio::DeviceCycleFlow::Output,
            Some(active[0].clone()),
            &active,
        );
        match result {
            crate::audio::DeviceCycleResult::Changed {
                previous: Some(previous),
                device,
                ..
            } => {
                assert_eq!(previous, active[0]);
                assert_eq!(device, active[1]);
            }
            other => panic!("unexpected cycle result: {other:?}"),
        }
    }

    #[test]
    fn selecting_current_input_or_output_is_an_explicit_noop() {
        for flow in [DeviceCycleFlow::Input, DeviceCycleFlow::Output] {
            let current = device("current-id", "Current");
            assert_eq!(
                default_selection_result(flow, Some(current.clone()), current.clone()),
                DeviceCycleResult::AlreadySelected {
                    flow,
                    device: current,
                }
            );
        }
    }

    #[test]
    fn selecting_a_different_endpoint_reports_a_change() {
        let previous = device("previous-id", "Previous");
        let target = device("target-id", "Target");
        assert_eq!(
            default_selection_result(
                DeviceCycleFlow::Output,
                Some(previous.clone()),
                target.clone()
            ),
            DeviceCycleResult::Changed {
                flow: DeviceCycleFlow::Output,
                previous: Some(previous),
                device: target,
            }
        );
    }

    #[test]
    fn unavailable_system_default_recovers_to_first_active_endpoint() {
        let current = device("missing-id", "Missing");
        let active = [device("current-id", "Current")];
        assert_eq!(
            plan_device_cycle(Some(&current), &active),
            DeviceCyclePlan::Select(active[0].clone())
        );
    }

    #[test]
    fn no_active_endpoints_is_a_noop() {
        assert_eq!(
            plan_device_cycle(None, &[]),
            DeviceCyclePlan::NoActiveEndpoints
        );
        let current = device("missing-id", "Missing");
        assert_eq!(
            plan_device_cycle(Some(&current), &[]),
            DeviceCyclePlan::NoActiveEndpoints
        );
    }

    #[test]
    fn duplicate_friendly_names_still_cycle_by_endpoint_id() {
        let active = [device("a", "USB microphone"), device("b", "USB microphone")];
        assert_eq!(
            plan_device_cycle(Some(&active[0]), &active),
            DeviceCyclePlan::Select(active[1].clone())
        );
    }

    #[test]
    fn default_devices_are_indexed_by_role_without_pseudo_entries() {
        let console = device("console", "Console");
        let multimedia = device("multimedia", "Multimedia");
        let communications = device("communications", "Communications");
        let defaults = DefaultDevices {
            console: Some(console.clone()),
            multimedia: Some(multimedia.clone()),
            communications: Some(communications.clone()),
        };
        assert_eq!(defaults.for_role(EndpointRole::Console), Some(&console));
        assert_eq!(
            defaults.for_role(EndpointRole::Multimedia),
            Some(&multimedia)
        );
        assert_eq!(
            defaults.for_role(EndpointRole::Communications),
            Some(&communications)
        );
    }

    #[test]
    fn system_cycle_applies_console_multimedia_and_communications_roles() {
        assert_eq!(
            ALL_ENDPOINT_ROLES,
            [
                EndpointRole::Console,
                EndpointRole::Multimedia,
                EndpointRole::Communications,
            ]
        );
    }
}
