//! "Start with Windows" via the documented per-user Run key (spec §53).
//!
//! HKCU\Software\Microsoft\Windows\CurrentVersion\Run — no service, no
//! elevation required. Reg* functions here return WIN32_ERROR rather than Result.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteKeyValueW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW, HKEY,
    HKEY_CURRENT_USER, REG_SZ, REG_VALUE_TYPE, RRF_RT_REG_SZ, KEY_SET_VALUE, KEY_QUERY_VALUE,
};

use crate::error::{Error, Result};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "WinShort";

fn command_line() -> String {
    let exe = std::env::current_exe().unwrap_or_default();
    format!("\"{}\"", exe.display())
}

pub fn is_enabled() -> bool {
    // SAFETY: fixed-size out buffer; read-only query.
    unsafe {
        let run = PCWSTR(HSTRING::from(RUN_KEY).as_ptr());
        let name = PCWSTR(HSTRING::from(VALUE_NAME).as_ptr());
        let mut buf = [0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let mut kind = REG_VALUE_TYPE(0);
        RegGetValueW(
            HKEY_CURRENT_USER,
            run,
            name,
            RRF_RT_REG_SZ,
            Some(&mut kind),
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        ) == ERROR_SUCCESS
    }
}

pub fn set_enabled(enable: bool) -> Result<()> {
    // SAFETY: opened with set+query rights; closed on all paths.
    unsafe {
        let mut hkey = HKEY::default();
        let run = PCWSTR(HSTRING::from(RUN_KEY).as_ptr());
        let err = RegOpenKeyExW(HKEY_CURRENT_USER, run, None, KEY_SET_VALUE | KEY_QUERY_VALUE, &mut hkey);
        if err != ERROR_SUCCESS {
            return Err(Error::os_ctx("RegOpenKeyExW(Run)", err.0, "opening Run key"));
        }
        let result = if enable {
            let data = HSTRING::from(command_line());
            RegSetKeyValueW(
                hkey,
                None,
                PCWSTR(HSTRING::from(VALUE_NAME).as_ptr()),
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                ((data.len() + 1) * 2) as u32,
            )
        } else {
            let del = RegDeleteKeyValueW(hkey, None, PCWSTR(HSTRING::from(VALUE_NAME).as_ptr()));
            // Missing value already means disabled: not an error.
            if del == windows::Win32::Foundation::WIN32_ERROR(2) { ERROR_SUCCESS } else { del }
        };
        let _ = RegCloseKey(hkey);
        if result == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(Error::os_ctx("registry write", result.0, "Run key update"))
        }
    }
}
