//! COM apartment initialization guard (#24): `CoUninitialize` runs on Drop
//! only when this object actually initialized the apartment.

use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
};

/// Owns one successful `CoInitializeEx` reference.
pub struct ComApartment {
    initialized: bool,
}

impl ComApartment {
    pub fn init_sta() -> Self {
        Self::init(COINIT_APARTMENTTHREADED)
    }

    pub fn init_mta() -> Self {
        Self::init(COINIT_MULTITHREADED)
    }

    fn init(coinit: COINIT) -> Self {
        let hr = unsafe { CoInitializeEx(None, coinit) };
        Self {
            initialized: hr.0 >= 0,
        }
    }

    /// False when the apartment could not be initialized (fatal for callers
    /// that require COM).
    pub fn ok(&self) -> bool {
        self.initialized
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}
