//! Undocumented Shell COM backend pinned to Windows 11 24H2/25H2
//! (builds 26100 and 26200–26299). See docs/VIRTUAL_DESKTOP_COMPAT.md.
//!
//! Microsoft reused IID 53F5CA0B across a vtable change; selection is by exact
//! build family before QueryService. Unknown builds are never probed.

use std::ffi::c_void;
use std::ops::Deref;

use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER};
use windows::Win32::UI::Shell::Common::IObjectArray;
use windows_core::{IUnknown, IUnknown_Vtbl, Interface, GUID, HRESULT, HSTRING};

use crate::desktop::backend::{
    BackendAvailability, BackendKind, DesktopError, VirtualDesktopBackend,
};
use crate::desktop::detect::OsBuild;
use crate::error::{Error, Result};

pub const CLSID_IMMERSIVE_SHELL: GUID = GUID::from_u128(0xc2f03a33_21f5_47fa_b4bb_156362a2f239);
pub const SID_VIRTUAL_DESKTOP_MANAGER_INTERNAL: GUID =
    GUID::from_u128(0xc5e0cdca_7b6e_41b2_9fc4_d93975cc467b);

#[repr(transparent)]
pub struct ComIn<'a, T: Interface> {
    raw: *mut c_void,
    marker: std::marker::PhantomData<&'a T>,
}

impl<'a, T: Interface> ComIn<'a, T> {
    pub fn new(value: &'a T) -> Self {
        Self {
            raw: value.as_raw(),
            marker: std::marker::PhantomData,
        }
    }
}

impl<T: Interface> Deref for ComIn<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: input COM pointer remains owned by caller for this call.
        unsafe { std::mem::transmute(&self.raw) }
    }
}

#[windows_core::interface("6D5140C1-7436-11CE-8034-00AA006009FA")]
pub unsafe trait ShellServiceProvider: IUnknown {
    pub unsafe fn query_service(
        &self,
        service: *const GUID,
        iid: *const GUID,
        object: *mut *mut c_void,
    ) -> HRESULT;
}

#[windows_core::interface("3F07F4BE-B107-441A-AF0F-39D82529072C")]
pub unsafe trait IVirtualDesktop: IUnknown {
    pub unsafe fn is_view_visible(&self, view: *mut c_void, visible: *mut u32) -> HRESULT;
    pub unsafe fn get_id(&self, id: *mut GUID) -> HRESULT;
    pub unsafe fn get_name(&self, name: *mut HSTRING) -> HRESULT;
    pub unsafe fn get_wallpaper(&self, wallpaper: *mut HSTRING) -> HRESULT;
    pub unsafe fn is_remote(&self, remote: *mut u32) -> HRESULT;
}

/// Build 26100+ layout. Slot 10 (`switch_desktop_and_move_foreground_view`)
/// is intentionally present; omitting it shifts every later method.
#[windows_core::interface("53F5CA0B-158F-4124-900C-057158060B27")]
pub unsafe trait IVirtualDesktopManagerInternal: IUnknown {
    pub unsafe fn get_count(&self, count: *mut u32) -> HRESULT;
    pub unsafe fn move_view_to_desktop(
        &self,
        view: *mut c_void,
        desktop: ComIn<IVirtualDesktop>,
    ) -> HRESULT;
    pub unsafe fn can_view_move_desktops(&self, view: *mut c_void, can_move: *mut i32) -> HRESULT;
    pub unsafe fn get_current_desktop(&self, desktop: *mut Option<IVirtualDesktop>) -> HRESULT;
    pub unsafe fn get_desktops(&self, desktops: *mut Option<IObjectArray>) -> HRESULT;
    pub unsafe fn get_adjacent_desktop(
        &self,
        desktop: ComIn<IVirtualDesktop>,
        direction: u32,
        adjacent: *mut Option<IVirtualDesktop>,
    ) -> HRESULT;
    pub unsafe fn switch_desktop(&self, desktop: ComIn<IVirtualDesktop>) -> HRESULT;
    pub unsafe fn switch_desktop_and_move_foreground_view(
        &self,
        desktop: ComIn<IVirtualDesktop>,
    ) -> HRESULT;
    pub unsafe fn create_desktop(&self, desktop: *mut Option<IVirtualDesktop>) -> HRESULT;
    pub unsafe fn move_desktop(&self, desktop: ComIn<IVirtualDesktop>, index: u32) -> HRESULT;
    pub unsafe fn remove_desktop(
        &self,
        desktop: ComIn<IVirtualDesktop>,
        fallback: ComIn<IVirtualDesktop>,
    ) -> HRESULT;
    pub unsafe fn find_desktop(
        &self,
        id: *const GUID,
        desktop: *mut Option<IVirtualDesktop>,
    ) -> HRESULT;
}

/// Classify a Win32/HRESULT failure into the typed desktop error (#20).
fn classify(e: &crate::error::Error) -> DesktopError {
    match e {
        crate::error::Error::Os { code, .. } => match *code {
            0x8001_0108 | 0x8007_06BA | 0x8007_06BE => DesktopError::RpcDisconnected,
            other => DesktopError::SwitchFailed(other as i32),
        },
        other => DesktopError::BackendUnavailable(other.to_string()),
    }
}

trait BackendError<T> {
    fn classify(self) -> std::result::Result<T, DesktopError>;
}

impl<T> BackendError<T> for crate::error::Result<T> {
    fn classify(self) -> std::result::Result<T, DesktopError> {
        self.map_err(|e| classify(&e))
    }
}

pub struct InternalBackend {
    manager: IVirtualDesktopManagerInternal,
}

