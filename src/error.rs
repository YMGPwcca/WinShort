//! Error type wrapping Win32/HRESULT failures with API context.

use std::fmt;



pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone)]
pub enum Error {
    /// A Windows API call failed. `code` is an HRESULT or a Win32 error code;
    /// `api` names the entry point, `context` adds call-site detail.
    Os {
        api: &'static str,
        code: u32,
        context: Option<String>,
    },
    /// Configuration problem (parse/validate).
    Config(String),
    /// Audio subsystem failure that is not a single API call.
    Audio(String),
    /// Virtual desktop subsystem failure.
    Desktop(String),
    /// Internal invariant violated (should be impossible; caught in debug).
    Internal(String),
}

impl Error {
    pub fn os(api: &'static str, code: u32) -> Self {
        Error::Os { api, code, context: None }
    }

    pub fn os_ctx(api: &'static str, code: u32, context: impl Into<String>) -> Self {
        Error::Os { api, code, context: Some(context.into()) }
    }

    /// Build from a `windows` crate error.
    pub fn win(api: &'static str, err: &windows_core::Error) -> Self {
        Error::Os { api, code: err.code().0 as u32, context: None }
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Error::Config(msg.into())
    }

    pub fn audio(msg: impl Into<String>) -> Self {
        Error::Audio(msg.into())
    }

    pub fn desktop(msg: impl Into<String>) -> Self {
        Error::Desktop(msg.into())
    }

    pub fn internal(what: impl Into<String>) -> Self {
        Error::Internal(what.into())
    }

    /// Best-effort symbolic name for well-known HRESULTs, else hex.
    ///
    /// Values cross-checked against the `windows` crate definitions
    /// (Win32::Foundation and Win32::Media::Audio, windows-0.62.2).
    pub fn code_name(code: u32) -> String {
        const AUDCLNT_E_DEVICE_INVALIDATED: u32 = 0x88890004; // Win32::Media::Audio
        const AUDCLNT_E_ENDPOINT_CREATE_FAILED: u32 = 0x8889000F;
        const AUDCLNT_E_SERVICE_NOT_RUNNING: u32 = 0x88890010;
        const E_ACCESSDENIED: u32 = 0x80070005; // Win32::Foundation
        const E_OUTOFMEMORY: u32 = 0x8007000E;
        const E_FAIL: u32 = 0x80004005;
        const E_NOTIMPL: u32 = 0x80004001;
        let named = match code {
            AUDCLNT_E_DEVICE_INVALIDATED => "AUDCLNT_E_DEVICE_INVALIDATED",
            AUDCLNT_E_ENDPOINT_CREATE_FAILED => "AUDCLNT_E_ENDPOINT_CREATE_FAILED",
            AUDCLNT_E_SERVICE_NOT_RUNNING => "AUDCLNT_E_SERVICE_NOT_RUNNING",
            E_ACCESSDENIED => "E_ACCESSDENIED",
            E_OUTOFMEMORY => "E_OUTOFMEMORY",
            E_FAIL => "E_FAIL",
            E_NOTIMPL => "E_NOTIMPL",
            _ => return format!("0x{code:08X}"),
        };
        format!("{named} (0x{code:08X})")
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn hresult_names_match_sdk_constants() {
        // Regression for #9: the decimal literals previously mapped the WRONG
        // values (e.g. E_ACCESSDENIED was bound to 0x8898... not 0x80070005).
        let cases: [(u32, &str); 7] = [
            (0x88890004, "AUDCLNT_E_DEVICE_INVALIDATED"),
            (0x8889000F, "AUDCLNT_E_ENDPOINT_CREATE_FAILED"),
            (0x88890010, "AUDCLNT_E_SERVICE_NOT_RUNNING"),
            (0x80070005, "E_ACCESSDENIED"),
            (0x8007000E, "E_OUTOFMEMORY"),
            (0x80004005, "E_FAIL"),
            (0x80004001, "E_NOTIMPL"),
        ];
        for (code, name) in cases {
            assert!(Error::code_name(code).starts_with(name), "{name}: {:#010X} -> {}", code, Error::code_name(code));
        }
        assert_eq!(Error::code_name(0x12345678), "0x12345678");
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Os { api, code, context } => {
                write!(f, "{api} failed: {}", Self::code_name(*code))?;
                if let Some(c) = context {
                    write!(f, " ({c})")?;
                }
                Ok(())
            }
            Error::Config(m) => write!(f, "configuration error: {m}"),
            Error::Audio(m) => write!(f, "audio error: {m}"),
            Error::Desktop(m) => write!(f, "virtual desktop error: {m}"),
            Error::Internal(w) => write!(f, "internal error: {w}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<windows_core::Error> for Error {
    fn from(e: windows_core::Error) -> Self {
        Error::Os { api: "<unknown>", code: e.code().0 as u32, context: None }
    }
}
