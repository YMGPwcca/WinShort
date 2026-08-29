//! Native Windows audio policy control used to set system defaults.
//!
//! `IPolicyConfig` is a de-facto Windows COM interface rather than a public
//! SDK contract. Its ABI is isolated here so the rest of the audio subsystem
//! only deals in endpoint IDs and typed errors.

use std::ffi::c_void;
use std::ptr::NonNull;

use windows::core::{IUnknown, Interface, GUID, HSTRING, PCWSTR};
use windows::Win32::Media::Audio::ERole;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

use crate::error::{Error, Result};

// {870af99c-171d-4f9e-af0d-e63df40c2bc9}
const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
// {f8679f50-850a-41cf-9c72-430f290290c8}
const IID_POLICY_CONFIG: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);

/// The undocumented IPolicyConfig vtable prefix used by Windows 7+.
///
/// `SetDefaultEndpoint` is slot 13: three IUnknown methods followed by ten
/// policy methods. The preceding signatures are intentionally opaque because
/// this wrapper never calls them; preserving their pointer-sized slots keeps
/// the ABI exact without inventing Rust types for unused methods.
#[repr(C)]
struct PolicyConfigVtable {
    base__: windows_core::IUnknown_Vtbl,
    _unused_policy_methods: [*const c_void; 10],
    set_default_endpoint: unsafe extern "system" fn(
        this: *mut c_void,
        device_id: PCWSTR,
        role: ERole,
    ) -> windows_core::HRESULT,
    _set_endpoint_visibility: *const c_void,
}

pub(crate) struct PolicyConfig {
    raw: NonNull<c_void>,
}

impl PolicyConfig {
    pub(crate) fn create() -> Result<Self> {
        let unknown: IUnknown = unsafe {
            CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
                .map_err(|error| Error::win("CoCreateInstance(CPolicyConfigClient)", &error))?
        };
        let mut raw = std::ptr::null_mut();
        unsafe {
            unknown
                .query(&IID_POLICY_CONFIG, &mut raw)
                .ok()
                .map_err(|error| Error::win("IUnknown::QueryInterface(IPolicyConfig)", &error))?;
        }
        let raw = NonNull::new(raw)
            .ok_or_else(|| Error::audio("IPolicyConfig returned a null interface"))?;
        Ok(Self { raw })
    }

    pub(crate) fn set_default_endpoint(&self, endpoint_id: &str, role: ERole) -> Result<()> {
        let endpoint_id = HSTRING::from(endpoint_id);
        let vtable = unsafe { &*(*(self.raw.as_ptr() as *const *const PolicyConfigVtable)) };
        let result = unsafe {
            (vtable.set_default_endpoint)(self.raw.as_ptr(), PCWSTR(endpoint_id.as_ptr()), role)
        };
        result
            .ok()
            .map_err(|error| Error::win("IPolicyConfig::SetDefaultEndpoint", &error))
    }
}

impl Drop for PolicyConfig {
    fn drop(&mut self) {
        let vtable = unsafe { &*(*(self.raw.as_ptr() as *const *const PolicyConfigVtable)) };
        unsafe {
            (vtable.base__.Release)(self.raw.as_ptr());
        }
    }
}
