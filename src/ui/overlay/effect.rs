//! Windows Graphics Effects ABI adapter; its unsafe contract is local to this module.

use std::ffi::c_void;
use windows::core::{implement, Interface, GUID, HSTRING, PCWSTR};
use windows::Foundation::{IPropertyValue, PropertyValue};
use windows::Graphics::Effects::{
    IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectSource_Impl, IGraphicsEffect_Impl,
};

windows::core::imp::define_interface!(
    IGraphicsEffectD2D1Interop,
    IGraphicsEffectD2D1Interop_Vtbl,
    0x2fc57384_a068_44d7_a331_30982fcf7177
);

windows::core::imp::interface_hierarchy!(IGraphicsEffectD2D1Interop, windows::core::IUnknown);

impl windows::core::RuntimeName for IGraphicsEffectD2D1Interop {}

#[repr(C)]
#[doc(hidden)]
#[allow(non_snake_case)]
pub struct IGraphicsEffectD2D1Interop_Vtbl {
    pub base__: windows::core::IUnknown_Vtbl,
    pub GetEffectId: unsafe extern "system" fn(*mut c_void, *mut GUID) -> windows::core::HRESULT,
    pub GetNamedPropertyMapping: unsafe extern "system" fn(
        *mut c_void,
        PCWSTR,
        *mut u32,
        *mut i32,
    ) -> windows::core::HRESULT,
    pub GetPropertyCount:
        unsafe extern "system" fn(*mut c_void, *mut u32) -> windows::core::HRESULT,
    pub GetProperty:
        unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> windows::core::HRESULT,
    pub GetSource:
        unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> windows::core::HRESULT,
    pub GetSourceCount: unsafe extern "system" fn(*mut c_void, *mut u32) -> windows::core::HRESULT,
}

#[allow(non_camel_case_types, non_snake_case)]
pub(crate) trait IGraphicsEffectD2D1Interop_Impl: windows::core::IUnknownImpl {
    fn GetEffectId(&self, id: *mut GUID) -> windows::core::Result<()>;
    fn GetNamedPropertyMapping(
        &self,
        name: PCWSTR,
        index: *mut u32,
        mapping: *mut i32,
    ) -> windows::core::Result<()>;
    fn GetPropertyCount(&self, count: *mut u32) -> windows::core::Result<()>;
    fn GetProperty(&self, index: u32, value: *mut *mut c_void) -> windows::core::Result<()>;
    fn GetSource(&self, index: u32, source: *mut *mut c_void) -> windows::core::Result<()>;
    fn GetSourceCount(&self, count: *mut u32) -> windows::core::Result<()>;
}

const CLSID_D2D1_GAUSSIAN_BLUR: GUID = GUID::from_u128(0x1feb6d69_2fe6_4ac9_8c58_1d7f93e7a6a5);

const E_INVALIDARG_HRESULT: windows::core::HRESULT = windows::core::HRESULT(0x80070057u32 as i32);

const E_POINTER_HRESULT: windows::core::HRESULT = windows::core::HRESULT(0x80004003u32 as i32);

fn invalid_argument<T>() -> windows::core::Result<T> {
    Err(windows::core::Error::from_hresult(E_INVALIDARG_HRESULT))
}

fn null_pointer<T>() -> windows::core::Result<T> {
    Err(windows::core::Error::from_hresult(E_POINTER_HRESULT))
}

#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
pub(super) struct GaussianBlurEffectGraph {
    pub(super) source: IGraphicsEffectSource,
}

impl IGraphicsEffectSource_Impl for GaussianBlurEffectGraph_Impl {}

impl IGraphicsEffect_Impl for GaussianBlurEffectGraph_Impl {
    fn Name(&self) -> windows::core::Result<HSTRING> {
        Ok(HSTRING::from("GaussianBlur"))
    }