impl InternalBackend {
    pub fn create(build: OsBuild) -> Result<Self> {
        if !build.native_shell_supported() {
            return Err(Error::desktop(format!("unsupported build {}", build.build)));
        }
        unsafe {
            let provider: ShellServiceProvider =
                CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_LOCAL_SERVER)
                    .map_err(|e| Error::win("CoCreateInstance(ImmersiveShell)", &e))?;
            let mut raw = std::ptr::null_mut();
            provider
                .query_service(
                    &SID_VIRTUAL_DESKTOP_MANAGER_INTERNAL,
                    &IVirtualDesktopManagerInternal::IID,
                    &mut raw,
                )
                .ok()
                .map_err(|e| Error::win("IServiceProvider::QueryService(VDMI)", &e))?;
            if raw.is_null() {
                return Err(Error::desktop("QueryService returned a null manager"));
            }
            let manager = IVirtualDesktopManagerInternal::from_raw(raw);
            // Validate only known, non-mutating slots before exposing backend.
            let count = desktop_array(&manager)?
                .GetCount()
                .map_err(|e| Error::win("IObjectArray::GetCount(compat validation)", &e))?;
            if count == 0 || count > 256 {
                return Err(Error::desktop(format!(
                    "compat validation returned implausible desktop count {count}"
                )));
            }
            Ok(Self { manager })
        }
    }

    fn current_id(&self) -> Result<GUID> {
        unsafe {
            let mut desktop = None;
            self.manager
                .get_current_desktop(&mut desktop)
                .ok()
                .map_err(|e| Error::win("GetCurrentDesktop", &e))?;
            desktop_id(&desktop.ok_or_else(|| Error::desktop("current desktop was null"))?)
        }
    }
}

impl VirtualDesktopBackend for InternalBackend {
    fn availability(&self) -> BackendAvailability {
        BackendAvailability::Available
    }

    fn desktop_count(&self) -> std::result::Result<usize, DesktopError> {
        let inner: crate::error::Result<usize> = (|| {
            let array = unsafe { desktop_array(&self.manager)? };
            unsafe { array.GetCount() }
                .map(|count| count as usize)
                .map_err(|e| Error::win("IObjectArray::GetCount", &e))
        })();
        inner.classify()
    }

    fn current_desktop(&self) -> std::result::Result<usize, DesktopError> {
        let inner: crate::error::Result<usize> = (|| -> crate::error::Result<usize> {
            let current = self.current_id()?;
            unsafe {
                let array = desktop_array(&self.manager)?;
                let count = array
                    .GetCount()
                    .map_err(|e| Error::win("IObjectArray::GetCount", &e))?;
                for index in 0..count {
                    let desktop: IVirtualDesktop = array
                        .GetAt(index)
                        .map_err(|e| Error::win("IObjectArray::GetAt", &e))?;
                    if desktop_id(&desktop)? == current {
                        return Ok(index as usize);
                    }
                }
            }
            Err(Error::desktop(
                "current desktop not found in Shell ordering",
            ))
        })();
        inner.classify()
    }

    fn switch_to(&self, index: usize) -> std::result::Result<(), DesktopError> {
        let inner: crate::error::Result<()> = (|| -> crate::error::Result<()> {
            unsafe {
                let array = desktop_array(&self.manager)?;
                let count = array
                    .GetCount()
                    .map_err(|e| Error::win("IObjectArray::GetCount", &e))?
                    as usize;
                if index >= count {
                    // Semantic refusal — must NEVER trigger input injection (#20).
                    return Err(crate::error::Error::desktop(format!(
                        "desktop {} does not exist (count {count})",
                        index + 1
                    )));
                }
                // current_desktop is already typed; surface its class directly.
                match self.current_desktop() {
                    Ok(current) if current == index => return Ok(()),
                    Ok(_) => {}
                    Err(e) => {
                        return Err(crate::error::Error::desktop(format!(
                            "current desktop unresolved: {e}"
                        )))
                    }
                }
                let desktop: IVirtualDesktop = array
                    .GetAt(index as u32)
                    .map_err(|e| Error::win("IObjectArray::GetAt(target)", &e))?;
                self.manager
                    .switch_desktop(ComIn::new(&desktop))
                    .ok()
                    .map_err(|e| Error::win("IVirtualDesktopManagerInternal::SwitchDesktop", &e))
            }
        })();
        match inner {
            Ok(()) => Ok(()),
            Err(e) => match &e {
                crate::error::Error::Desktop(message) if message.contains("does not exist") => {
                    Err(DesktopError::TargetOutOfRange {
                        requested: index,
                        count: self.desktop_count().unwrap_or(0),
                    })
                }
                other => Err(classify(other)),
            },
        }
    }

    fn kind(&self) -> BackendKind {
        BackendKind::NativeShell
    }
}

unsafe fn desktop_array(manager: &IVirtualDesktopManagerInternal) -> Result<IObjectArray> {
    // SAFETY: manager proxy is valid on this STA thread (worker-owned).
    let mut desktops = None;
    unsafe {
        manager
            .get_desktops(&mut desktops)
            .ok()
            .map_err(|e| Error::win("IVirtualDesktopManagerInternal::GetDesktops", &e))?;
    }
    desktops.ok_or_else(|| Error::desktop("GetDesktops returned null"))
}

unsafe fn desktop_id(desktop: &IVirtualDesktop) -> Result<GUID> {
    // SAFETY: desktop proxy obtained from the Shell array on this thread.
    let mut id = GUID::zeroed();
    unsafe {
        desktop
            .get_id(&mut id)
            .ok()
            .map_err(|e| Error::win("IVirtualDesktop::GetId", &e))?;
    }
    Ok(id)
}
