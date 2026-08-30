//! Undocumented Shell COM backend pinned to Windows 11 24H2/25H2
//! (builds 26100 and 26200–26299). See docs/VIRTUAL_DESKTOP_COMPAT.md.
//!
//! Microsoft reused IID 53F5CA0B across a vtable change; selection is by exact
//! build family before QueryService. Unknown builds are never probed.

use std::ffi::c_void;
use std::ops::Deref;

use crate::desktop::backend::{DesktopError, VirtualDesktopBackend};
use crate::desktop::detect::OsBuild;
use crate::error::{Error, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER};
use windows::Win32::UI::Shell::{
    Common::IObjectArray, IVirtualDesktopManager, VirtualDesktopManager,
};
use windows_core::{IUnknown, IUnknown_Vtbl, Interface, GUID, HRESULT, HSTRING};

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

// IApplicationView is intentionally opaque here. WinShort only obtains a view
// from IApplicationViewCollection and passes that same COM pointer to the
// build-pinned IVirtualDesktopManagerInternal move methods; no view vtable slot
// is called directly.
#[windows_core::interface("372E1D3B-38D3-42E4-A15B-8AB2B178F513")]
pub unsafe trait IApplicationView: IUnknown {}

// This service/IID has remained stable across the Windows generations WinShort
// targets. Only the prefix through GetViewForHwnd is declared because later
// slots are never called. The first three declarations preserve the real vtable
// position of GetViewForHwnd.
#[windows_core::interface("1841C6D7-4F9D-42C0-AF41-8747538F10E5")]
pub unsafe trait IApplicationViewCollection: IUnknown {
    pub unsafe fn get_views(&self, views: *mut *mut c_void) -> HRESULT;
    pub unsafe fn get_views_by_zorder(&self, views: *mut *mut c_void) -> HRESULT;
    pub unsafe fn get_views_by_app_user_model_id(
        &self,
        app_user_model_id: *const u16,
        views: *mut *mut c_void,
    ) -> HRESULT;
    pub unsafe fn get_view_for_hwnd(
        &self,
        window: HWND,
        view: *mut Option<IApplicationView>,
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

/// Move failures are semantic failures, not switch failures. Preserve the RPC
/// class so the controller can rebuild a dead Explorer proxy, but never label
/// an ordinary move rejection (notably E_ACCESSDENIED) as SwitchDesktop.
fn classify_move(e: &crate::error::Error) -> DesktopError {
    match e {
        crate::error::Error::Os { code, .. }
            if matches!(*code, 0x8001_0108 | 0x8007_06BA | 0x8007_06BE) =>
        {
            DesktopError::RpcDisconnected
        }
        other => DesktopError::MoveUnavailable(other.to_string()),
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
    window_manager: Option<IVirtualDesktopManager>,
    view_collection: Option<IApplicationViewCollection>,
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
            let window_manager =
                match CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_INPROC_SERVER) {
                    Ok(manager) => Some(manager),
                    Err(error) => {
                        crate::warn_!("public virtual desktop window manager unavailable: {error}");
                        None
                    }
                };

            // The documented MoveWindowToDesktop API rejects cross-process HWNDs
            // with E_ACCESSDENIED. Resolve the Shell application view service so
            // all WinShort move actions can use GetViewForHwnd + MoveViewToDesktop.
            let view_collection = {
                let mut view_raw = std::ptr::null_mut();
                match provider
                    .query_service(
                        &IApplicationViewCollection::IID,
                        &IApplicationViewCollection::IID,
                        &mut view_raw,
                    )
                    .ok()
                {
                    Ok(()) if !view_raw.is_null() => {
                        Some(IApplicationViewCollection::from_raw(view_raw))
                    }
                    Ok(()) => {
                        crate::warn_!(
                            "IServiceProvider::QueryService(IApplicationViewCollection) returned null"
                        );
                        None
                    }
                    Err(error) => {
                        crate::warn_!(
                            "IServiceProvider::QueryService(IApplicationViewCollection) failed: {error}"
                        );
                        None
                    }
                }
            };

            Ok(Self {
                manager,
                window_manager,
                view_collection,
            })
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

    pub fn current_desktop_id(&self) -> std::result::Result<GUID, DesktopError> {
        self.current_id().classify()
    }

    pub fn desktop_ids(&self) -> std::result::Result<Vec<GUID>, DesktopError> {
        let inner: Result<Vec<GUID>> = (|| unsafe {
            let array = desktop_array(&self.manager)?;
            let count = array
                .GetCount()
                .map_err(|e| Error::win("IObjectArray::GetCount", &e))?;
            let mut ids = Vec::with_capacity(count as usize);
            for index in 0..count {
                let desktop: IVirtualDesktop = array
                    .GetAt(index)
                    .map_err(|e| Error::win("IObjectArray::GetAt", &e))?;
                ids.push(desktop_id(&desktop)?);
            }
            Ok(ids)
        })();
        inner.classify()
    }

    fn desktop_for_id(&self, id: GUID) -> std::result::Result<IVirtualDesktop, DesktopError> {
        let inner: crate::error::Result<Option<IVirtualDesktop>> = (|| unsafe {
            let array = desktop_array(&self.manager)?;
            let count = array
                .GetCount()
                .map_err(|e| Error::win("IObjectArray::GetCount", &e))?;
            for index in 0..count {
                let desktop: IVirtualDesktop = array
                    .GetAt(index)
                    .map_err(|e| Error::win("IObjectArray::GetAt", &e))?;
                if desktop_id(&desktop)? == id {
                    return Ok(Some(desktop));
                }
            }
            Ok(None)
        })();
        inner.classify()?.ok_or_else(|| {
            DesktopError::NavigationUnavailable(
                "desktop identity was not found in Shell ordering".into(),
            )
        })
    }

    pub fn create_desktop(&self) -> std::result::Result<GUID, DesktopError> {
        let inner: crate::error::Result<GUID> = (|| unsafe {
            let mut desktop = None;
            self.manager
                .create_desktop(&mut desktop)
                .ok()
                .map_err(|e| Error::win("IVirtualDesktopManagerInternal::CreateDesktop", &e))?;
            let desktop = desktop.ok_or_else(|| Error::desktop("CreateDesktop returned null"))?;
            desktop_id(&desktop)
        })();
        inner.map_err(|error| {
            DesktopError::CreationUnavailable(format!("CreateDesktop failed: {error}"))
        })
    }

    pub fn switch_to_id(&self, id: GUID) -> std::result::Result<(), DesktopError> {
        if self.current_desktop_id()? == id {
            return Ok(());
        }
        let desktop = self.desktop_for_id(id)?;
        unsafe {
            self.manager
                .switch_desktop(ComIn::new(&desktop))
                .ok()
                .map_err(|e| {
                    classify(&Error::win(
                        "IVirtualDesktopManagerInternal::SwitchDesktop",
                        &e,
                    ))
                })
        }
    }

    pub fn move_window_to_desktop_id(
        &self,
        hwnd: HWND,
        desktop_id: GUID,
    ) -> std::result::Result<(), DesktopError> {
        let view_collection = self.view_collection.as_ref().ok_or_else(|| {
            DesktopError::MoveUnavailable(
                "IApplicationViewCollection is unavailable; cross-process window moves are disabled"
                    .into(),
            )
        })?;
        let desktop = self.desktop_for_id(desktop_id)?;

        let mut view = None;
        unsafe {
            view_collection
                .get_view_for_hwnd(hwnd, &mut view)
                .ok()
                .map_err(|e| {
                    classify_move(&Error::win(
                        "IApplicationViewCollection::GetViewForHwnd",
                        &e,
                    ))
                })?;
        }
        let view = view.ok_or_else(|| {
            DesktopError::MoveUnavailable(
                "IApplicationViewCollection::GetViewForHwnd returned no application view".into(),
            )
        })?;

        let mut can_move = 0i32;
        unsafe {
            self.manager
                .can_view_move_desktops(view.as_raw(), &mut can_move)
                .ok()
                .map_err(|e| {
                    classify_move(&Error::win(
                        "IVirtualDesktopManagerInternal::CanViewMoveDesktops",
                        &e,
                    ))
                })?;
        }
        if can_move == 0 {
            return Err(DesktopError::MoveUnavailable(
                "Shell reports that this application view cannot move between virtual desktops"
                    .into(),
            ));
        }

        unsafe {
            self.manager
                .move_view_to_desktop(view.as_raw(), ComIn::new(&desktop))
                .ok()
                .map_err(|e| {
                    classify_move(&Error::win(
                        "IVirtualDesktopManagerInternal::MoveViewToDesktop",
                        &e,
                    ))
                })
        }
    }

    pub fn remove_desktop_id(
        &self,
        id: GUID,
        fallback_id: GUID,
    ) -> std::result::Result<(), DesktopError> {
        if id == fallback_id {
            return Err(DesktopError::NavigationUnavailable(
                "special workspace fallback cannot be the workspace itself".into(),
            ));
        }
        let desktop = self.desktop_for_id(id)?;
        let fallback = self.desktop_for_id(fallback_id)?;
        unsafe {
            self.manager
                .remove_desktop(ComIn::new(&desktop), ComIn::new(&fallback))
                .ok()
                .map_err(|e| {
                    classify(&Error::win(
                        "IVirtualDesktopManagerInternal::RemoveDesktop",
                        &e,
                    ))
                })
        }
    }

    pub fn window_desktop_id(&self, hwnd: HWND) -> std::result::Result<GUID, DesktopError> {
        let Some(window_manager) = &self.window_manager else {
            return Err(DesktopError::MoveUnavailable(
                "public VirtualDesktopManager is unavailable".into(),
            ));
        };
        unsafe {
            window_manager.GetWindowDesktopId(hwnd).map_err(|e| {
                classify(&Error::win(
                    "IVirtualDesktopManager::GetWindowDesktopId",
                    &e,
                ))
            })
        }
    }
}

impl VirtualDesktopBackend for InternalBackend {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_denied_move_is_not_reported_as_switch_failure() {
        let error = Error::os(
            "IVirtualDesktopManagerInternal::MoveViewToDesktop",
            0x8007_0005,
        );
        let classified = classify_move(&error);
        assert!(matches!(
            classified,
            DesktopError::MoveUnavailable(message)
                if message.contains("E_ACCESSDENIED") && message.contains("MoveViewToDesktop")
        ));
    }

    #[test]
    fn move_rpc_disconnect_keeps_retryable_error_class() {
        let classified = classify_move(&Error::os(
            "IVirtualDesktopManagerInternal::MoveViewToDesktop",
            0x8007_06BA,
        ));
        assert_eq!(classified, DesktopError::RpcDisconnected);
    }
}
