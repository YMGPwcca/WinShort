//! Nullable COM interface ABI bridge. This module preserves the release ABI regression contract.

use super::properties::{element_not_enabled_error, unsupported_error};
use super::providers::SettingsAutomationNodeProvider_Impl;
use super::providers::SettingsAutomationRootProvider_Impl;
use std::sync::OnceLock;
use windows::core::{IUnknown_Vtbl, Interface};
use windows::Win32::UI::Accessibility::{
    IRawElementProviderFragment, IRawElementProviderFragmentRoot,
    IRawElementProviderFragmentRoot_Vtbl, IRawElementProviderFragment_Vtbl,
    IRawElementProviderSimple, IRawElementProviderSimple_Vtbl, NavigateDirection, UIA_PATTERN_ID,
};

pub(super) fn invalid_argument<T>() -> windows::core::Result<T> {
    Err(windows::core::Error::from_hresult(windows::core::HRESULT(
        0x80070057u32 as i32,
    )))
}

pub(super) fn element_not_enabled<T>() -> windows::core::Result<T> {
    Err(element_not_enabled_error())
}

pub(super) fn unsupported<T>() -> windows::core::Result<T> {
    Err(unsupported_error())
}

fn no_nullable_result_error() -> windows::core::Error {
    // Internal sentinel for the generated high-level trait; raw nullable
    // thunks consume None directly and never expose this HRESULT.
    windows::core::Error::from_hresult(windows::core::HRESULT(0x80004002u32 as i32))
}

pub(super) fn require_nullable_result<T>(
    result: windows::core::Result<Option<T>>,
) -> windows::core::Result<T> {
    result.and_then(|value| value.ok_or_else(no_nullable_result_error))
}

unsafe fn write_nullable_interface<T, F>(
    pretval: *mut *mut core::ffi::c_void,
    produce: F,
) -> windows::core::HRESULT
where
    T: Interface,
    F: FnOnce() -> windows::core::Result<Option<T>>,
{
    if pretval.is_null() {
        return windows::core::HRESULT(0x80004003u32 as i32);
    }
    unsafe {
        pretval.write(std::ptr::null_mut());
    }
    match produce() {
        Ok(Some(value)) => {
            unsafe {
                pretval.write(value.into_raw());
            }
            windows::core::HRESULT(0)
        }
        Ok(None) => windows::core::HRESULT(0),
        Err(error) => error.into(),
    }
}

static ROOT_SIMPLE_VTABLE: OnceLock<IRawElementProviderSimple_Vtbl> = OnceLock::new();

static ROOT_FRAGMENT_VTABLE: OnceLock<IRawElementProviderFragment_Vtbl> = OnceLock::new();

static ROOT_FRAGMENT_ROOT_VTABLE: OnceLock<IRawElementProviderFragmentRoot_Vtbl> = OnceLock::new();

static NODE_SIMPLE_VTABLE: OnceLock<IRawElementProviderSimple_Vtbl> = OnceLock::new();

static NODE_FRAGMENT_VTABLE: OnceLock<IRawElementProviderFragment_Vtbl> = OnceLock::new();

fn copy_iunknown_vtable(source: &IUnknown_Vtbl) -> IUnknown_Vtbl {
    IUnknown_Vtbl {
        QueryInterface: source.QueryInterface,
        AddRef: source.AddRef,
        Release: source.Release,
    }
}

unsafe fn replace_vtable<I: Interface>(interface: &I, replacement: &'static I::Vtable) {
    unsafe {
        (interface.as_raw() as *mut *const I::Vtable).write(replacement);
    }
}

unsafe extern "system" fn root_simple_get_pattern_provider(
    this: *mut core::ffi::c_void,
    patternid: UIA_PATTERN_ID,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-1) as *const SettingsAutomationRootProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.pattern_provider_result(patternid)) }
}

unsafe extern "system" fn root_fragment_navigate(
    this: *mut core::ffi::c_void,
    direction: NavigateDirection,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-2) as *const SettingsAutomationRootProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.navigate_result(direction)) }
}

unsafe extern "system" fn root_element_provider_from_point(
    this: *mut core::ffi::c_void,
    x: f64,
    y: f64,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-3) as *const SettingsAutomationRootProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.element_provider_from_point_result(x, y)) }
}

unsafe extern "system" fn root_get_focus(
    this: *mut core::ffi::c_void,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-3) as *const SettingsAutomationRootProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.focus_result()) }
}

