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

    /// Best-effort symbolic name for well-known audio HRESULTs, else hex.
    pub fn code_name(code: u32) -> String {
        let h = code as i32;
        let named = match h {
            -2_004_287_168 => "AUDCLNT_E_DEVICE_INVALIDATED",
            -2_004_287_167 => "AUDCLNT_E_ENDPOINT_CREATE_FAILED",
            -2_144_672_157 => "AUDCLNT_E_SERVICE_NOT_RUNNING",
            -2_147_014_883 => "E_ACCESSDENIED", // 0x80070005
            -2_147_467_259 => "E_FAIL",         // 0x80004005
            -2_147_467_263 => "E_NOTIMPL",      // 0x80004001
            -2_147_481_650 => "E_OUTOFMEMORY",  // 0x8007000E
            _ => return format!("0x{code:08X}"),
        };
        format!("{named} (0x{code:08X})")
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