    fn SetName(&self, _name: &HSTRING) -> windows::core::Result<()> {
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for GaussianBlurEffectGraph_Impl {
    fn GetEffectId(&self, id: *mut GUID) -> windows::core::Result<()> {
        if id.is_null() {
            return null_pointer();
        }
        unsafe { *id = CLSID_D2D1_GAUSSIAN_BLUR };
        Ok(())
    }

    fn GetNamedPropertyMapping(
        &self,
        name: PCWSTR,
        index: *mut u32,
        mapping: *mut i32,
    ) -> windows::core::Result<()> {
        if name.0.is_null() || index.is_null() || mapping.is_null() {
            return null_pointer();
        }
        let name = unsafe { name.to_string()? };
        let property_index = match name.as_str() {
            "BlurAmount" => 0,
            "Optimization" => 1,
            "BorderMode" => 2,
            _ => return invalid_argument(),
        };
        unsafe {
            *index = property_index;
            *mapping = 1;
        }
        Ok(())
    }

    fn GetPropertyCount(&self, count: *mut u32) -> windows::core::Result<()> {
        if count.is_null() {
            return null_pointer();
        }
        unsafe { *count = 3 };
        Ok(())
    }

    fn GetProperty(&self, index: u32, value: *mut *mut c_void) -> windows::core::Result<()> {
        if value.is_null() {
            return null_pointer();
        }
        let property: IPropertyValue = match index {
            0 => PropertyValue::CreateSingle(18.0)?.cast()?,
            1 | 2 => PropertyValue::CreateUInt32(1)?.cast()?,
            _ => return invalid_argument(),
        };
        unsafe { *value = property.into_raw() };
        Ok(())
    }

    fn GetSource(&self, index: u32, source: *mut *mut c_void) -> windows::core::Result<()> {
        if source.is_null() {
            return null_pointer();
        }
        if index != 0 {
            return invalid_argument();
        }
        unsafe { *source = self.source.clone().into_raw() };
        Ok(())
    }

    fn GetSourceCount(&self, count: *mut u32) -> windows::core::Result<()> {
        if count.is_null() {
            return null_pointer();
        }
        unsafe { *count = 1 };
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Vtbl {
    pub(crate) const fn new<Identity: IGraphicsEffectD2D1Interop_Impl, const OFFSET: isize>() -> Self
    {
        unsafe extern "system" fn get_effect_id<
            Identity: IGraphicsEffectD2D1Interop_Impl,
            const OFFSET: isize,
        >(
            this: *mut c_void,
            id: *mut GUID,
        ) -> windows::core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IGraphicsEffectD2D1Interop_Impl::GetEffectId(this, id).into()
            }
        }
        unsafe extern "system" fn get_named_property_mapping<
            Identity: IGraphicsEffectD2D1Interop_Impl,
            const OFFSET: isize,
        >(
            this: *mut c_void,
            name: PCWSTR,
            index: *mut u32,
            mapping: *mut i32,
        ) -> windows::core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IGraphicsEffectD2D1Interop_Impl::GetNamedPropertyMapping(this, name, index, mapping)
                    .into()
            }
        }
        unsafe extern "system" fn get_property_count<
            Identity: IGraphicsEffectD2D1Interop_Impl,
            const OFFSET: isize,
        >(
            this: *mut c_void,
            count: *mut u32,
        ) -> windows::core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IGraphicsEffectD2D1Interop_Impl::GetPropertyCount(this, count).into()
            }
        }
        unsafe extern "system" fn get_property<
            Identity: IGraphicsEffectD2D1Interop_Impl,
            const OFFSET: isize,
        >(
            this: *mut c_void,
            index: u32,
            value: *mut *mut c_void,
        ) -> windows::core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IGraphicsEffectD2D1Interop_Impl::GetProperty(this, index, value).into()
            }
        }
        unsafe extern "system" fn get_source<
            Identity: IGraphicsEffectD2D1Interop_Impl,
            const OFFSET: isize,
        >(
            this: *mut c_void,
            index: u32,
            source: *mut *mut c_void,
        ) -> windows::core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IGraphicsEffectD2D1Interop_Impl::GetSource(this, index, source).into()
            }
        }
        unsafe extern "system" fn get_source_count<
            Identity: IGraphicsEffectD2D1Interop_Impl,
            const OFFSET: isize,
        >(
            this: *mut c_void,
            count: *mut u32,
        ) -> windows::core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IGraphicsEffectD2D1Interop_Impl::GetSourceCount(this, count).into()
            }
        }
        Self {
            base__: windows::core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            GetEffectId: get_effect_id::<Identity, OFFSET>,
            GetNamedPropertyMapping: get_named_property_mapping::<Identity, OFFSET>,
            GetPropertyCount: get_property_count::<Identity, OFFSET>,
            GetProperty: get_property::<Identity, OFFSET>,
            GetSource: get_source::<Identity, OFFSET>,
            GetSourceCount: get_source_count::<Identity, OFFSET>,
        }
    }

    pub(crate) fn matches(iid: &GUID) -> bool {
        iid == &<IGraphicsEffectD2D1Interop as windows::core::Interface>::IID
    }
}
