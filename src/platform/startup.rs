//! "Start with Windows" via the documented per-user Run key (spec §53).
//!
//! HKCU\Software\Microsoft\Windows\CurrentVersion\Run — no service, no
//! elevation required. The REGISTRY is the single source of truth (#16):
//! `is_enabled` compares the registered command with the running executable,
//! so a stale path reads as disabled instead of silently launching the old
//! binary.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteKeyValueW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW, HKEY,
    HKEY_CURRENT_USER, REG_SZ, REG_VALUE_TYPE, RRF_RT_REG_SZ, KEY_SET_VALUE, KEY_QUERY_VALUE,
};

use crate::error::{Error, Result};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "WinShort";

/// Registry state for diagnostics (#16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupState {
    Enabled,
    /// A Run value exists but its command differs from this executable.
    Stale { registered: String, current: String },
    Disabled,
}

fn command_line() -> Result<String> {
    let exe = std::env::current_exe()
        .map_err(|e| Error::config(format!("current_exe: {e}")))?;
    Ok(format!("\"{}\"", exe.display()))
}

/// Read the raw registered command, if any.
fn registered_command() -> Result<Option<String>> {
    // SAFETY: fixed-size out buffer; read-only query; HSTRING owners bound.
    unsafe {
        let run_h = HSTRING::from(RUN_KEY);
        let name_h = HSTRING::from(VALUE_NAME);
        let run = PCWSTR(run_h.as_ptr());
        let name = PCWSTR(name_h.as_ptr());
        let mut buf = [0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let mut kind = REG_VALUE_TYPE(0);
        let res = RegGetValueW(
            HKEY_CURRENT_USER,
            run,
            name,
            RRF_RT_REG_SZ,
            Some(&mut kind),
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        );
        if res != ERROR_SUCCESS {
            return Ok(None); // missing value == disabled
        }
        let len = (size as usize / 2).min(buf.len());
        let end = buf[..len].iter().rposition(|&c| c != 0).map_or(0, |p| p + 1);
        Ok(Some(String::from_utf16_lossy(&buf[..end])))
    }
}

fn same_command(a: &str, b: &str) -> bool {
    a.trim_matches('"').eq_ignore_ascii_case(b.trim_matches('"'))
}

pub fn startup_state() -> Result<StartupState> {
    let Some(registered) = registered_command()? else {
        return Ok(StartupState::Disabled);
    };
    let current = command_line()?;
    if same_command(&registered, &current) {
        Ok(StartupState::Enabled)
    } else {
        Ok(StartupState::Stale { registered, current })
    }
}

pub fn is_enabled() -> bool {
    matches!(startup_state(), Ok(StartupState::Enabled))
}

pub fn set_enabled(enable: bool) -> Result<()> {
    // SAFETY: opened with set+query rights; closed on all paths.
    unsafe {
        let mut hkey = HKEY::default();
        // SAFETY: `run_h` owns the wide string passed to RegOpenKeyExW.
        let run_h = HSTRING::from(RUN_KEY);
        let run = PCWSTR(run_h.as_ptr());
        let err = RegOpenKeyExW(HKEY_CURRENT_USER, run, None, KEY_SET_VALUE | KEY_QUERY_VALUE, &mut hkey);
        if err != ERROR_SUCCESS {
            return Err(Error::os_ctx("RegOpenKeyExW(Run)", err.0, "opening Run key"));
        }
        let result = if enable {
            let data = HSTRING::from(command_line()?);
            // SAFETY: value name owner lives through the call.
            let name_h = HSTRING::from(VALUE_NAME);
            RegSetKeyValueW(
                hkey,
                None,
                PCWSTR(name_h.as_ptr()),
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                ((data.len() + 1) * 2) as u32,
            )
        } else {
            // SAFETY: value name owner lives through the call.
            let name_h = HSTRING::from(VALUE_NAME);
            let del = RegDeleteKeyValueW(hkey, None, PCWSTR(name_h.as_ptr()));
            // Missing value already means disabled: not an error.
            if del == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND { ERROR_SUCCESS } else { del }
        };
        let _ = RegCloseKey(hkey);
        if result == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(Error::os_ctx("registry write", result.0, "Run key update"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_command_ignores_quotes_and_case() {
        assert!(same_command("\"C:\\Apps\\Win Short.exe\"", "c:\\apps\\win short.exe"));
        assert!(!same_command("\"C:\\A\\x.exe\"", "C:\\B\\x.exe"));
    }

    #[test]
    fn command_line_is_quoted() {
        let cmd = command_line().unwrap();
        assert!(cmd.starts_with('"') && cmd.ends_with('"'));
    }
}