unsafe extern "system" fn node_simple_get_pattern_provider(
    this: *mut core::ffi::c_void,
    patternid: UIA_PATTERN_ID,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-1) as *const SettingsAutomationNodeProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.pattern_provider_result(patternid)) }
}

unsafe extern "system" fn node_simple_host_raw_element_provider(
    this: *mut core::ffi::c_void,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-1) as *const SettingsAutomationNodeProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.host_raw_element_provider_result()) }
}

unsafe extern "system" fn node_fragment_navigate(
    this: *mut core::ffi::c_void,
    direction: NavigateDirection,
    pretval: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    let this = unsafe {
        &*((this as *const *const ()).offset(-2) as *const SettingsAutomationNodeProvider_Impl)
    };
    unsafe { write_nullable_interface(pretval, || this.navigate_result(direction)) }
}

fn root_simple_vtable(
    original: &IRawElementProviderSimple_Vtbl,
) -> &'static IRawElementProviderSimple_Vtbl {
    ROOT_SIMPLE_VTABLE.get_or_init(|| IRawElementProviderSimple_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        ProviderOptions: original.ProviderOptions,
        GetPatternProvider: root_simple_get_pattern_provider,
        GetPropertyValue: original.GetPropertyValue,
        HostRawElementProvider: original.HostRawElementProvider,
    })
}

fn root_fragment_vtable(
    original: &IRawElementProviderFragment_Vtbl,
) -> &'static IRawElementProviderFragment_Vtbl {
    ROOT_FRAGMENT_VTABLE.get_or_init(|| IRawElementProviderFragment_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        Navigate: root_fragment_navigate,
        GetRuntimeId: original.GetRuntimeId,
        BoundingRectangle: original.BoundingRectangle,
        GetEmbeddedFragmentRoots: original.GetEmbeddedFragmentRoots,
        SetFocus: original.SetFocus,
        FragmentRoot: original.FragmentRoot,
    })
}

fn root_fragment_root_vtable(
    original: &IRawElementProviderFragmentRoot_Vtbl,
) -> &'static IRawElementProviderFragmentRoot_Vtbl {
    ROOT_FRAGMENT_ROOT_VTABLE.get_or_init(|| IRawElementProviderFragmentRoot_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        ElementProviderFromPoint: root_element_provider_from_point,
        GetFocus: root_get_focus,
    })
}

fn node_simple_vtable(
    original: &IRawElementProviderSimple_Vtbl,
) -> &'static IRawElementProviderSimple_Vtbl {
    NODE_SIMPLE_VTABLE.get_or_init(|| IRawElementProviderSimple_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        ProviderOptions: original.ProviderOptions,
        GetPatternProvider: node_simple_get_pattern_provider,
        GetPropertyValue: original.GetPropertyValue,
        HostRawElementProvider: node_simple_host_raw_element_provider,
    })
}

fn node_fragment_vtable(
    original: &IRawElementProviderFragment_Vtbl,
) -> &'static IRawElementProviderFragment_Vtbl {
    NODE_FRAGMENT_VTABLE.get_or_init(|| IRawElementProviderFragment_Vtbl {
        base__: copy_iunknown_vtable(&original.base__),
        Navigate: node_fragment_navigate,
        GetRuntimeId: original.GetRuntimeId,
        BoundingRectangle: original.BoundingRectangle,
        GetEmbeddedFragmentRoots: original.GetEmbeddedFragmentRoots,
        SetFocus: original.SetFocus,
        FragmentRoot: original.FragmentRoot,
    })
}

pub(super) fn install_root_vtables(
    root: &IRawElementProviderFragmentRoot,
) -> windows::core::Result<()> {
    let simple: IRawElementProviderSimple = root.cast()?;
    let fragment: IRawElementProviderFragment = root.cast()?;
    let simple_vtable = root_simple_vtable(simple.vtable());
    let fragment_vtable = root_fragment_vtable(fragment.vtable());
    let root_vtable = root_fragment_root_vtable(root.vtable());
    unsafe {
        replace_vtable(&simple, simple_vtable);
        replace_vtable(&fragment, fragment_vtable);
        replace_vtable(root, root_vtable);
    }
    Ok(())
}

pub(super) fn install_node_vtables(
    simple: &IRawElementProviderSimple,
) -> windows::core::Result<()> {
    let fragment: IRawElementProviderFragment = simple.cast()?;
    let simple_vtable = node_simple_vtable(simple.vtable());
    let fragment_vtable = node_fragment_vtable(fragment.vtable());
    unsafe {
        replace_vtable(simple, simple_vtable);
        replace_vtable(&fragment, fragment_vtable);
    }
    Ok(())
}
