//! OS build detection for virtual-desktop layout selection.
//!
//! Reads the documented registry location (`HKLM\...\CurrentVersion`), which
//! every Windows version maintains. Build number drives the fail-closed
//! whitelist in `internal_api` (see VIRTUAL_DESKTOP_COMPAT.md).

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_LOCAL_MACHINE, REG_VALUE_TYPE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsBuild {
    pub build: u32,
    pub update_revision: u32,
}

impl OsBuild {
    /// Builds whose undocumented interface layout WinShort has pinned
    /// (24H2 / 25H2 era). Anything else must fail closed.
    pub fn native_shell_supported(&self) -> bool {
        self.build == 26_100 || (26_200..=26_299).contains(&self.build)
    }
}

/// Query the current OS build.
pub fn detect() -> Result<OsBuild> {
    let subkey = PCWSTR(HSTRING::from("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion").as_ptr());
    let build_str = query_string(subkey, "CurrentBuildNumber")?;
    let build: u32 = build_str
        .trim()
        .parse()
        .map_err(|_| Error::desktop(format!("malformed CurrentBuildNumber `{build_str}`")))?;
    let update_revision = query_dword(subkey, "UBR").unwrap_or(0);
    Ok(OsBuild { build, update_revision })
}

fn query_string(subkey: PCWSTR, value: &str) -> Result<String> {
    let name = PCWSTR(HSTRING::from(value).as_ptr());
    let mut buf = [0u16; 256];
    let mut size = (buf.len() * 2) as u32;
    let mut kind = REG_VALUE_TYPE(0);
    // SAFETY: buffers sized above; documented registry API.
    let res = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey,
            name,
            RRF_RT_REG_SZ,
            Some(&mut kind),
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if res.is_err() {
        return Err(Error::os_ctx("RegGetValueW", res.0, value));
    }
    let len = (size as usize / 2).min(buf.len());
    while len > 0 && buf[len - 1] == 0 {
        // trim NUL via slice bound below
        break;
    }
    let end = buf[..len].iter().rposition(|&c| c != 0).map_or(0, |p| p + 1);
    Ok(String::from_utf16_lossy(&buf[..end]))
}

fn query_dword(subkey: PCWSTR, value: &str) -> Result<u32> {
    let name = PCWSTR(HSTRING::from(value).as_ptr());
    let mut data = 0u32;
    let mut size = 4u32;
    let mut kind = REG_VALUE_TYPE(0);
    // SAFETY: dword-sized out buffer; documented registry API.
    let res = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey,
            name,
            RRF_RT_REG_DWORD,
            Some(&mut kind),
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
    };
    if res.is_err() {
        return Err(Error::os_ctx("RegGetValueW", res.0, value));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_matches_this_machine() {
        // This development machine runs 26200 (25H2).
        let b = OsBuild { build: 26_200, update_revision: 9168 };
        assert!(b.native_shell_supported());
        assert!(!OsBuild { build: 22_631, update_revision: 0 }.native_shell_supported());
        assert!(!OsBuild { build: 27_000, update_revision: 0 }.native_shell_supported());
    }

    #[test]
    fn detect_reads_live_build() {
        // Live smoke: this test runs on Windows; detection must succeed and
        // match the known dev-machine build family.
        let b = detect().expect("registry build detection");
        assert!(b.build >= 26_100, "unexpected build {}", b.build);
    }
}
